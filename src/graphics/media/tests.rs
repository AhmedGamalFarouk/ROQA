use super::*;
use crate::layout::{NOTCH, SurfaceKind};
use crate::media::View;
use windows::Win32::Graphics::DirectWrite::DWRITE_TEXT_METRICS;

#[test]
fn art_uploads_once_per_entry_and_no_art_or_a_failed_upload_releases_it() {
    let art = IconPixels {
        width: 1,
        height: 1,
        pixels: vec![0; 4],
    };
    let mut slot = None;
    let mut uploads = 0;
    let mut paint = |slot: &mut Option<(u64, u32)>, entry: Option<u64>| {
        cached(slot, entry.map(|entry| (entry, &art)), |_| {
            uploads += 1;
            Ok(uploads)
        })
        .unwrap()
    };
    // Repaints of unchanged art (progress ticks, visualizer, clock) upload nothing more.
    for _ in 0..15 {
        assert_eq!(paint(&mut slot, Some(1)), Some(1));
    }
    // A new entry uploads once; no art releases it, and its return uploads again.
    assert_eq!(paint(&mut slot, Some(2)), Some(2));
    assert_eq!(paint(&mut slot, Some(2)), Some(2));
    assert_eq!(paint(&mut slot, None), None);
    assert_eq!(slot, None);
    assert_eq!(paint(&mut slot, Some(2)), Some(3));
    // A failed upload holds nothing, so the next paint retries.
    let failed = cached(&mut slot, Some((4, &art)), |_| Err::<u32, _>(E_FAIL.into()));
    assert!(failed.is_err());
    assert_eq!(slot, None);
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left,
        top,
        right,
        bottom,
    }
}
/// The media body as `Canvas::paint_notch` places it in a window-sized surface at (0, 0).
fn body(dpi: u32, progress: f32) -> D2D_RECT_F {
    let work = Bounds {
        x: 0,
        y: 0,
        width: 3840,
        height: 2160,
    };
    let window = layout::placement(work, dpi, SurfaceKind::Notch, 0).width;
    let pixels = layout::notch_body(window, dpi, layout::notch_shape(progress, true));
    rect(
        pixels.left as f32,
        0.0,
        (pixels.left + pixels.width) as f32,
        pixels.height as f32,
    )
}
fn track(title: &str, artist: &str, playing: bool) -> View {
    View::Track(Track {
        title: title.into(),
        artist: artist.into(),
        playing,
        ..Track::default()
    })
}
/// The clock's measured width, as `notch_text` passes it.
fn clock_width(time: &str, progress: f32, dpi: u32) -> f32 {
    let font = media_clock(motion::smooth(progress), dpi as f32 / 96.0);
    text_width(time, &font).unwrap()
}
/// The scene `Desktop` builds from a media view, planned as `notch_text` plans it.
fn plan<'a>(view: &'a View, time: &'a str, progress: f32, dpi: u32) -> MediaPlan<'a> {
    played(view, time, progress, dpi, None)
}
fn played<'a>(
    view: &'a View,
    time: &'a str,
    progress: f32,
    dpi: u32,
    played: Option<Progress>,
) -> MediaPlan<'a> {
    planned(view, time, progress, dpi, played, None)
}
fn volume<'a>(
    view: &'a View,
    time: &'a str,
    progress: f32,
    dpi: u32,
    volume: Option<Level>,
) -> MediaPlan<'a> {
    planned(view, time, progress, dpi, None, volume)
}
fn planned<'a>(
    view: &'a View,
    time: &'a str,
    progress: f32,
    dpi: u32,
    played: Option<Progress>,
    volume: Option<Level>,
) -> MediaPlan<'a> {
    let scene = NotchScene {
        time,
        date: "Tuesday",
        hijri: "",
        track: view.shown(),
        art: None,
        progress,
        dpi,
        played,
        volume,
        bars: [0; BARS],
        timer: None,
        reminders: &[],
        notice: None,
        extra: Extra::None,
        footer: None,
        shot: None,
        shelf: &[],
        shelf_undo: false,
        shelf_hover: false,
        clips: false,
        low_battery: false,
        tab: Tab::Media,
        rail: None,
        eyes: None,
        privacy: [0.0; 3],
        grip: Grip::default(),
        line_wash: 0.0,
        volume_fill: volume.map_or(0.0, |level| f32::from(level.percent)),
        weather: None,
        prayer: None,
        leaving: None,
        fx: Fx::REST,
    };
    let clock = clock_width(time, progress, dpi);
    media_plan(&scene, scene.track.unwrap(), &body(dpi, progress), clock)
}

#[test]
fn album_art_fills_the_cover_slot_and_only_the_pill_carries_a_badge() {
    let song = track("Song", "Artist", true);
    let art = IconPixels {
        width: 72,
        height: 72,
        pixels: vec![0; 72 * 72 * 4],
    };
    let planned = |progress| {
        let scene = NotchScene {
            time: "9:50 pm",
            date: "Tuesday",
            hijri: "",
            track: song.shown(),
            art: Some((1, &art)),
            progress,
            dpi: 96,
            played: None,
            volume: None,
            bars: [0; BARS],
            timer: None,
            reminders: &[],
            notice: None,
            extra: Extra::None,
            footer: None,
            shot: None,
            shelf: &[],
            shelf_undo: false,
            shelf_hover: false,
            clips: false,
            low_battery: false,
            tab: Tab::Media,
            rail: None,
            eyes: None,
            privacy: [0.0; 3],
            grip: Grip::default(),
            line_wash: 0.0,
            volume_fill: 0.0,
            weather: None,
            prayer: None,
            leaving: None,
            fx: Fx::REST,
        };
        let clock = clock_width("9:50 pm", progress, 96);
        let plan = media_plan(&scene, scene.track.unwrap(), &body(96, progress), clock);
        (plan.cover, plan.art.is_some(), plan.badge)
    };
    let (cover, drawn, badge) = planned(0.0);
    // The art takes exactly the no-art slot; nothing else moves.
    assert_eq!(cover, plan(&song, "9:50 pm", 0.0, 96).cover);
    assert!(drawn);
    // An 11 badge 4 past the cover's corner (99, 5)-(119, 25), with a centred 6 glyph.
    let badge = badge.unwrap();
    assert_eq!(badge.circle.rect, rect(112.0, 18.0, 123.0, 29.0));
    assert_eq!(badge.circle.radiusX, 5.5);
    assert_eq!(badge.glyph, rect(115.0, 21.0, 121.0, 27.0));
    assert_eq!(badge.alpha, 1.0);
    let (cover, drawn, badge) = planned(1.0);
    // The panel cover, 80 at (16, 24), has no badge: its play/pause control shows the state.
    let left = body(96, 1.0).left;
    assert_eq!(cover.rect, rect(left + 16.0, 24.0, left + 96.0, 104.0));
    assert!(drawn && badge.is_none());
    assert!(plan(&song, "9:50 pm", 0.0, 96).badge.is_none());
}
/// Text, rectangle, size and opacity of each drawn line.
fn lines<'a>(plan: &'a MediaPlan) -> Vec<(&'a str, D2D_RECT_F, f32, f32)> {
    plan.lines
        .iter()
        .map(|line| (line.text.as_ref(), line.rect, line.font.size, line.alpha))
        .collect()
}

#[test]
fn no_player_views_draw_the_plain_clock_pill() {
    for view in [View::NoPlayer, View::Permission, View::Error] {
        assert!(view.shown().is_none());
        assert_eq!(layout::notch_shape(0.0, view.shown().is_some()), NOTCH);
    }
}

#[test]
fn collapsed_playing_pill_draws_cover_slot_title_and_clock() {
    let song = track("Song", "Artist", true);
    let plan = plan(&song, "9:50 pm", 0.0, 96);
    // 262 x 30 body centred in the 440 DIP window: x 89..351.
    assert_eq!(plan.cover.rect, rect(99.0, 5.0, 119.0, 25.0));
    assert_eq!(plan.cover.radiusX, 5.0);
    assert_eq!(plan.glyph, rect(104.0, 10.0, 114.0, 20.0));
    assert!(plan.playing);
    assert_eq!(plan.dim, 1.0);
    // No title in the pill (and no elapsed time without a duration): the clock and the level bars.
    assert_eq!(
        lines(&plan),
        [("9:50 pm", rect(119.0, 3.0, 337.0, 27.0), 15.0, 1.0)]
    );
    assert!(plan.pill_meter.is_some());
    let clock = &plan.lines[0].font;
    assert_eq!(
        (clock.weight, clock.designator, clock.align),
        (
            DWRITE_FONT_WEIGHT_BOLD,
            TIME_DESIGNATOR,
            DWRITE_TEXT_ALIGNMENT_TRAILING
        )
    );
    // The pill draws no controls.
    assert!(plan.chips.is_empty());
}

#[test]
fn paused_and_pending_metadata_draw_the_plain_clock_shape() {
    for view in [track("Song", "Artist", false), track("", "", true)] {
        assert!(view.shown().is_none());
        assert_eq!(layout::notch_shape(0.0, view.shown().is_some()), NOTCH);
    }
}

#[test]
fn expanded_panel_places_cover_title_artist_and_clock_above_the_reserved_rows() {
    let song = track("Song", "Artist", true);
    let plan = plan(&song, "9:50 pm", 1.0, 96);
    // 360 x 128 body at x 40..400: cover 80 at (16, 24), title column from 108.
    assert_eq!(plan.cover.rect, rect(56.0, 24.0, 136.0, 104.0));
    assert_eq!(plan.cover.radiusX, 14.0);
    assert_eq!(plan.glyph, rect(84.0, 52.0, 108.0, 76.0));
    let title_end = 384.0 - clock_width("9:50 pm", 1.0, 96) - 6.0;
    assert_eq!(
        lines(&plan),
        [
            ("9:50 pm", rect(136.0, 7.0, 384.0, 31.0), 12.0, 0.60),
            ("Song", rect(148.0, 18.0, title_end, 42.0), 15.0, 1.0),
            // The artist lane ends before the visualizer slot.
            ("Artist", rect(148.0, 40.0, 344.0, 60.0), 12.0, 0.60),
        ]
    );
    assert_eq!(plan.lines[0].font.weight, DWRITE_FONT_WEIGHT_NORMAL);
    assert_eq!(plan.lines[1].font.weight, DWRITE_FONT_WEIGHT_SEMI_BOLD);
    // Text stays above the progress, controls and volume rows (from 63 DIP).
    assert!(plan.lines.iter().all(|line| line.rect.bottom <= 63.0));
}

#[test]
fn expanded_panel_draws_previous_play_pause_and_next_in_the_controls_row() {
    let mut song = track("Song", "Artist", true);
    if let View::Track(track) = &mut song {
        track.controls = [false, true, true];
    }
    let panel = plan(&song, "9:50 pm", 1.0, 96);
    // Body x 40..400; controls at 108, 146 and 200 DIP from its left, 84..112 from its top.
    let drawn: Vec<_> = panel
        .chips
        .iter()
        .map(|c| (c.rect, c.glyph, c.shape, c.filled, c.alpha))
        .collect();
    assert_eq!(
        drawn,
        [
            (
                rect(148.0, 84.0, 180.0, 112.0),
                rect(155.0, 89.0, 173.0, 107.0),
                Glyph::Previous,
                false,
                DISABLED
            ),
            (
                rect(186.0, 84.0, 234.0, 112.0),
                rect(202.0, 90.0, 218.0, 106.0),
                Glyph::Pause,
                true,
                1.0
            ),
            (
                rect(240.0, 84.0, 272.0, 112.0),
                rect(247.0, 89.0, 265.0, 107.0),
                Glyph::Next,
                false,
                1.0
            ),
            // Mute, disabled here: the scene has no audio endpoint.
            (
                rect(284.0, 88.0, 308.0, 108.0),
                rect(287.0, 89.0, 305.0, 107.0),
                Glyph::Speaker,
                false,
                DISABLED
            ),
        ]
    );
    assert!(track("Song", "Artist", false).shown().is_none());
    // The controls fade in with the artist and keep their place while the body grows.
    let opening = plan(&song, "9:50 pm", 0.6, 96);
    let reveal = motion::reveal(0.6);
    assert!(reveal > 0.0 && reveal < 1.0);
    let body = body(96, 0.6);
    assert_eq!(opening.chips[1].rect.left, body.left + 146.0);
    assert_eq!(opening.chips[1].alpha, reveal);
    assert_eq!(opening.chips[0].alpha, reveal * DISABLED);
}

#[test]
fn the_panel_draws_elapsed_bar_and_total_between_the_text_and_the_controls() {
    let song = track("Song", "Artist", true);
    let row = Progress {
        elapsed: 83,
        total: 296,
        filled: 41,
    };
    let panel = played(&song, "9:50 pm", 1.0, 96, Some(row));
    // Body x 40..400: elapsed from 108, bar 148..304 centred on y 71, total trailing at 344.
    let ([track, fill], alpha) = panel.bar.expect("a bar");
    assert_eq!(track.rect, rect(188.0, 68.0, 344.0, 74.0));
    assert_eq!(fill.rect, rect(188.0, 68.0, 229.0, 74.0));
    assert_eq!((track.radiusX, alpha), (3.0, 1.0));
    assert_eq!(
        lines(&panel)[3..],
        [
            ("1:23", rect(148.0, 63.0, 184.0, 79.0), 11.0, 0.60),
            ("-3:33", rect(348.0, 63.0, 384.0, 79.0), 11.0, 0.60),
        ]
    );
    let total = &panel.lines[4].font;
    assert_eq!(total.align, DWRITE_TEXT_ALIGNMENT_TRAILING);
    // Above the controls row and inside the body at every scale.
    assert!(panel.chips.iter().all(|chip| chip.rect.top >= 79.0));
    for dpi in [144, 192] {
        let panel = played(&song, "9:50 pm", 1.0, dpi, Some(row));
        let body = body(dpi, 1.0);
        let ([track, _], _) = panel.bar.unwrap();
        for item in panel.lines.iter().map(|l| l.rect).chain([track.rect]) {
            assert!(item.left >= body.left && item.right <= body.right, "{dpi}");
        }
    }
    // It fades in with the controls; collapsed or without a row, nothing is drawn.
    let opening = played(&song, "9:50 pm", 0.6, 96, Some(row));
    assert_eq!(opening.bar.unwrap().1, motion::reveal(0.6));
    assert!(played(&song, "9:50 pm", 0.0, 96, Some(row)).bar.is_none());
    let plain = plan(&song, "9:50 pm", 1.0, 96);
    assert!(plain.bar.is_none() && plain.lines.len() == 3);
}

#[test]
fn controls_stay_inside_the_panel_body_at_any_scale() {
    let song = track("Song", "Artist", true);
    for dpi in [96, 144, 192] {
        let plan = plan(&song, "9:50 pm", 1.0, dpi);
        let body = body(dpi, 1.0);
        for item in plan.chips.iter().map(|c| c.rect) {
            assert!(item.left >= body.left && item.right <= body.right, "{dpi}");
            assert!(item.top >= body.top && item.bottom <= body.bottom, "{dpi}");
        }
    }
}

#[test]
fn meter_bars_sit_right_of_the_controls_bottom_aligned_and_fade_in_with_them() {
    let song = track("Song", "Artist", true);
    let meter = |progress| {
        let scene = NotchScene {
            time: "9:50 pm",
            date: "Tuesday",
            hijri: "",
            track: song.shown(),
            art: None,
            progress,
            dpi: 96,
            played: None,
            volume: None,
            bars: [4, 20, 12, 8, 4],
            timer: None,
            reminders: &[],
            notice: None,
            extra: Extra::None,
            footer: None,
            shot: None,
            shelf: &[],
            shelf_undo: false,
            shelf_hover: false,
            clips: false,
            low_battery: false,
            tab: Tab::Media,
            rail: None,
            eyes: None,
            privacy: [0.0; 3],
            grip: Grip::default(),
            line_wash: 0.0,
            volume_fill: 0.0,
            weather: None,
            prayer: None,
            leaving: None,
            fx: Fx::REST,
        };
        if progress == 0.0 {
            // The collapsed pill's bars follow the same live level: 3 DIP flat to 12 DIP full.
            let clock = clock_width("9:50 pm", 0.0, 96);
            let plan = media_plan(&scene, scene.track.unwrap(), &body(96, 0.0), clock);
            let heights = plan
                .pill_meter
                .unwrap()
                .0
                .map(|bar| bar.rect.bottom - bar.rect.top);
            assert_eq!(heights, [3.0, 12.0, 8.0, 5.0, 3.0]);
        }
        meter_bars(&scene, &body(96, progress))
    };
    let body = body(96, 1.0);
    let (bars, alpha) = meter(1.0).unwrap();
    assert_eq!(alpha, METER_FILL);
    let lefts = bars.map(|bar| bar.rect.left - body.left);
    assert_eq!(lefts, [312.0, 319.0, 326.0, 333.0, 340.0]);
    assert_eq!(bars[4].rect.right - body.left, 344.0);
    assert!(bars.iter().all(|bar| bar.rect.bottom - body.top == 52.0));
    assert_eq!(bars[1].rect.top - body.top, 32.0);
    assert_eq!(bars[0].rect.bottom - bars[0].rect.top, 4.0);
    assert_eq!(meter(0.6).unwrap().1, METER_FILL * motion::reveal(0.6));
    assert!(meter(0.0).is_none());
}

#[test]
fn the_meter_update_rect_holds_every_bar_at_any_height_and_scale() {
    let song = track("Song", "Artist", true);
    for dpi in [96, 144, 192] {
        let window = layout::placement(
            Bounds {
                x: 0,
                y: 0,
                width: 3840,
                height: 2160,
            },
            dpi,
            SurfaceKind::Notch,
            0,
        )
        .width;
        let update = meter_update(window, dpi);
        let (slot, _) = layout::meter(dpi);
        for peak in [0.0, 0.5, 1.0] {
            let heights = layout::meter_heights([peak; BARS], dpi);
            let scene = NotchScene {
                time: "9:50 pm",
                date: "Tuesday",
                hijri: "",
                track: song.shown(),
                art: None,
                progress: 1.0,
                dpi,
                played: None,
                volume: None,
                bars: heights,
                timer: None,
                reminders: &[],
                notice: None,
                extra: Extra::None,
                footer: None,
                shot: None,
                shelf: &[],
                shelf_undo: false,
                shelf_hover: false,
                clips: false,
                low_battery: false,
                tab: Tab::Media,
                rail: None,
                eyes: None,
                privacy: [0.0; 3],
                grip: Grip::default(),
                line_wash: 0.0,
                volume_fill: 0.0,
                weather: None,
                prayer: None,
                leaving: None,
                fx: Fx::REST,
            };
            let (bars, _) = meter_bars(&scene, &body(dpi, 1.0)).unwrap();
            for bar in bars {
                let (left, top) = (update.left as f32, update.top as f32);
                assert!(bar.rect.left >= left && bar.rect.right <= update.right as f32);
                assert!(bar.rect.top >= top && bar.rect.bottom <= update.bottom as f32);
            }
        }
        assert_eq!(update.bottom - update.top, slot.height);
    }
}

#[test]
fn the_speaker_follows_the_level_and_the_thumb_grows_with_the_pointer() {
    assert_eq!(Glyph::speaker(0, false), Glyph::Muted);
    assert_eq!(Glyph::speaker(70, true), Glyph::Muted);
    assert_eq!(Glyph::speaker(1, false), Glyph::SpeakerLow);
    assert_eq!(Glyph::speaker(33, false), Glyph::SpeakerLow);
    assert_eq!(Glyph::speaker(34, false), Glyph::Speaker);
    assert_eq!(Glyph::speaker(100, false), Glyph::Speaker);
    let song = track("Song", "Artist", true);
    let mut scene = NotchScene {
        time: "9:50 pm",
        date: "Tuesday",
        hijri: "",
        track: song.shown(),
        art: None,
        progress: 1.0,
        dpi: 96,
        played: None,
        volume: Some(Level {
            percent: 40,
            muted: false,
        }),
        bars: [0; BARS],
        timer: None,
        reminders: &[],
        notice: None,
        extra: Extra::None,
        footer: None,
        shot: None,
        shelf: &[],
        shelf_undo: false,
        shelf_hover: false,
        clips: false,
        low_battery: false,
        tab: Tab::Media,
        rail: None,
        eyes: None,
        privacy: [0.0; 3],
        grip: Grip::default(),
        line_wash: 0.0,
        volume_fill: 0.0,
        weather: None,
        prayer: None,
        leaving: None,
        fx: Fx::REST,
    };
    let radius = |scene: &NotchScene| volume_bar(scene, &body(96, 1.0)).unwrap().thumb.2;
    let rest = radius(&scene);
    scene.grip = Grip {
        hover: true,
        drag: false,
    };
    let hover = radius(&scene);
    scene.grip = Grip {
        hover: true,
        drag: true,
    };
    let press = radius(&scene);
    assert!(rest < hover && hover < press);
    let readout = volume_bar(&scene, &body(96, 1.0)).unwrap().readout;
    assert_eq!(readout.unwrap().0, "40%", "a press shows the percent");
}

#[test]
fn volume_row_fills_to_the_shown_percent_and_dims_while_muted_or_unavailable() {
    let song = track("Song", "Artist", true);
    let level = |percent, muted| Some(Level { percent, muted });
    let panel = volume(&song, "9:50 pm", 1.0, 96, level(40, false));
    let bar = panel.volume.unwrap();
    // Bar x 312..358 in the body at 40; 4 DIP thick, centred in 88..108; the thumb sits on the fill end.
    assert_eq!(bar.track.rect, rect(312.0, 96.0, 358.0, 100.0));
    assert_eq!(bar.fill.rect, rect(312.0, 96.0, 312.0 + 18.0, 100.0));
    assert_eq!(bar.thumb, (330.0, 98.0, 4.5));
    assert!(bar.readout.is_none());
    assert_eq!((bar.track_alpha, bar.fill_alpha), (VOLUME_TRACK, 1.0));
    assert_eq!(
        (panel.chips[3].shape, panel.chips[3].alpha),
        (Glyph::Speaker, 1.0)
    );
    let muted = volume(&song, "9:50 pm", 1.0, 96, level(40, true));
    assert_eq!(muted.chips[3].shape, Glyph::Muted);
    assert_eq!(muted.chips[3].alpha, 1.0); // Still a live toggle.
    assert_eq!(muted.volume.unwrap().fill_alpha, MUTED_FILL);
    let unavailable = volume(&song, "9:50 pm", 1.0, 96, None);
    let bar = unavailable.volume.unwrap();
    assert_eq!((bar.fill.rect.right, bar.fill_alpha), (312.0, DISABLED));
    // The row is not drawn while collapsed.
    assert!(
        volume(&song, "9:50 pm", 0.0, 96, level(40, false))
            .volume
            .is_none()
    );
    let full = volume(&song, "9:50 pm", 1.0, 192, level(100, false))
        .volume
        .unwrap();
    assert_eq!(full.fill.rect.right, full.track.rect.right);
    assert!(full.track.rect.bottom <= body(192, 1.0).bottom);
}

#[test]
fn empty_artist_and_title_are_not_laid_out() {
    let texts = |view: &View| -> Vec<String> {
        let plan = plan(view, "9:50 pm", 1.0, 96);
        plan.lines.iter().map(|l| l.text.to_string()).collect()
    };
    assert_eq!(texts(&track("Song", "", true)), ["9:50 pm", "Song"]);
    assert_eq!(texts(&track("", "Artist", true)), ["9:50 pm", "Artist"]);
}

#[test]
fn media_rows_scale_and_stay_inside_the_body_at_100_150_and_200_percent() {
    let song = track("Song", "Artist", true);
    for dpi in [96, 144, 192] {
        let scale = dpi as f32 / 96.0;
        for progress in [0.5, 1.0] {
            let body = body(dpi, progress);
            let open = motion::smooth(progress);
            let plan = plan(&song, "9:50 pm", progress, dpi);
            let (clock, title) = (plan.lines[0].rect, plan.lines[1].rect);
            assert_eq!(clock.right, body.right - (14.0 + 2.0 * open) * scale);
            assert_eq!(title.left, body.left + (38.0 + 70.0 * open) * scale);
            assert_eq!(plan.lines[1].font.size, (12.0 + 3.0 * open) * scale);
            let cover = plan.cover.rect;
            assert_eq!(cover.right - cover.left, (20.0 + 60.0 * open) * scale);
            for item in plan.lines.iter().map(|l| l.rect).chain([cover]) {
                assert!(item.left >= body.left && item.right <= body.right, "{dpi}");
                assert!(item.top >= body.top && item.bottom <= body.bottom, "{dpi}");
            }
        }
    }
}

#[test]
fn the_title_column_ends_before_the_measured_clock_and_never_inverts() {
    let song = track("Song", "", true);
    // English, Korean, German, Tamil and Malayalam afternoon formats, then one no locale has.
    let long = "9:50 ante meridiem ante meridiem ante meridiem ante meridiem";
    for time in [
        "9:50 pm",
        "\u{c624}\u{d6c4} 9:50",
        "9:50 nachm.",
        "\u{baa}\u{bbf}\u{bb1}\u{bcd}\u{baa}\u{b95}\u{bb2}\u{bcd} 9:50",
        "\u{d09}\u{d1a}\u{d4d}\u{d1a}\u{d24}\u{d3f}\u{d30}\u{d3f}\u{d1e}\u{d4d}\u{d1e}\u{d4d} 9:50",
        long,
    ] {
        for dpi in [96, 144, 192] {
            let scale = dpi as f32 / 96.0;
            for progress in [0.5, 0.75, 1.0] {
                let clock = clock_width(time, progress, dpi);
                let plan = plan(&song, time, progress, dpi);
                let title = plan.lines[1].rect;
                assert!(title.left >= plan.cover.rect.right, "{time} {dpi}");
                assert!(title.right >= title.left, "{time} {dpi} {progress}");
                let gap = (8.0 - 2.0 * motion::smooth(progress)) * scale;
                if time != long {
                    // Real formats leave a title column.
                    assert!(title.right - title.left > 40.0 * scale, "{time} {dpi}");
                    assert_eq!(title.right, plan.lines[0].rect.right - clock - gap);
                }
            }
        }
    }
    // A clock wider than the row leaves an empty, not inverted, title column.
    let plan = plan(&song, long, 0.5, 96);
    assert_eq!(plan.lines[1].rect.right, plan.lines[1].rect.left);
}

fn metrics(text: &str, align: DWRITE_TEXT_ALIGNMENT, width: f32) -> (DWRITE_TEXT_METRICS, bool) {
    let font = Font {
        family: TEXT_FONT,
        size: 13.0,
        weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
        designator: 1.0,
        align,
    };
    let layout = text_layout(text, &font, width, 24.0).unwrap();
    let mut metrics = DWRITE_TEXT_METRICS::default();
    // SAFETY: a local layout and a valid output struct.
    unsafe {
        layout.GetMetrics(&mut metrics).unwrap();
        let rtl = layout.GetReadingDirection() == DWRITE_READING_DIRECTION_RIGHT_TO_LEFT;
        (metrics, rtl)
    }
}

#[test]
fn baked_glass_is_built_once_and_again_only_for_a_new_key_or_after_a_failure() {
    let (mut slot, mut builds) = (None, 0);
    let mut get = |key: u32, fail: bool| {
        baked(&mut slot, key, || {
            builds += 1;
            if fail {
                Err(Error::new(E_FAIL, "build"))
            } else {
                Ok(key * 10)
            }
        })
    };
    // Repaints and resizes ask with the same key: one build.
    assert_eq!(
        (
            get(1, false).unwrap(),
            get(1, false).unwrap(),
            get(1, true).unwrap()
        ),
        (10, 10, 10)
    );
    // A new key (another theme's sheen) rebuilds; a failed rebuild keeps the old value and retries.
    assert!(get(2, true).is_err());
    assert_eq!(get(2, false).unwrap(), 20);
    assert_eq!(get(2, false).unwrap(), 20);
    assert_eq!(builds, 3);
}

#[test]
fn long_titles_trim_to_one_line_within_their_column() {
    // About the panel's title column beside a short clock (mockup: 190 DIP).
    let column = 190.0;
    for long in [
        "An Extremely Long Song Title That Cannot Possibly Fit In The Notch (Remastered 2026)",
        "\u{623}\u{63a}\u{646}\u{64a}\u{629} \u{637}\u{648}\u{64a}\u{644}\u{629} \u{62c}\u{62f}\u{627}\u{64b} \u{644}\u{627} \u{62a}\u{62a}\u{633}\u{639} \u{641}\u{64a} \u{627}\u{644}\u{634}\u{631}\u{64a}\u{637} \u{627}\u{644}\u{639}\u{644}\u{648}\u{64a} \u{645}\u{647}\u{645}\u{627} \u{62d}\u{627}\u{648}\u{644}\u{646}\u{627}",
    ] {
        let (natural, _) = metrics(long, DWRITE_TEXT_ALIGNMENT_LEADING, 10_000.0);
        assert!(natural.width > column, "the sample must overflow");
        let (trimmed, _) = metrics(long, DWRITE_TEXT_ALIGNMENT_LEADING, column);
        assert_eq!(trimmed.lineCount, 1);
        assert!(trimmed.left >= 0.0 && trimmed.left + trimmed.width <= column);
    }
}

#[test]
fn rtl_titles_read_right_to_left_from_the_columns_right_edge() {
    let arabic = "\u{623}\u{63a}\u{646}\u{64a}\u{629}";
    let (metrics_rtl, rtl) = metrics(arabic, DWRITE_TEXT_ALIGNMENT_LEADING, 190.0);
    assert!(rtl);
    assert!(
        metrics_rtl.left > 100.0,
        "starts at the right: {}",
        metrics_rtl.left
    );
    let (metrics_ltr, rtl) = metrics("Song", DWRITE_TEXT_ALIGNMENT_LEADING, 190.0);
    assert!(!rtl);
    assert!(metrics_ltr.left < 1.0);
    // The accepted centred date and trailing clock keep their left-to-right paragraph.
    for align in [DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_TRAILING] {
        assert!(!metrics(arabic, align, 190.0).1);
    }
}

#[test]
fn reading_direction_follows_the_first_letter() {
    assert!(right_to_left("\u{623}\u{63a}\u{646}\u{64a}\u{629} - Song"));
    assert!(right_to_left("\u{5e9}\u{5d9}\u{5e8}"));
    // Digits, spaces and symbols before the first letter are skipped.
    assert!(right_to_left(
        "2026 \u{1F600} \u{623}\u{63a}\u{646}\u{64a}\u{629}"
    ));
    assert!(!right_to_left("Song - \u{623}\u{63a}\u{646}\u{64a}\u{629}"));
    assert!(!right_to_left("\u{661}\u{662}"));
    assert!(!right_to_left(""));
}
