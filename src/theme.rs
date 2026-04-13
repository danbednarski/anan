//! Custom theme for a warm, professional genealogy app.
//!
//! Palette inspired by archival/heritage aesthetics: deep teal primary,
//! warm amber accents, soft off-white background, clear typography
//! hierarchy. Every color was chosen for WCAG AA contrast against its
//! expected background.

use iced::{Color, Theme};
use iced::theme::Palette;

/// Deep teal - the primary brand color. Used for the home card,
/// selected states, and primary actions.
pub const PRIMARY: Color = Color::from_rgb(0.106, 0.286, 0.396);

/// Warm amber - accent color for highlights and secondary actions.
pub const ACCENT: Color = Color::from_rgb(0.737, 0.424, 0.145);

/// Near-black for primary text. High contrast on light backgrounds.
pub const TEXT: Color = Color::from_rgb(0.176, 0.204, 0.212);

/// Muted gray for secondary labels, timestamps, IDs.
pub const TEXT_MUTED: Color = Color::from_rgb(0.388, 0.431, 0.447);

/// Warm off-white background. Easier on the eyes than pure white.
pub const BG: Color = Color::from_rgb(0.980, 0.976, 0.961);

/// Card surface - slightly brighter than background.
pub const CARD: Color = Color::from_rgb(1.0, 1.0, 1.0);

/// Subtle border for cards and separators.
pub const BORDER: Color = Color::from_rgb(0.875, 0.902, 0.914);

/// Danger / delete confirmation.
pub const DANGER: Color = Color::from_rgb(0.839, 0.188, 0.192);

/// Success / confirmation.
pub const SUCCESS: Color = Color::from_rgb(0.0, 0.722, 0.580);

/// Home card background - lighter tint of primary.
pub const HOME_BG: Color = Color::from_rgb(0.169, 0.369, 0.486);

/// Home card hover.
pub const HOME_HOVER: Color = Color::from_rgb(0.129, 0.329, 0.446);

/// Ancestor card background - very light warm tone.
pub const ANCESTOR_BG: Color = Color::from_rgb(0.949, 0.941, 0.925);

/// Ancestor card hover.
pub const ANCESTOR_HOVER: Color = Color::from_rgb(0.922, 0.910, 0.890);

/// Connector line color.
pub const CONNECTOR: Color = Color::from_rgb(0.780, 0.808, 0.820);

/// Context menu background.
pub const MENU_BG: Color = Color::from_rgb(0.988, 0.988, 0.980);

/// Build the custom iced theme.
pub fn gramps_theme() -> Theme {
    Theme::custom(
        "Gramps".to_string(),
        Palette {
            background: BG,
            text: TEXT,
            primary: PRIMARY,
            success: SUCCESS,
            danger: DANGER,
        },
    )
}
