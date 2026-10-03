use function_config::{AccentColor, AppConfig, ThemeStyle};
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
        let is_light = config.theme_style == ThemeStyle::StudioLight;
        let accent = match config.accent_color {
            AccentColor::White => {
                if is_light {
                    rgb(0x14120a)
                } else {
                    rgb(0xf1f0ef)
                }
            }
            AccentColor::Cyan => if is_light { rgb(0x007aff) } else { rgb(0x64d2ff) },
            AccentColor::Emerald => if is_light { rgb(0x16803c) } else { rgb(0x30d158) },
            AccentColor::Violet => if is_light { rgb(0x7a3db8) } else { rgb(0xbf8cff) },
            AccentColor::Amber => if is_light { rgb(0xb05a00) } else { rgb(0xffb340) },
        };

        match config.theme_style {
            ThemeStyle::CarbonDark | ThemeStyle::ObsidianOled => Self {
                mode: ThemeMode::Dark,
                surface_base: rgb(0x14120a), // Function Dark (#14120A)
                surface_elevated: rgb(0x1c1a12), // Warm elevated
                surface_floating: rgb(0x222017),
                surface_active: rgb(0x28251d), // Warm active
                surface_input: rgb(0x1c1a12),  // Warm input
                border_subtle: rgb(0x2e2a20),  // Restrained hairline
                border_focus: accent,
                text_primary: rgb(0xf1f0ef),   // Function Light (#F1F0EF)
                text_secondary: rgb(0xa8a49c), // Warm gray
                text_muted: rgb(0x6e6a62),     // Warm smoke
                accent_primary: accent,
                accent_hover: rgb(0xd1cdc7),
                ..Self::dark()
            },
            ThemeStyle::SlateMidnight => Self {
                mode: ThemeMode::Dark,
                surface_base: rgb(0x14120a),
                surface_elevated: rgb(0x18160e),
                surface_floating: rgb(0x1f1d14),
                surface_active: rgb(0x252319),
                surface_input: rgb(0x18160e),
                border_subtle: rgb(0x2c281e),
                border_focus: accent,
                text_primary: rgb(0xf1f0ef),
                text_secondary: rgb(0xa8a49c),
                text_muted: rgb(0x6e6a62),
                accent_primary: accent,
                accent_hover: rgb(0xd1cdc7),
                ..Self::dark()
            },
            ThemeStyle::StudioLight => Self {
                mode: ThemeMode::Light,
                surface_base: rgb(0xf7f6f4),
                surface_elevated: rgb(0xffffff),
                surface_floating: rgb(0xffffff),
                surface_active: rgb(0xeeebe6),
                surface_input: rgb(0xeeebe6),
                border_subtle: rgb(0xdad6cf),
                border_focus: rgb(0x14120a),
                text_primary: rgb(0x14120a),
                text_secondary: rgb(0x6e6a62),
                text_muted: rgb(0x948f86),
                accent_primary: rgb(0x14120a),
                accent_hover: rgb(0x2e2a20),
                ..Self::light()
            },
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: ThemeMode::Dark,

            // Function Warm Black (#14120A)
            surface_base: rgb(0x14120a),
            surface_elevated: rgb(0x1c1a12),
            surface_floating: rgb(0x222017),
            surface_active: rgb(0x28251d),
            surface_input: rgb(0x1c1a12),

            // Subtle warm hairline
            border_subtle: rgb(0x2e2a20),
            border_focus: rgb(0xf1f0ef),

            // Function Off-White (#F1F0EF)
            text_primary: rgb(0xf1f0ef),
            text_secondary: rgb(0xa8a49c),
            text_muted: rgb(0x6e6a62),

            // Semantic status colors (warm, technical, restrained - no neon)
            status_idle: rgb(0x6e6a62),
            status_listening: rgb(0xf1f0ef),
            status_processing: rgb(0xd1cdc7),
            status_acting: rgb(0xf1f0ef),
            status_success: rgb(0xe2dfd8),
            status_error: rgb(0xd4756f),

            accent_primary: rgb(0xf1f0ef),
            accent_hover: rgb(0xd1cdc7),
        }
    }

    pub fn light() -> Self {
        Self {
            mode: ThemeMode::Light,

            surface_base: rgb(0xf7f6f4),
            surface_elevated: rgb(0xffffff),
            surface_floating: rgb(0xffffff),
            surface_active: rgb(0xeeebe6),
            surface_input: rgb(0xeeebe6),

            border_subtle: rgb(0xdad6cf),
            border_focus: rgb(0x14120a),

            text_primary: rgb(0x14120a),
            text_secondary: rgb(0x6e6a62),
            text_muted: rgb(0x948f86),

            status_idle: rgb(0x948f86),
            status_listening: rgb(0x14120a),
            status_processing: rgb(0x3e3a30),
            status_acting: rgb(0x14120a),
            status_success: rgb(0x2e2a20),
            status_error: rgb(0xb84a44),

            accent_primary: rgb(0x14120a),
            accent_hover: rgb(0x2e2a20),
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
