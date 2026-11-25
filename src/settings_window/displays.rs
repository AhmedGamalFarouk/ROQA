use super::*;

pub(super) const HINT: &str = "Choose where ROQA appears.";

pub(super) struct Face {
    identity: String,
    metadata: String,
    tile: bool,
    stacked: bool,
    meta_height: i32,
}

fn map(rects: &[Bounds], width: i32, height: i32, inset: i32, single: i32) -> Option<Vec<RECT>> {
    let left = rects.iter().map(|r| r.x).min()?;
    let top = rects.iter().map(|r| r.y).min()?;
    let right = rects.iter().map(|r| r.x + r.width).max()?;
    let bottom = rects.iter().map(|r| r.y + r.height).max()?;
    for (index, rect) in rects.iter().enumerate() {
        if rect.width <= 0
            || rect.height <= 0
            || rects[..index].iter().any(|other| {
                rect.x < other.x + other.width
                    && other.x < rect.x + rect.width
                    && rect.y < other.y + other.height
                    && other.y < rect.y + rect.height
            })
        {
            return None;
        }
    }
    let (span_width, span_height) = (f64::from(right - left), f64::from(bottom - top));
    let limit = if rects.len() == 1 {
        width.min(single)
    } else {
        width
    };
    let fit = (f64::from(limit) / span_width).min(f64::from(height) / span_height);
    let x = |value| {
        ((f64::from(width) - span_width * fit) / 2.0 + f64::from(value - left) * fit).round() as i32
    };
    let y = |value| {
        ((f64::from(height) - span_height * fit) / 2.0 + f64::from(value - top) * fit).round()
            as i32
    };
    Some(
        rects
            .iter()
            .map(|r| RECT {
                left: x(r.x) + inset,
                top: y(r.y) + inset,
                right: x(r.x + r.width) - inset,
                bottom: y(r.y + r.height) - inset,
            })
            .collect(),
    )
}

fn measure(dc: HDC, label: &str, width: i32) -> RECT {
    let mut words: Vec<u16> = label.encode_utf16().collect();
    let mut rect = RECT {
        right: width.max(1),
        ..Default::default()
    };
    // SAFETY: initialized text and rectangle on the live layout DC.
    unsafe {
        DrawTextW(
            dc,
            &mut words,
            &mut rect,
            DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
        )
    };
    rect
}

fn grouped(value: i32) -> String {
    if value >= 1000 {
        format!("{},{:03}", value / 1000, value % 1000)
    } else {
        value.to_string()
    }
}

impl SettingsWindow {
    pub(super) fn is_primary_display(&self, index: usize) -> bool {
        let at_origin = |monitor: &Monitor| monitor.rect.x == 0 && monitor.rect.y == 0;
        at_origin(&self.monitors[index])
            && self
                .monitors
                .iter()
                .filter(|monitor| at_origin(monitor))
                .count()
                == 1
    }

    pub(super) fn place_displays(
        &self,
        width: i32,
        top: i32,
        scale: f64,
        dc: HDC,
        geometry: &mut Geometry,
    ) -> Result<i32> {
        let px = |dip| (f64::from(dip) * scale).round() as i32;
        if self.monitors.is_empty() {
            geometry.display_empty = Some(RECT {
                left: px(16),
                right: width - px(16),
                top,
                bottom: top + px(44),
            });
            return Ok(top + px(44));
        }
        let inset = px(16).min(width / 4);
        let inner = width - 2 * inset;
        let rects: Vec<_> = self.monitors.iter().map(|monitor| monitor.rect).collect();
        let mut faces: Vec<_> = self
            .monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| {
                let identity = if self.is_primary_display(index) {
                    format!("{}  Primary", index + 1)
                } else {
                    (index + 1).to_string()
                };
                Face {
                    identity,
                    metadata: format!(
                        "{} × {} · {}%",
                        grouped(monitor.rect.width),
                        grouped(monitor.rect.height),
                        monitor.dpi * 100 / 96
                    ),
                    tile: true,
                    stacked: false,
                    meta_height: px(16),
                }
            })
            .collect();
        let tiles = map(&rects, inner, px(160), px(4), px(264)).filter(|tiles| {
            tiles.iter().zip(&faces).all(|(tile, face)| {
                let minimum_width = px(128).max(measure(dc, &face.metadata, 4096).right + px(24));
                tile.right - tile.left >= minimum_width && tile.bottom - tile.top >= px(96)
            })
        });
        // SAFETY: reads a style bit of our own live Settings dialog.
        let rtl =
            unsafe { GetWindowLongPtrW(self.window, GWL_EXSTYLE) } as u32 & WS_EX_LAYOUTRTL.0 != 0;
        let mut order: Vec<_> = (0..rects.len()).collect();
        order.sort_by_key(|&index| {
            let (left, top) = tiles
                .as_ref()
                .map_or((rects[index].x, rects[index].y), |tiles| {
                    (tiles[index].left, tiles[index].top)
                });
            (top, if rtl { -left } else { left }, index)
        });
        let mut next = top + px(12);
        let mut previous = self.h(NOTCH);
        for index in order {
            let face = &mut faces[index];
            let rect = if let Some(tiles) = &tiles {
                let tile = tiles[index];
                RECT {
                    left: tile.left + inset,
                    top: tile.top + top + px(12),
                    right: tile.right + inset,
                    bottom: tile.bottom + top + px(12),
                }
            } else {
                face.identity = format!("Display {}", face.identity);
                face.tile = false;
                face.stacked = inner < px(268);
                let text_width = inner - px(if face.stacked { 36 } else { 140 });
                face.meta_height = measure(dc, &face.metadata, text_width).bottom.max(px(16));
                let height =
                    (px(32) + face.meta_height + if face.stacked { px(26) } else { 0 }).max(px(52));
                let row = RECT {
                    left: inset,
                    top: next,
                    right: width - inset,
                    bottom: next + height,
                };
                next = row.bottom + px(4);
                row
            };
            let Some(item) = self.items.iter().find(|item| item.id == MONITOR + index) else {
                continue;
            };
            // SAFETY: our own live sibling controls; the order is also the dialog manager's Tab order.
            unsafe {
                SetWindowPos(
                    item.h,
                    Some(previous),
                    rect.left,
                    rect.top,
                    (rect.right - rect.left).max(1),
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_NOCOPYBITS,
                )?
            };
            item.row.set((rect.top, rect.bottom));
            previous = item.h;
        }
        geometry.displays = faces;
        Ok(if tiles.is_some() {
            top + px(184)
        } else {
            next + px(8)
        })
    }
}

pub(super) fn draw(note: &NMCUSTOMDRAW, p: &Paint, face: &Face) {
    let px = |dip| (f64::from(dip) * p.scale).round() as i32;
    let (dc, rect, hwnd) = (note.hdc, note.rc, note.hdr.hwndFrom);
    let on = checked(hwnd);
    let focused = focus_ring(hwnd);
    let centre = (rect.left + rect.right) / 2;
    let mid = (rect.top + rect.bottom) / 2;
    let hot = note.uItemState.0 & (CDIS_HOT.0 | CDIS_SELECTED.0) != 0;
    let quiet = (p.pal.card & 0xfefefe) / 2 + (p.pal.sel & 0xfefefe) / 2;
    let surface = if p.pal.contrast {
        p.pal.well
    } else if on || hot {
        p.pal.sel
    } else {
        quiet
    };
    let outline = if p.pal.contrast { p.pal.text } else { surface };
    let switch_left = if face.tile || face.stacked {
        centre + px(4)
    } else {
        rect.right - px(54)
    };
    let switch_mid = if face.tile {
        mid + px(27)
    } else if face.stacked {
        rect.bottom - px(16)
    } else {
        mid
    };
    let switch = RECT {
        left: switch_left,
        top: switch_mid - px(12),
        right: switch_left + px(42),
        bottom: switch_mid + px(12),
    };
    let knob_left = if on {
        switch.right - px(20)
    } else {
        switch.left + px(4)
    };
    let knob = RECT {
        left: knob_left,
        top: switch_mid - px(8),
        right: knob_left + px(16),
        bottom: switch_mid + px(8),
    };
    let focus = RECT {
        left: rect.left + px(6),
        top: mid - px(10),
        right: rect.left + px(9),
        bottom: mid + px(10),
    };
    let track = swap(if on { p.pal.accent } else { p.pal.off });
    // SAFETY: the notification supplies the live paint DC and rectangle.
    unsafe {
        fill(dc, rect, p.pal.card);
        batch_gdi(dc, rect, |batch| {
            batch.round(rect, px(8), swap(surface), swap(outline));
            batch.round(switch, px(12), track, track);
            batch.round(knob, px(8), swap(p.pal.thumb), swap(p.pal.thumb));
            if focused {
                batch.round(focus, px(2), swap(p.pal.text), swap(p.pal.text));
            }
        });
        let left = rect.left + px(18);
        let right = if face.tile || face.stacked {
            rect.right - px(if face.tile { 18 } else { 12 })
        } else {
            switch.left - px(68)
        };
        let (title_top, title_bottom) = if face.tile {
            (mid - px(40), mid - px(10))
        } else {
            (rect.top + px(6), rect.top + px(26))
        };
        let alignment = if face.tile { DT_CENTER } else { DT_LEFT };
        let title = RECT {
            left,
            right: if face.tile {
                rect.right - px(18)
            } else {
                right
            },
            top: title_top,
            bottom: title_bottom,
        };
        SelectObject(dc, p.bold.into());
        if face.tile {
            let (number, primary) = face
                .identity
                .split_once("  ")
                .unwrap_or((&face.identity, ""));
            SelectObject(dc, p.display_number.into());
            let number_width = measure(dc, number, 4096).right;
            SelectObject(dc, p.display_meta.into());
            let primary_width = if primary.is_empty() {
                0
            } else {
                measure(dc, primary, 4096).right + px(6)
            };
            let start = centre - (number_width + primary_width) / 2;
            write(
                dc,
                primary,
                RECT {
                    left: start + number_width + px(6),
                    ..title
                },
                p.pal.dim,
                DT_LEFT | DT_NOPREFIX,
            );
            SelectObject(dc, p.display_number.into());
            write(
                dc,
                number,
                RECT {
                    left: start,
                    ..title
                },
                p.pal.text,
                DT_LEFT | DT_NOPREFIX,
            );
        } else {
            write(dc, &face.identity, title, p.pal.text, DT_LEFT | DT_NOPREFIX);
        }
        SelectObject(dc, p.display_meta.into());
        let metadata = RECT {
            top: title.bottom + px(2),
            bottom: title.bottom + px(2) + face.meta_height,
            ..title
        };
        if face.tile {
            write(
                dc,
                &face.metadata,
                metadata,
                p.pal.dim,
                alignment | DT_NOPREFIX,
            );
        } else {
            let (mut metadata, mut words) =
                (metadata, face.metadata.encode_utf16().collect::<Vec<_>>());
            SetTextColor(dc, rgb(p.pal.dim));
            SetBkMode(dc, TRANSPARENT);
            DrawTextW(
                dc,
                &mut words,
                &mut metadata,
                DT_LEFT | DT_WORDBREAK | DT_NOPREFIX,
            );
        }
        write(
            dc,
            if on { "On" } else { "Off" },
            RECT {
                left: rect.left,
                right: switch.left - px(8),
                top: switch.top,
                bottom: switch.bottom,
            },
            p.pal.text,
            DT_RIGHT | DT_NOPREFIX,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_preserves_negative_offsets_and_gaps_and_rejects_overlaps() {
        let left = Bounds {
            x: -1920,
            y: -200,
            width: 1920,
            height: 1080,
        };
        let primary = Bounds { x: 0, y: 0, ..left };
        let tiles = map(&[left, primary], 560, 160, 4, 264).unwrap();
        assert_eq!(tiles[1].left - tiles[0].right, 8);
        assert!(tiles[0].top < tiles[1].top);
        assert_eq!(
            tiles[0].right - tiles[0].left,
            tiles[1].right - tiles[1].left
        );
        assert!(map(&[primary, primary], 560, 160, 4, 264).is_none());
        let single = map(&[primary], 560, 160, 4, 264).unwrap()[0];
        assert_eq!(single.left + single.right, 560);
        assert!(single.right - single.left <= 256);
    }
}
