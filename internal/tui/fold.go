package tui

import (
	"crypto/sha256"
	"encoding/binary"
	"fmt"
	"strings"
	"time"

	tea "charm.land/bubbletea/v2"
)

const foldFrameDelay = 95 * time.Millisecond

func foldFrames(seed uint64, width int, ascii bool) []string {
	var input [16]byte
	binary.BigEndian.PutUint64(input[:8], seed)
	binary.BigEndian.PutUint64(input[8:], uint64(max(width, 0)))
	shape := sha256.Sum256(input[:])
	offset := int(shape[0] % 3)

	if ascii {
		return normalizeFrames([]string{
			"+----------------------------+\n|                            |\n|       a quiet letter       |\n|                            |\n+----------------------------+",
			"+----------------------------+\n|\\                          /|\n|  \\                      /  |\n|    \\                  /    |\n+------\\______________/------+",
			fmt.Sprintf("+----------------------------+\n|\\                          /|\n|  \\                      /  |\n|    \\%s*%s/    |\n+------\\______________/------+", spaces(8+offset), spaces(9-offset)),
			"  +------------------------+\n / \\                      / \\\n/   \\____________________/   \\\n\\            *             /\n \\________________________/",
			"       /|\\\n +-----'|'-----+\n/               \\\n\\       *       /\n \\             /\n  \\___________/",
		})
	}

	return normalizeFrames([]string{
		"╭────────────────────────────╮\n│                            │\n│       a quiet letter       │\n│                            │\n╰────────────────────────────╯",
		"╭────────────────────────────╮\n│╲                          ╱│\n│  ╲                      ╱  │\n│    ╲                  ╱    │\n╰──────╲______________╱──────╯",
		fmt.Sprintf("╭────────────────────────────╮\n│╲                          ╱│\n│  ╲                      ╱  │\n│    ╲%s◆%s╱    │\n╰──────╲______________╱──────╯", spaces(8+offset), spaces(9-offset)),
		"  ╭────────────────────────╮\n ╱ ╲                      ╱ ╲\n╱   ╲____________________╱   ╲\n╲            ◆             ╱\n ╲________________________╱",
		"       ╱│╲\n ╭─────╯│╰─────╮\n╱               ╲\n╲       ◆       ╱\n ╲             ╱\n  ╲___________╱",
	})
}

func normalizeFrames(frames []string) []string {
	for index, frame := range frames {
		frames[index] = normalizeArt(frame)
	}
	return frames
}

func normalizeArt(art string) string {
	lines := strings.Split(strings.Trim(art, "\n"), "\n")
	width := 0
	for index, line := range lines {
		lines[index] = strings.TrimSpace(line)
		width = max(width, len([]rune(lines[index])))
	}
	for index, line := range lines {
		lineWidth := len([]rune(line))
		left := (width - lineWidth) / 2
		lines[index] = spaces(left) + line + spaces(width-lineWidth-left)
	}
	return strings.Join(lines, "\n")
}

func spaces(n int) string {
	return strings.Repeat(" ", n)
}

func nextFoldTick(id uint64) tea.Cmd {
	return tea.Tick(foldFrameDelay, func(time.Time) tea.Msg {
		return foldTickMsg{id: id}
	})
}
