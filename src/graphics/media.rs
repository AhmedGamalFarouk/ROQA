//! The Media notch: the plain pill and the media track (`media_page`), its plan (cover, text, chips, progress,
//! volume bar and meter bars) and the Direct2D drawing of it. Moved whole out of `graphics.rs`.

use super::*;

/// The Media page, and the pill every page starts from: the media track or the plain clock, date and status.
pub(super) fn media_page(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    art: Option<&ID2D1Bitmap1>,
    status: Option<&str>,
    p: &Palette,
) -> Result<()> {
    // The host's line (a glance or a media status) can wash on hover; the default never does.
    let wash = if status.is_some() {
        scene.line_wash
    } else {
        0.0
    };
    let status = (scene.notice.is_none() || scene.extra.footer())
        .then(|| status.unwrap_or("Nothing playing"));
    if let Some(track) = scene.track {
        let font = media_clock(motion::smooth(scene.progress), scale);
        let clock = text_width(scene.time, &font)?;
        return media_text(context, &media_plan(scene, track, body, clock), art, p);
    }
    let open = motion::smooth(scene.progress);
    // The clock and date keep the left zone and the mascot sits in the right one with the status under it; the
    // pill's own clock is the same text centred, so opening only slides it left (the zone's right edge moves in).
    let zone = 130.0 * open;
    let row = |middle: f32, half: f32| D2D_RECT_F {
        left: body.left + 12.0 * scale,
        top: body.top + (middle - half) * scale,
        right: body.right - (12.0 + zone) * scale,
        bottom: body.top + (middle + half) * scale,
    };
    let size = 15.0 + 17.0 * open;
    let clock = Font {
        // Text optical size for the small pill clock, Display once it is large.
        family: if size < 20.0 { TEXT_FONT } else { DISPLAY_FONT },
        size: size * scale,
        weight: DWRITE_FONT_WEIGHT_BOLD,
        designator: TIME_DESIGNATOR,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    draw_text(
        context,
        scene.time,
        &clock,
        row(15.0 + 36.0 * open, 24.0),
        with_alpha(p.ink, p.ink_alpha(tab_art::clock_shown(scene))),
    )?;
    let date = motion::reveal(scene.progress);
    if date <= 0.0 {
        return Ok(());
    }
    let font = Font {
        family: TEXT_FONT,
        size: 13.0 * scale,
        weight: DWRITE_FONT_WEIGHT_NORMAL,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    };
    draw_text(
        context,
        scene.date,
        &font,
        row(82.0 - 6.0 * (1.0 - date), 10.0),
        with_alpha(p.ink, p.ink_alpha(0.60 * date)),
    )?;
    if !scene.hijri.is_empty() {
        draw_text(
            context,
            scene.hijri,
            &Font {
                size: 11.0 * scale,
                ..font
            },
            row(100.0 - 6.0 * (1.0 - date), 9.0),
            with_alpha(p.ink, p.ink_alpha(0.45 * date)),
        )?;
    }
    // The shared header: the page's title at the left (the tune disc takes the right).
    let title = layout::page_title(scene.dpi);
    draw_text(
        context,
        "Media",
        &Font {
            size: 13.0 * scale,
            weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
            align: DWRITE_TEXT_ALIGNMENT_LEADING,
            ..font
        },
        D2D_RECT_F {
            left: body.left + title.x as f32,
            top: body.top + title.y as f32,
            right: body.left + (title.x + title.width) as f32,
            bottom: body.top + (title.y + title.height) as f32,
        },
        with_alpha(p.ink, p.ink_alpha(date)),
    )?;
    // The mascot rests beside the clock, even with its shows off (then a still pair), the status line under it.
    let centre = layout::NOTCH_EXPANDED.width - 16.0 - 56.0;
    let frame = scene.eyes.unwrap_or(Frame {
        mood: Mood::Idle,
        pose: Pose::Rest,
        alpha: 1.0,
        look: 0.0,
    });
    tab_art::panel_eyes(
        context,
        body,
        scale,
        date,
        frame,
        ((centre, 56.0, 1.0), 0.0),
        p,
    )?;
    let eyes_y = 72.0;
    let Some(line) = status else {
        return Ok(());
    };
    let font = Font {
        size: 11.0 * scale,
        ..font
    };
    if wash > 0.0 {
        // A capsule 24 high around the text, 8 each side (`layout::glance_at` is the click target).
        let half = (text_width(line, &font)? / scale / 2.0 + 8.0).min(64.0);
        let wash_rect = D2D_RECT_F {
            left: body.left + (centre - half) * scale,
            top: body.top + (eyes_y + 10.0) * scale,
            right: body.left + (centre + half) * scale,
            bottom: body.top + (eyes_y + 34.0) * scale,
        };
        // SAFETY: live draw context between BeginDraw/EndDraw; the brush is scoped to this draw.
        unsafe {
            let ink = with_alpha(p.ink, p.ink_alpha(0.10 * wash * date));
            let brush = context.CreateSolidColorBrush(&ink, None)?;
            context.FillRoundedRectangle(&rounded(wash_rect, 12.0 * scale), &brush);
        }
    }
    draw_text(
        context,
        line,
        &font,
        D2D_RECT_F {
            left: body.left + (centre - 56.0) * scale,
            top: body.top + (eyes_y + 14.0) * scale,
            right: body.left + (centre + 56.0) * scale,
            bottom: body.top + (eyes_y + 30.0) * scale,
        },
        with_alpha(p.ink, p.ink_alpha((0.60 + 0.40 * wash) * date)),
    )
}

/// Fill of the cover slot when the track has no album art; art fills the same geometry.
const COVER_FILL: f32 = 0.20;
/// Side of the panel cover, the largest drawn album art; the host decodes art for it.
pub const COVER_DIPS: f32 = 80.0;

/// One text element of the media notch, in physical pixels.
struct Line<'a> {
    text: Cow<'a, str>,
    font: Font,
    rect: D2D_RECT_F,
    alpha: f32,
}

/// What the media notch draws for one frame, in physical pixels: the cover slot with its centred
/// play-state glyph (a triangle while playing, two bars while paused), the visible text lines, then
/// the panel's controls once they fade in.
struct MediaPlan<'a> {
    cover: D2D1_ROUNDED_RECT,
    /// Album art fills the cover in place of the slot and its glyph.
    art: Option<&'a IconPixels>,
    /// The pill's play-state badge on the art.
    badge: Option<Badge>,
    glyph: D2D_RECT_F,
    playing: bool,
    /// Glyph opacity, and a factor of the title's: 1 playing, 0.6 paused.
    dim: f32,
    lines: Vec<Line<'a>>,
    /// The progress bar's track and filled part, and their opacity.
    bar: Option<([D2D1_ROUNDED_RECT; 2], f32)>,
    chips: Vec<Chip>,
    players: Vec<PlayerChip<'a>>,
    volume: Option<VolumeBar>,
    /// The visualizer's bars and their opacity.
    meter: Option<([D2D1_ROUNDED_RECT; BARS], f32)>,
    /// The collapsed pill's level bars and their opacity.
    pill_meter: Option<([D2D1_ROUNDED_RECT; BARS], f32)>,
}

/// A black circle carrying the play-state glyph at the art's bottom-right corner, faded by `alpha`.
struct Badge {
    circle: D2D1_ROUNDED_RECT,
    glyph: D2D_RECT_F,
    alpha: f32,
}
/// The volume level bar: a thin track, and its fill to the shown percent, dimmed while muted or
/// without an endpoint. Both fade in with the controls.
struct VolumeBar {
    track: D2D1_ROUNDED_RECT,
    track_alpha: f32,
    fill: D2D1_ROUNDED_RECT,
    fill_alpha: f32,
    /// Muted or without an endpoint: high contrast draws the fill in the system grey-text colour.
    dim: bool,
    /// The thumb at the fill's end: centre x, centre y and radius, in pixels.
    thumb: (f32, f32, f32),
    /// The percent readout above the thumb while pressed.
    readout: Option<(String, D2D_RECT_F)>,
}
/// The visualizer's bars: secondary, decorative content, white 60 % like the artist.
const METER_FILL: f32 = 0.60;
/// The bar's drawn thickness in DIPs, centred in its hit box.
const VOLUME_THICKNESS: f32 = 4.0;
/// The thumb's radius at rest, under the pointer and while pressed, in DIPs.
const THUMB: [f32; 3] = [4.5, 6.0, 7.5];
/// The bar's track: white 40 % is about 3.7 : 1 on black, the non-text contrast a target needs.
const VOLUME_TRACK: f32 = 0.40;
/// The fill while muted: white 60 % over the track is about 3.2 : 1 against it, and dimmer than the
/// audible white fill.
const MUTED_FILL: f32 = 0.60;

/// One media control: a line glyph, with a soft accent wash for play/pause. A control
/// the player does not accept now is drawn at `DISABLED` opacity.
struct Chip {
    rect: D2D_RECT_F,
    glyph: D2D_RECT_F,
    shape: Glyph,
    /// The primary action: a solid accent capsule.
    filled: bool,
    /// A transport capsule (previous, play/pause, next); the mute toggle is a bare glyph.
    capsule: bool,
    alpha: f32,
    /// The player does not accept it now: high contrast draws it in the system grey-text colour.
    disabled: bool,
    /// A mode that is on (shuffle, repeat): its glyph wears the accent.
    lit: bool,
}

/// A player chip under the cover when two or more players are open: the shown one wears the accent.
struct PlayerChip<'a> {
    rect: D2D_RECT_F,
    name: &'a str,
    chosen: bool,
    alpha: f32,
    font: Font,
}
/// The shuffle and repeat chips of the shown player, for a player that accepts the change: under the cover while
/// fewer than two players are open, else (the player chips have that row) under the volume controls. A mode that is
/// on wears the accent.
fn mode_chips(scene: &NotchScene, track: &Track, body: &D2D_RECT_F) -> Vec<Chip> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return Vec::new();
    }
    let scale = scene.dpi as f32 / 96.0;
    let repeat = match track.repeat {
        Some(1) => Glyph::RepeatOne,
        _ => Glyph::Repeat,
    };
    [
        (track.shuffle, Glyph::Shuffle, track.shuffle == Some(true)),
        (track.repeat.map(|_| true), repeat, track.repeat > Some(0)),
    ]
    .into_iter()
    .enumerate()
    .filter_map(|(index, (offered, shape, lit))| {
        offered?;
        let cell = layout::mode_chip(scene.dpi, track.players.len(), index);
        let rect = D2D_RECT_F {
            left: body.left + cell.x as f32,
            top: body.top + cell.y as f32,
            right: body.left + (cell.x + cell.width) as f32,
            bottom: body.top + (cell.y + cell.height) as f32,
        };
        let size = (12.0 * scale).round();
        Some(Chip {
            rect,
            glyph: pixel_square(
                rect.left + ((cell.width as f32 - size) / 2.0).round(),
                rect.top + ((cell.height as f32 - size) / 2.0).round(),
                size,
            ),
            shape,
            filled: false,
            capsule: true,
            alpha: reveal,
            disabled: false,
            lit,
        })
    })
    .collect()
}

/// The chips for `track`'s players, placed like the controls and faded in with them.
fn player_chips<'a>(
    scene: &NotchScene,
    track: &'a Track,
    body: &D2D_RECT_F,
) -> Vec<PlayerChip<'a>> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return Vec::new();
    }
    let scale = scene.dpi as f32 / 96.0;
    track
        .players
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let cell = layout::player_chip(scene.dpi, track.players.len(), index);
            PlayerChip {
                rect: D2D_RECT_F {
                    left: body.left + cell.x as f32,
                    top: body.top + cell.y as f32,
                    right: body.left + (cell.x + cell.width) as f32,
                    bottom: body.top + (cell.y + cell.height) as f32,
                },
                name,
                chosen: index == track.chosen,
                alpha: reveal,
                font: Font {
                    family: TEXT_FONT,
                    size: 11.0 * scale,
                    weight: DWRITE_FONT_WEIGHT_NORMAL,
                    designator: 1.0,
                    align: DWRITE_TEXT_ALIGNMENT_CENTER,
                },
            }
        })
        .collect()
}

/// The collapsed pill's bar levels while a track plays.
const PILL_LEVELS: [f32; BARS] = [0.4, 0.8, 1.0, 0.6, 0.3];
/// The media clock at `open` (`smooth(progress)`): the pill's 15 DIP bold clock settles into the
/// panel's 12 DIP regular one, trailing-aligned so it is never trimmed.
fn media_clock(open: f32, scale: f32) -> Font {
    Font {
        family: TEXT_FONT,
        size: (15.0 - 3.0 * open) * scale,
        weight: if open < 0.5 {
            DWRITE_FONT_WEIGHT_BOLD
        } else {
            DWRITE_FONT_WEIGHT_NORMAL
        },
        designator: TIME_DESIGNATOR,
        align: DWRITE_TEXT_ALIGNMENT_TRAILING,
    }
}

/// Direction B ("Cover accent"; docs/design/roqa-visual-spec.md, "Media notch"). Every element
/// moves by `smooth(p)` from its pill position to its panel position, in DIPs from the body's left
/// and top edges (the clock from its right edge). The title column ends before the clock's measured
/// width (`clock`, physical pixels) and never inverts. The artist fades in below the title as the
/// date does, and so do the controls (`media_chips`), the progress row (`progress_row`) and the
/// volume bar (`volume_bar`). Empty text is skipped.
fn media_plan<'a>(
    scene: &NotchScene<'a>,
    track: &'a Track,
    body: &D2D_RECT_F,
    clock: f32,
) -> MediaPlan<'a> {
    let scale = scene.dpi as f32 / 96.0;
    let open = motion::smooth(scene.progress);
    let at = |pill: f32, panel: f32| pill + (panel - pill) * open;
    // The close spring's undershoot briefly makes the pill shorter than 30: keep its row centred.
    let middle = (layout::notch_shape(scene.progress, true).height / 2.0).min(15.0);
    // Cover: 20 at (10, middle - 10), radius 5 -> COVER_DIPS at (16, 24), radius 14. Glyph: 10 -> 24.
    let size = (at(20.0, COVER_DIPS) * scale).round();
    let cover = pixel_square(
        body.left + at(10.0, 16.0) * scale,
        body.top + at(middle - 10.0, 24.0) * scale,
        size,
    );
    let glyph = (at(10.0, 24.0) * scale).round();
    let inset = ((size - glyph) / 2.0).round();
    // Badge: 11 with a 6 glyph, 4 past the cover's right and bottom edges. Only the pill has one;
    // it fades out as the panel opens, whose play/pause control shows the state.
    let badge = (scene.art.is_some() && open < 1.0).then(|| {
        let side = (11.0 * scale).round();
        let circle = pixel_square(
            cover.right + 4.0 * scale - side,
            cover.bottom + 4.0 * scale - side,
            side,
        );
        let glyph = (6.0 * scale).round();
        let inset = ((side - glyph) / 2.0).round();
        Badge {
            circle: rounded(circle, side / 2.0),
            glyph: pixel_square(circle.left + inset, circle.top + inset, glyph),
            alpha: 1.0 - open,
        }
    });
    // Clock: trailing edge 14 -> 16 from the right, centre middle -> 19. Title: from 38 -> 108,
    // ending 8 -> 6 before the clock, centre middle -> 30 (15 DIP, hierarchy over the artist).
    let end = body.right - at(14.0, 16.0) * scale;
    let leading = body.left + at(38.0, layout::MEDIA_COLUMN) * scale;
    let title = (leading, (end - clock - at(8.0, 6.0) * scale).max(leading));
    let row = |(left, right): (f32, f32), middle: f32, half: f32| D2D_RECT_F {
        left,
        top: body.top + (middle - half) * scale,
        right,
        bottom: body.top + (middle + half) * scale,
    };
    let font = |size: f32, weight| Font {
        family: TEXT_FONT,
        size: size * scale,
        weight,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_LEADING,
    };
    let dim = if track.playing { 1.0 } else { 0.60 };
    let artist = motion::reveal(scene.progress);
    // The title makes way while the mascot's eyes are in its lane.
    let title_fade = tab_art::clock_shown(scene);
    // The pill's lane: five level bars, then the elapsed time. They fade with the title's lane.
    let pill_text = leading + 24.0 * scale;
    // The pill's lane is gone by 40 % open and the title only starts there, so the two never overlap.
    let fade = title_fade * (1.0 - open / 0.4).max(0.0);
    let pill_meter = (fade > 0.0).then(|| {
        let bars = std::array::from_fn(|index| {
            // Still bars (the live level would cost a redraw 15 times a second for as long as music plays); flat when paused.
            let level = if track.playing {
                PILL_LEVELS[index]
            } else {
                0.0
            };
            let height = ((3.0 + 9.0 * level) * scale).round();
            let left = (leading + index as f32 * 4.0 * scale).round();
            let top = body.top + middle * scale - height / 2.0;
            let rect = D2D_RECT_F {
                left,
                top,
                right: left + 2.0 * scale,
                bottom: top + height,
            };
            rounded(rect, scale)
        });
        (bars, METER_FILL * fade * dim)
    });
    let mut lines = vec![
        Line {
            text: scene.time.into(),
            font: media_clock(open, scale),
            rect: row((cover.right, end), at(middle, 19.0), 12.0),
            alpha: at(1.0, 0.60),
        },
        Line {
            text: track.title.as_str().into(),
            font: font(
                at(12.0, 15.0),
                if open < 0.5 {
                    DWRITE_FONT_WEIGHT_NORMAL
                } else {
                    DWRITE_FONT_WEIGHT_SEMI_BOLD
                },
            ),
            rect: row(title, at(middle, 30.0), 12.0),
            // The pill shows the elapsed time and level bars instead; the title arrives with the panel.
            alpha: at(0.85, 1.0) * dim * title_fade * ((open - 0.4) / 0.6).max(0.0),
        },
        Line {
            text: scene
                .played
                .map_or_else(String::new, |p| p.times()[0].clone())
                .into(),
            font: font(12.0, DWRITE_FONT_WEIGHT_NORMAL),
            rect: row((pill_text, title.1.max(pill_text)), middle, 12.0),
            alpha: dim * fade,
        },
        Line {
            text: track.artist.as_str().into(),
            font: font(12.0, DWRITE_FONT_WEIGHT_NORMAL),
            rect: row(
                // The artist lane ends before the visualizer slot (312).
                (title.0, (body.left + 304.0 * scale).max(title.0)),
                50.0 + 6.0 * (1.0 - artist),
                10.0,
            ),
            alpha: 0.60 * artist,
        },
    ];
    let bar = progress_row(&mut lines, scene, body);
    let mut chips = media_chips(scene, track, body);
    chips.extend(mode_chips(scene, track, body));
    let players = player_chips(scene, track, body);
    MediaPlan {
        cover: rounded(cover, at(5.0, layout::TILE_RADIUS) * scale),
        art: scene.art.map(|(_, pixels)| pixels),
        badge,
        glyph: pixel_square(cover.left + inset, cover.top + inset, glyph),
        playing: track.playing,
        dim,
        lines: lines
            .into_iter()
            .filter(|line| line.alpha > 0.0 && !line.text.is_empty())
            .collect(),
        bar,
        chips,
        players,
        volume: volume_bar(scene, body),
        meter: meter_bars(scene, body),
        pill_meter,
    }
}

/// The visualizer slot in surface pixels for a surface `width` wide, with the open panel's body.
pub(super) fn meter_update(width: i32, dpi: u32) -> RECT {
    let left = layout::notch_body(width, dpi, layout::notch_shape(1.0, true)).left;
    let (slot, _) = layout::meter(dpi);
    RECT {
        left: left + slot.x,
        top: slot.y,
        right: left + slot.x + slot.width,
        bottom: slot.y + slot.height,
    }
}

/// The visualizer right of the controls row, placed and revealed like it: fully rounded bars spread
/// across the slot (`layout::meter`), bottom-aligned at the scene's heights.
fn meter_bars(scene: &NotchScene, body: &D2D_RECT_F) -> Option<([D2D1_ROUNDED_RECT; BARS], f32)> {
    meter_rects(scene.progress, scene.dpi, scene.bars, body)
}

/// `meter_bars` for bar `heights` at an animation `progress`, without a whole scene.
pub(super) fn meter_rects(
    progress: f32,
    dpi: u32,
    heights: [i32; BARS],
    body: &D2D_RECT_F,
) -> Option<([D2D1_ROUNDED_RECT; BARS], f32)> {
    let reveal = motion::reveal(progress);
    if reveal <= 0.0 {
        return None;
    }
    let (slot, bar) = layout::meter(dpi);
    let bottom = body.top + (slot.y + slot.height) as f32;
    let bars = std::array::from_fn(|index| {
        let left =
            body.left + (slot.x + index as i32 * (slot.width - bar) / (BARS as i32 - 1)) as f32;
        let rect = D2D_RECT_F {
            left,
            top: bottom - heights[index] as f32,
            right: left + bar as f32,
            bottom,
        };
        rounded(rect, bar as f32 / 2.0)
    });
    Some((bars, METER_FILL * reveal))
}

/// The panel's progress row (`layout::media_progress`), fading in with the controls: the elapsed
/// time in 11 DIP from x 100, the 4 DIP bar centred in its target, and the total time trailing at
/// 16 from the right, all centred on y 57. Adds the times to `lines` and returns the bar's track,
/// fill and opacity; none while the row is hidden or the scene has none.
fn progress_row(
    lines: &mut Vec<Line>,
    scene: &NotchScene,
    body: &D2D_RECT_F,
) -> Option<([D2D1_ROUNDED_RECT; 2], f32)> {
    let reveal = motion::reveal(scene.progress);
    let played = scene.played.filter(|_| reveal > 0.0)?;
    let scale = scene.dpi as f32 / 96.0;
    let target = layout::media_progress(scene.dpi);
    let thickness = (6.0 * scale).round();
    let left = body.left + target.x as f32;
    let top = (body.top + target.y as f32 + (target.height as f32 - thickness) / 2.0).round();
    let track = D2D_RECT_F {
        left,
        top,
        right: left + target.width as f32,
        bottom: top + thickness,
    };
    let fill = D2D_RECT_F {
        right: left + played.filled as f32,
        ..track
    };
    let [elapsed, total] = played.times();
    for (text, (from, y, w, h), align) in [
        (
            elapsed,
            layout::MEDIA_TIMES[0],
            DWRITE_TEXT_ALIGNMENT_LEADING,
        ),
        (
            total,
            layout::MEDIA_TIMES[1],
            DWRITE_TEXT_ALIGNMENT_TRAILING,
        ),
    ] {
        lines.push(Line {
            text: text.into(),
            font: Font {
                family: TEXT_FONT,
                size: 11.0 * scale,
                weight: DWRITE_FONT_WEIGHT_NORMAL,
                designator: 1.0,
                align,
            },
            rect: D2D_RECT_F {
                left: body.left + from * scale,
                top: body.top + (y - 1.0) * scale,
                right: body.left + (from + w) * scale,
                bottom: body.top + (y + h + 1.0) * scale,
            },
            alpha: 0.60 * reveal,
        });
    }
    let radius = thickness / 2.0;
    Some(([rounded(track, radius), rounded(fill, radius)], reveal))
}

/// The level bar right of the mute toggle, placed and revealed like the controls; the fill ends at
/// the shown whole percent.
fn volume_bar(scene: &NotchScene, body: &D2D_RECT_F) -> Option<VolumeBar> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return None;
    }
    let bar = layout::volume_bar(scene.dpi);
    let thickness = (VOLUME_THICKNESS * scene.dpi as f32 / 96.0).round();
    let left = body.left + bar.x as f32;
    let top = body.top + bar.y as f32 + ((bar.height as f32 - thickness) / 2.0).round();
    let percent = scene.volume.map_or(0, |level| level.percent);
    let radius = THUMB[usize::from(scene.grip.hover) + usize::from(scene.grip.drag)]
        * scene.dpi as f32
        / 96.0;
    let filled = (bar.width as f32 * scene.volume_fill.clamp(0.0, 100.0) / 100.0).round();
    let rect = |width: f32| {
        rounded(
            D2D_RECT_F {
                left,
                top,
                right: left + width,
                bottom: top + thickness,
            },
            thickness / 2.0,
        )
    };
    Some(VolumeBar {
        track: rect(bar.width as f32),
        track_alpha: reveal * VOLUME_TRACK,
        fill: rect(filled),
        fill_alpha: reveal
            * match scene.volume {
                Some(level) if !level.muted => 1.0,
                Some(_) => MUTED_FILL,
                None => DISABLED,
            },
        dim: scene.volume.is_none_or(|level| level.muted),
        thumb: (left + filled, top + thickness / 2.0, radius),
        readout: (scene.grip.drag && scene.volume.is_some()).then(|| {
            let scale = scene.dpi as f32 / 96.0;
            let middle = left + filled;
            let bottom = top + thickness / 2.0 - radius - 2.0 * scale;
            (
                format!("{percent}%"),
                D2D_RECT_F {
                    left: middle - 22.0 * scale,
                    top: bottom - 14.0 * scale,
                    right: middle + 22.0 * scale,
                    bottom,
                },
            )
        }),
    })
}

/// Previous, play/pause and next keep their panel place in the body, so the body clip reveals
/// them as it opens; they fade in with the artist. None are planned while they are invisible.
fn media_chips(scene: &NotchScene, track: &Track, body: &D2D_RECT_F) -> Vec<Chip> {
    let scale = scene.dpi as f32 / 96.0;
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return Vec::new();
    }
    let toggle = if track.playing {
        Glyph::Pause
    } else {
        Glyph::Play
    };
    let speaker = scene.volume.map_or(Glyph::Speaker, |level| {
        Glyph::speaker(level.percent, level.muted)
    });
    let [previous, play_pause, next] = track.controls;
    let enabled = [previous, play_pause, next, scene.volume.is_some()];
    layout::media_controls(scene.dpi)
        .into_iter()
        .zip([Glyph::Previous, toggle, Glyph::Next, speaker])
        .enumerate()
        .map(|(index, (control, shape))| {
            let rect = D2D_RECT_F {
                left: body.left + control.x as f32,
                top: body.top + control.y as f32,
                right: body.left + (control.x + control.width) as f32,
                bottom: body.top + (control.y + control.height) as f32,
            };
            let filled = index == PLAY_PAUSE;
            // A 16 DIP glyph inside the circle, 18 DIP on its own (the 24-unit icon grid has its own margin).
            let size = (if filled { 16.0 } else { 18.0 } * scale)
                .round()
                .min(rect.right - rect.left)
                .min(rect.bottom - rect.top);
            let centre = |extent: i32| ((extent as f32 - size) / 2.0).round();
            Chip {
                rect,
                glyph: pixel_square(
                    rect.left + centre(control.width),
                    rect.top + centre(control.height),
                    size,
                ),
                shape,
                filled,
                capsule: index < PLAY_PAUSE + 2,
                alpha: reveal * if enabled[index] { 1.0 } else { DISABLED },
                disabled: !enabled[index],
                lit: false,
            }
        })
        .collect()
}

fn media_text(
    context: &ID2D1DeviceContext,
    plan: &MediaPlan,
    bitmap: Option<&ID2D1Bitmap1>,
    p: &Palette,
) -> Result<()> {
    let state = if plan.playing {
        Glyph::Play
    } else {
        Glyph::Pause
    };
    if let (Some(art), Some(bitmap)) = (plan.art, bitmap) {
        fill_art(context, &plan.cover, art.width, bitmap, plan.dim)?;
    } else {
        // SAFETY: context is live between surface BeginDraw/EndDraw; brushes are scoped to this draw.
        unsafe {
            let slot = context.CreateSolidColorBrush(&tint(p.track(COVER_FILL)), None)?;
            context.FillRoundedRectangle(&plan.cover, &slot);
        }
        glyphs::draw(
            context,
            plan.glyph,
            state,
            with_alpha(p.ink, p.ink_alpha(plan.dim)),
        )?;
    }
    if let Some(badge) = &plan.badge {
        // SAFETY: as above.
        unsafe {
            let circle = context.CreateSolidColorBrush(&with_alpha(p.on_ink, badge.alpha), None)?;
            context.FillRoundedRectangle(&badge.circle, &circle);
        }
        let color = with_alpha(p.ink, p.ink_alpha(plan.dim * badge.alpha));
        glyphs::draw(context, badge.glyph, state, color)?;
    }
    if let Some(([track, fill], alpha)) = &plan.bar {
        // SAFETY: as above.
        unsafe {
            draw_track(context, p, track, COVER_FILL * alpha)?;
            if fill.rect.right > fill.rect.left {
                let brush = context
                    .CreateSolidColorBrush(&with_alpha(p.accent, p.ink_alpha(*alpha)), None)?;
                context.FillRoundedRectangle(fill, &brush);
            }
        }
    }
    for line in &plan.lines {
        draw_text(
            context,
            &line.text,
            &line.font,
            line.rect,
            with_alpha(p.ink, p.ink_alpha(line.alpha)),
        )?;
    }
    for chip in &plan.chips {
        let color = tint(p.ink_dim(chip.disabled, chip.alpha));
        if chip.capsule {
            let radius = (chip.rect.bottom - chip.rect.top) / 2.0;
            // The primary play/pause is a solid accent capsule, the others a faint ink one.
            let fill = if chip.filled {
                with_alpha(p.accent, p.ink_alpha(chip.alpha))
            } else {
                with_alpha(p.ink, p.ink_alpha(0.12 * chip.alpha))
            };
            // SAFETY: as above.
            unsafe {
                let circle = context.CreateSolidColorBrush(&fill, None)?;
                context.FillRoundedRectangle(&rounded(chip.rect, radius), &circle);
            }
        }
        if chip.filled {
            glyphs::draw(
                context,
                chip.glyph,
                chip.shape,
                with_alpha(p.ink_on_accent(), p.ink_alpha(chip.alpha)),
            )?;
        } else if chip.lit {
            glyphs::draw(
                context,
                chip.glyph,
                chip.shape,
                with_alpha(p.accent, p.ink_alpha(chip.alpha)),
            )?;
        } else {
            glyphs::draw(context, chip.glyph, chip.shape, color)?;
        }
    }
    for chip in &plan.players {
        let fill = if chip.chosen {
            tint(p.accent_dim(false, 0.35 * chip.alpha))
        } else {
            with_alpha(p.ink, p.ink_alpha(0.12 * chip.alpha))
        };
        // SAFETY: as above.
        unsafe {
            let brush = context.CreateSolidColorBrush(&fill, None)?;
            let radius = (chip.rect.bottom - chip.rect.top) / 2.0;
            context.FillRoundedRectangle(&rounded(chip.rect, radius), &brush);
        }
        let text = D2D_RECT_F {
            left: chip.rect.left + 6.0,
            right: (chip.rect.right - 6.0).max(chip.rect.left + 6.0),
            ..chip.rect
        };
        let strength = if chip.chosen { 0.95 } else { 0.65 };
        draw_text(
            context,
            chip.name,
            &chip.font,
            text,
            with_alpha(p.ink, p.ink_alpha(strength * chip.alpha)),
        )?;
    }
    if let Some(bar) = &plan.volume {
        // SAFETY: as above.
        unsafe {
            draw_track(context, p, &bar.track, bar.track_alpha)?;
            let fill = context
                .CreateSolidColorBrush(&tint(p.accent_dim(bar.dim, bar.fill_alpha)), None)?;
            context.FillRoundedRectangle(&bar.fill, &fill);
            let (x, y, r) = bar.thumb;
            let thumb = context
                .CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(bar.fill_alpha)), None)?;
            let dot = D2D_RECT_F {
                left: x - r,
                top: y - r,
                right: x + r,
                bottom: y + r,
            };
            context.FillRoundedRectangle(&rounded(dot, r), &thumb);
        }
        if let Some((text, rect)) = &bar.readout {
            let font = Font {
                family: TEXT_FONT,
                size: (rect.bottom - rect.top) * 0.85,
                weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
                designator: 1.0,
                align: DWRITE_TEXT_ALIGNMENT_CENTER,
            };
            draw_text(
                context,
                text,
                &font,
                *rect,
                with_alpha(p.ink, p.ink_alpha(0.90)),
            )?;
        }
    }
    for (bars, alpha) in plan.meter.iter().chain(&plan.pill_meter) {
        // SAFETY: as above.
        unsafe {
            let brush =
                context.CreateSolidColorBrush(&with_alpha(p.ink, p.ink_alpha(*alpha)), None)?;
            for bar in bars {
                context.FillRoundedRectangle(bar, &brush);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
