use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub banner: Color,
    pub highlight: Color,
    pub foreground: Color,
    pub hotkey: Color,
    pub active: Color,
    pub muted: Color,
}

impl Theme {
    pub const SUNSET: Self = Self {
        background: Color::Rgb(0x18, 0x17, 0x1C),
        banner: Color::Rgb(0xD2, 0xD7, 0x00),
        highlight: Color::Rgb(0xFF, 0x6F, 0x3C),
        foreground: Color::Rgb(0xF4, 0xEF, 0xE6),
        hotkey: Color::Rgb(0xFF, 0x7A, 0x45),
        active: Color::Rgb(0xFF, 0xA7, 0x70),
        muted: Color::Rgb(0x8C, 0x87, 0x92),
    };

    pub const MIDNIGHT: Self = Self {
        background: Color::Rgb(0x11, 0x18, 0x27),
        banner: Color::Rgb(0x67, 0xE8, 0xF9),
        highlight: Color::Rgb(0x60, 0xA5, 0xFA),
        foreground: Color::Rgb(0xE5, 0xE7, 0xEB),
        hotkey: Color::Rgb(0xF4, 0x72, 0xB6),
        active: Color::Rgb(0xC0, 0x84, 0xFC),
        muted: Color::Rgb(0x6B, 0x72, 0x80),
    };

    pub const MONO: Self = Self {
        background: Color::Rgb(0x12, 0x12, 0x12),
        banner: Color::Rgb(0xFA, 0xFA, 0xFA),
        highlight: Color::Rgb(0xD4, 0xD4, 0xD4),
        foreground: Color::Rgb(0xE5, 0xE5, 0xE5),
        hotkey: Color::Rgb(0xFF, 0xFF, 0xFF),
        active: Color::Rgb(0xA3, 0xA3, 0xA3),
        muted: Color::Rgb(0x73, 0x73, 0x73),
    };

    pub const CANDY: Self = Self {
        background: Color::Rgb(0xFF, 0xFF, 0xFF),
        banner: Color::Rgb(0xFF, 0x20, 0x58),
        highlight: Color::Rgb(0xFF, 0xA8, 0xCE),
        foreground: Color::Rgb(0x3B, 0x22, 0x31),
        hotkey: Color::Rgb(0xD8, 0x1B, 0x60),
        active: Color::Rgb(0xFF, 0x7F, 0xBA),
        muted: Color::Rgb(0x8B, 0x6F, 0x7D),
    };

    pub const MEADOW: Self = Self {
        background: Color::Rgb(0xFF, 0xEE, 0xAD),
        banner: Color::Rgb(0xFF, 0xF5, 0xC9),
        highlight: Color::Rgb(0x48, 0xB8, 0x91),
        foreground: Color::Rgb(0x2D, 0x49, 0x43),
        hotkey: Color::Rgb(0x4B, 0x7F, 0x6F),
        active: Color::Rgb(0x96, 0xCE, 0xB4),
        muted: Color::Rgb(0x70, 0x89, 0x7C),
    };

    pub const LAVENDER: Self = Self {
        background: Color::Rgb(0xEB, 0xE1, 0xEF),
        banner: Color::Rgb(0x75, 0x3A, 0xD1),
        highlight: Color::Rgb(0xD8, 0xC0, 0xF2),
        foreground: Color::Rgb(0x31, 0x22, 0x43),
        hotkey: Color::Rgb(0x67, 0x3C, 0xA3),
        active: Color::Rgb(0x8A, 0x5B, 0xD6),
        muted: Color::Rgb(0x78, 0x69, 0x82),
    };

    pub fn builtins() -> Vec<(String, Self)> {
        vec![
            ("Sunset".into(), Self::SUNSET),
            ("Midnight".into(), Self::MIDNIGHT),
            ("Mono".into(), Self::MONO),
            ("Candy".into(), Self::CANDY),
            ("Meadow".into(), Self::MEADOW),
            ("Lavender".into(), Self::LAVENDER),
        ]
    }

    pub fn load_all() -> Vec<(String, Self)> {
        let mut themes = Self::builtins();
        let Some(path) = dirs::config_dir().map(|dir| dir.join("NTIV").join("themes.json")) else {
            return themes;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return themes;
        };
        let Ok(definitions) = serde_json::from_slice::<Vec<ThemeDefinition>>(&bytes) else {
            return themes;
        };
        themes.extend(definitions.into_iter().filter_map(|definition| {
            let theme = Self {
                background: parse_color(&definition.background)?,
                banner: parse_color(&definition.banner)?,
                highlight: parse_color(&definition.highlight)?,
                foreground: parse_color(&definition.foreground)?,
                hotkey: parse_color(&definition.hotkey)?,
                active: parse_color(&definition.active)?,
                muted: parse_color(&definition.muted)?,
            };
            Some((definition.name, theme))
        }));
        themes
    }

    pub fn base(self) -> Style {
        Style::default().fg(self.foreground).bg(self.background)
    }

    pub fn border(self, active: bool) -> Style {
        Style::default()
            .fg(if active { self.highlight } else { self.muted })
            .bg(self.background)
    }

    pub fn selected(self) -> Style {
        Style::default()
            .fg(self.background)
            .bg(self.active)
            .add_modifier(Modifier::BOLD)
    }
}

#[derive(Debug, Deserialize)]
struct ThemeDefinition {
    name: String,
    background: String,
    banner: String,
    highlight: String,
    foreground: String,
    hotkey: String,
    active: String,
    muted: String,
}

fn parse_color(value: &str) -> Option<Color> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color::Rgb(red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_theme_colors() {
        assert_eq!(parse_color("#FFA770"), Some(Color::Rgb(0xFF, 0xA7, 0x70)));
        assert_eq!(parse_color("bad"), None);
    }

    #[test]
    fn requested_theme_colors_are_exact() {
        assert_eq!(Theme::CANDY.background, Color::Rgb(0xFF, 0xFF, 0xFF));
        assert_eq!(Theme::CANDY.active, Color::Rgb(0xFF, 0x7F, 0xBA));
        assert_eq!(Theme::MEADOW.background, Color::Rgb(0xFF, 0xEE, 0xAD));
        assert_eq!(Theme::MEADOW.active, Color::Rgb(0x96, 0xCE, 0xB4));
        assert_eq!(Theme::LAVENDER.background, Color::Rgb(0xEB, 0xE1, 0xEF));
        assert_eq!(Theme::LAVENDER.active, Color::Rgb(0x8A, 0x5B, 0xD6));
    }
}
