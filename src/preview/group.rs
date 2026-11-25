use super::{
    Placement, Thumbnail,
    close_button::{CloseButton, color},
    pointed_window, track_leave,
};
use crate::{
    dock,
    graphics::{Glyph, batch_gdi},
    layout::Bounds,
    native_window::NativeWindow,
    theme::{Palette, Rgb},
    window_model::{WindowKey, WindowModel},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HWND, POINT, RECT},
        UI::{
            Controls::WM_MOUSELEAVE,
            Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture},
            WindowsAndMessaging::{
                MSG, WM_CANCELMODE, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
                WM_MOUSEWHEEL, WindowFromPoint,
            },
        },
    },
    core::{Error, Result},
};

#[derive(Clone, Copy)]
struct Layout {
    bounds: Bounds,
    dpi: u32,
    rows: usize,
    overflow: bool,
    strip: bool,
}

impl Layout {
    fn strip(at: Placement, count: usize) -> Option<Self> {
        if !(2..=3).contains(&count) {
            return None;
        }
        let px = |dip| dock::px(dip, at.dpi);
        let width = count as i32 * (px(176) + px(8)) + px(8);
        let height = px(136) + 2 * px(8);
        let above = at.dock.y - px(8);
        if width > at.work.width || above - height < at.work.y || above > at.work.y + at.work.height
        {
            return None;
        }
        Some(Self {
            bounds: anchored(at, width, height, above - height),
            dpi: at.dpi,
            rows: count,
            overflow: false,
            strip: true,
        })
    }

    fn list(at: Placement, count: usize) -> Option<Self> {
        let px = |dip| dock::px(dip, at.dpi);
        let bottom = at.work.y + at.work.height;
        let above = (at.dock.y - px(8)).min(bottom);
        let below = (at.dock.y + at.dock.height + px(8)).max(at.work.y);
        for (edge, space, is_above) in [
            (above, above - at.work.y, true),
            (below, bottom - below, false),
        ] {
            let rows = (((space - 2 * px(8)) / px(36)).clamp(0, 6) as usize).min(count);
            if rows == 0 {
                continue;
            }
            let overflow = rows < count;
            let rail = if !overflow {
                0
            } else if rows == 1 {
                2 * px(32)
            } else {
                px(32)
            };
            let width = px(288).min(at.work.width);
            if width < px(56) + px(32) + rail {
                continue;
            }
            let height = rows as i32 * px(36) + 2 * px(8);
            let y = if is_above { edge - height } else { edge };
            return Some(Self {
                bounds: anchored(at, width, height, y),
                dpi: at.dpi,
                rows,
                overflow,
                strip: false,
            });
        }
        None
    }

    fn px(self, dip: i32) -> i32 {
        dock::px(dip, self.dpi)
    }

    fn tile(self, slot: usize) -> Bounds {
        let pad = self.px(8);
        if self.strip {
            let width = self.px(176);
            return Bounds {
                x: pad + slot as i32 * (width + pad),
                y: pad,
                width,
                height: self.px(136),
            };
        }
        let rail = if !self.overflow {
            0
        } else if self.rows == 1 {
            2 * self.px(32)
        } else {
            self.px(32)
        };
        let height = self.px(36);
        Bounds {
            x: pad,
            y: pad + slot as i32 * height,
            width: self.bounds.width - 2 * pad - rail,
            height,
        }
    }

    fn close(self, tile: Bounds) -> Bounds {
        let side = self.px(32);
        let inset = if self.strip {
            self.px(4)
        } else {
            (tile.height - side) / 2
        };
        Bounds {
            x: tile.x + tile.width - side - inset,
            y: tile.y + inset,
            width: side,
            height: side,
        }
    }

    fn scroll(self, down: bool) -> Bounds {
        let side = self.px(32);
        let pad = self.px(8);
        let mut x = self.bounds.width - pad - side;
        let mut y = pad;
        if self.rows == 1 {
            x -= if down { 0 } else { side };
            y += (self.px(36) - side) / 2;
        } else if down {
            y = self.bounds.height - pad - side;
        }
        Bounds {
            x,
            y,
            width: side,
            height: side,
        }
    }

    fn window_hit(self, slot: usize, key: WindowKey, point: POINT) -> Option<Hit> {
        let tile = self.tile(slot);
        contains(tile, point).then(|| {
            if inside_circle(self.close(tile), point) {
                Hit::Close(key)
            } else {
                Hit::Activate(key)
            }
        })
    }
}

fn anchored(at: Placement, width: i32, height: i32, y: i32) -> Bounds {
    Bounds {
        x: (at.slot_x - width / 2).clamp(at.work.x, at.work.x + at.work.width - width),
        y,
        width,
        height,
    }
}

fn rect(bounds: Bounds) -> RECT {
    RECT {
        left: bounds.x,
        top: bounds.y,
        right: bounds.x + bounds.width,
        bottom: bounds.y + bounds.height,
    }
}

fn contains(bounds: Bounds, point: POINT) -> bool {
    let area = rect(bounds);
    (area.left..area.right).contains(&point.x) && (area.top..area.bottom).contains(&point.y)
}

fn inside_circle(bounds: Bounds, point: POINT) -> bool {
    if !contains(bounds, point) {
        return false;
    }
    let side = bounds.width;
    let dx = 2 * (point.x - bounds.x) + 1 - side;
    let dy = 2 * (point.y - bounds.y) + 1 - side;
    dx * dx + dy * dy <= side * side
}

fn tint(body: Rgb, ink: Rgb, percent: u16) -> u32 {
    let mix = |base: u8, top: u8| {
        ((u16::from(base) * (100 - percent) + u16::from(top) * percent) / 100) as u8
    };
    color((mix(body.0, ink.0), mix(body.1, ink.1), mix(body.2, ink.2)))
}

fn inset(bounds: Bounds, by: i32) -> Bounds {
    Bounds {
        x: bounds.x + by,
        y: bounds.y + by,
        width: bounds.width - 2 * by,
        height: bounds.height - 2 * by,
    }
}

fn within(bounds: Bounds, size: (i32, i32)) -> Option<Bounds> {
    let (width, height) = size;
    if width <= 0 || height <= 0 {
        return None;
    }
    let scale = (f64::from(bounds.width) / f64::from(width))
        .min(f64::from(bounds.height) / f64::from(height))
        .min(1.0);
    let width = (f64::from(width) * scale).round().max(1.0) as i32;
    let height = (f64::from(height) * scale).round().max(1.0) as i32;
    Some(Bounds {
        x: bounds.x + (bounds.width - width) / 2,
        y: bounds.y + (bounds.height - height) / 2,
        width,
        height,
    })
}

fn on_screen(mut bounds: Bounds, panel: Bounds) -> Bounds {
    bounds.x += panel.x;
    bounds.y += panel.y;
    bounds
}

struct Tile {
    close: CloseButton,
    image: Thumbnail,
}

struct Entry {
    key: WindowKey,
    title: String,
    tile: Option<Tile>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Hit {
    Activate(WindowKey),
    Close(WindowKey),
    Scroll(bool),
}

pub(super) struct Panel {
    entries: Vec<Option<Entry>>,
    window: NativeWindow,
    layout: Layout,
    palette: Palette,
    offset: usize,
    hovered: Option<Hit>,
    pressed: Option<(HWND, Hit)>,
    wheel: i32,
}

impl Panel {
    pub(super) fn show(
        targets: Vec<WindowKey>,
        model: &WindowModel,
        executable: Option<&str>,
        palette: Palette,
        at: Placement,
    ) -> Result<Self> {
        let list = || {
            Layout::list(at, targets.len())
                .ok_or_else(|| Error::new(E_FAIL, "No room for a window list."))
        };
        let window = NativeWindow::create_overlay("ROQA Window Previews", Some(at.owner))?;
        let strip = Layout::strip(at, targets.len()).and_then(|layout| {
            let tiles: Result<Vec<_>> = targets
                .iter()
                .enumerate()
                .map(|(slot, &target)| {
                    let image = Thumbnail::create(window.0, target)?;
                    let tile = layout.tile(slot);
                    let well = Bounds {
                        height: layout.px(106),
                        ..tile
                    };
                    let bounds = within(inset(well, layout.px(8)), image.size()?)
                        .ok_or_else(|| Error::new(E_FAIL, "No thumbnail image."))?;
                    image.place(on_screen(bounds, layout.bounds))?;
                    let close = CloseButton::create(image.window.0, at.dpi, palette)?;
                    close.place_at(on_screen(layout.close(tile), layout.bounds))?;
                    Ok(Tile { close, image })
                })
                .collect();
            tiles.ok().map(|tiles| (layout, tiles))
        });
        let (layout, tiles) = match strip {
            Some(strip) => strip,
            None => (list()?, Vec::new()),
        };
        let app = executable
            .and_then(|path| std::path::Path::new(path).file_stem())
            .and_then(std::ffi::OsStr::to_str)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or("Window");
        let mut tiles = tiles.into_iter();
        let entries = targets
            .into_iter()
            .map(|key| {
                let title = model
                    .windows
                    .iter()
                    .find(|window| window.key == key)
                    .map(|window| window.title.trim())
                    .filter(|title| !title.is_empty())
                    .unwrap_or(app);
                Some(Entry {
                    key,
                    title: title.to_owned(),
                    tile: tiles.next(),
                })
            })
            .collect();
        let panel = Self {
            entries,
            window,
            layout,
            palette,
            offset: 0,
            hovered: None,
            pressed: None,
            wheel: 0,
        };
        panel.repaint()?;
        panel.window.show();
        for tile in panel
            .entries
            .iter()
            .flatten()
            .filter_map(|entry| entry.tile.as_ref())
        {
            tile.image.window.show();
            tile.close.show();
        }
        Ok(panel)
    }

    fn owns(&self, hwnd: HWND) -> bool {
        hwnd == self.window.0
            || self.layout.strip
                && self.entries.iter().flatten().any(|entry| {
                    entry.tile.as_ref().is_some_and(|tile| {
                        [tile.image.window.0, tile.close.hwnd()].contains(&hwnd)
                    })
                })
    }

    pub(super) fn under_pointer(&self) -> bool {
        // SAFETY: queries the scalar capture HWND on the host thread.
        self.owns(pointed_window()) || self.owns(unsafe { GetCapture() })
    }

    pub(super) fn refresh(&mut self, windows: &[WindowKey]) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry
                .as_ref()
                .is_some_and(|entry| !windows.contains(&entry.key))
            {
                *entry = None;
                changed = true;
            }
        }
        let any = self.entries.iter().any(Option::is_some);
        if changed && any {
            self.cancel_press();
            self.hovered = None;
            let _ = self.repaint();
        }
        any
    }

    fn can_scroll(&self, down: bool) -> bool {
        self.layout.overflow
            && if down {
                self.offset + self.layout.rows < self.entries.len()
            } else {
                self.offset > 0
            }
    }

    fn hit(&self, point: POINT) -> Option<Hit> {
        let point = POINT {
            x: point.x - self.layout.bounds.x,
            y: point.y - self.layout.bounds.y,
        };
        for down in [false, true] {
            if self.can_scroll(down) && contains(self.layout.scroll(down), point) {
                return Some(Hit::Scroll(down));
            }
        }
        self.entries[self.offset..][..self.layout.rows]
            .iter()
            .enumerate()
            .find_map(|(slot, entry)| self.layout.window_hit(slot, entry.as_ref()?.key, point))
    }

    fn cancel_press(&mut self) {
        if let Some((window, _)) = self.pressed.take() {
            // SAFETY: releases only this panel's own capture.
            unsafe {
                if GetCapture() == window {
                    let _ = ReleaseCapture();
                }
            }
        }
    }

    fn scroll(&mut self, steps: i32) {
        self.cancel_press();
        let offset = self
            .offset
            .saturating_add_signed(steps as isize)
            .min(self.entries.len() - self.layout.rows);
        if offset != self.offset {
            self.offset = offset;
            self.hovered = None;
            let _ = self.repaint();
        }
    }

    pub(super) fn message(&mut self, message: &MSG) -> bool {
        if !self.owns(message.hwnd) {
            return false;
        }
        let thumbnails = if self.layout.strip {
            self.layout.rows
        } else {
            0
        };
        for entry in self.entries.iter_mut().take(thumbnails).flatten() {
            if let Some(tile) = &mut entry.tile
                && message.hwnd == tile.close.hwnd()
            {
                if tile.close.message(message) {
                    dock::close(&[entry.key]);
                }
                return false;
            }
        }
        match message.message {
            WM_MOUSEMOVE => {
                if message.wParam.0 & 1 == 0 {
                    self.cancel_press();
                }
                track_leave(message.hwnd);
                let hovered = self.hit(message.pt);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    let _ = self.repaint();
                }
            }
            WM_MOUSELEAVE => {
                if !self.owns(pointed_window()) && self.hovered.take().is_some() {
                    let _ = self.repaint();
                }
            }
            WM_LBUTTONDOWN => {
                self.cancel_press();
                if let Some(hit) = self.hit(message.pt) {
                    // SAFETY: message HWND is one of this panel's live windows.
                    unsafe {
                        SetCapture(message.hwnd);
                    }
                    self.pressed = Some((message.hwnd, hit));
                }
            }
            WM_LBUTTONUP => {
                // SAFETY: scalar queries; an overlapping foreign window is not part of the panel.
                let (capture, over) = unsafe { (GetCapture(), WindowFromPoint(message.pt)) };
                let hit = self.owns(over).then(|| self.hit(message.pt)).flatten();
                let matched = self
                    .pressed
                    .is_some_and(|(window, action)| window == capture && Some(action) == hit);
                self.cancel_press();
                if matched {
                    match hit {
                        Some(Hit::Activate(key)) => {
                            return dock::perform(dock::Action::Activate(key));
                        }
                        Some(Hit::Close(key)) => dock::close(&[key]),
                        Some(Hit::Scroll(down)) => self.scroll(if down { 1 } else { -1 }),
                        None => {}
                    }
                }
            }
            WM_CANCELMODE | WM_CAPTURECHANGED => self.cancel_press(),
            WM_MOUSEWHEEL if self.layout.overflow => {
                self.wheel += (message.wParam.0 >> 16) as i16 as i32;
                let steps = self.wheel / 120;
                self.wheel %= 120;
                self.scroll(-steps);
            }
            _ => {}
        }
        false
    }

    fn repaint(&self) -> Result<()> {
        let layout = self.layout;
        let body = (
            self.palette.body.0,
            self.palette.body.1,
            self.palette.body.2,
        );
        let tile = color((
            self.palette.tile.0,
            self.palette.tile.1,
            self.palette.tile.2,
        ));
        let ink = color(self.palette.ink);
        let hover = tint(body, self.palette.ink, 14);
        let disabled = tint(body, self.palette.ink, 36);
        let body = color(body);
        let size = 12.0 * layout.dpi as f32 / 96.0;
        self.window.present(layout.bounds, |dc, panel| {
            batch_gdi(dc, panel, |draw| {
                draw.round(panel, layout.px(20), body, body);
                for (slot, entry) in self.entries[self.offset..][..layout.rows]
                    .iter()
                    .enumerate()
                {
                    let Some(entry) = entry else {
                        continue;
                    };
                    let cell = layout.tile(slot);
                    let fill = if self.hovered == Some(Hit::Activate(entry.key)) {
                        hover
                    } else {
                        tile
                    };
                    draw.round(
                        rect(cell),
                        layout.px(if layout.strip { 12 } else { 18 }),
                        fill,
                        fill,
                    );
                    let mut title = rect(cell);
                    title.left += layout.px(8);
                    title.right -= layout.px(8);
                    if layout.strip {
                        title.top += layout.px(106);
                    } else {
                        let close = layout.close(cell);
                        title.right = close.x - layout.px(4);
                        let active = self.hovered == Some(Hit::Close(entry.key));
                        let (fill, glyph) = if active {
                            (ink, color(self.palette.on_ink))
                        } else {
                            (body, ink)
                        };
                        draw.round(rect(close), close.width / 2, fill, fill);
                        draw.glyph(
                            rect(inset(close, (close.width - close.width * 20 / 32) / 2)),
                            Glyph::Close,
                            glyph,
                        );
                    }
                    draw.title(title, &entry.title, size, ink);
                }
                if layout.overflow {
                    for down in [false, true] {
                        let control = layout.scroll(down);
                        let fill = if self.hovered == Some(Hit::Scroll(down)) {
                            hover
                        } else {
                            tile
                        };
                        let glyph = if self.can_scroll(down) { ink } else { disabled };
                        draw.round(rect(control), control.width / 2, fill, fill);
                        let label = inset(control, layout.px(10));
                        draw.title(rect(label), if down { "⌄" } else { "⌃" }, size, glyph);
                    }
                }
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(dpi: u32) -> Placement {
        Placement {
            owner: HWND::default(),
            slot_x: -990,
            dpi,
            dock: Bounds {
                x: -990,
                y: 615,
                width: 360,
                height: 65,
            },
            work: Bounds {
                x: -1000,
                y: -120,
                width: 1000,
                height: 800,
            },
        }
    }

    fn centre(bounds: Bounds) -> POINT {
        POINT {
            x: bounds.x + bounds.width / 2,
            y: bounds.y + bounds.height / 2,
        }
    }

    #[test]
    fn strip_keeps_each_preview_and_close_on_the_origin_monitor() {
        let at = placement(144);
        let layout = Layout::strip(at, 3).unwrap();
        assert_eq!(layout.bounds.x, at.work.x);
        assert_eq!(layout.bounds.width, 840);
        assert_eq!(layout.bounds.y + layout.bounds.height, at.dock.y - 12);
        for slot in 0..3 {
            let key = WindowKey {
                handle: slot + 1,
                process: 3,
            };
            let tile = layout.tile(slot);
            assert_eq!(
                layout.window_hit(slot, key, centre(tile)),
                Some(Hit::Activate(key))
            );
            assert_eq!(
                layout.window_hit(slot, key, centre(layout.close(tile))),
                Some(Hit::Close(key))
            );
            assert_eq!(layout.window_hit((slot + 1) % 3, key, centre(tile)), None);
        }
    }

    #[test]
    fn one_row_overflow_fits_below_with_independent_scroll_controls() {
        let mut at = placement(96);
        at.work.width = 180;
        at.work.height = 140;
        at.dock.y = -105;
        at.dock.height = 60;
        assert!(Layout::strip(at, 2).is_none());
        let list = Layout::list(at, 12).unwrap();
        assert_eq!(list.rows, 1);
        assert!(list.overflow);
        assert_eq!(list.bounds.y, -37);
        let down = list.scroll(true);
        let up = list.scroll(false);
        assert_eq!(up.y, down.y);
        assert_eq!(up.x + up.width, down.x);
        let key = WindowKey {
            handle: 12,
            process: 2,
        };
        assert_eq!(
            list.window_hit(0, key, centre(list.close(list.tile(0)))),
            Some(Hit::Close(key))
        );
        assert_eq!(list.window_hit(0, key, centre(up)), None);
        assert_eq!(list.window_hit(0, key, centre(down)), None);
    }

    #[test]
    fn thumbnails_never_upscale_and_extreme_sources_keep_a_visible_pixel() {
        let well = Bounds {
            x: 8,
            y: 8,
            width: 160,
            height: 90,
        };
        assert_eq!(within(well, (4, 8)).unwrap().width, 4);
        assert_eq!(within(well, (1920, 1080)).unwrap(), well);
        assert_eq!(within(well, (2560, 2)).unwrap().height, 1);
        assert!(within(well, (0, 8)).is_none());
    }
}
