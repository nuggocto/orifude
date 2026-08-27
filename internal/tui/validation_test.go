package tui

import (
	"errors"
	"strings"
	"testing"
)

func TestBodyValidationBoundariesAndControls(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name string
		body string
		want error
	}{
		{name: "one code point", body: "界"},
		{name: "newlines", body: "one\ntwo"},
		{name: "maximum", body: strings.Repeat("界", maxBodyCodePoints)},
		{name: "empty", body: "", want: errBodyEmpty},
		{name: "limit plus one", body: strings.Repeat("a", maxBodyCodePoints+1), want: errBodyCodePoints},
		{name: "oversized bytes", body: strings.Repeat("界", maxBodyBytes), want: errBodyBytes},
		{name: "invalid UTF-8", body: string([]byte{0xff}), want: errBodyUTF8},
		{name: "escape", body: "hello\x1b[2J", want: errBodyControl},
		{name: "tab", body: "hello\tworld", want: errBodyControl},
		{name: "bidi control", body: "hello\u202eworld", want: errBodyControl},
		{name: "default ignorable", body: "hello\u034fworld", want: errBodyControl},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			err := validateBody(test.body)
			if !errors.Is(err, test.want) {
				t.Fatalf("validateBody() error = %v, want %v", err, test.want)
			}
		})
	}
}

func TestAliasValidationAppliesUnicodeAndReservationRules(t *testing.T) {
	t.Parallel()

	valid := []string{
		"quiet branch",
		"éclair",
		"кира",
		"عَرَبِي",
		"ひかり空",
		"ソラー7",
		"山田_7",
	}
	for _, alias := range valid {
		t.Run("valid "+alias, func(t *testing.T) {
			canonical, key, err := normalizeAlias(alias)
			if err != nil {
				t.Fatalf("normalizeAlias(%q): %v", alias, err)
			}
			if canonical == "" || key == "" {
				t.Fatalf("normalizeAlias(%q) returned an empty canonical value", alias)
			}
		})
	}

	invalid := []struct {
		name  string
		alias string
		want  error
	}{
		{name: "too short", alias: "a", want: errAliasLength},
		{name: "too long", alias: strings.Repeat("a", maxAliasPoints+1), want: errAliasLength},
		{name: "mixed scripts", alias: "aЖ", want: errAliasScript},
		{name: "leading space", alias: " aoi", want: errAliasCharacters},
		{name: "double space", alias: "ao  i", want: errAliasCharacters},
		{name: "trailing space", alias: "aoi ", want: errAliasCharacters},
		{name: "emoji", alias: "aoi😀", want: errAliasCharacters},
		{name: "format character", alias: "ao\u200di", want: errAliasCharacters},
		{name: "mark after separator", alias: "a-\u0301", want: errAliasCharacters},
		{name: "reserved", alias: "MORI", want: errAliasUnavailable},
		{name: "fixture reserved", alias: "aoi", want: errAliasUnavailable},
		{name: "confusable reserved", alias: "ｍｏｒｉ", want: errAliasUnavailable},
		{name: "cross-script mark", alias: "a\u064b", want: errAliasScript},
		{name: "invisible mark", alias: "mo\u034fri", want: errAliasCharacters},
		{name: "whole-script confusable", alias: "αοι", want: errAliasUnavailable},
	}

	for _, test := range invalid {
		t.Run(test.name, func(t *testing.T) {
			_, _, err := normalizeAlias(test.alias)
			if !errors.Is(err, test.want) {
				t.Fatalf("normalizeAlias(%q) error = %v, want %v", test.alias, err, test.want)
			}
		})
	}
}

func TestAliasNormalizationUsesExactNFCAndTR39Key(t *testing.T) {
	t.Parallel()

	canonical, key, err := normalizeAlias("e\u0301clair")
	if err != nil {
		t.Fatalf("normalizeAlias: %v", err)
	}
	if canonical != "éclair" {
		t.Fatalf("canonical alias = %q, want %q", canonical, "éclair")
	}
	wantKey, err := aliasKey("ÉCLAIR")
	if err != nil {
		t.Fatalf("aliasKey: %v", err)
	}
	if key != wantKey {
		t.Fatalf("TR39 key = %q, want %q", key, wantKey)
	}
}

func TestTerminalControlsAreVisiblyNeutralized(t *testing.T) {
	t.Parallel()

	got := neutralizeTerminalText("hello\x1b]8;;https://example.com\aopen\u202e")
	for _, forbidden := range []string{"\x1b", "\a", "\u202e"} {
		if strings.Contains(got, forbidden) {
			t.Fatalf("neutralized output still contains control %q: %q", forbidden, got)
		}
	}
	if !strings.Contains(got, `\u{001B}`) || !strings.Contains(got, `\u{202E}`) {
		t.Fatalf("neutralized output does not visibly identify controls: %q", got)
	}
}
