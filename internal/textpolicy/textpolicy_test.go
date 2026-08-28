package textpolicy

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
		{name: "maximum", body: strings.Repeat("界", MaxBodyCodePoints)},
		{name: "empty", body: "", want: ErrBodyEmpty},
		{name: "limit plus one", body: strings.Repeat("a", MaxBodyCodePoints+1), want: ErrBodyCodePoints},
		{name: "oversized bytes", body: strings.Repeat("界", MaxBodyBytes), want: ErrBodyBytes},
		{name: "invalid UTF-8", body: string([]byte{0xff}), want: ErrBodyUTF8},
		{name: "escape", body: "hello\x1b[2J", want: ErrBodyControl},
		{name: "tab", body: "hello\tworld", want: ErrBodyControl},
		{name: "bidi control", body: "hello\u202eworld", want: ErrBodyControl},
		{name: "default ignorable", body: "hello\u034fworld", want: ErrBodyControl},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			err := ValidateBody(test.body)
			if !errors.Is(err, test.want) {
				t.Fatalf("ValidateBody() error = %v, want %v", err, test.want)
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
			canonical, key, err := NormalizeAlias(alias)
			if err != nil {
				t.Fatalf("NormalizeAlias(%q): %v", alias, err)
			}
			if canonical == "" || key == "" {
				t.Fatalf("NormalizeAlias(%q) returned an empty canonical value", alias)
			}
		})
	}

	invalid := []struct {
		name  string
		alias string
		want  error
	}{
		{name: "too short", alias: "a", want: ErrAliasLength},
		{name: "too long", alias: strings.Repeat("a", MaxAliasCodePoints+1), want: ErrAliasLength},
		{name: "mixed scripts", alias: "aЖ", want: ErrAliasScript},
		{name: "leading space", alias: " aoi", want: ErrAliasCharacters},
		{name: "double space", alias: "ao  i", want: ErrAliasCharacters},
		{name: "trailing space", alias: "aoi ", want: ErrAliasCharacters},
		{name: "emoji", alias: "aoi😀", want: ErrAliasCharacters},
		{name: "format character", alias: "ao\u200di", want: ErrAliasCharacters},
		{name: "mark after separator", alias: "a-\u0301", want: ErrAliasCharacters},
		{name: "reserved", alias: "MORI", want: ErrAliasUnavailable},
		{name: "fixture reserved", alias: "aoi", want: ErrAliasUnavailable},
		{name: "confusable reserved", alias: "ｍｏｒｉ", want: ErrAliasUnavailable},
		{name: "cross-script mark", alias: "a\u064b", want: ErrAliasScript},
		{name: "invisible mark", alias: "mo\u034fri", want: ErrAliasCharacters},
		{name: "whole-script confusable", alias: "αοι", want: ErrAliasUnavailable},
		{name: "expanded comparison key", alias: strings.Repeat("ﷺ", MaxAliasCodePoints), want: ErrAliasCharacters},
	}

	for _, test := range invalid {
		t.Run(test.name, func(t *testing.T) {
			_, _, err := NormalizeAlias(test.alias)
			if !errors.Is(err, test.want) {
				t.Fatalf("NormalizeAlias(%q) error = %v, want %v", test.alias, err, test.want)
			}
		})
	}
}

func TestAliasNormalizationUsesExactNFCAndTR39Key(t *testing.T) {
	t.Parallel()

	canonical, key, err := NormalizeAlias("e\u0301clair")
	if err != nil {
		t.Fatalf("NormalizeAlias: %v", err)
	}
	if canonical != "éclair" {
		t.Fatalf("canonical alias = %q, want %q", canonical, "éclair")
	}
	_, wantKey, err := NormalizeAlias("ÉCLAIR")
	if err != nil {
		t.Fatalf("NormalizeAlias comparison: %v", err)
	}
	if key != wantKey {
		t.Fatalf("TR39 key = %q, want %q", key, wantKey)
	}
}
