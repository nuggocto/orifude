package tui

import (
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"
)

// Styles contains the semantic styles used by every layout.
type Styles struct {
	Title     lipgloss.Style
	Body      lipgloss.Style
	Label     lipgloss.Style
	Muted     lipgloss.Style
	Active    lipgloss.Style
	Danger    lipgloss.Style
	Panel     lipgloss.Style
	Help      lipgloss.Style
	Counter   lipgloss.Style
	Selection lipgloss.Style
	Paper     lipgloss.Style
	PaperText lipgloss.Style
	PaperMute lipgloss.Style
	Art       lipgloss.Style
}

var asciiBorder = lipgloss.Border{
	Top: "-", Bottom: "-", Left: "|", Right: "|",
	TopLeft: "+", TopRight: "+", BottomLeft: "+", BottomRight: "+",
}

func newStyles(dark bool, profile colorprofile.Profile, theme string, ascii bool) Styles {
	switch theme {
	case "light":
		dark = false
	case "dark":
		dark = true
	}

	plain := theme == "mono" || profile == colorprofile.Ascii || profile == colorprofile.NoTTY
	base := lipgloss.NewStyle()
	if plain {
		return Styles{
			Title:     base.Bold(true),
			Body:      base,
			Label:     base.Bold(true),
			Muted:     base,
			Active:    base.Bold(true).Underline(true),
			Danger:    base.Bold(true),
			Panel:     base.BorderStyle(asciiBorder).BorderLeft(true).Padding(0, 2),
			Help:      base,
			Counter:   base,
			Selection: base.Reverse(true),
			Paper:     base.Border(asciiBorder).Padding(0, 1),
			PaperText: base,
			PaperMute: base,
			Art:       base,
		}
	}

	pick := lipgloss.LightDark(dark)
	ink := pick(lipgloss.Color("#292823"), lipgloss.Color("#E9E3D8"))
	moss := pick(lipgloss.Color("#59634B"), lipgloss.Color("#B7C39B"))
	clay := pick(lipgloss.Color("#795F43"), lipgloss.Color("#D3B27E"))
	branch := pick(lipgloss.Color("#706659"), lipgloss.Color("#948778"))
	ash := pick(lipgloss.Color("#6D665D"), lipgloss.Color("#A59C90"))
	ember := pick(lipgloss.Color("#934C45"), lipgloss.Color("#E68176"))
	selection := pick(lipgloss.Color("#DED2BF"), lipgloss.Color("#514A3F"))
	panel := base.Foreground(ink)
	paperBorder := lipgloss.NormalBorder()
	panelBorder := lipgloss.NormalBorder()
	if ascii {
		paperBorder = asciiBorder
		panelBorder = asciiBorder
	}
	return Styles{
		Title:     panel.Foreground(clay).Bold(true),
		Body:      panel.Foreground(ink),
		Label:     panel.Foreground(clay).Bold(true),
		Muted:     panel.Foreground(ash),
		Active:    panel.Foreground(moss).Bold(true),
		Danger:    panel.Foreground(ember).Bold(true),
		Panel:     panel.BorderStyle(panelBorder).BorderLeft(true).BorderForeground(branch).Padding(0, 2),
		Help:      panel.Foreground(ash),
		Counter:   panel.Foreground(ash),
		Selection: base.Foreground(ink).Background(selection),
		Paper:     base.Foreground(ink).Border(paperBorder).BorderForeground(branch).Padding(0, 1),
		PaperText: base.Foreground(ink),
		PaperMute: base.Foreground(ash),
		Art:       base.Foreground(ash),
	}
}
