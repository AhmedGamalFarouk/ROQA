//! Themes (docs/design/roqa-themes.md, PROVISIONAL): a pure palette per theme. Drawing reads a `Palette`
//! fixed when a surface is created; a theme change rebuilds the surfaces, so there is no live state here.

/// Red, green, blue and alpha (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub f32);

pub type Rgb = (u8, u8, u8);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The notch body and the dock bar at rest, one flat fill.
    pub body: Rgba,
    /// The dock bar with the pointer inside.
    pub dock_hover: Rgba,
    /// Text, glyphs, bars and rings; each element applies its own alpha.
    pub ink: Rgb,
    /// A glyph on an ink-filled circle, and the badge circle.
    pub on_ink: Rgb,
    /// The dock's picker tiles.
    pub glyph: Rgb,
    /// A dock group whose icon is unavailable.
    pub tile: Rgba,
    /// Notch shadow strength (Classic 0.9).
    pub shadow: f32,
    /// Glass only: a 1 px stroke along the outline, with its alpha at the top and at the bottom.
    pub rim: Option<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Today's look: black body, white ink.
    #[default]
    Classic,
    Light,
    Midnight,
    Glass,
}

impl Theme {
    pub const ALL: [Self; 4] = [Self::Classic, Self::Light, Self::Midnight, Self::Glass];

    /// The stored name.
    pub fn key(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Light => "light",
            Self::Midnight => "midnight",
            Self::Glass => "glass",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.key() == key)
    }

    /// The name the settings window shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic (black)",
            Self::Light => "Light",
            Self::Midnight => "Midnight",
            Self::Glass => "Liquid glass",
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Classic => Palette {
                body: Rgba(0, 0, 0, 1.0),
                dock_hover: Rgba(20, 20, 22, 1.0),
                ink: (255, 255, 255),
                on_ink: (0, 0, 0),
                glyph: (220, 225, 232),
                tile: Rgba(58, 58, 64, 1.0),
                shadow: 0.9,
                rim: None,
            },
            Self::Light => Palette {
                body: Rgba(244, 245, 247, 1.0),
                dock_hover: Rgba(232, 234, 238, 1.0),
                ink: (20, 22, 26),
                on_ink: (255, 255, 255),
                glyph: (52, 56, 64),
                tile: Rgba(205, 208, 214, 1.0),
                shadow: 0.35,
                rim: None,
            },
            Self::Midnight => Palette {
                body: Rgba(11, 16, 32, 1.0),
                dock_hover: Rgba(22, 30, 56, 1.0),
                ink: (232, 238, 255),
                on_ink: (11, 16, 32),
                glyph: (190, 205, 255),
                tile: Rgba(40, 50, 84, 1.0),
                shadow: 0.9,
                rim: None,
            },
            Self::Glass => Palette {
                body: Rgba(22, 28, 40, 0.82),
                dock_hover: Rgba(22, 28, 40, 0.90),
                ink: (255, 255, 255),
                on_ink: (10, 14, 22),
                glyph: (235, 240, 250),
                tile: Rgba(255, 255, 255, 0.16),
                shadow: 0.0,
                rim: Some((0.55, 0.10)),
            },
        }
    }
}

impl Palette {
    /// The dock bar fill for a hover progress of `inside` (0 at rest, 1 with the pointer in).
    pub fn dock_fill(&self, inside: f32) -> Rgba {
        let (a, b) = (self.body, self.dock_hover);
        let mix = |from: u8, to: u8| {
            (f32::from(from) + (f32::from(to) - f32::from(from)) * inside).round() as u8
        };
        Rgba(
            mix(a.0, b.0),
            mix(a.1, b.1),
            mix(a.2, b.2),
            a.3 + (b.3 - a.3) * inside,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luminance((r, g, b): Rgb) -> f32 {
        let channel = |c: f32| {
            let c = c / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r as f32) + 0.7152 * channel(g as f32) + 0.0722 * channel(b as f32)
    }

    /// `top` at `alpha` over `bottom`.
    fn over(top: Rgb, alpha: f32, bottom: Rgb) -> Rgb {
        let mix =
            |t: u8, b: u8| (f32::from(t) * alpha + f32::from(b) * (1.0 - alpha)).round() as u8;
        (
            mix(top.0, bottom.0),
            mix(top.1, bottom.1),
            mix(top.2, bottom.2),
        )
    }

    fn contrast(a: Rgb, b: Rgb) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn classic_is_the_legacy_look() {
        let p = Theme::Classic.palette();
        assert_eq!(Theme::default(), Theme::Classic);
        assert_eq!(
            (p.body, p.ink, p.on_ink),
            (Rgba(0, 0, 0, 1.0), (255, 255, 255), (0, 0, 0))
        );
        assert_eq!(p.dock_fill(0.0), Rgba(0, 0, 0, 1.0));
        assert_eq!(p.dock_fill(1.0), Rgba(20, 20, 22, 1.0));
        assert_eq!(
            (p.glyph, p.tile, p.shadow, p.rim),
            ((220, 225, 232), Rgba(58, 58, 64, 1.0), 0.9, None)
        );
    }

    #[test]
    fn keys_round_trip_and_unknown_names_are_refused() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_key(theme.key()), Some(theme));
            assert!(!theme.label().is_empty());
        }
        let mut keys: Vec<_> = Theme::ALL.iter().map(|t| t.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), Theme::ALL.len());
        for bad in ["", "Classic", "dark", "glass "] {
            assert_eq!(Theme::from_key(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn text_stays_readable_over_the_worst_backdrop_in_every_theme() {
        for theme in Theme::ALL {
            let p = theme.palette();
            let Rgba(r, g, b, alpha) = p.body;
            // A translucent body is judged over white and over black; an opaque one is the same either way.
            for backdrop in [(255, 255, 255), (0, 0, 0)] {
                let body = over((r, g, b), alpha, backdrop);
                let primary = contrast(p.ink, body);
                let dim = contrast(over(p.ink, 0.60, body), body);
                assert!(
                    primary >= 7.0,
                    "{theme:?} primary {primary} over {backdrop:?}"
                );
                assert!(dim >= 4.0, "{theme:?} 60% text {dim} over {backdrop:?}");
                // The glyph on an ink circle must read too.
                assert!(contrast(p.on_ink, p.ink) >= 7.0, "{theme:?} on_ink");
            }
        }
    }

    #[test]
    fn only_glass_is_translucent_and_has_a_rim() {
        for theme in Theme::ALL {
            let p = theme.palette();
            let glass = theme == Theme::Glass;
            assert_eq!(p.body.3 < 1.0, glass, "{theme:?}");
            assert_eq!(p.rim.is_some(), glass, "{theme:?}");
            assert!(
                (0.0..=1.0).contains(&p.shadow) && (glass == (p.shadow == 0.0)),
                "{theme:?}"
            );
            if let Some((top, bottom)) = p.rim {
                assert!(top > bottom && bottom > 0.0 && top <= 1.0);
            }
        }
    }

    #[test]
    fn the_dock_fill_blends_between_rest_and_hover() {
        for theme in Theme::ALL {
            let p = theme.palette();
            assert_eq!(p.dock_fill(0.0), p.body);
            assert_eq!(p.dock_fill(1.0), p.dock_hover);
            let mid = p.dock_fill(0.5);
            assert!(mid.3 >= p.body.3.min(p.dock_hover.3) && mid.3 <= p.body.3.max(p.dock_hover.3));
        }
        let light = Theme::Light.palette().dock_fill(0.5);
        assert_eq!((light.0, light.1, light.2), (238, 240, 243));
    }
}
