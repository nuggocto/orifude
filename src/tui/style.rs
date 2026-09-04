use std::env;
use std::ffi::{OsStr, OsString};

use ratatui::style::{Color, Modifier, Style};

use crate::storage::{ColorMode, GlyphMode, Settings};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorCapability {
    TrueColor,
    Ansi256,
    Ansi16,
    Monochrome,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TerminalEnvironment {
    no_color: bool,
    term: Option<OsString>,
    color_term: Option<OsString>,
    term_program: Option<OsString>,
    windows_terminal: bool,
}

impl TerminalEnvironment {
    #[must_use]
    pub fn capture() -> Self {
        Self {
            no_color: env::var_os("NO_COLOR").is_some(),
            term: env::var_os("TERM"),
            color_term: env::var_os("COLORTERM"),
            term_program: env::var_os("TERM_PROGRAM"),
            windows_terminal: env::var_os("WT_SESSION").is_some(),
        }
    }

    #[must_use]
    pub const fn color_disabled(&self) -> bool {
        self.no_color
    }

    #[cfg(test)]
    fn with_values(
        no_color: bool,
        term: Option<&str>,
        color_term: Option<&str>,
        term_program: Option<&str>,
        windows_terminal: bool,
    ) -> Self {
        Self {
            no_color,
            term: term.map(OsString::from),
            color_term: color_term.map(OsString::from),
            term_program: term_program.map(OsString::from),
            windows_terminal,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StyleProfile {
    capability: ColorCapability,
    glyph_mode: GlyphMode,
}

impl StyleProfile {
    #[must_use]
    pub const fn new(capability: ColorCapability, glyph_mode: GlyphMode) -> Self {
        Self {
            capability,
            glyph_mode,
        }
    }

    #[must_use]
    pub fn resolve(settings: Settings, detected: ColorCapability, color_disabled: bool) -> Self {
        let capability = match settings.color_mode {
            ColorMode::Monochrome => ColorCapability::Monochrome,
            ColorMode::Auto if color_disabled => ColorCapability::Monochrome,
            ColorMode::Auto | ColorMode::Color => detected,
        };
        Self::new(capability, settings.glyph_mode)
    }

    #[must_use]
    #[cfg(test)]
    pub const fn capability(self) -> ColorCapability {
        self.capability
    }

    #[must_use]
    pub const fn glyph_mode(self) -> GlyphMode {
        self.glyph_mode
    }

    #[must_use]
    pub fn title(self) -> Style {
        self.foreground(PaletteColor::Ink)
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn highlighted_title() -> Style {
        Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED)
    }

    #[must_use]
    pub fn border(self) -> Style {
        self.foreground(PaletteColor::Branch)
    }

    #[must_use]
    pub fn active(self) -> Style {
        self.foreground(PaletteColor::Moss)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    }

    #[must_use]
    pub fn muted() -> Style {
        Style::default().add_modifier(Modifier::DIM)
    }

    #[must_use]
    pub fn error(self) -> Style {
        self.foreground(PaletteColor::Ember)
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn paper(self) -> Style {
        self.foreground(PaletteColor::Clay)
    }

    #[must_use]
    pub fn ink_mark(self) -> Style {
        self.foreground(PaletteColor::Clay)
            .add_modifier(Modifier::BOLD)
    }

    #[must_use]
    pub fn ink(self) -> Style {
        self.foreground(PaletteColor::Ink)
    }

    fn foreground(self, color: PaletteColor) -> Style {
        match map_color(self.capability, color) {
            Some(foreground) => Style::default().fg(foreground),
            None => Style::default(),
        }
    }
}

#[derive(Clone, Copy)]
enum PaletteColor {
    Ink,
    Moss,
    Clay,
    Branch,
    Ember,
}

#[must_use]
pub fn detect_color(environment: &TerminalEnvironment) -> ColorCapability {
    if value_is(environment.term.as_deref(), b"dumb") {
        return ColorCapability::Monochrome;
    }
    if value_contains(environment.color_term.as_deref(), b"truecolor")
        || value_contains(environment.color_term.as_deref(), b"24bit")
        || value_contains(environment.term.as_deref(), b"truecolor")
        || value_contains(environment.term.as_deref(), b"direct")
        || environment.windows_terminal
        || value_contains(environment.term_program.as_deref(), b"wezterm")
        || value_contains(environment.term_program.as_deref(), b"ghostty")
    {
        return ColorCapability::TrueColor;
    }
    if value_contains(environment.term.as_deref(), b"256color") {
        return ColorCapability::Ansi256;
    }
    ColorCapability::Ansi16
}

fn value_is(value: Option<&OsStr>, expected: &[u8]) -> bool {
    value.is_some_and(|value| value.as_encoded_bytes().eq_ignore_ascii_case(expected))
}

fn value_contains(value: Option<&OsStr>, needle: &[u8]) -> bool {
    let Some(value) = value else {
        return false;
    };
    value
        .as_encoded_bytes()
        .windows(needle.len())
        .any(|part| part.eq_ignore_ascii_case(needle))
}

fn map_color(capability: ColorCapability, color: PaletteColor) -> Option<Color> {
    match capability {
        ColorCapability::TrueColor => Some(match color {
            PaletteColor::Ink | PaletteColor::Branch => Color::Reset,
            PaletteColor::Moss => Color::Rgb(0x85, 0x8a, 0x72),
            PaletteColor::Clay => Color::Rgb(0xa4, 0x8b, 0x68),
            PaletteColor::Ember => Color::Rgb(0xa4, 0x5b, 0x52),
        }),
        ColorCapability::Ansi256 => Some(match color {
            PaletteColor::Ink | PaletteColor::Branch => Color::Reset,
            PaletteColor::Moss => Color::Indexed(101),
            PaletteColor::Clay => Color::Indexed(137),
            PaletteColor::Ember => Color::Indexed(131),
        }),
        ColorCapability::Ansi16 => Some(match color {
            PaletteColor::Ink | PaletteColor::Branch => Color::Reset,
            PaletteColor::Moss => Color::Green,
            PaletteColor::Clay => Color::Yellow,
            PaletteColor::Ember => Color::Red,
        }),
        ColorCapability::Monochrome => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_detection_is_conservative_and_honors_no_color() {
        let true_color = TerminalEnvironment::with_values(
            false,
            Some("xterm-256color"),
            Some("truecolor"),
            None,
            false,
        );
        assert_eq!(detect_color(&true_color), ColorCapability::TrueColor);

        let indexed =
            TerminalEnvironment::with_values(false, Some("screen-256color"), None, None, false);
        assert_eq!(detect_color(&indexed), ColorCapability::Ansi256);

        let plain = TerminalEnvironment::with_values(false, Some("xterm"), None, None, false);
        assert_eq!(detect_color(&plain), ColorCapability::Ansi16);

        let disabled = TerminalEnvironment::with_values(
            true,
            Some("xterm-256color"),
            Some("truecolor"),
            None,
            true,
        );
        assert_eq!(detect_color(&disabled), ColorCapability::TrueColor);
        assert_eq!(
            StyleProfile::resolve(Settings::default(), detect_color(&disabled), true).capability(),
            ColorCapability::Monochrome
        );
    }

    #[test]
    fn explicit_preferences_respect_the_detected_capability_ceiling() {
        let settings = Settings {
            color_mode: ColorMode::Color,
            ..Settings::default()
        };
        assert_eq!(
            StyleProfile::resolve(settings, ColorCapability::Monochrome, true).capability(),
            ColorCapability::Monochrome
        );
        assert_eq!(
            StyleProfile::resolve(settings, ColorCapability::TrueColor, true).capability(),
            ColorCapability::TrueColor
        );

        let settings = Settings {
            color_mode: ColorMode::Monochrome,
            ..Settings::default()
        };
        assert_eq!(
            StyleProfile::resolve(settings, ColorCapability::TrueColor, false).capability(),
            ColorCapability::Monochrome
        );
    }
}
