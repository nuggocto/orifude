package tui

import (
	"errors"
	"fmt"
	"sort"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/disciplinedware/go-confusables"
	"golang.org/x/text/cases"
	"golang.org/x/text/unicode/norm"
)

const (
	maxBodyCodePoints = 2_000
	maxBodyBytes      = 12 * 1024
	minAliasPoints    = 2
	maxAliasPoints    = 24
	pinnedUnicode     = "17.0.0"
)

var (
	errBodyEmpty        = errors.New("write at least one character")
	errBodyCodePoints   = errors.New("letter exceeds 2,000 Unicode code points")
	errBodyBytes        = errors.New("letter exceeds 12 KiB")
	errBodyUTF8         = errors.New("letter is not valid UTF-8")
	errBodyControl      = errors.New("letter contains an unsafe control character")
	errAliasLength      = errors.New("alias must contain 2 to 24 Unicode code points")
	errAliasCharacters  = errors.New("alias contains an unsupported character")
	errAliasScript      = errors.New("alias must use one writing system")
	errAliasUnavailable = errors.New("alias is unavailable")
	errUnicodeVersion   = errors.New("Unicode validation tables do not match the pinned version")
)

var aliasScripts = func() []string {
	names := make([]string, 0, len(unicode.Scripts))
	for name := range unicode.Scripts {
		if name != "Common" && name != "Inherited" {
			names = append(names, name)
		}
	}
	sort.Strings(names)
	return names
}()

var reservedAliasKeys = func() []string {
	aliases := []string{"orifude", "aoi", "mori"}
	keys := make([]string, 0, len(aliases))
	for _, alias := range aliases {
		key, err := aliasKey(alias)
		if err != nil {
			panic(err)
		}
		keys = append(keys, key)
	}
	return keys
}()

func validateBody(body string) error {
	if !utf8.ValidString(body) {
		return errBodyUTF8
	}
	if body == "" {
		return errBodyEmpty
	}
	if len(body) > maxBodyBytes {
		return errBodyBytes
	}
	if utf8.RuneCountInString(body) > maxBodyCodePoints {
		return errBodyCodePoints
	}
	for _, r := range body {
		if r != '\n' && unsafeTextRune(r) {
			return errBodyControl
		}
	}
	return nil
}

func normalizeAlias(raw string) (string, string, error) {
	if !utf8.ValidString(raw) {
		return "", "", errAliasCharacters
	}
	if unicode.Version != pinnedUnicode || norm.Version != pinnedUnicode || confusables.Default().UnicodeVersion() != pinnedUnicode {
		return "", "", errUnicodeVersion
	}

	alias := norm.NFC.String(raw)
	count := utf8.RuneCountInString(alias)
	if count < minAliasPoints || count > maxAliasPoints {
		return "", "", errAliasLength
	}

	var script string
	hasLetter := false
	previousLetterOrMark := false
	previousSpace := false
	for index, r := range alias {
		if unsafeTextRune(r) {
			return "", "", errAliasCharacters
		}
		switch {
		case r == ' ':
			if index == 0 || previousSpace {
				return "", "", errAliasCharacters
			}
			previousSpace = true
			previousLetterOrMark = false
			continue
		case r == '-' || r == '_' || (r >= '0' && r <= '9'):
			previousSpace = false
			previousLetterOrMark = false
			continue
		case unicode.IsLetter(r):
			hasLetter = true
		case unicode.IsMark(r):
			if !previousLetterOrMark {
				return "", "", errAliasCharacters
			}
			if scriptFor(r) == "" && !inheritedMarkAllowed(script, r) {
				return "", "", errAliasScript
			}
		default:
			return "", "", errAliasCharacters
		}

		runeScript := scriptFor(r)
		if runeScript == "" {
			if unicode.IsLetter(r) && !(script == "Japanese" && r == 'ー') {
				return "", "", errAliasCharacters
			}
		} else if script == "" {
			script = runeScript
		} else if script != runeScript {
			return "", "", errAliasScript
		}
		previousLetterOrMark = true
		previousSpace = false
	}

	if !hasLetter || strings.HasSuffix(alias, " ") {
		return "", "", errAliasCharacters
	}
	key, err := aliasKey(alias)
	if err != nil {
		return "", "", err
	}
	for _, reserved := range reservedAliasKeys {
		if key == reserved {
			return "", "", errAliasUnavailable
		}
	}
	return alias, key, nil
}

func aliasKey(alias string) (string, error) {
	if unicode.Version != pinnedUnicode || norm.Version != pinnedUnicode || confusables.Default().UnicodeVersion() != pinnedUnicode {
		return "", errUnicodeVersion
	}
	folded := cases.Fold().String(norm.NFKC.String(alias))
	return norm.NFD.String(confusables.Default().Skeleton(folded)), nil
}

func inheritedMarkAllowed(script string, r rune) bool {
	switch script {
	case "Latin", "Greek", "Cyrillic":
		return r >= 0x0300 && r <= 0x036f || r >= 0x1ab0 && r <= 0x1aff || r >= 0x1dc0 && r <= 0x1dff || r >= 0xfe20 && r <= 0xfe2f
	case "Arabic":
		return r >= 0x0610 && r <= 0x061a || r >= 0x064b && r <= 0x065f || r == 0x0670 || r >= 0x06d6 && r <= 0x06ed || r >= 0x08d3 && r <= 0x08ff
	case "Hebrew":
		return r >= 0x0591 && r <= 0x05bd || r == 0x05bf || r >= 0x05c1 && r <= 0x05c2 || r >= 0x05c4 && r <= 0x05c5 || r == 0x05c7
	case "Japanese":
		return r == 0x3099 || r == 0x309a
	}
	return false
}

func scriptFor(r rune) string {
	if unicode.In(r, unicode.Han, unicode.Hiragana, unicode.Katakana) || r == 'ー' {
		return "Japanese"
	}
	for _, name := range aliasScripts {
		if unicode.Is(unicode.Scripts[name], r) {
			return name
		}
	}
	return ""
}

func neutralizeTerminalText(text string) string {
	text = strings.ToValidUTF8(text, `\u{FFFD}`)
	var out strings.Builder
	for _, r := range text {
		if r != '\n' && unsafeTextRune(r) {
			fmt.Fprintf(&out, "\\u{%04X}", r)
			continue
		}
		out.WriteRune(r)
	}
	return out.String()
}

func unsafeTextRune(r rune) bool {
	return unicode.IsControl(r) || unicode.In(r, unicode.Cf, unicode.Other_Default_Ignorable_Code_Point, unicode.Variation_Selector)
}
