//! Themes (docs/design/roqa-themes.md, PROVISIONAL): a pure palette per theme. Drawing reads a `Palette`
//! fixed when a surface is created; a theme change rebuilds the surfaces, so there is no live state here.

/// Red, green, blue and alpha (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub f32);

pub type Rgb = (u8, u8, u8);

/// The one ROQA accent, Lamp, on every dark surface; Light takes the deeper `LAMP_LIGHT` (5:1 on its body).
pub const LAMP: Rgb = (255, 180, 84);
pub const LAMP_LIGHT: Rgb = (154, 82, 0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// The notch body and the dock bar at rest, one flat fill.
    pub body: Rgba,
    /// High contrast only: the system (bar track, grey text) colours; `None` in every theme.
    pub high_contrast: Option<(Rgb, Rgb)>,
    /// The dock bar with the pointer inside.
    pub dock_hover: Rgba,
    /// Text, glyphs, bars and rings; each element applies its own alpha.
    pub ink: Rgb,
    /// The progress and volume fills and the focus rings: the system accent where it reads, else `ink`.
    pub accent: Rgb,
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
    /// Liquid glass only: the baked body gradient (`top` and `bottom` around `body`) and the specular blob.
    pub sheen: Option<Sheen>,
}

/// The extra layers of the baked glass look, drawn from cached brushes (`graphics::Glass`); never live blur.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sheen {
    pub top: Rgba,
    pub bottom: Rgba,
    /// Peak alpha of the white specular blob near the top.
    pub spec: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Today's look: black body, white ink.
    #[default]
    Classic,
    Light,
    Midnight,
    Frosted,
    /// Baked liquid glass: gradient, specular blob and a lit rim, no live blur.
    Liquid,
}

impl Theme {
    pub const ALL: [Self; 5] = [
        Self::Classic,
        Self::Light,
        Self::Midnight,
        Self::Frosted,
        Self::Liquid,
    ];

    /// The stored name.
    pub fn key(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Light => "light",
            Self::Midnight => "midnight",
            Self::Frosted => "frosted-tint",
            Self::Liquid => "liquid-glass",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        // The old name of Frosted tint, still in saved settings.
        let key = if key == "glass" { "frosted-tint" } else { key };
        Self::ALL.into_iter().find(|theme| theme.key() == key)
    }

    /// The name the settings window shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic (black)",
            Self::Light => "Light",
            Self::Midnight => "Midnight",
            Self::Frosted => "Frosted tint",
            Self::Liquid => "Liquid glass",
        }
    }

    /// The palette with the system `accent` on its fills and rings. Any accent under `ACCENT_CONTRAST` against
    /// the body over a white or a black backdrop (a Glass body is translucent) keeps Lamp instead, like the text
    /// rule in the tests. Light deepens the accent (`deepen`) so the selected tab and chip read as an accent
    /// wash with dark accent text, not as a disabled grey.
    pub fn palette_with(self, accent: Rgb) -> Palette {
        let palette = self.palette();
        let accent = if self == Self::Light {
            deepen(accent)
        } else {
            accent
        };
        let reads = palette.fills().into_iter().all(|Rgba(r, g, b, alpha)| {
            [(255, 255, 255), (0, 0, 0)].into_iter().all(|backdrop| {
                contrast(accent, over((r, g, b), alpha, backdrop)) >= ACCENT_CONTRAST
            })
        });
        Palette {
            accent: if reads { accent } else { palette.accent },
            ..palette
        }
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Classic => Palette {
                high_contrast: None,
                body: Rgba(0, 0, 0, 1.0),
                dock_hover: Rgba(20, 20, 22, 1.0),
                ink: (255, 255, 255),
                accent: LAMP,
                on_ink: (0, 0, 0),
                glyph: (220, 225, 232),
                tile: Rgba(58, 58, 64, 1.0),
                shadow: 0.9,
                rim: None,
                sheen: None,
            },
            Self::Light => Palette {
                high_contrast: None,
                body: Rgba(236, 238, 242, 1.0),
                dock_hover: Rgba(226, 229, 234, 1.0),
                ink: (20, 22, 26),
                accent: LAMP_LIGHT,
                on_ink: (255, 255, 255),
                glyph: (52, 56, 64),
                tile: Rgba(198, 202, 209, 1.0),
                shadow: 0.55,
                rim: None,
                sheen: None,
            },
            Self::Midnight => Palette {
                high_contrast: None,
                body: Rgba(11, 16, 32, 1.0),
                dock_hover: Rgba(22, 30, 56, 1.0),
                ink: (232, 238, 255),
                accent: LAMP,
                on_ink: (11, 16, 32),
                glyph: (190, 205, 255),
                tile: Rgba(40, 50, 84, 1.0),
                shadow: 0.9,
                rim: None,
                sheen: None,
            },
            Self::Frosted => Palette {
                high_contrast: None,
                body: Rgba(16, 20, 32, 0.74),
                dock_hover: Rgba(16, 20, 32, 0.88),
                ink: (255, 255, 255),
                accent: LAMP,
                on_ink: (10, 14, 22),
                glyph: (235, 240, 250),
                tile: Rgba(255, 255, 255, 0.16),
                shadow: 0.0,
                rim: Some((0.70, 0.14)),
                sheen: None,
            },
            Self::Liquid => Palette {
                high_contrast: None,
                body: Rgba(16, 24, 42, 0.76),
                dock_hover: Rgba(16, 24, 42, 0.88),
                ink: (255, 255, 255),
                accent: LAMP,
                on_ink: (10, 14, 22),
                glyph: (235, 240, 250),
                tile: Rgba(255, 255, 255, 0.16),
                shadow: 0.0,
                rim: Some((0.95, 0.16)),
                sheen: Some(Sheen {
                    top: Rgba(24, 32, 56, 0.78),
                    bottom: Rgba(14, 20, 38, 0.74),
                    spec: 0.16,
                }),
            },
        }
    }
}

/// Queries the Windows system accent colour from `HKCU\Software\Microsoft\Windows\DWM\AccentColor`.
/// Returns `None` if the registry value cannot be read or is absent.
pub fn windows_accent_color() -> Option<Rgb> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
    use windows::core::w;

    let key = w!(r"Software\Microsoft\Windows\DWM");
    let name = w!("AccentColor");
    let (mut value, mut size) = (0u32, 4u32);
    // SAFETY: reads one DWORD into a local.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if read.is_ok() {
        // AccentColor is stored as 0xAABBGGRR.
        let r = (value & 0xff) as u8;
        let g = ((value >> 8) & 0xff) as u8;
        let b = ((value >> 16) & 0xff) as u8;
        Some((r, g, b))
    } else {
        None
    }
}

/// Read only at construction or a system notification; never polls or changes Windows settings.
pub fn system_palette(theme: Theme, accent: Option<Rgb>) -> Palette {
    use windows::Win32::{
        Graphics::Gdi::{
            COLOR_BTNFACE, COLOR_GRAYTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor,
        },
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{
                SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
        },
    };
    let mut contrast = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: correctly sized, writable HIGHCONTRASTW. This SPI action only queries system state.
    unsafe {
        let queried = SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&mut contrast as *mut HIGHCONTRASTW).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        if let Err(error) = &queried {
            eprintln!("ROQA: high-contrast query failed: {error}");
        }
        // A failed query falls back to the selected theme rather than ending the host.
        if queried.is_ok() && contrast.dwFlags & HCF_HIGHCONTRASTON == HCF_HIGHCONTRASTON {
            let rgb = |color: windows::Win32::Graphics::Gdi::SYS_COLOR_INDEX| {
                let value = GetSysColor(color);
                (value as u8, (value >> 8) as u8, (value >> 16) as u8)
            };
            return Palette::contrast(
                rgb(COLOR_WINDOW),
                rgb(COLOR_WINDOWTEXT),
                rgb(COLOR_BTNFACE),
                rgb(COLOR_GRAYTEXT),
            );
        }
    }
    match accent {
        Some(accent) => theme.palette_with(accent),
        None => theme.palette(),
    }
}

impl Palette {
    fn contrast(body: Rgb, ink: Rgb, tile: Rgb, gray: Rgb) -> Self {
        // The button face usually equals the window colour in a high-contrast theme: then use grey text.
        let track = if tile == body { gray } else { tile };
        let opaque = |(r, g, b)| Rgba(r, g, b, 1.0);
        Self {
            body: opaque(body),
            dock_hover: opaque(body),
            ink,
            accent: ink, // high contrast keeps the system colours
            on_ink: body,
            glyph: ink,
            tile: opaque(tile),
            shadow: 0.0,
            rim: None,
            sheen: None,
            high_contrast: Some((track, gray)),
        }
    }
    pub fn ink_on_accent(&self) -> Rgb {
        self.contrasting_ink(self.accent, self.on_ink)
    }

    pub fn rail_selected_ink(&self) -> Rgb {
        let Rgba(r, g, b, _) = self.body;
        let fill = over(self.accent, self.ink_alpha(0.22), (r, g, b));
        self.contrasting_ink(
            fill,
            if self.high_contrast.is_some() {
                self.on_ink
            } else {
                self.accent
            },
        )
    }

    fn contrasting_ink(&self, fill: Rgb, preferred: Rgb) -> Rgb {
        if contrast(preferred, fill) >= 4.5 {
            return preferred;
        }
        [self.ink, self.on_ink, (0, 0, 0), (255, 255, 255)]
            .into_iter()
            .max_by(|&a, &b| contrast(a, fill).total_cmp(&contrast(b, fill)))
            .unwrap_or(preferred)
    }

    /// Every body fill a text or accent colour must read on: the gradient stops, or the one flat fill.
    pub fn fills(&self) -> [Rgba; 3] {
        match self.sheen {
            Some(s) => [s.top, self.body, s.bottom],
            None => [self.body; 3],
        }
    }
    /// Settled secondary text and glyphs use the selected system foreground at full contrast.
    pub fn ink_alpha(&self, alpha: f32) -> f32 {
        if self.high_contrast.is_some() && alpha > 0.0 {
            1.0
        } else if self.ink.0 < 128 {
            // Dark ink on a light body (Light): faint text and glyphs read weaker there, so lift the curve.
            alpha.powf(0.6)
        } else {
            alpha
        }
    }
    /// The 1 px edge alpha of a flat body: `base` on dark bodies, firmer on a light one that would merge
    /// with a white page.
    pub fn edge_alpha(&self, base: f32) -> f32 {
        if self.ink.0 < 128 { base * 2.6 } else { base }
    }
    /// A fill: the accent, or when `dim` (muted) the system grey-text colour in high contrast, where the
    /// accent is ink anyway.
    pub fn accent_dim(&self, dim: bool, alpha: f32) -> (Rgb, f32) {
        let (color, alpha) = self.ink_dim(dim, alpha);
        (
            if color == self.ink {
                self.accent
            } else {
                color
            },
            alpha,
        )
    }
    /// A bar track: the system track colour in high contrast (never the fill's ink), else faint ink.
    pub fn track(&self, alpha: f32) -> (Rgb, f32) {
        match self.high_contrast {
            Some((track, _)) => (track, self.ink_alpha(alpha)),
            None => (self.ink, alpha),
        }
    }
    /// High contrast with no distinct track colour: bars draw a 1 px ink outline, not a grey fill.
    pub fn track_outline(&self) -> bool {
        matches!(self.high_contrast, Some((track, gray)) if track == gray)
    }
    /// Ink, or when `dim` (disabled, muted, missing) the system grey-text colour in high contrast.
    pub fn ink_dim(&self, dim: bool, alpha: f32) -> (Rgb, f32) {
        match self.high_contrast {
            Some((_, gray)) if dim => (gray, self.ink_alpha(alpha)),
            _ => (self.ink, self.ink_alpha(alpha)),
        }
    }

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

/// `accent` a fifth darker, for text and washes on the Light theme's pale body.
fn deepen((r, g, b): Rgb) -> Rgb {
    let dark = |c: u8| (f32::from(c) * 0.8).round() as u8;
    (dark(r), dark(g), dark(b))
}

/// WCAG 1.4.11 non-text contrast, for the accent against the body.
const ACCENT_CONTRAST: f32 = 3.0;

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
    let mix = |t: u8, b: u8| (f32::from(t) * alpha + f32::from(b) * (1.0 - alpha)).round() as u8;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_tab_and_current_date_read_on_their_actual_fills() {
        let themes = [
            Theme::Classic,
            Theme::Light,
            Theme::Midnight,
            Theme::Frosted,
            Theme::Liquid,
        ];
        let palettes = themes
            .into_iter()
            .flat_map(|theme| {
                [
                    theme.palette(),
                    theme.palette_with((0, 120, 212)),
                    theme.palette_with((240, 160, 50)),
                ]
            })
            .chain([
                Palette::contrast((0, 0, 0), (255, 255, 0), (0, 0, 0), (128, 128, 128)),
                Palette::contrast((255, 255, 255), (0, 0, 0), (255, 255, 255), (128, 128, 128)),
            ]);
        for palette in palettes {
            let Rgba(r, g, b, _) = palette.body;
            let wash = over(palette.accent, palette.ink_alpha(0.22), (r, g, b));
            assert!(contrast(palette.rail_selected_ink(), wash) >= 4.5);
            assert!(contrast(palette.ink_on_accent(), palette.accent) >= 4.5);
        }
    }

    #[test]
    fn high_contrast_is_opaque_and_uses_only_supplied_system_colours() {
        let palette = Palette::contrast((1, 2, 3), (250, 251, 252), (4, 5, 6), (7, 8, 9));
        assert_eq!(palette.body, Rgba(1, 2, 3, 1.0));
        assert_eq!(palette.ink, (250, 251, 252));
        assert_eq!(palette.on_ink, (1, 2, 3));
        assert_eq!(palette.ink_alpha(0.6), 1.0);
        assert_eq!(palette.ink_alpha(0.0), 0.0);
        assert_eq!((palette.shadow, palette.rim), (0.0, None));
    }
    #[test]
    fn light_lifts_faint_ink_and_edge_while_dark_themes_keep_them() {
        let (light, classic) = (Theme::Light.palette(), Theme::Classic.palette());
        assert!(light.ink_alpha(0.55) > 0.65 && light.ink_alpha(0.55) < 0.8);
        assert_eq!((light.ink_alpha(0.0), light.ink_alpha(1.0)), (0.0, 1.0));
        assert_eq!(classic.ink_alpha(0.55), 0.55);
        assert!(light.edge_alpha(0.10) > 0.2 && classic.edge_alpha(0.10) == 0.10);
    }
    #[test]
    fn high_contrast_keeps_track_and_disabled_distinct_from_ink() {
        let p = Palette::contrast((1, 2, 3), (250, 251, 252), (4, 5, 6), (7, 8, 9));
        assert_eq!(p.track(0.4), ((4, 5, 6), 1.0));
        assert_ne!(p.track(0.4).0, p.ink);
        assert_eq!(p.ink_dim(true, 0.3), ((7, 8, 9), 1.0));
        assert_eq!(p.ink_dim(false, 0.3), (p.ink, 1.0));
        // A face colour equal to the window would hide the track: grey text is used instead.
        let same = Palette::contrast((1, 2, 3), (250, 251, 252), (1, 2, 3), (7, 8, 9));
        assert_eq!(same.track(0.4).0, (7, 8, 9));
        // ...and the track is then an outline, so it is not the dim (grey) fill; tiles never use ink.
        assert!(same.track_outline() && !p.track_outline());
        assert_ne!(same.track(0.4).0, same.ink);
        assert_ne!(p.track(0.12).0, p.glyph);
        // Other themes keep the alpha look.
        let classic = Theme::Classic.palette();
        assert_eq!(
            (classic.track(0.4), classic.ink_dim(true, 0.3)),
            ((classic.ink, 0.4), (classic.ink, 0.3))
        );
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
        assert_eq!(Theme::from_key("glass"), Some(Theme::Frosted));
        assert_eq!(Theme::Frosted.key(), "frosted-tint");
        assert_eq!(Theme::Frosted.label(), "Frosted tint");
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
            // A translucent body is judged over white and over black; an opaque one is the same either way.
            for (Rgba(r, g, b, alpha), backdrop) in p
                .fills()
                .into_iter()
                .flat_map(|fill| [(fill, (255, 255, 255)), (fill, (0, 0, 0))])
            {
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
    fn lamp_is_the_one_accent_and_a_system_accent_replaces_it_only_where_it_reads() {
        let blue = (0, 120, 215);
        for theme in Theme::ALL {
            let want = if theme == Theme::Light {
                LAMP_LIGHT
            } else {
                LAMP
            };
            assert_eq!(theme.palette().accent, want, "{theme:?}");
        }
        // Light deepens a chosen accent; a bright one that cannot read on it falls back to Lamp's deep form.
        assert_eq!(Theme::Light.palette_with(blue).accent, (0, 96, 172));
        assert_eq!(Theme::Light.palette_with((255, 196, 0)).accent, LAMP_LIGHT);
        // High contrast keeps the system colours.
        let high = Palette::contrast((1, 2, 3), (250, 251, 252), (4, 5, 6), (7, 8, 9));
        assert_eq!(high.accent, high.ink);
        assert_eq!(high.accent_dim(true, 0.3), ((7, 8, 9), 1.0));
        assert_eq!(high.accent_dim(false, 0.3), (high.ink, 1.0));
        // A bright system accent shows on the dark bodies; a dark one falls back to Lamp.
        let bright = (255, 196, 0);
        assert_eq!(Theme::Classic.palette_with(bright).accent, bright);
        assert_eq!(Theme::Midnight.palette_with(bright).accent, bright);
        for dark in [(0, 0, 0), (10, 10, 30)] {
            assert_eq!(Theme::Classic.palette_with(dark).accent, LAMP);
            assert_eq!(Theme::Midnight.palette_with(dark).accent, LAMP);
        }
        // Lamp and any chosen accent read over the worst backdrop.
        for theme in Theme::ALL {
            let p = theme.palette();
            for accent in [
                blue,
                bright,
                (0, 0, 0),
                (255, 255, 255),
                (128, 128, 128),
                (200, 30, 30),
            ] {
                let chosen = theme.palette_with(accent).accent;
                for (Rgba(r, g, b, alpha), backdrop) in p
                    .fills()
                    .into_iter()
                    .flat_map(|fill| [(fill, (255, 255, 255)), (fill, (0, 0, 0))])
                {
                    let body = over((r, g, b), alpha, backdrop);
                    assert!(
                        contrast(chosen, body) >= ACCENT_CONTRAST,
                        "{theme:?} {chosen:?}"
                    );
                }
            }
        }
        let classic = Theme::Classic.palette_with(bright);
        assert_eq!(classic.accent_dim(false, 0.5), (bright, 0.5));
    }

    #[test]
    fn only_glass_is_translucent_and_has_a_rim() {
        for theme in Theme::ALL {
            let p = theme.palette();
            let glass = matches!(theme, Theme::Frosted | Theme::Liquid);
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
    fn liquid_glass_is_its_own_theme_and_keeps_text_readable_under_the_specular_blob() {
        assert_eq!(Theme::from_key("liquid-glass"), Some(Theme::Liquid));
        assert_eq!(Theme::Liquid.label(), "Liquid glass");
        assert_ne!(Theme::Liquid.palette(), Theme::Frosted.palette());
        assert!(Theme::Frosted.palette().sheen.is_none());
        let p = Theme::Liquid.palette();
        let sheen = p.sheen.expect("liquid glass has a sheen");
        let (top, bottom) = (sheen.top, sheen.bottom);
        assert!(top.3 < 1.0 && bottom.3 < 1.0 && sheen.spec > 0.0 && sheen.spec < 0.3);
        // The blob adds white over the lightest stop: large text and glyphs still keep 4.5:1.
        let Rgba(r, g, b, alpha) = top;
        for backdrop in [(255, 255, 255), (0, 0, 0)] {
            let lit = over(
                (255, 255, 255),
                sheen.spec,
                over((r, g, b), alpha, backdrop),
            );
            assert!(contrast(p.ink, lit) >= 4.5, "blob over {backdrop:?}");
        }
        // Flat themes list one fill, glass lists the three stops.
        assert_eq!(Theme::Classic.palette().fills(), [Rgba(0, 0, 0, 1.0); 3]);
        assert_eq!(p.fills(), [top, p.body, bottom]);
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
        assert_eq!((light.0, light.1, light.2), (231, 234, 238));
    }

    #[test]
    fn system_palette_with_none_accent_keeps_theme_default() {
        let default_classic = system_palette(Theme::Classic, None);
        assert_eq!(default_classic.accent, Theme::Classic.palette().accent);

        let default_light = system_palette(Theme::Light, None);
        assert_eq!(default_light.accent, Theme::Light.palette().accent);

        let custom_accent = (0, 120, 215);
        let tinted_classic = system_palette(Theme::Classic, Some(custom_accent));
        assert_eq!(tinted_classic.accent, custom_accent);
        assert_eq!(Theme::Classic.palette().accent, LAMP);
    }
}
