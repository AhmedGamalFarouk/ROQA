//! Direct2D drawing for the notch tabs and the mascot (features tabs-eyes): the hanging handle with its five
//! icons, the pages other than Media, the opacity layer that fades the pill out as a page fades in, and the
//! squash-capsule eyes. The icons are `glyphs.rs`, drawn in the theme and accent colours. Nothing here runs unless the host puts a rail, a page or eyes in the scene.

use super::*;
use crate::{
    eyes::{self, Eye},
    layout::NotchBody,
    tabs::Tab,
};
use windows::Win32::Graphics::Direct2D::{D2D1_LAYER_OPTIONS1_NONE, D2D1_LAYER_PARAMETERS1};

/// Draws `draw` at `opacity` (a layer), or directly at full opacity. The layer exists only for the open and
/// close frames, where the pill and a page cross-fade.
pub(super) fn faded(
    context: &ID2D1DeviceContext,
    opacity: f32,
    draw: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if opacity >= 1.0 {
        return draw();
    }
    if opacity <= 0.0 {
        return Ok(());
    }
    let parameters = D2D1_LAYER_PARAMETERS1 {
        contentBounds: D2D_RECT_F {
            left: f32::MIN,
            top: f32::MIN,
            right: f32::MAX,
            bottom: f32::MAX,
        },
        geometricMask: ManuallyDrop::new(None),
        maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        maskTransform: Matrix3x2::identity(),
        opacity,
        opacityBrush: ManuallyDrop::new(None),
        layerOptions: D2D1_LAYER_OPTIONS1_NONE,
    };
    // SAFETY: context is live between surface BeginDraw/EndDraw; the layer is popped below on the same
    // context, whatever `draw` returns, and the parameters hold no COM references.
    unsafe { context.PushLayer(&parameters, None) };
    let drawn = draw();
    // SAFETY: balances the PushLayer above.
    unsafe { context.PopLayer() };
    drawn
}

/// The compact dot rail draws below the body, with the same capsule silhouette.
pub(super) fn rail(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    handle: &RailScene,
    surface: D2D_RECT_F,
    p: &Palette,
) -> Result<()> {
    let scale = scene.dpi as f32 / 96.0;
    let width = surface.right - surface.left;
    let media = scene.track.is_some();
    let Some(hang) = layout::rail_bounds(
        width as i32,
        scene.dpi,
        scene.progress,
        media,
        handle.layout,
    ) else {
        return Ok(());
    };
    let shape = D2D_RECT_F {
        left: surface.left + hang.x as f32,
        top: surface.top + hang.y as f32,
        right: surface.left + (hang.x + hang.width) as f32,
        bottom: surface.top + (hang.y + hang.height) as f32,
    };
    let ears = (layout::RAIL_EARS * scale).round().min(hang.height as f32);
    let radius = layout::rail_radius(
        NotchBody {
            left: hang.x,
            width: hang.width,
            height: hang.height,
            ears: ears as i32,
        },
        scene.dpi,
    );
    let outline = notch_outline(context, &shape, radius, ears)?;
    // SAFETY: context is live between surface BeginDraw/EndDraw; brushes and geometry are scoped to this draw.
    unsafe {
        // The names sit on this fill, so a glass theme backs it fully opaquely (no backdrop text doubles).
        let Rgba(red, green, blue, alpha) = p.body;
        let alpha = if p.rim.is_some() { 1.0 } else { alpha };
        let fill = context.CreateSolidColorBrush(&color(Rgba(red, green, blue, alpha)), None)?;
        context.FillGeometry(&outline, &fill, None);
        // While the handle grows out of the panel its items stay inside it.
        context.PushAxisAlignedClip(&shape, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
    }
    let drawn = items(context, scene, handle, hang, surface, p);
    // SAFETY: balances the PushAxisAlignedClip above on the same live context.
    unsafe { context.PopAxisAlignedClip() };
    drawn
}

fn rail_font(scale: f32) -> Font {
    Font {
        family: TEXT_FONT,
        size: 10.5 * scale,
        weight: DWRITE_FONT_WEIGHT_SEMI_BOLD,
        designator: 1.0,
        align: DWRITE_TEXT_ALIGNMENT_CENTER,
    }
}

/// A cell scaled about its centre (the indicator's squash, the press).
fn squashed(cell: D2D_RECT_F, (sx, sy): (f32, f32)) -> D2D_RECT_F {
    let (mx, my) = (
        (cell.left + cell.right) / 2.0,
        (cell.top + cell.bottom) / 2.0,
    );
    let (hx, hy) = (
        (cell.right - cell.left) / 2.0 * sx,
        (cell.bottom - cell.top) / 2.0 * sy,
    );
    D2D_RECT_F {
        left: mx - hx,
        top: my - hy,
        right: mx + hx,
        bottom: my + hy,
    }
}

/// Quiet dots at rest; the hovered dot lends room to its glyph and name.
fn items(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    handle: &RailScene,
    hang: Bounds,
    surface: D2D_RECT_F,
    p: &Palette,
) -> Result<()> {
    let scale = scene.dpi as f32 / 96.0;
    let reveal = motion::reveal(scene.progress);
    let rect = |item: Bounds| D2D_RECT_F {
        left: surface.left + item.x as f32,
        top: surface.top + item.y as f32,
        right: surface.left + (item.x + item.width) as f32,
        bottom: surface.top + (item.y + item.height) as f32,
    };
    let capsule = |cell: D2D_RECT_F, colour: D2D1_COLOR_F| -> Result<()> {
        // SAFETY: the brush and rounded fill belong to the live frame context.
        unsafe {
            let brush = context.CreateSolidColorBrush(&colour, None)?;
            context.FillRoundedRectangle(&rounded(cell, (cell.bottom - cell.top) / 2.0), &brush);
        }
        Ok(())
    };
    for (index, &tab) in handle.tabs.iter().enumerate() {
        let cell = rect(layout::rail_item(hang, scene.dpi, handle.layout, index));
        let expansion = handle.layout.expansion[index].clamp(0.0, 1.0);
        let active = index == handle.layout.active;
        capsule(
            cell,
            with_alpha(p.ink, p.ink_alpha((0.32 - 0.22 * expansion) * reveal)),
        )?;
        if expansion <= 0.65 {
            continue;
        }
        let alpha = motion::smooth((expansion - 0.65) / 0.35) * reveal;
        let ink = with_alpha(
            if active { p.rail_selected_ink() } else { p.ink },
            p.ink_alpha(alpha),
        );
        // A horizontal glyph/name pair keeps the expanded capsule only 24 DIP high.
        glyphs::draw(
            context,
            pixel_square(
                cell.left + 8.0 * scale,
                cell.top + (cell.bottom - cell.top - 16.0 * scale) / 2.0,
                16.0 * scale,
            ),
            Glyph::tab(tab),
            ink,
        )?;
        draw_text(
            context,
            tab.name(),
            &rail_font(scale),
            D2D_RECT_F {
                left: cell.left + 28.0 * scale,
                right: cell.right - 6.0 * scale,
                ..cell
            },
            with_alpha(p.ink, p.ink_alpha(alpha)),
        )?;
    }
    // The accent indicator travels through the current moving centres on the existing slide clock.
    let mut indicator = rect(layout::rail_slot(
        hang,
        scene.dpi,
        handle.layout,
        handle.ind,
    ));
    if indicator.bottom - indicator.top > 9.0 * scale {
        indicator.top = indicator.bottom - 3.0 * scale;
        let inset = (10.0 * scale).min((indicator.right - indicator.left) / 4.0);
        indicator.left += inset;
        indicator.right -= inset;
    }
    let press = 1.0 - 0.07 * handle.press;
    capsule(
        squashed(indicator, ((1.06 - 0.06 * handle.pop) * press, press)),
        with_alpha(p.accent, p.ink_alpha(reveal)),
    )
}

/// What a page draws with: the frame's context, the body's corner, the scale, the page's reveal and the palette.
/// Everything is placed in DIPs from the body's corner on the shared grid (`layout`: 16 margins, the header row, the
/// content from `BODY_Y`, 8 gutters, 24 high chips, 14 radius tiles), so no page measures its own padding.
struct Page<'a> {
    context: &'a ID2D1DeviceContext,
    body: &'a D2D_RECT_F,
    scale: f32,
    reveal: f32,
    dpi: u32,
    p: &'a Palette,
}

const LEAD: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_LEADING;
const MIDDLE: DWRITE_TEXT_ALIGNMENT = DWRITE_TEXT_ALIGNMENT_CENTER;
const NORMAL: DWRITE_FONT_WEIGHT = DWRITE_FONT_WEIGHT_NORMAL;
const SEMI: DWRITE_FONT_WEIGHT = DWRITE_FONT_WEIGHT_SEMI_BOLD;
const BOLD: DWRITE_FONT_WEIGHT = DWRITE_FONT_WEIGHT_BOLD;

impl Page<'_> {
    fn ink(&self, alpha: f32) -> D2D1_COLOR_F {
        with_alpha(self.p.ink, self.p.ink_alpha(alpha * self.reveal))
    }
    fn accent(&self, alpha: f32) -> D2D1_COLOR_F {
        with_alpha(self.p.accent, self.p.ink_alpha(alpha * self.reveal))
    }
    fn on_accent(&self) -> D2D1_COLOR_F {
        with_alpha(self.p.ink_on_accent(), self.p.ink_alpha(self.reveal))
    }
    /// A rect from DIPs off the body's corner.
    fn at(&self, x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.body.left + x * self.scale,
            top: self.body.top + y * self.scale,
            right: self.body.left + (x + w) * self.scale,
            bottom: self.body.top + (y + h) * self.scale,
        }
    }
    /// A layout slot, already in pixels from the body's corner.
    fn slot(&self, slot: Bounds) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.body.left + slot.x as f32,
            top: self.body.top + slot.y as f32,
            right: self.body.left + (slot.x + slot.width) as f32,
            bottom: self.body.top + (slot.y + slot.height) as f32,
        }
    }
    fn font(&self, size: f32, weight: DWRITE_FONT_WEIGHT, align: DWRITE_TEXT_ALIGNMENT) -> Font {
        Font {
            family: TEXT_FONT,
            size: size * self.scale,
            weight,
            designator: 1.0,
            align,
        }
    }
    /// Display face, tabular digits first (the time's am/pm is smaller).
    fn display(&self, size: f32, weight: DWRITE_FONT_WEIGHT, align: DWRITE_TEXT_ALIGNMENT) -> Font {
        Font {
            family: DISPLAY_FONT,
            designator: TIME_DESIGNATOR,
            ..self.font(size, weight, align)
        }
    }
    fn text(&self, text: &str, font: &Font, rect: D2D_RECT_F, colour: D2D1_COLOR_F) -> Result<()> {
        draw_text(self.context, text, font, rect, colour)
    }
    /// A filled rounded rect, a capsule without a radius (in DIPs).
    fn fill(&self, rect: D2D_RECT_F, radius: Option<f32>, colour: D2D1_COLOR_F) -> Result<()> {
        let radius = radius.map_or((rect.bottom - rect.top) / 2.0, |dips| dips * self.scale);
        // SAFETY: a brush and a fill on the frame's own device context.
        unsafe {
            let brush = self.context.CreateSolidColorBrush(&colour, None)?;
            self.context
                .FillRoundedRectangle(&rounded(rect, radius), &brush);
        }
        Ok(())
    }
    /// A soft tile on the grid's 14 radius.
    fn tile(&self, rect: D2D_RECT_F) -> Result<()> {
        self.fill(rect, Some(layout::TILE_RADIUS), self.ink(0.07))
    }
    /// A page button: a filled capsule. The one primary action of a page is a solid accent capsule with dark ink;
    /// the others are a faint ink fill with ink text.
    fn chip(&self, rect: D2D_RECT_F, label: &str) -> Result<()> {
        // Presets (Focus among them) are equal choices; only a running timer's Pause or Resume leads.
        let primary = matches!(label, "Pause" | "Resume");
        let (fill, ink) = if primary {
            (self.accent(1.0), self.on_accent())
        } else if label == "Focus" {
            // The Pomodoro preset reads apart from the plain times by its ink, not as a call to action.
            (self.ink(0.14), self.accent(1.0))
        } else {
            (self.ink(0.14), self.ink(0.9))
        };
        self.fill(rect, None, fill)?;
        self.text(label, &self.font(11.0, SEMI, MIDDLE), rect, ink)
    }
    /// A page's one-line message, with the "Set city" chip under it when the page needs a place.
    fn message(&self, text: &str, set_city: bool) -> Result<()> {
        self.text(
            text,
            &self.font(12.0, NORMAL, LEAD),
            self.at(layout::PAD, layout::BODY_Y, 328.0, 18.0),
            self.ink(0.85),
        )?;
        if set_city {
            self.chip(self.slot(layout::page_button(0, 0, self.dpi)), "Set city")?;
        }
        Ok(())
    }
    /// The footer line, unless the power notice has its row.
    fn footer(&self, scene: &NotchScene, text: &str, alpha: f32) -> Result<()> {
        if scene.notice.is_some() || text.is_empty() {
            return Ok(());
        }
        let (x, y, w, h) = layout::footer(scene.track.is_some());
        self.text(
            text,
            &self.font(FOOTER_SIZE, NORMAL, LEAD),
            self.at(x, y, w, h),
            self.ink(alpha),
        )
    }
}

/// A page other than Media (`tab`, and whether it is the one leaving a switch): the shared header (title, the
/// mascot's resting eyes, the tune disc at the top right) and its content, all at `reveal` so they arrive as the
/// pill's content leaves.
pub(super) fn page(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    (tab, leaving): (Tab, bool),
    body: &D2D_RECT_F,
    scale: f32,
    reveal: f32,
    p: &Palette,
) -> Result<()> {
    if reveal <= 0.0 {
        return Ok(());
    }
    let g = Page {
        context,
        body,
        scale,
        reveal,
        dpi: scene.dpi,
        p,
    };
    // A tab switch slides the page in from the side it came from: the title and clock first, the content a
    // beat later (the stagger).
    let slide = |lag: f32| {
        let t = ((scene.fx.page - lag) / (1.0 - lag)).clamp(0.0, 1.0);
        // The page that leaves slides out the other way, as the new one comes in.
        let offset = if leaving {
            -scene.fx.dir * 18.0 * scale * motion::smooth(t)
        } else {
            scene.fx.dir * 18.0 * scale * (1.0 - motion::smooth(t))
        };
        // SAFETY: a transform on the frame's live context; the notch resets it after the page.
        unsafe { context.SetTransform(&Matrix3x2::translation(offset, 0.0)) };
    };
    slide(0.0);
    let title = match scene.weather.filter(|_| tab == Tab::Weather) {
        Some(view) => view.title.as_str(),
        None => match scene
            .timer
            .filter(|view| tab == Tab::Timer && !view.label.is_empty())
        {
            Some(view) => view.label.as_str(),
            None => tab.name(),
        },
    };
    if tab == Tab::Shelf {
        shelf_switch(&g, scene)?;
    } else {
        g.text(
            title,
            &g.font(13.0, SEMI, LEAD),
            g.slot(layout::page_title(scene.dpi)),
            g.ink(1.0),
        )?;
    }
    // The mascot keeps a small watch in the header's middle, leaning to a running countdown.
    if let Some(frame) = scene.eyes {
        let toward =
            if tab == Tab::Timer && scene.timer.is_some_and(|view| !view.done && !view.paused) {
                -5.0
            } else {
                0.0
            };
        let middle = (body.right - body.left) / scale / 2.0;
        let eyes = (middle, layout::HEAD_Y + layout::HEAD_EYES.1 / 2.0, 0.55);
        panel_eyes(context, body, scale, reveal, frame, (eyes, toward), p)?;
    }
    slide(0.25);
    match tab {
        Tab::Timer => timer_page(&g, scene),
        Tab::Shelf => shelf_page(&g, scene),
        Tab::Weather => weather_page(&g, scene),
        Tab::Prayer => prayer_page(&g, scene),
        Tab::Widgets => widgets_page(&g, scene),
        _ => Ok(()),
    }
}

/// "25 min", "1 h 30 min": a countdown's full length for the caption under its capsule.
fn length(total: u64) -> String {
    match total {
        0..60 => format!("{total} s"),
        60..3600 => format!("{} min", total.div_ceil(60)),
        _ if total % 3600 < 60 => format!("{} h", total / 3600),
        _ => format!("{} h {} min", total / 3600, total % 3600 / 60),
    }
}

/// The Timer page. Running or paused: the digits at the left, the controls right of them, the progress capsule
/// across the grid (a Pomodoro draws its round as segments, focus long, breaks short, the last one the long
/// break) and the countdown's full length under it. Idle or just finished: one line and the presets as capsules.
fn timer_page(g: &Page, scene: &NotchScene) -> Result<()> {
    let Some(view) = scene.timer.filter(|view| !view.done) else {
        // Idle, or the last one just finished: the preset buttons start the next.
        let line = if scene.timer.is_some() {
            "Done. Rest a little."
        } else {
            "Pick a time"
        };
        g.text(
            line,
            &g.display(15.0, SEMI, LEAD),
            g.at(layout::PAD, layout::BODY_Y, 228.0, 22.0),
            g.ink(1.0),
        )?;
        for (index, control) in crate::timer::controls(scene.timer).iter().enumerate() {
            let slot = layout::page_button(index, 0, scene.dpi);
            g.chip(g.slot(slot), &control.label())?;
        }
        g.chip(g.slot(layout::remind_button(scene.dpi)), "Remind")?;
        g.chip(g.slot(layout::watch_button(scene.dpi)), "Stopwatch")?;
        return reminder_rows(g, scene);
    };
    g.text(
        &view.panel,
        &g.display(30.0, BOLD, LEAD),
        g.slot(layout::page_value(scene.dpi)),
        g.ink(if view.paused { 0.55 } else { 1.0 }),
    )?;
    let controls = crate::timer::controls(Some(view));
    for (index, control) in controls.iter().enumerate() {
        let slot = layout::page_button(index, controls.len(), scene.dpi);
        g.chip(g.slot(slot), &control.label())?;
    }
    if view.watch {
        return g.text(
            &view.laps,
            &g.font(11.0, NORMAL, LEAD),
            g.at(layout::PAD, 92.0, 328.0, 14.0),
            g.ink(0.6),
        );
    }
    timer_bar(g, view)?;
    g.text(
        &format!("of {}", length(view.total)),
        &g.font(11.0, NORMAL, LEAD),
        g.at(layout::PAD, 92.0, 200.0, 14.0),
        g.ink(0.6),
    )
}

/// The waiting reminders under the presets: the time, the words and a quiet cancel on each row.
fn reminder_rows(g: &Page, scene: &NotchScene) -> Result<()> {
    for (index, (time, words)) in scene.reminders.iter().enumerate() {
        let row = g.slot(layout::remind_row(index, scene.dpi));
        let cancel = g.slot(layout::remind_cancel(index, scene.dpi));
        let time_end = row.left + 52.0 * g.scale;
        let font = g.font(11.0, NORMAL, LEAD);
        let across = D2D_RECT_F {
            right: time_end,
            ..row
        };
        g.text(time, &font, across, g.ink(0.6))?;
        let across = D2D_RECT_F {
            left: time_end + 4.0 * g.scale,
            right: cancel.left,
            ..row
        };
        g.text(words, &font, across, g.ink(0.85))?;
        let side = 12.0 * g.scale;
        let square = pixel_square(
            cancel.left + (cancel.right - cancel.left - side) / 2.0,
            cancel.top + (cancel.bottom - cancel.top - side) / 2.0,
            side,
        );
        glyphs::draw(g.context, square, Glyph::Close, g.ink(0.45))?;
    }
    Ok(())
}

/// The countdown's capsule: one track with its fill, or for a Pomodoro the round's segments (focus 4 units, a
/// short break 2, the closing long break 3) where finished ones are full, the current one fills with the time
/// gone and the rest wait. Paused dims the fill.
fn timer_bar(g: &Page, view: &crate::timer::View) -> Result<()> {
    let (y, height, gap) = (80.0, 6.0, 4.0);
    let width = layout::NOTCH_EXPANDED.width - 2.0 * layout::PAD;
    let elapsed = view.elapsed();
    let (current, count) = view
        .cycle
        .map_or((0, 1), |(at, count)| (usize::from(at), usize::from(count)));
    let weight = |index: usize| match (view.cycle, index) {
        (None, _) => 1.0,
        (_, index) if index % 2 == 0 => 4.0,
        (_, index) if index + 1 == count => 3.0,
        _ => 2.0,
    };
    let unit = (width - gap * (count - 1) as f32) / (0..count).map(weight).sum::<f32>();
    let lit = g.accent(if view.paused { 0.5 } else { 1.0 });
    let mut left = layout::PAD;
    for index in 0..count {
        let segment = g.at(left, y, unit * weight(index), height);
        g.fill(segment, None, g.ink(0.16))?;
        let share = match index.cmp(&current) {
            std::cmp::Ordering::Less => 1.0,
            std::cmp::Ordering::Equal => elapsed,
            std::cmp::Ordering::Greater => 0.0,
        };
        // The running segment shows its start at once, so a fresh countdown never reads unstarted.
        if share > 0.0 || index == current {
            // A started segment never draws narrower than a capsule's own height.
            let wide = (segment.right - segment.left) * share;
            let part = D2D_RECT_F {
                right: segment.left + wide.max(height * g.scale),
                ..segment
            };
            g.fill(part, None, lit)?;
        }
        left += unit * weight(index) + gap;
    }
    Ok(())
}

/// The Shelf page: empty, a soft dashed drop zone with its hint and an Add files capsule; otherwise Add files in
/// the header's action slot, the file capsules in a two column grid and, after a removal, Undo in the grid's
/// free cell.
fn shelf_page(g: &Page, scene: &NotchScene) -> Result<()> {
    if scene.clips {
        return copied_page(g, scene);
    }
    let empty = scene.shelf.first().is_some_and(|slot| slot.add);
    if empty {
        let zone = g.slot(layout::shelf_zone(scene.dpi));
        g.tile(zone)?;
        drop_zone(g.context, zone, g.scale, g.ink(0.22))?;
        glyphs::draw(
            g.context,
            g.at(layout::PAD + 20.0, 56.0, 20.0, 20.0),
            Glyph::Shelf,
            g.ink(0.7),
        )?;
        g.text(
            "Drop a file here.",
            &g.font(13.0, SEMI, LEAD),
            g.at(layout::PAD + 56.0, 48.0, 250.0, 20.0),
            g.ink(0.95),
        )?;
        g.text(
            "It will wait.",
            &g.font(11.0, NORMAL, LEAD),
            g.at(layout::PAD + 56.0, 70.0, 250.0, 16.0),
            g.ink(0.6),
        )?;
    }
    shelf_tools(g, scene)?;
    shelf_row(g.context, scene, g.body, g.scale, g.p)
}

/// Secondary actions appear on page hover, away from the focal row.
fn shelf_tools(g: &Page, scene: &NotchScene) -> Result<()> {
    if scene.shelf_hover {
        let slot = g.slot(layout::shelf_action(scene.dpi, false));
        if scene.clips {
            g.text("Clear", &g.font(10.5, NORMAL, MIDDLE), slot, g.ink(0.7))?;
        } else {
            glyphs::draw(
                g.context,
                pixel_square(
                    (slot.left + slot.right) / 2.0 - 8.0 * g.scale,
                    slot.top + g.scale,
                    16.0 * g.scale,
                ),
                Glyph::Add,
                g.ink(0.7),
            )?;
        }
    }
    if scene.shelf_undo {
        g.chip(g.slot(layout::shelf_action(scene.dpi, true)), "Undo")?;
    }
    Ok(())
}

/// The Files | Copied switch stays neutral so the focal row leads.
fn shelf_switch(g: &Page, scene: &NotchScene) -> Result<()> {
    let half = |index: usize| g.slot(layout::shelf_switch(scene.dpi, index));
    let (files, copied) = (half(0), half(1));
    g.fill(
        D2D_RECT_F {
            right: copied.right,
            ..files
        },
        None,
        g.ink(0.08),
    )?;
    for (index, rect, label) in [(0, files, "Files"), (1, copied, "Copied")] {
        let on = (index == 1) == scene.clips;
        if on {
            g.fill(rect, None, g.ink(0.10))?;
        }
        let ink = if on { g.ink(0.95) } else { g.ink(0.8) };
        g.text(label, &g.font(10.5, NORMAL, MIDDLE), rect, ink)?;
    }
    Ok(())
}

/// The Copied list: secondary hover actions and two clips; empty is a soft
/// tile with a short line (no outline: only the drop zone has one).
fn copied_page(g: &Page, scene: &NotchScene) -> Result<()> {
    if scene.shelf.is_empty() {
        let zone = g.slot(layout::shelf_zone(scene.dpi));
        g.tile(zone)?;
        glyphs::draw(
            g.context,
            pixel_square(
                zone.left + 16.0 * g.scale,
                zone.top + 18.0 * g.scale,
                20.0 * g.scale,
            ),
            Glyph::File,
            g.ink(0.8),
        )?;
        let at = |dy: f32, h: f32| g.at(layout::PAD + 48.0, layout::BODY_Y + dy, 270.0, h);
        g.text(
            "Nothing copied yet.",
            &g.font(13.0, SEMI, LEAD),
            at(9.0, 18.0),
            g.ink(0.95),
        )?;
        return g.text(
            "Copy some text. It waits here.",
            &g.font(11.0, NORMAL, LEAD),
            at(28.0, 16.0),
            g.ink(0.6),
        );
    }
    shelf_tools(g, scene)?;
    shelf_row(g.context, scene, g.body, g.scale, g.p)
}

/// The empty shelf's soft drop zone: a dashed r14 outline, the one outline in the grammar.
fn drop_zone(
    context: &ID2D1DeviceContext,
    zone: D2D_RECT_F,
    scale: f32,
    colour: D2D1_COLOR_F,
) -> Result<()> {
    use windows::Win32::Graphics::Direct2D::{
        D2D1_CAP_STYLE_ROUND, D2D1_DASH_STYLE_CUSTOM, D2D1_LINE_JOIN_ROUND,
        D2D1_STROKE_STYLE_PROPERTIES, ID2D1Factory,
    };
    // SAFETY: a factory, a stroke style and a brush local to this draw, on the frame's live context.
    unsafe {
        let factory: ID2D1Factory = context.GetFactory()?;
        let dashes = [0.1_f32, 2.4];
        let style = factory.CreateStrokeStyle(
            &D2D1_STROKE_STYLE_PROPERTIES {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 1.0,
                dashStyle: D2D1_DASH_STYLE_CUSTOM,
                dashOffset: 0.0,
            },
            Some(&dashes),
        )?;
        let brush = context.CreateSolidColorBrush(&colour, None)?;
        context.DrawRoundedRectangle(
            &rounded(zone, layout::TILE_RADIUS * scale),
            &brush,
            1.5 * scale,
            &style,
        );
    }
    Ok(())
}

/// The Widgets page: tiles of one size on the grid (190 x 72, the 14 radius, an 8 gutter), two across; a filled
/// lower slot makes two rows of compact 190 x 52 tiles. Each tile has at most one accent: today in the week, the
/// battery ring, a usage bar's fill, the water bar.
fn widgets_page(g: &Page, scene: &NotchScene) -> Result<()> {
    use crate::widgets::{WidgetKind, speed};
    if let Some(month) = scene.month {
        return month_page(g, scene, month);
    }
    let tall = scene.widgets.tall();
    let tile = if tall {
        layout::WIDGET_COMPACT
    } else {
        layout::WIDGET_TILE
    };
    // A standard value, or its compact twin.
    let pick = |standard: f32, compact: f32| if tall { compact } else { standard };
    for (index, kind) in scene.widgets.cells().into_iter().enumerate() {
        let Some(kind) = kind else {
            continue;
        };
        let (x, y) = (
            layout::PAD + (tile.0 + layout::GUTTER) * (index % 2) as f32,
            layout::BODY_Y + (tile.1 + layout::GUTTER) * (index / 2) as f32,
        );
        let at = |dx: f32, dy: f32, w: f32, h: f32| g.at(x + dx, y + dy, w, h);
        g.tile(at(0.0, 0.0, tile.0, tile.1))?;
        // The big reading, its line and an optional bar's percent: the usage tiles' common face.
        let reading = |big: &str, line: &str, bar: Option<u32>| -> Result<()> {
            g.text(
                big,
                &g.display(pick(24.0, 20.0), BOLD, LEAD),
                at(14.0, pick(6.0, 4.0), 162.0, pick(28.0, 24.0)),
                g.ink(1.0),
            )?;
            g.text(
                line,
                &g.font(pick(11.0, 10.0), NORMAL, LEAD),
                at(14.0, pick(35.0, 28.0), 162.0, pick(16.0, 14.0)),
                g.ink(0.6),
            )?;
            let Some(percent) = bar else {
                return Ok(());
            };
            // The track and the fill are both capsules; the fill is the one accent here.
            let (bar_y, bar_h) = (pick(56.0, 43.0), pick(6.0, 5.0));
            g.fill(at(14.0, bar_y, 162.0, bar_h), None, g.ink(0.18))?;
            if percent > 0 {
                g.fill(
                    at(
                        14.0,
                        bar_y,
                        (162.0 * percent as f32 / 100.0).max(bar_h),
                        bar_h,
                    ),
                    None,
                    g.accent(1.0),
                )?;
            }
            Ok(())
        };
        let percent_text = |p: Option<u32>| p.map_or("--".into(), |p| format!("{p}%"));
        match kind {
            WidgetKind::WeekCalendar => {
                // SAFETY: GetLocalTime returns a value and needs no resources.
                let date = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
                let sunday = scene.widgets.week_sunday;
                // Sunday first is the same week with today one cell further along (`week_numbers`' rotation).
                let days = crate::widgets::week_numbers(
                    date.wYear,
                    date.wMonth,
                    date.wDay,
                    if sunday {
                        (date.wDayOfWeek + 1) % 7
                    } else {
                        date.wDayOfWeek
                    },
                );
                let today = if sunday {
                    usize::from(date.wDayOfWeek)
                } else {
                    usize::from((date.wDayOfWeek + 6) % 7)
                };
                let labels = if sunday {
                    ["S", "M", "T", "W", "T", "F", "S"]
                } else {
                    ["M", "T", "W", "T", "F", "S", "S"]
                };
                // The date heads the tile, the week strip sits under it: no empty band. Compact: strip first.
                g.text(
                    scene.date,
                    &g.font(pick(12.0, 10.0), SEMI, LEAD),
                    at(14.0, pick(8.0, 38.0), 162.0, pick(16.0, 12.0)),
                    g.ink(0.9),
                )?;
                for (i, (label, day)) in labels.into_iter().zip(days).enumerate() {
                    let dx = 11.0 + i as f32 * 24.0;
                    g.text(
                        label,
                        &g.font(9.0, SEMI, MIDDLE),
                        at(dx, pick(27.0, 4.0), 22.0, 12.0),
                        g.ink(0.55),
                    )?;
                    let cell = at(dx + 1.0, pick(42.0, 17.0), 20.0, 20.0);
                    if i == today {
                        g.fill(cell, None, g.accent(1.0))?;
                    }
                    let colour = if i == today {
                        g.on_accent()
                    } else {
                        g.ink(0.9)
                    };
                    g.text(&day.to_string(), &g.font(10.0, SEMI, MIDDLE), cell, colour)?;
                }
            }
            WidgetKind::Network => {
                let bits = scene.widgets.net_bits;
                match scene.stats.1 {
                    Some((down, up)) => reading(
                        &speed(down, bits),
                        &format!("Down, {} up", speed(up, bits)),
                        None,
                    )?,
                    None => reading("--", "Network speed", None)?,
                }
            }
            WidgetKind::Cpu => {
                let busy = scene.stats.0.is_some_and(|p| p >= 90);
                reading(
                    &percent_text(scene.stats.0),
                    if busy {
                        "Processor is busy"
                    } else {
                        "Processor in use"
                    },
                    Some(scene.stats.0.unwrap_or(0)),
                )?;
            }
            WidgetKind::Memory | WidgetKind::Disk => {
                let (title, now) = if kind == WidgetKind::Memory {
                    ("Memory".to_owned(), crate::widgets::memory())
                } else {
                    let drive = scene.widgets.disk_drive;
                    (
                        drive.map_or("Disk".to_owned(), |d| format!("Disk {}", d as char)),
                        crate::widgets::disk(drive),
                    )
                };
                let full = now.as_ref().is_some_and(|r| r.percent >= 90);
                let line = match &now {
                    Some(r) if full => format!("{title} getting full, {}", r.detail),
                    Some(r) => format!("{title} in use, {}", r.detail),
                    None => format!("{title} in use"),
                };
                reading(
                    &percent_text(now.as_ref().map(|r| r.percent)),
                    &line,
                    Some(now.map_or(0, |r| r.percent)),
                )?;
            }
            WidgetKind::Water => match scene.glance.water {
                Some((drank, goal)) if goal > 0 => reading(
                    &format!("{drank} of {goal}"),
                    "Glasses of water today",
                    Some((u32::from(drank) * 100 / u32::from(goal)).min(100)),
                )?,
                _ => reading("--", "Glasses of water today", None)?,
            },
            WidgetKind::NextPrayer => match scene.prayer {
                Some(view) if view.message.is_none() && !view.next.is_empty() => {
                    reading(&view.next, &format!("Next prayer, {}", view.left), None)?;
                }
                Some(view) if view.message.is_some() => {
                    reading("--", view.message.unwrap_or_default(), None)?;
                }
                _ => reading("--", "Turn on the Prayer tab", None)?,
            },
            WidgetKind::Weather => match scene.weather {
                Some(view) if view.message.is_none() && !view.temp.is_empty() => {
                    let glyph = at(14.0, pick(14.0, 10.0), 32.0, 32.0);
                    glyphs::draw(g.context, glyph, Glyph::weather(view.icon), g.ink(0.9))?;
                    g.text(
                        &view.temp,
                        &g.display(pick(24.0, 20.0), BOLD, LEAD),
                        at(56.0, pick(6.0, 4.0), 124.0, pick(28.0, 24.0)),
                        g.ink(1.0),
                    )?;
                    g.text(
                        view.cond,
                        &g.font(pick(11.0, 10.0), NORMAL, LEAD),
                        at(56.0, pick(35.0, 28.0), 124.0, pick(16.0, 14.0)),
                        g.ink(0.6),
                    )?;
                }
                Some(view) if view.message.is_some() => {
                    reading("--", view.message.unwrap_or_default(), None)?;
                }
                _ => reading("--", "Turn on the Weather tab", None)?,
            },
            WidgetKind::Battery => {
                let raw = crate::power::Raw::read();
                let now = raw.filter(|r| r.flag & 128 == 0 && r.percent <= 100);
                let percent = now.map_or(0, |r| r.percent);
                // The ring: centre and radius in DIPs on the tile.
                let (cx, cy, radius, stroke) = if tall {
                    (28.0, 26.0, 16.0, 3.5)
                } else {
                    (38.0, 36.0, 22.0, 4.0)
                };
                let glyph = at(cx - 12.0, cy - 12.0, 24.0, 24.0);
                // No battery: a muted glyph and one line, not an empty ring.
                if now.is_none() {
                    glyphs::draw(g.context, glyph, Glyph::Charging, g.ink(0.35))?;
                } else {
                    // SAFETY: brushes and line segments use the live context; no geometry is retained.
                    unsafe {
                        let track = g.context.CreateSolidColorBrush(&g.ink(0.18), None)?;
                        let accent = g.context.CreateSolidColorBrush(&g.accent(1.0), None)?;
                        let point = |angle: f32| Vector2 {
                            X: g.body.left + (x + cx + radius * angle.cos()) * g.scale,
                            Y: g.body.top + (y + cy + radius * angle.sin()) * g.scale,
                        };
                        const STEPS: usize = 48;
                        for step in 0..STEPS {
                            let turn = |n: usize| {
                                -std::f32::consts::FRAC_PI_2
                                    + n as f32 * std::f32::consts::TAU / STEPS as f32
                            };
                            g.context.DrawLine(
                                point(turn(step)),
                                point(turn(step + 1)),
                                if step * 100 < usize::from(percent) * STEPS {
                                    &accent
                                } else {
                                    &track
                                },
                                stroke * g.scale,
                                None,
                            );
                        }
                    }
                }
                let charging = now.is_some_and(|r| r.flag & 8 != 0);
                if charging {
                    glyphs::draw(g.context, glyph, Glyph::Charging, g.accent(1.0))?;
                }
                let text_x = pick(76.0, 60.0);
                let text_w = 180.0 - text_x;
                if let Some(r) = now {
                    g.text(
                        &format!("{}%", r.percent),
                        &g.display(pick(24.0, 20.0), BOLD, LEAD),
                        at(text_x, pick(12.0, 4.0), text_w, pick(28.0, 24.0)),
                        g.ink(1.0),
                    )?;
                }
                let status = match now {
                    None => "No battery here".to_owned(),
                    Some(r) if r.low() => "Running low".to_owned(),
                    Some(_) if charging => "Charging".to_owned(),
                    Some(r) if r.ac == 1 => "Plugged in".to_owned(),
                    Some(r) if r.ac == 0 => match r.left {
                        Some(secs) => format!(
                            "{} left",
                            crate::prayer_tab::countdown(secs).trim_start_matches("in ")
                        ),
                        None => "On battery".to_owned(),
                    },
                    _ => "Unknown".to_owned(),
                };
                g.text(
                    &status,
                    &g.font(pick(11.0, 10.0), NORMAL, LEAD),
                    if now.is_some() {
                        at(text_x, pick(42.0, 29.0), text_w, pick(16.0, 14.0))
                    } else {
                        at(text_x, pick(20.0, 8.0), text_w, 16.0)
                    },
                    g.ink(0.6),
                )?;
                if now.is_none() {
                    // One quiet sentence over two lines: a text box is a single line.
                    for (n, line) in ["Pick another widget", "in Settings."]
                        .into_iter()
                        .enumerate()
                    {
                        g.text(
                            line,
                            &g.font(10.0, NORMAL, LEAD),
                            at(text_x, pick(38.0, 24.0) + 12.0 * n as f32, text_w, 14.0),
                            g.ink(0.45),
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
/// The month page: two tiles, the month's name with its arrows on the left and six weeks on the right. Today is
/// the one accent.
fn month_page(g: &Page, scene: &NotchScene, (year, month): (u16, u16)) -> Result<()> {
    use crate::widgets::{MONTHS, month_grid};
    let (w, h) = (
        layout::WIDGET_TILE.0,
        layout::WIDGET_COMPACT.1 * 2.0 + layout::GUTTER,
    );
    let at = |x: f32, y: f32, w: f32, h: f32| g.at(layout::PAD + x, layout::BODY_Y + y, w, h);
    g.tile(at(0.0, 0.0, w, h))?;
    g.tile(at(w + layout::GUTTER, 0.0, w, h))?;
    g.text(
        MONTHS[usize::from(month - 1)],
        &g.display(24.0, BOLD, LEAD),
        at(14.0, 8.0, 162.0, 28.0),
        g.ink(1.0),
    )?;
    g.text(
        &year.to_string(),
        &g.font(11.0, NORMAL, LEAD),
        at(14.0, 38.0, 162.0, 16.0),
        g.ink(0.6),
    )?;
    for ((x, y, width, height), glyph) in layout::MONTH_ARROWS
        .into_iter()
        .zip([Glyph::Back, Glyph::Forward])
    {
        g.fill(g.at(x, y, width, height), None, g.ink(0.14))?;
        glyphs::draw(
            g.context,
            g.at(
                x + (width - 16.0) / 2.0,
                y + (height - 16.0) / 2.0,
                16.0,
                16.0,
            ),
            glyph,
            g.ink(0.9),
        )?;
    }
    // SAFETY: GetLocalTime returns a value and needs no resources.
    let date = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let today = (date.wYear, date.wMonth) == (year, month);
    let sunday = scene.widgets.week_sunday;
    let labels = if sunday {
        ["S", "M", "T", "W", "T", "F", "S"]
    } else {
        ["M", "T", "W", "T", "F", "S", "S"]
    };
    let left = w + layout::GUTTER + 8.0;
    for (i, label) in labels.into_iter().enumerate() {
        g.text(
            label,
            &g.font(9.0, SEMI, MIDDLE),
            at(left + i as f32 * 25.0, 4.0, 25.0, 12.0),
            g.ink(0.55),
        )?;
    }
    for (cell, (day, inside)) in month_grid(year, month, sunday).into_iter().enumerate() {
        let (x, y) = (
            left + (cell % 7) as f32 * 25.0,
            18.0 + (cell / 7) as f32 * 15.0,
        );
        let now = inside && today && day == date.wDay;
        if now {
            g.fill(at(x + 5.5, y + 0.5, 14.0, 14.0), None, g.accent(1.0))?;
        }
        let colour = match (now, inside) {
            (true, _) => g.on_accent(),
            (_, true) => g.ink(0.9),
            _ => g.ink(0.3),
        };
        g.text(
            &day.to_string(),
            &g.font(10.0, SEMI, MIDDLE),
            at(x, y, 25.0, 15.0),
            colour,
        )?;
    }
    Ok(())
}
/// The Weather page: the condition glyph and the big temperature at the left with the condition and the day's range
/// under them, the next hours as columns on one soft tile at the right (four, on both panel kinds), and the
/// attribution as the footer. With no data one line says why.
fn weather_page(g: &Page, scene: &NotchScene) -> Result<()> {
    let Some(view) = scene.weather else {
        return Ok(());
    };
    if let Some(message) = view.message {
        return g.message(message, view.set_city);
    }
    glyphs::draw(
        g.context,
        g.at(layout::PAD, 40.0, 36.0, 36.0),
        Glyph::weather(view.icon),
        g.ink(1.0),
    )?;
    g.text(
        &view.temp,
        &g.display(34.0, BOLD, LEAD),
        g.at(60.0, 36.0, 108.0, 42.0),
        g.ink(1.0),
    )?;
    g.text(
        view.cond,
        &g.font(12.0, NORMAL, LEAD),
        g.at(layout::PAD, 80.0, 96.0, 16.0),
        g.ink(0.8),
    )?;
    g.text(
        &view.range,
        &g.font(11.0, NORMAL, LEAD),
        g.at(110.0, 80.0, 60.0, 16.0),
        g.ink(0.6),
    )?;
    // The hours sit together on one soft tile, so the strip reads as a unit.
    const PITCH: f32 = 42.0;
    let tile_x = layout::PAD + 328.0 - 4.0 * PITCH;
    g.tile(g.at(tile_x, 36.0, 4.0 * PITCH, 60.0))?;
    for (index, cell) in view.hours.iter().take(4).enumerate() {
        let x = tile_x + PITCH * index as f32;
        g.text(
            &cell.label,
            &g.font(10.0, NORMAL, MIDDLE),
            g.at(x, 42.0, PITCH, 12.0),
            g.ink(0.6),
        )?;
        glyphs::draw(
            g.context,
            g.at(x + (PITCH - 14.0) / 2.0, 56.0, 14.0, 14.0),
            Glyph::weather(cell.icon),
            g.ink(0.85),
        )?;
        g.text(
            &cell.temp,
            &g.font(12.0, SEMI, MIDDLE),
            g.at(x, 73.0, PITCH, 14.0),
            g.ink(1.0),
        )?;
    }
    g.footer(scene, &view.footer, 0.45)
}

/// The footer line's size (DIPs): a step above the old 10, in a 13 DIP row (`layout::footer`).
const FOOTER_SIZE: f32 = 11.0;

/// The Prayer page: the next prayer's name and the countdown as the hero at the left with the Hijri date under
/// them, the sun arc across its five stops at the right, and the five times as capsule chips along the bottom
/// (the next one in the accent wash, the current one brighter).
fn prayer_page(g: &Page, scene: &NotchScene) -> Result<()> {
    let Some(view) = scene.prayer else {
        return Ok(());
    };
    if let Some(message) = view.message {
        return g.message(message, view.set_city);
    }
    g.text(
        &view.next,
        &g.display(24.0, BOLD, LEAD),
        g.at(layout::PAD, 28.0, 176.0, 26.0),
        g.ink(1.0),
    )?;
    g.text(
        &view.left,
        &g.display(14.0, SEMI, LEAD),
        g.at(layout::PAD, 54.0, 176.0, 18.0),
        g.accent(1.0),
    )?;
    g.text(
        &view.hijri,
        &g.font(10.5, NORMAL, LEAD),
        g.at(layout::PAD, 72.0, 176.0, 14.0),
        g.ink(0.55),
    )?;
    // Today's moon under the sun arc, on the Hijri line: the month it counts is the moon's.
    let moon = view.moon;
    glyphs::moon(
        g.context,
        g.at(226.0, 72.0, 14.0, 14.0),
        moon.lit,
        moon.right,
        g.ink(0.7),
    )?;
    g.text(
        moon.name,
        &g.font(10.5, NORMAL, LEAD),
        g.at(244.0, 72.0, 92.0, 14.0),
        g.ink(0.55),
    )?;
    sun_arc(g, view)?;
    for (index, row) in view.rows.iter().enumerate() {
        let chip = g.slot(layout::prayer_chip(index, scene.dpi));
        let fill = if row.name == view.next {
            g.accent(0.25)
        } else if row.current {
            g.ink(0.14)
        } else {
            g.ink(0.07)
        };
        g.fill(chip, None, fill)?;
        let alpha = if row.current || row.name == view.next {
            1.0
        } else {
            0.6
        };
        let (left, top) = (chip.left, chip.top);
        let line = |dy: f32, h: f32| D2D_RECT_F {
            left,
            top: top + dy * g.scale,
            right: chip.right,
            bottom: top + (dy + h) * g.scale,
        };
        g.text(
            row.name,
            &g.font(9.5, NORMAL, MIDDLE),
            line(2.0, 12.0),
            g.ink(alpha),
        )?;
        let weight = if row.current { SEMI } else { NORMAL };
        g.text(
            &row.time,
            &g.font(11.0, weight, MIDDLE),
            line(12.0, 13.0),
            g.ink(if row.current { 1.0 } else { 0.85 }),
        )?;
    }
    Ok(())
}

/// The five prayer stops on a shallow arc beside the hero, with the sun on it: the part it has passed in the
/// accent, the rest faint. The sun is the one glow in the system (a halo under the disc).
fn sun_arc(g: &Page, view: &crate::prayer_tab::View) -> Result<()> {
    // Stop `u` (0 to 1) along the arc: x from 206 to 336, y a parabola 22 DIP high over a base at 66.
    let point = |u: f32| Vector2 {
        X: g.body.left + (206.0 + u * 130.0) * g.scale,
        Y: g.body.top + (66.0 - 22.0 * (1.0 - (2.0 * u - 1.0).powi(2))) * g.scale,
    };
    let sun = view.sun.clamp(0.0, 1.0);
    // SAFETY: brushes and shapes live for this draw on the frame's own device context.
    unsafe {
        let (passed, ahead) = (
            g.context.CreateSolidColorBrush(&g.accent(0.6), None)?,
            g.context.CreateSolidColorBrush(&g.ink(0.22), None)?,
        );
        const STEPS: usize = 24;
        for step in 0..STEPS {
            let (a, b) = (step as f32 / STEPS as f32, (step + 1) as f32 / STEPS as f32);
            let brush = if b <= sun { &passed } else { &ahead };
            g.context
                .DrawLine(point(a), point(b), brush, 2.0 * g.scale, None);
        }
        for stop in 0..5 {
            let u = stop as f32 / 4.0;
            g.context.FillEllipse(
                &D2D1_ELLIPSE {
                    point: point(u),
                    radiusX: 3.0 * g.scale,
                    radiusY: 3.0 * g.scale,
                },
                if u <= sun { &passed } else { &ahead },
            );
        }
        let at = point(sun);
        for (radius, alpha) in [(9.0, 0.18), (4.0, 1.0)] {
            let brush = g.context.CreateSolidColorBrush(&g.accent(alpha), None)?;
            g.context.FillEllipse(
                &D2D1_ELLIPSE {
                    point: at,
                    radiusX: radius * g.scale,
                    radiusY: radius * g.scale,
                },
                &brush,
            );
        }
    }
    Ok(())
}

/// Where the eye pair is centred in the pill, in DIPs from the body's left: the middle of the media pill's
/// title lane (design: 91 + 40), or the plain pill's middle, where the clock fades out for them.
fn eye_centre(media: bool, body_width: f32) -> f32 {
    if media { 131.0 } else { body_width / 2.0 }
}

/// How visible the eyes are at an open `progress`: fully until the hover swell, gone as the body opens.
pub(super) fn eye_alpha(progress: f32) -> f32 {
    1.0 - motion::smooth((progress - 0.08) / 0.15)
}

/// How much of the pill's time (plain) or title (media) the eyes have replaced right now, 0 to 1.
pub(super) fn eyes_shown(scene: &NotchScene) -> f32 {
    scene
        .eyes
        .map_or(0.0, |frame| frame.alpha * eye_alpha(scene.progress))
}

/// Fade through a clear midpoint: clock and eyes never share visible pixels.
pub(super) fn clock_shown(scene: &NotchScene) -> f32 {
    (1.0 - 2.0 * eyes_shown(scene)).max(0.0)
}

/// The two capsules for the scene's mood and pose, at the pill's middle. Lids are flat cuts of the capsule
/// (a clip), not body-coloured overpaint, so they hold on every theme.
pub(super) fn draw_eyes(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    frame: eyes::Frame,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let alpha = (2.0 * eyes_shown(scene) - 1.0).max(0.0);
    if alpha <= 0.0 {
        return Ok(());
    }
    let media = scene.track.is_some();
    // The close spring's undershoot briefly makes the pill shorter than 30: keep the row centred.
    let middle = (layout::notch_shape(scene.progress, media).height / 2.0).min(15.0);
    let Eye {
        w,
        h,
        dy,
        top,
        bottom,
    } = eyes::eye(frame.mood, frame.pose);
    let (w, h) = (w * scale, h * scale);
    let centre_y = body.top + (middle + dy) * scale;
    // SAFETY: context is live between surface BeginDraw/EndDraw; the brush is scoped to this draw and
    // every clip pushed below is popped in the same iteration.
    unsafe {
        let brush =
            context.CreateSolidColorBrush(&with_alpha(p.accent, p.ink_alpha(alpha)), None)?;
        for side in [-1.0f32, 1.0] {
            let middle_x = body.left + eye_centre(media, (body.right - body.left) / scale) * scale;
            let centre_x = middle_x + (side * eyes::GAP / 2.0 + frame.look) * scale;
            // Wave: the right eye hops 5 DIPs and tilts 22 degrees about its centre.
            let wave = frame.pose == eyes::Pose::Wave && side > 0.0;
            let centre_y = if wave {
                centre_y - 5.0 * scale
            } else {
                centre_y
            };
            let capsule = D2D_RECT_F {
                left: centre_x - w / 2.0,
                top: centre_y - h / 2.0,
                right: centre_x + w / 2.0,
                bottom: centre_y + h / 2.0,
            };

            let lids = D2D_RECT_F {
                top: capsule.top + top * h,
                bottom: capsule.bottom - bottom * h,
                ..capsule
            };
            if lids.bottom <= lids.top {
                continue;
            }
            context.PushAxisAlignedClip(&lids, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            if wave {
                context.SetTransform(&Matrix3x2::rotation_around(
                    22.0,
                    Vector2 {
                        X: centre_x,
                        Y: centre_y,
                    },
                ));
            }
            context.FillRoundedRectangle(&rounded(capsule, w.min(h) / 2.0), &brush);
            if wave {
                context.SetTransform(&Matrix3x2::identity());
            }
            context.PopAxisAlignedClip();
        }
    }
    Ok(())
}

/// Resting eyes in the open panel, in the accent at the page's reveal: static, so they cost nothing. `spot` is
/// the pair's centre (DIPs from the body's corner) and its size factor on the base capsule; `toward` shifts them
/// (DIPs) to look at the page's content.
pub(super) fn panel_eyes(
    context: &ID2D1DeviceContext,
    body: &D2D_RECT_F,
    scale: f32,
    alpha: f32,
    frame: eyes::Frame,
    ((cx, cy, k), toward): ((f32, f32, f32), f32),
    p: &Palette,
) -> Result<()> {
    if alpha <= 0.0 {
        return Ok(());
    }
    let Eye {
        w,
        h,
        dy,
        top,
        bottom,
    } = eyes::eye(frame.mood, eyes::Pose::Rest);
    let (w, h) = (w * k * scale, h * k * scale);
    let y = body.top + (cy + dy * k) * scale;
    // SAFETY: a brush and fills on the frame's own device context; each clip is popped in its iteration.
    unsafe {
        let brush =
            context.CreateSolidColorBrush(&with_alpha(p.accent, p.ink_alpha(alpha)), None)?;
        for side in [-1.0f32, 1.0] {
            let x = body.left + (cx + side * eyes::GAP * k / 2.0 + toward) * scale;
            let capsule = D2D_RECT_F {
                left: x - w / 2.0,
                top: y - h / 2.0,
                right: x + w / 2.0,
                bottom: y + h / 2.0,
            };
            let lids = D2D_RECT_F {
                top: capsule.top + top * h,
                bottom: capsule.bottom - bottom * h,
                ..capsule
            };
            context.PushAxisAlignedClip(&lids, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            context.FillRoundedRectangle(&rounded(capsule, w.min(h) / 2.0), &brush);
            context.PopAxisAlignedClip();
        }
    }
    Ok(())
}

/// The low-battery bolt: a 10 DIP glyph inset 14 from the closed pill's left edge, fading out as the pill opens.
/// Drawn only while the battery is low and the Widgets setting for it is on (`Glance::low`).
pub(super) fn draw_low(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let alpha = 1.0 - scene.progress * 4.0;
    if !scene.glance.low || alpha <= 0.0 {
        return Ok(());
    }
    let middle =
        (layout::notch_shape(scene.progress, scene.track.is_some()).height / 2.0).min(15.0);
    let side = 10.0 * scale;
    let (x, y) = (
        body.left + 14.0 * scale,
        body.top + middle * scale - side / 2.0,
    );
    let rect = D2D_RECT_F {
        left: x,
        top: y,
        right: x + side,
        bottom: y + side,
    };
    glyphs::draw(context, rect, Glyph::Charging, with_alpha(p.glyph, alpha))
}

/// The privacy dots (features B5): 6 DIP circles inset 14 from the body's right edge, on the pill's middle line,
/// the camera one nearest the edge. Only a dot with some opacity is drawn.
pub(super) fn draw_privacy(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let middle =
        (layout::notch_shape(scene.progress, scene.track.is_some()).height / 2.0).min(15.0);
    let (r, y) = (3.0 * scale, body.top + middle * scale);
    // Orange for the microphone, green for the camera: fixed signal colours on every theme. A muted mic's dot
    // takes the mic's place in the theme's ink (`privacy::mic_dot` is that place).
    let muted = scene.privacy[2];
    for (slot, (alpha, colour)) in [
        (scene.privacy[1], (34, 197, 94)),
        (scene.privacy[0] * (1.0 - muted), (245, 158, 11)),
        (p.ink_alpha(muted), p.ink),
    ]
    .into_iter()
    .enumerate()
    {
        if alpha <= 0.0 {
            continue;
        }
        let x = body.right - (14.0 + 9.0 * slot.min(1) as f32) * scale;
        let dot = D2D_RECT_F {
            left: x - r,
            top: y - r,
            right: x + r,
            bottom: y + r,
        };
        // SAFETY: context is live between surface BeginDraw/EndDraw; the brush lives for this draw.
        unsafe {
            let brush = context.CreateSolidColorBrush(&with_alpha(colour, alpha), None)?;
            context.FillRoundedRectangle(&rounded(dot, r), &brush);
        }
    }
    Ok(())
}

/// Side of a shelf icon inside its capsule, in DIPs.
const SHELF_ICON: f32 = 16.0;

/// Outline file capsules share one focal row; the separate footer pager stays neutral.
fn shelf_row(
    context: &ID2D1DeviceContext,
    scene: &NotchScene,
    body: &D2D_RECT_F,
    scale: f32,
    p: &Palette,
) -> Result<()> {
    let reveal = motion::reveal(scene.progress);
    if reveal <= 0.0 {
        return Ok(());
    }
    let media = scene.track.is_some();
    let ink = |alpha: f32| with_alpha(p.ink, p.ink_alpha(alpha));
    for (index, slot) in scene.shelf.iter().enumerate().filter(|(_, slot)| !slot.add) {
        let cell = layout::shelf_slot(
            media,
            if slot.more {
                crate::shelf::VISIBLE - 1
            } else {
                index
            },
            scene.dpi,
        );
        let (left, top) = (body.left + cell.x as f32, body.top + cell.y as f32);
        let rect = D2D_RECT_F {
            left,
            top,
            right: left + cell.width as f32,
            bottom: top + cell.height as f32,
        };
        let alpha = reveal * if slot.missing { DISABLED } else { 1.0 };
        // SAFETY: a brush and a fill on the frame's own device context.
        unsafe {
            let brush = context.CreateSolidColorBrush(&ink(0.10 * reveal), None)?;
            context.FillRoundedRectangle(&rounded(rect, cell.height as f32 / 2.0), &brush);
        }
        let side = (SHELF_ICON * scale).round();
        let square = pixel_square(
            left + 10.0 * scale,
            top + ((cell.height as f32 - side) / 2.0).round(),
            side,
        );
        let name = if slot.more { "Next" } else { slot.name };
        let font = Font {
            family: TEXT_FONT,
            size: 11.5 * scale,
            weight: DWRITE_FONT_WEIGHT_NORMAL,
            designator: 1.0,
            align: DWRITE_TEXT_ALIGNMENT_LEADING,
        };
        // A file capsule shows Share beside the cross while hovered: the name gives up that room.
        let share = scene.shelf_hover && !scene.clips && !slot.missing;
        let line = D2D_RECT_F {
            left: left + 32.0 * scale,
            right: rect.right
                - if slot.more {
                    12.0
                } else if share {
                    52.0
                } else {
                    28.0
                } * scale,
            ..rect
        };
        draw_text(context, name, &font, line, ink(0.9 * alpha))?;
        if slot.more {
            glyphs::draw(context, square, Glyph::More, ink(reveal))?;
            continue;
        }
        glyphs::draw(context, square, Glyph::File, ink(0.7 * alpha))?;
        if !scene.shelf_hover {
            continue;
        }
        // The remove cross and Share, centred in their targets (`layout::shelf_remove`).
        let buttons = [(false, Glyph::Close), (true, Glyph::Share)];
        for (is_share, glyph) in buttons.into_iter().filter(|&(s, _)| share || !s) {
            let target = layout::shelf_remove(index, scene.dpi, is_share);
            glyphs::draw(
                context,
                pixel_square(
                    body.left + target.x as f32 + (target.width as f32 - 16.0 * scale) / 2.0,
                    body.top + target.y as f32 + (target.height as f32 - 16.0 * scale) / 2.0,
                    16.0 * scale,
                ),
                glyph,
                ink(0.6 * reveal),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_rail_labels_fit_at_enlarged_effective_dpi() {
        for dpi in [96, 144, 192, 240, 288, 336, 432] {
            let mut rail = layout::Rail {
                count: Tab::ALL.len(),
                active: 0,
                expansion: [0.0; 7],
            };
            let window = (layout::NOTCH_WINDOW.0 as f32 * dpi as f32 / 96.0).round() as i32;
            let font = rail_font(dpi as f32 / 96.0);
            for (index, tab) in Tab::ALL.into_iter().enumerate() {
                rail.expansion.fill(0.0);
                rail.expansion[index] = 1.0;
                let bounds = layout::rail_bounds(window, dpi, 1.0, false, rail).unwrap();
                let cell = layout::rail_item(bounds, dpi, rail, index);
                let label = super::super::text_width(tab.name(), &font).unwrap();
                assert!(
                    label + 34.0 * dpi as f32 / 96.0 <= cell.width as f32,
                    "{tab:?} at {dpi}: {label}"
                );
                assert!(cell.x + cell.width <= window);
            }
        }
    }
}
