//! Laying out one page: the group headings, the filled panels and each row's controls, in the page's own
//! coordinates (its content window's). Run when a page is shown or the size, font or text changes, never to scroll.

use super::*;

/// What the page paints for itself: group headings, panels and the boxes round edits (content coordinates).
#[derive(Default)]
pub(super) struct Geometry {
    pub labels: Vec<(RECT, &'static str)>,
    pub cards: Vec<RECT>,
    pub fields: Vec<RECT>,
    pub displays: Vec<displays::Face>,
    pub display_hint: Option<RECT>,
    pub display_empty: Option<RECT>,
}

/// DIPs: a closed drop-down's height, the edit boxes' and the buttons', and a list row's.
pub(super) const BOX: i32 = 32;
const LIST_ROW: i32 = 36;

/// How wide a row's value control is, in DIPs: numbers 80, the timer 128, the usual pickers 216, long pickers
/// and path fields 260.
pub(super) fn control_width(kind: Kind) -> i32 {
    match kind {
        Kind::Combo(w) if w >= 230 => 260,
        Kind::Combo(_) => 216,
        Kind::Edit(w) if w <= 70 => 80,
        Kind::Edit(w) if w <= 120 => 128,
        Kind::Edit(_) => 260,
        _ => 0,
    }
}

/// Sizes a drop-down: its closed box exactly `box_px` high (the control adds its own borders to the field's
/// height, so the first try is corrected by what it came to), list rows `LIST_ROW` high, and at most eight rows
/// shown before the list scrolls (the default is thirty).
pub(super) fn size_combo(h: HWND, box_px: i32, row_px: i32) {
    // SAFETY: messages and a rectangle query on our own live combo box.
    unsafe {
        let set = |which: usize, height: i32| {
            SendMessageW(
                h,
                CB_SETITEMHEIGHT,
                Some(WPARAM(which)),
                Some(LPARAM(height as isize)),
            )
        };
        SendMessageW(h, CB_SETMINVISIBLE, Some(WPARAM(8)), None);
        set(0, row_px);
        set(usize::MAX, box_px - 6);
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_ok() && r.bottom - r.top != box_px {
            set(usize::MAX, box_px - 6 + box_px - (r.bottom - r.top));
        }
    }
}

impl SettingsWindow {
    /// Moves page `page`'s controls to their places in a content window `width` pixels wide and returns what
    /// the page paints and its height. Captions are measured with `font` on a DC of the window, so a wrapped
    /// caption gets its lines.
    pub(super) fn place(
        &self,
        page: usize,
        scale: f64,
        width: i32,
        font: HFONT,
    ) -> Result<(Geometry, i32)> {
        // SAFETY: a common DC of our own live window, given the live font and released before returning.
        let dc = unsafe {
            let dc = GetDC(Some(self.window));
            SelectObject(dc, font.into());
            dc
        };
        let placed = self.place_at(page, scale, width, dc);
        // SAFETY: the DC taken above.
        unsafe { ReleaseDC(Some(self.window), dc) };
        placed
    }

    fn place_at(&self, page: usize, scale: f64, width: i32, dc: HDC) -> Result<(Geometry, i32)> {
        let px = |v: i32| (f64::from(v) * scale).round() as i32;
        let mut geometry = Geometry::default();
        let mut y = px(4);
        for (index, card) in PAGES[page].cards.iter().enumerate() {
            geometry.labels.push((
                RECT {
                    left: 0,
                    top: y,
                    right: width,
                    bottom: y + px(22),
                },
                card.label,
            ));
            y += px(22 + 6);
            if page == 0 && index == DISPLAYS_CARD {
                geometry.display_hint = Some(RECT {
                    left: 0,
                    top: y - px(2),
                    right: width,
                    bottom: y + px(14),
                });
                y += px(20);
                let top = y;
                y = self.place_displays(width, y, scale, dc, &mut geometry)?;
                geometry.cards.push(RECT {
                    left: 0,
                    top,
                    right: width,
                    bottom: y,
                });
                y += px(16);
                continue;
            }
            let top = y;
            y += px(4);
            let rows = self
                .items
                .iter()
                .filter(|i| usize::from(i.page) == page && usize::from(i.card) == index);
            for item in rows {
                let (height, field) = self.row(item, scale, (px(16), width - px(16)), y, dc)?;
                item.row.set((y, y + height));
                geometry.fields.extend(field);
                y += height;
            }
            y += px(4);
            geometry.cards.push(RECT {
                left: 0,
                top,
                right: width,
                bottom: y,
            });
            y += px(16);
        }
        Ok((geometry, y - px(16) + px(12)))
    }

    /// Places one row from `y` and returns its height: a label or caption that wraps makes the row taller.
    fn row(
        &self,
        item: &Item,
        scale: f64,
        (a, b): (i32, i32),
        y: i32,
        dc: HDC,
    ) -> Result<(i32, Option<RECT>)> {
        let px = |v: i32| (f64::from(v) * scale).round() as i32;
        // The wrapped height of a static's text at width `w` (0 when there is none).
        let lines = |h: HWND, w: i32| {
            if h.0.is_null() {
                return 0;
            }
            let mut words: Vec<u16> = text(h, 256).encode_utf16().collect();
            let mut r = RECT {
                right: w.max(1),
                ..Default::default()
            };
            // SAFETY: measures a local buffer on the caller's live DC.
            unsafe { DrawTextW(dc, &mut words, &mut r, DT_CALCRECT | DT_WORDBREAK) };
            r.bottom
        };
        let put = |h: HWND, x: i32, y: i32, w: i32, ht: i32| -> Result<()> {
            if h.0.is_null() {
                return Ok(());
            }
            // SAFETY: our own live child. NOCOPYBITS: a moved control repaints itself.
            unsafe {
                SetWindowPos(
                    h,
                    None,
                    x,
                    y,
                    w.max(1),
                    ht,
                    SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOCOPYBITS,
                )?;
            }
            Ok(())
        };
        match item.kind {
            Kind::Check => {
                let (w, cap) = (b - a - px(58), lines(item.cap, b - a - px(58)));
                let top = if cap > 0 { px(6) } else { 0 };
                let height = if cap > 0 {
                    (px(32) + cap).max(px(52))
                } else {
                    px(42)
                };
                put(item.h, a, y + top, b - a, height - top)?;
                put(item.cap, a, y + px(30), w, cap)?;
                if !item.cap.0.is_null() {
                    // The switch spans the caption's line; above it the caption shows (it was hidden under it).
                    // SAFETY: our own live child; only its place in the stacking order changes.
                    let _ = unsafe {
                        SetWindowPos(
                            item.cap,
                            Some(HWND_TOP),
                            0,
                            0,
                            0,
                            0,
                            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                        )
                    };
                }
                Ok((height, None))
            }
            Kind::Combo(_) | Kind::Edit(_) => {
                let w = px(control_width(item.kind)).min(b - a);
                let side = b - a - w - px(8);
                // Under 160 DIPs left for the label, the value goes under it instead of beside it.
                let stacked = side < px(160);
                let wide = if stacked { b - a } else { side };
                let (l, c) = (lines(item.label, wide).max(px(20)), lines(item.cap, wide));
                let height = match (stacked, c > 0) {
                    (true, _) => px(8) + l + c + px(6) + px(BOX) + px(8),
                    (false, true) => (px(10) + l + c).max(px(52)),
                    (false, false) => (l + px(20)).max(px(42)),
                };
                let boxed = if stacked {
                    let top = y + height - px(8) - px(BOX);
                    RECT {
                        left: a,
                        top,
                        right: a + w,
                        bottom: top + px(BOX),
                    }
                } else {
                    let top = y + (height - px(BOX)) / 2;
                    RECT {
                        left: b - w,
                        top,
                        right: b,
                        bottom: top + px(BOX),
                    }
                };
                let field = if matches!(item.kind, Kind::Edit(_)) {
                    put(
                        item.h,
                        boxed.left + px(14),
                        boxed.top + px(7),
                        w - px(28),
                        px(18),
                    )?;
                    Some(boxed)
                } else {
                    size_combo(item.h, px(BOX), px(LIST_ROW));
                    put(item.h, boxed.left, boxed.top, w, px(BOX))?;
                    None
                };
                let top = if stacked || c > 0 {
                    y + px(8)
                } else {
                    y + (height - l) / 2
                };
                put(item.label, a, top, wide, l)?;
                put(item.cap, a, top + l, wide, c)?;
                Ok((height, field))
            }
            Kind::Btn => {
                // The button is as wide as its text plus margins (never narrower than 96 DIP), so no label clips.
                let mut words: Vec<u16> = text(item.h, 256).encode_utf16().collect();
                let mut measured = RECT::default();
                // SAFETY: measures a local buffer on the caller's live DC.
                unsafe { DrawTextW(dc, &mut words, &mut measured, DT_CALCRECT | DT_SINGLELINE) };
                let width = (measured.right + px(32)).max(px(96)).min(b - a);
                let side = b - a - width - px(8);
                let l = lines(item.label, side);
                let height = (l + px(20)).max(px(42));
                put(
                    item.h,
                    b - width,
                    y + (height - px(BOX)) / 2,
                    width,
                    px(BOX),
                )?;
                put(item.label, a, y + (height - l) / 2, side, l)?;
                Ok((height, None))
            }
            Kind::Note => {
                let l = lines(item.label, b - a).max(px(20));
                put(item.label, a, y + px(11), b - a, l)?;
                Ok(((l + px(22)).max(px(42)), None))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_controls_have_the_specified_widths() {
        assert_eq!(control_width(Kind::Edit(70)), 80, "numbers");
        assert_eq!(control_width(Kind::Edit(120)), 128, "the timer");
        assert_eq!(control_width(Kind::Edit(200)), 260, "paths");
        assert_eq!(control_width(Kind::Edit(220)), 260);
        assert_eq!(control_width(Kind::Combo(120)), 216, "the usual pickers");
        assert_eq!(control_width(Kind::Combo(170)), 216);
        assert_eq!(control_width(Kind::Combo(240)), 260, "long pickers");
        assert_eq!(control_width(Kind::Check), 0);
    }
}
