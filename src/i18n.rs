//! Tiny two-language text layer (English / German).
//!
//! The active language lives in a process-wide atomic so that every module
//! (UI, audio thread, project I/O) can look up texts without passing state
//! around. Texts are written inline next to each other, English first:
//!
//! ```ignore
//! ui.label(tr("Tempo", "Tempo"));
//! let msg = trf!("Saved: {}", "Gespeichert: {}", path.display());
//! ```

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Lang {
    En,
    De,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];

    /// Name of the language in that language (for the language picker).
    pub fn label(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
        }
    }

    /// Parses a language code such as `en` or `de` (used by `--lang`).
    pub fn from_code(code: &str) -> Option<Lang> {
        match code.to_ascii_lowercase().as_str() {
            "en" | "english" => Some(Lang::En),
            "de" | "german" | "deutsch" => Some(Lang::De),
            _ => None,
        }
    }

    /// Language of the Windows user interface; anything but German maps to English.
    pub fn system_default() -> Lang {
        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            extern "system" {
                fn GetUserDefaultUILanguage() -> u16;
            }
            const LANG_GERMAN: u16 = 0x07;
            // SAFETY: plain Win32 call without arguments or preconditions.
            let id = unsafe { GetUserDefaultUILanguage() };
            // The low 10 bits hold the primary language identifier.
            if id & 0x3ff == LANG_GERMAN {
                return Lang::De;
            }
        }
        Lang::En
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(lang: Lang) {
    CURRENT.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == Lang::De as u8 {
        Lang::De
    } else {
        Lang::En
    }
}

/// Returns the text for the active language.
pub fn tr<'a>(en: &'a str, de: &'a str) -> &'a str {
    match lang() {
        Lang::En => en,
        Lang::De => de,
    }
}

/// A static text in both languages, for names stored in constant tables
/// (instruments, presets, beats, ...).
#[derive(Clone, Copy, Debug)]
pub struct Text {
    pub en: &'static str,
    pub de: &'static str,
}

impl Text {
    pub const fn new(en: &'static str, de: &'static str) -> Self {
        Self { en, de }
    }

    /// The text in the active language.
    pub fn get(&self) -> &'static str {
        tr(self.en, self.de)
    }
}

/// `format!` in the active language: `trf!("English {}", "Deutsch {}", args...)`.
/// Use positional `{}` placeholders; both templates take the same arguments.
#[macro_export]
macro_rules! trf {
    ($en:literal, $de:literal $(, $arg:expr)* $(,)?) => {
        match $crate::i18n::lang() {
            $crate::i18n::Lang::En => format!($en $(, $arg)*),
            $crate::i18n::Lang::De => format!($de $(, $arg)*),
        }
    };
}
