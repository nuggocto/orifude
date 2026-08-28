package main

import (
	"fmt"
	"os"
	"runtime/debug"

	tea "charm.land/bubbletea/v2"

	"github.com/nuggocto/orifude/internal/api"
	"github.com/nuggocto/orifude/internal/identity"
	"github.com/nuggocto/orifude/internal/tui"
)

const defaultAPIOrigin = "https://api.orifude.com"

var version = "dev"

func main() {
	if os.Getenv("ORIFUDE_OFFLINE_DEMO") == "1" {
		run(tui.New())
		return
	}
	origin := os.Getenv("ORIFUDE_API_URL")
	if origin == "" {
		origin = defaultAPIOrigin
	}
	client, err := api.NewClient(origin, nil)
	if err != nil {
		fatal(err)
	}
	store, err := identity.NewStore()
	if err != nil {
		fatal(err)
	}
	if version == "dev" {
		if info, ok := debug.ReadBuildInfo(); ok && info.Main.Version != "" && info.Main.Version != "(devel)" {
			version = info.Main.Version
		}
	}
	runtime := &tui.Runtime{Client: client, Store: store, Version: version}
	run(tui.NewOnline(runtime))
}

func run(model tui.Model) {
	if _, err := tea.NewProgram(model).Run(); err != nil {
		fatal(err)
	}
}

func fatal(err error) {
	if err != nil {
		fmt.Fprintf(os.Stderr, "orifude: %v\n", err)
		os.Exit(1)
	}
}
