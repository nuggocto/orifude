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
	appVersion := resolvedVersion(version)
	if len(os.Args) == 2 && (os.Args[1] == "--version" || os.Args[1] == "version") {
		fmt.Printf("orifude %s\n", appVersion)
		return
	}
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
	runtime := &tui.Runtime{Client: client, Store: store, Version: appVersion}
	run(tui.NewOnline(runtime))
}

func resolvedVersion(linked string) string {
	if linked != "dev" {
		return linked
	}
	if info, ok := debug.ReadBuildInfo(); ok && info.Main.Version != "" && info.Main.Version != "(devel)" {
		return info.Main.Version
	}
	return linked
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
