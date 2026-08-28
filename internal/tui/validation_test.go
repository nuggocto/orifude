package tui

import (
	"strings"
	"testing"
)

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
