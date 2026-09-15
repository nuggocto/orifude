//! Preferences and their database representation.

use rusqlite::{TransactionBehavior, params};

use crate::storage::{Storage, StorageError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMode {
    Auto,
    Color,
    Monochrome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GlyphMode {
    Unicode,
    Ascii,
}

impl GlyphMode {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::Ascii => "ascii",
        }
    }

    fn from_database(value: &str) -> Result<Self, StorageError> {
        match value {
            "unicode" => Ok(Self::Unicode),
            "ascii" => Ok(Self::Ascii),
            _ => Err(StorageError::Corrupt),
        }
    }
}

impl ColorMode {
    const fn database_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Color => "color",
            Self::Monochrome => "monochrome",
        }
    }

    fn from_database(value: &str) -> Result<Self, StorageError> {
        match value {
            "auto" => Ok(Self::Auto),
            "color" => Ok(Self::Color),
            "monochrome" => Ok(Self::Monochrome),
            _ => Err(StorageError::Corrupt),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyBindings {
    pub fold: char,
    pub brush: char,
    pub undo: char,
    pub reset: char,
    pub preview: char,
    pub help: char,
    pub quit: char,
}

impl KeyBindings {
    #[must_use]
    pub fn is_conflict_free(self) -> bool {
        let keys = [
            self.fold,
            self.brush,
            self.undo,
            self.reset,
            self.preview,
            self.help,
            self.quit,
        ];
        keys.iter().enumerate().all(|(index, key)| {
            (key.is_ascii_graphic() || (index == 4 && *key == ' '))
                && !matches!(key, 'h' | 'j' | 'k' | 'l' | 't' | 'v' | 'x')
                && keys.iter().filter(|candidate| *candidate == key).count() == 1
        })
    }

    fn database_values(self) -> [String; 7] {
        [
            self.fold,
            self.brush,
            self.undo,
            self.reset,
            self.preview,
            self.help,
            self.quit,
        ]
        .map(|key| key.to_string())
    }

    fn from_database(values: [&str; 7]) -> Result<Self, StorageError> {
        let mut keys = ['\0'; 7];
        for (index, value) in values.into_iter().enumerate() {
            let mut characters = value.chars();
            keys[index] = characters.next().ok_or(StorageError::Corrupt)?;
            if characters.next().is_some() {
                return Err(StorageError::Corrupt);
            }
        }
        let bindings = Self {
            fold: keys[0],
            brush: keys[1],
            undo: keys[2],
            reset: keys[3],
            preview: keys[4],
            help: keys[5],
            quit: keys[6],
        };
        bindings
            .is_conflict_free()
            .then_some(bindings)
            .ok_or(StorageError::Corrupt)
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            fold: 'f',
            brush: 'b',
            undo: 'u',
            reset: 'r',
            preview: ' ',
            help: '?',
            quit: 'q',
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Settings {
    pub color_mode: ColorMode,
    pub glyph_mode: GlyphMode,
    pub reduced_motion: bool,
    pub instant_reveal: bool,
    pub lesson_complete: bool,
    pub bindings: KeyBindings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::Auto,
            glyph_mode: GlyphMode::Unicode,
            reduced_motion: false,
            instant_reveal: false,
            lesson_complete: false,
            bindings: KeyBindings::default(),
        }
    }
}

struct StoredSettings {
    color_mode: String,
    glyph_mode: String,
    reduced_motion: bool,
    instant_reveal: bool,
    lesson_complete: bool,
    bindings: [String; 7],
}

impl Storage {
    /// Reads rendering-independent user preferences.
    ///
    /// # Errors
    ///
    /// Returns when persisted settings are corrupt or SQLite cannot read them.
    pub fn settings(&self) -> Result<Settings, StorageError> {
        let stored = self.connection.query_row(
            "SELECT color_mode, glyph_mode, reduced_motion, instant_reveal,
                    lesson_complete, bind_fold, bind_brush, bind_undo, bind_reset,
                    bind_preview, bind_help, bind_quit
             FROM settings WHERE singleton = 1",
            [],
            |row| {
                Ok(StoredSettings {
                    color_mode: row.get("color_mode")?,
                    glyph_mode: row.get("glyph_mode")?,
                    reduced_motion: row.get("reduced_motion")?,
                    instant_reveal: row.get("instant_reveal")?,
                    lesson_complete: row.get("lesson_complete")?,
                    bindings: [
                        row.get("bind_fold")?,
                        row.get("bind_brush")?,
                        row.get("bind_undo")?,
                        row.get("bind_reset")?,
                        row.get("bind_preview")?,
                        row.get("bind_help")?,
                        row.get("bind_quit")?,
                    ],
                })
            },
        )?;
        Ok(Settings {
            color_mode: ColorMode::from_database(&stored.color_mode)?,
            glyph_mode: GlyphMode::from_database(&stored.glyph_mode)?,
            reduced_motion: stored.reduced_motion,
            instant_reveal: stored.instant_reveal,
            lesson_complete: stored.lesson_complete,
            bindings: KeyBindings::from_database(stored.bindings.each_ref().map(String::as_str))?,
        })
    }

    /// Durably writes preferences without depending on terminal UI types.
    ///
    /// # Errors
    ///
    /// Returns a typed database or filesystem error with prior settings intact.
    pub fn save_settings(&mut self, settings: Settings) -> Result<(), StorageError> {
        if !settings.bindings.is_conflict_free() {
            return Err(StorageError::InvalidSettings);
        }
        self.verify_footprint()?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let color = settings.color_mode.database_value();
        let glyphs = settings.glyph_mode.database_value();
        let bindings = settings.bindings.database_values();
        let changed = transaction.execute(
            "UPDATE settings
             SET color_mode = ?1, glyph_mode = ?2, reduced_motion = ?3,
                 instant_reveal = ?4, lesson_complete = ?5, bind_fold = ?6,
                 bind_brush = ?7, bind_undo = ?8, bind_reset = ?9,
                 bind_preview = ?10, bind_help = ?11, bind_quit = ?12
             WHERE singleton = 1",
            params![
                color,
                glyphs,
                settings.reduced_motion,
                settings.instant_reveal,
                settings.lesson_complete,
                bindings[0],
                bindings[1],
                bindings[2],
                bindings[3],
                bindings[4],
                bindings[5],
                bindings[6],
            ],
        )?;
        if changed != 1 {
            return Err(StorageError::Corrupt);
        }
        transaction.commit().map_err(StorageError::from)
    }
}
