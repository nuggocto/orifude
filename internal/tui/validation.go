package tui

import (
	"fmt"
	"strings"
	"unicode"
)

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
