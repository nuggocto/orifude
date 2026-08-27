package tui

import (
	"reflect"
	"strings"
	"testing"
	"unicode"
)

func TestFoldFramesAreDeterministicAndASCIIHasNoControls(t *testing.T) {
	t.Parallel()

	first := foldFrames(42, 80, false)
	second := foldFrames(42, 80, false)
	if !reflect.DeepEqual(first, second) {
		t.Fatal("equal seed and size produced different fold frames")
	}
	if len(first) < 2 {
		t.Fatal("fold needs more than one frame")
	}
	variants := map[string]struct{}{}
	for seed := uint64(0); seed < 32; seed++ {
		variants[foldFrames(seed, 80, false)[2]] = struct{}{}
	}
	if len(variants) < 2 {
		t.Fatal("fold seed does not affect the shape")
	}

	for _, frame := range foldFrames(42, 80, true) {
		for _, r := range frame {
			if r != '\n' && (r > unicode.MaxASCII || unicode.IsControl(r)) {
				t.Fatalf("ASCII fold contains unsafe rune %U in %q", r, frame)
			}
		}
		if strings.ContainsRune(frame, '\x00') {
			t.Fatalf("ASCII fold contains NUL: %q", frame)
		}
	}
}

func TestFoldArtworkKeepsAStableCenteredCanvas(t *testing.T) {
	t.Parallel()

	for _, ascii := range []bool{false, true} {
		for _, frame := range foldFrames(42, 80, ascii) {
			lines := strings.Split(frame, "\n")
			wantWidth := len([]rune(lines[0]))
			for _, line := range lines[1:] {
				if width := len([]rune(line)); width != wantWidth {
					t.Fatalf("fold row width = %d, want %d in %q", width, wantWidth, frame)
				}
			}
		}
	}
	for _, mark := range []string{normalizeArt(unicodeMark), normalizeArt(asciiMark)} {
		if strings.Contains(strings.ToUpper(mark), "ORIFUDE") {
			t.Fatalf("decorative mark contains the wordmark: %q", mark)
		}
	}
}

func TestClosedFoldCentersItsSpineAndSeal(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		ascii bool
		seal  rune
		spine rune
	}{
		{name: "unicode", seal: '◆', spine: '│'},
		{name: "ascii", ascii: true, seal: '*', spine: '|'},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			frames := foldFrames(42, 80, test.ascii)
			lines := strings.Split(frames[len(frames)-1], "\n")
			width := len([]rune(lines[0]))
			sealColumn := -1
			spineCount := 0
			for _, line := range lines {
				for column, character := range []rune(line) {
					if character == test.seal {
						sealColumn = column
					}
					if character == test.spine {
						spineCount++
						if column != width/2 {
							t.Fatalf("closed fold width=%d spine column=%d, want %d", width, column, width/2)
						}
					}
				}
			}
			if width%2 == 0 || sealColumn != width/2 {
				t.Fatalf("closed fold width=%d seal column=%d, want an odd canvas with seal at %d", width, sealColumn, width/2)
			}
			if spineCount != 2 {
				t.Fatalf("closed fold spine cells=%d, want 2", spineCount)
			}
		})
	}
}

func TestReducedMotionSkipsUnfoldAnimation(t *testing.T) {
	t.Parallel()

	m := New()
	letter := incomingFixture()
	m.setCurrent(letter)
	m.reducedMotion = true
	command := m.startAnimation(ScreenUnfold, true)
	if command != nil {
		t.Fatal("reduced motion scheduled a decorative tick")
	}
	if m.screen != ScreenRead || m.animating {
		t.Fatalf("reduced motion left screen=%v animating=%v", m.screen, m.animating)
	}
}

func TestStaleFoldTickCannotAdvanceCurrentAnimation(t *testing.T) {
	t.Parallel()

	m := New()
	letter := incomingFixture()
	m.setCurrent(letter)
	m.startAnimation(ScreenUnfold, true)
	frame := m.foldFrame
	next, _ := m.Update(foldTickMsg{id: m.animationID - 1})
	m = next.(Model)
	if m.foldFrame != frame {
		t.Fatalf("stale tick changed frame from %d to %d", frame, m.foldFrame)
	}
}
