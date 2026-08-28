package textpolicy

import (
	"errors"
	"sort"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/disciplinedware/go-confusables"
	"golang.org/x/text/cases"
	"golang.org/x/text/unicode/norm"
)

const (
	// MaxBodyCodePoints is the maximum number of Unicode code points in a body.
	MaxBodyCodePoints = 2_000
	// MaxBodyBytes is the maximum UTF-8 body size.
	MaxBodyBytes = 12 * 1024
	// MaxAliasCodePoints is the maximum number of Unicode code points in an alias.
	MaxAliasCodePoints = 24
	minAliasCodePoints = 2
	maxAliasKeyBytes   = 512
	pinnedUnicode      = "17.0.0"
)

var (
	// ErrBodyEmpty means a body has no content.
	ErrBodyEmpty = errors.New("write at least one character")
	// ErrBodyCodePoints means a body exceeds MaxBodyCodePoints.
	ErrBodyCodePoints = errors.New("letter exceeds 2,000 Unicode code points")
	// ErrBodyBytes means a body exceeds MaxBodyBytes.
	ErrBodyBytes = errors.New("letter exceeds 12 KiB")
	// ErrBodyUTF8 means a body is not valid UTF-8.
	ErrBodyUTF8 = errors.New("letter is not valid UTF-8")
	// ErrBodyControl means a body contains a forbidden control or formatting rune.
	ErrBodyControl = errors.New("letter contains an unsafe control character")
	// ErrAliasLength means an alias is outside the allowed code-point range.
	ErrAliasLength = errors.New("alias must contain 2 to 24 Unicode code points")
	// ErrAliasCharacters means an alias contains a forbidden character.
	ErrAliasCharacters = errors.New("alias contains an unsupported character")
	// ErrAliasScript means an alias mixes writing systems.
	ErrAliasScript = errors.New("alias must use one writing system")
	// ErrAliasUnavailable means an alias has a reserved comparison key.
	ErrAliasUnavailable = errors.New("alias is unavailable")
	// ErrUnicodeVersion means runtime Unicode tables do not match the policy version.
	ErrUnicodeVersion = errors.New("Unicode validation tables do not match the pinned version")
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

// ValidateBody validates a letter or reply body.
func ValidateBody(body string) error {
	if !utf8.ValidString(body) {
		return ErrBodyUTF8
	}
	if body == "" {
		return ErrBodyEmpty
	}
	if len(body) > MaxBodyBytes {
		return ErrBodyBytes
	}
	if utf8.RuneCountInString(body) > MaxBodyCodePoints {
		return ErrBodyCodePoints
	}
	for _, r := range body {
		if r != '\n' && unsafeTextRune(r) {
			return ErrBodyControl
		}
	}
	return nil
}

// NormalizeAlias returns the NFC alias and its case-folded TR39 comparison key.
func NormalizeAlias(raw string) (string, string, error) {
	if !utf8.ValidString(raw) {
		return "", "", ErrAliasCharacters
	}
	if unicode.Version != pinnedUnicode || norm.Version != pinnedUnicode || confusables.Default().UnicodeVersion() != pinnedUnicode {
		return "", "", ErrUnicodeVersion
	}

	alias := norm.NFC.String(raw)
	count := utf8.RuneCountInString(alias)
	if count < minAliasCodePoints || count > MaxAliasCodePoints {
		return "", "", ErrAliasLength
	}

	var script string
	hasLetter := false
	previousLetterOrMark := false
	previousSpace := false
	for index, r := range alias {
		if unsafeTextRune(r) {
			return "", "", ErrAliasCharacters
		}
		switch {
		case r == ' ':
			if index == 0 || previousSpace {
				return "", "", ErrAliasCharacters
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
				return "", "", ErrAliasCharacters
			}
			if scriptFor(r) == "" && !inheritedMarkAllowed(script, r) {
				return "", "", ErrAliasScript
			}
		default:
			return "", "", ErrAliasCharacters
		}

		runeScript := scriptFor(r)
		if runeScript == "" {
			if unicode.IsLetter(r) && !(script == "Japanese" && r == 'ー') {
				return "", "", ErrAliasCharacters
			}
		} else if script == "" {
			script = runeScript
		} else if script != runeScript {
			return "", "", ErrAliasScript
		}
		previousLetterOrMark = true
		previousSpace = false
	}

	if !hasLetter || strings.HasSuffix(alias, " ") {
		return "", "", ErrAliasCharacters
	}
	key, err := aliasKey(alias)
	if err != nil {
		return "", "", err
	}
	if len(key) > maxAliasKeyBytes {
		return "", "", ErrAliasCharacters
	}
	for _, reserved := range reservedAliasKeys {
		if key == reserved {
			return "", "", ErrAliasUnavailable
		}
	}
	return alias, key, nil
}

func aliasKey(alias string) (string, error) {
	if unicode.Version != pinnedUnicode || norm.Version != pinnedUnicode || confusables.Default().UnicodeVersion() != pinnedUnicode {
		return "", ErrUnicodeVersion
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

func unsafeTextRune(r rune) bool {
	return unicode.IsControl(r) || unicode.In(r, unicode.Cf, unicode.Other_Default_Ignorable_Code_Point, unicode.Variation_Selector)
}
