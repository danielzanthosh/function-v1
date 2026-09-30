use assistant_config::{AccentColor, AppConfig, ThemeStyle};
use gpui::{rgb, Rgba};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub mode: ThemeMode,

    // Core Surfaces
    pub surface_base: Rgba,
    pub surface_elevated: Rgba,
    pub surface_floating: Rgba,
    pub surface_active: Rgba,
    pub surface_input: Rgba,

    // Borders
    pub border_subtle: Rgba,
    pub border_focus: Rgba,

    // Text & Content Hierarchy
    pub text_primary: Rgba,
    pub text_secondary: Rgba,
    pub text_muted: Rgba,

    // Semantic States
    pub status_idle: Rgba,
    pub status_listening: Rgba,
    pub status_processing: Rgba,
    pub status_acting: Rgba,
    pub status_success: Rgba,
    pub status_error: Rgba,

    // Active Accents
    pub accent_primary: Rgba,
    pub accent_hover: Rgba,
}

impl Theme {
    pub fn from_config(config: &AppConfig) -> Self {
        let accent = match config.accent_color {
            AccentColor::White => rgb(0xffffff),
            AccentColor::Cyan => rgb(0x0ea5e9),
            AccentColor::Emerald => rgb(0x10b981),
            AccentColor::Violet => rgb(0x8b5cf6),
            AccentColor::Amber => rgb(0xf59e0b),
        };

        match config.theme_style {
            ThemeStyle::CarbonDark => Self {
                mode: ThemeMode::Dark,
                surface_base: rgb(0x000000),     // Void (#000000)
                surface_elevated: rgb(0x0a0a0a), // Carbon (#0A0A0A)
                surface_floating: rgb(0x0f0f0f),
                surface_active: rgb(0x181818),   // Graphite elevated
                surface_input: rgb(0x141414),    // Graphite (#141414)
                border_subtle: rgb(0x242424),    // Ash (#2A2A2A)
                border_focus: accent,
                text_primary: rgb(0xffffff),     // White (#FFFFFF)
                text_secondary: rgb(0xa3a3a3),   // Silver (#A3A3A3)
                text_muted: rgb(0x666666),       // Smoke (#666666)
                accent_primary: accent,
                accent_hover: rgb(0xd4d4d8),
                ..Self::dark()
            },
            ThemeStyle::ObsidianOled => Self {
                mode: ThemeMode::Dark,
                surface_base: rgb(0x000000),
                surface_elevated: rgb(0x050505),
                surface_floating: rgb(0x09090b),
                surface_active: rgb(0x121212),
                surface_input: rgb(0x0a0a0a),
                border_subtle: rgb(0x1c1c1f),
                border_focus: accent,
                text_primary: rgb(0xffffff),
                text_secondary: rgb(0xa1a1aa),
                text_muted: rgb(0x52525b),
                accent_primary: accent,
                accent_hover: rgb(0xd4d4d8),
                ..Self::dark()
            },
            ThemeStyle::SlateMidnight => Self {
                mode: ThemeMode::Dark,
                surface_base: rgb(0x080c14),
                surface_elevated: rgb(0x0f172a),
                surface_floating: rgb(0x131d34),
                surface_active: rgb(0x1e293b),
                surface_input: rgb(0x17223b),
                border_subtle: rgb(0x293548),
                border_focus: accent,
                text_primary: rgb(0xf8fafc),
                text_secondary: rgb(0x94a3b8),
                text_muted: rgb(0x64748b),
                accent_primary: accent,
                accent_hover: rgb(0xcfd8e3),
                ..Self::dark()
            },
            ThemeStyle::StudioLight => Self {
                mode: ThemeMode::Light,
                surface_base: rgb(0xf7f7f5),
                surface_elevated: rgb(0xffffff),
                surface_floating: rgb(0xffffff),
                surface_active: rgb(0xf0f0ee),
                surface_input: rgb(0xf0f0ee),
                border_subtle: rgb(0xd8d8d5),
                border_focus: accent,
                text_primary: rgb(0x111111),
                text_secondary: rgb(0x666666),
                text_muted: rgb(0x888888),
                accent_primary: accent,
                accent_hover: rgb(0x333333),
                ..Self::light()
            },
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,

            // Void (#000000)
            surface_base: rgb(0x000000),
            // Carbon (#0A0A0A)
            surface_elevated: rgb(0x0a0a0a),
            surface_floating: rgb(0x0a0a0a),
            // Graphite (#141414)
            surface_active: rgb(0x141414),
            surface_input: rgb(0x141414),

            // Ash (#2A2A2A)
            border_subtle: rgb(0x2a2a2a),
            // White focus border
            border_focus: rgb(0xffffff),

            // White (#FFFFFF)
            text_primary: rgb(0xffffff),
            // Silver (#A3A3A3)
            text_secondary: rgb(0xa3a3a3),
            // Smoke (#666666)
            text_muted: rgb(0x666666),

            // Semantic status colors
            status_idle: rgb(0x666666),
            status_listening: rgb(0x60a5fa), // Information
            status_processing: rgb(0xfacc15), // Warning
            status_acting: rgb(0xffffff),     // White convergence
            status_success: rgb(0x4ade80),    // Success
            status_error: rgb(0xf87171),      // Error

            accent_primary: rgb(0xffffff),
            accent_hover: rgb(0xa3a3a3),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,

            // Primary background (#F7F7F5)
            surface_base: rgb(0xf7f7f5),
            // Primary surface (#FFFFFF)
            surface_elevated: rgb(0xffffff),
            surface_floating: rgb(0xffffff),
            // Secondary surface (#F0F0EE)
            surface_active: rgb(0xf0f0ee),
            surface_input: rgb(0xf0f0ee),

            // Border (#D8D8D5)
            border_subtle: rgb(0xd8d8d5),
            border_focus: rgb(0x111111),

            // Primary text (#111111)
            text_primary: rgb(0x111111),
            // Secondary text (#666666)
            text_secondary: rgb(0x666666),
            // Smoke muted
            text_muted: rgb(0x888888),

            // Semantic status colors
            status_idle: rgb(0x888888),
            status_listening: rgb(0x2563eb),
            status_processing: rgb(0xd97706),
            status_acting: rgb(0x111111),
            status_success: rgb(0x16a34a),
            status_error: rgb(0xdc2626),

            accent_primary: rgb(0x111111),
            accent_hover: rgb(0x333333),
        }
    }

    pub fn toggle(&self) -> Self {
        match self.mode {
            ThemeMode::Dark => Self::light(),
            ThemeMode::Light => Self::dark(),
        }
    }

    pub fn is_dark(&self) -> bool {
        self.mode == ThemeMode::Dark
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}
