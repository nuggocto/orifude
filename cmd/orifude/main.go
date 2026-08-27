package main

import (
	"fmt"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/nuggocto/orifude/internal/tui"
)

func main() {
	if _, err := tea.NewProgram(tui.New()).Run(); err != nil {
		fmt.Fprintf(os.Stderr, "orifude: %v\n", err)
		os.Exit(1)
	}
}
