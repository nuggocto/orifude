use std::fmt::{self, Write};

use crate::storage::GlyphMode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeText(Box<str>);

impl SafeText {
    #[must_use]
    #[cfg(test)]
    pub fn external(value: &str, max_scalars: usize, glyph_mode: GlyphMode) -> Self {
        Self::external_display(&value, max_scalars, glyph_mode)
    }

    #[must_use]
    pub fn external_display(
        value: &dyn fmt::Display,
        max_scalars: usize,
        glyph_mode: GlyphMode,
    ) -> Self {
        let mut writer = BoundedText::new(max_scalars, glyph_mode);
        let _bounded_result = write!(&mut writer, "{value}");
        writer.finish()
    }

    #[must_use]
    pub fn internal(value: &'static str) -> Self {
        debug_assert!(!value.chars().any(char::is_control));
        Self(Box::from(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

struct BoundedText {
    output: String,
    max_scalars: usize,
    scalars: usize,
    glyph_mode: GlyphMode,
    truncated: bool,
}

impl BoundedText {
    fn new(max_scalars: usize, glyph_mode: GlyphMode) -> Self {
        Self {
            output: String::with_capacity(max_scalars),
            max_scalars,
            scalars: 0,
            glyph_mode,
            truncated: false,
        }
    }

    fn finish(mut self) -> SafeText {
        if self.truncated && self.max_scalars > 0 {
            self.output.pop();
            self.output.push(match self.glyph_mode {
                GlyphMode::Unicode => '…',
                GlyphMode::Ascii => '~',
            });
        }
        SafeText(self.output.into_boxed_str())
    }
}

impl Write for BoundedText {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for character in value.chars() {
            if self.scalars == self.max_scalars {
                self.truncated = true;
                return Err(fmt::Error);
            }
            self.output.push(safe_character(character, self.glyph_mode));
            self.scalars += 1;
        }
        Ok(())
    }
}

fn safe_character(character: char, glyph_mode: GlyphMode) -> char {
    if character.is_control() {
        return match glyph_mode {
            GlyphMode::Unicode => '�',
            GlyphMode::Ascii => '?',
        };
    }
    match glyph_mode {
        GlyphMode::Unicode => character,
        GlyphMode::Ascii if character.is_ascii() => character,
        GlyphMode::Ascii => '?',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_text_cannot_emit_controls_or_exceed_its_scalar_limit() {
        let safe = SafeText::external("ink\u{1b}[31m\nbranch", 10, GlyphMode::Unicode);
        assert_eq!(safe.as_str().chars().count(), 10);
        assert!(!safe.as_str().chars().any(char::is_control));
        assert!(!safe.as_str().contains('\u{1b}'));
    }

    #[test]
    fn ascii_mode_replaces_unicode_and_marks_truncation() {
        let safe = SafeText::external("paper 枝 weather", 8, GlyphMode::Ascii);
        assert_eq!(safe.as_str(), "paper ?~");
        assert!(safe.as_str().is_ascii());
    }

    #[test]
    fn display_formatting_stops_at_the_render_bound() {
        struct ManyPieces;

        impl std::fmt::Display for ManyPieces {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                for _ in 0..1_000 {
                    formatter.write_str("branch")?;
                }
                Ok(())
            }
        }

        let safe = SafeText::external_display(&ManyPieces, 12, GlyphMode::Ascii);
        assert_eq!(safe.as_str(), "branchbranc~");
    }
}
