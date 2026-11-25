//! How Settings draws its own parts (the rail, switches, drop-down boxes, buttons) and the two window procedures that route
//! the controls' messages: the dialog's (colours, custom draw) and the drop-down subclass.

use super::*;

/// A rail row: a 40 high capsule (12 in from each side) with the destination's glyph and name; the selected one
/// is a lighter fill with the accent dot at its end and its glyph in the accent.
fn draw_side(item: &DRAWITEMSTRUCT, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let Some(route) = ROUTES.get(item.itemID as usize) else {
        return;
    };
    let (dc, row) = (item.hDC, item.rcItem);
    let cell = RECT {
        left: row.left + px(12),
        top: row.top + px(2),
        right: row.right - px(12),
        bottom: row.bottom - px(2),
    };
    let middle = (cell.top + cell.bottom) / 2;
    let tile = RECT {
        left: cell.left + px(10),
        top: middle - px(10),
        right: cell.left + px(30),
        bottom: middle + px(10),
    };
    let dot = RECT {
        left: cell.right - px(12),
        top: middle - px(3),
        right: cell.right - px(6),
        bottom: middle + px(3),
    };
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    let focused = selected && focus_ring(item.hwndItem);
    let ink = if selected { p.pal.accent } else { p.pal.text };
    // SAFETY: the DC and rectangle come from this owner-draw message.
    unsafe {
        fill(dc, row, p.pal.side);
        batch_gdi(dc, row, |b| {
            if selected {
                ringed_in(b, cell, (cell.bottom - cell.top) / 2, p.pal.sel, focused, p);
                b.round(dot, px(3), swap(p.pal.accent), swap(p.pal.accent));
            }
            b.glyph(tile, route.glyph, swap(ink));
        });
        SelectObject(dc, p.font.into());
        let name = RECT {
            left: tile.right + px(10),
            right: if selected {
                dot.left - px(4)
            } else {
                cell.right - px(6)
            },
            ..cell
        };
        let colour = if selected { p.pal.seltext } else { p.pal.text };
        write(
            dc,
            route.name,
            name,
            colour,
            DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
    }
}

fn is_toggle(h: HWND) -> bool {
    // SAFETY: scalar query of a live child.
    (unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32) & 0xf == BS_AUTOCHECKBOX as u32
}

/// A checkbox drawn as a switch (the control stays a native checkbox for its state and clicks): the title on
/// the left, a pill with a thumb on the right.
fn draw_toggle(note: &NMCUSTOMDRAW, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let (dc, r, h) = (note.hdc, note.rc, note.hdr.hwndFrom);
    // SAFETY: scalar query of a live child.
    let style = unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32;
    let tall = style & 0xc00 == BS_TOP as u32;
    let on = checked(h);
    let mid = if tall {
        r.top + px(14)
    } else {
        (r.top + r.bottom) / 2
    };
    let pill = RECT {
        left: r.right - px(42),
        top: mid - px(12),
        right: r.right,
        bottom: mid + px(12),
    };
    // A 16 DIP circle, 4 in from the pill's ends and edges (rounded per side, so it is never 21 x 20 px).
    let knob_side = px(16);
    let (off_at, on_at) = (pill.left + px(4), pill.right - px(4) - knob_side);
    // The thumb slides 150 ms when the state changes: the control remembers (state, start tick) in a window
    // property, and while it moves the toggle invalidates itself, so nothing runs once it settles.
    let thumb = {
        let key = w!("roqa.slide");
        // SAFETY: a property of this live child and the tick count; no pointers are involved.
        let (state, now) = (unsafe { GetPropW(h, key) }.0 as u64, unsafe {
            GetTickCount64()
        });
        let (first, raw) = (state == 0, state.wrapping_sub(2));
        let (mut since, flipped) = (raw >> 1, !first && (raw & 1 == 1) != on);
        if first || flipped {
            // First sight settles at once (since 0); a flip starts now. Stored plus 2 so it is never 0.
            since = if first { 0 } else { now };
            // SAFETY: the value is a plain number stored as the property handle.
            let _ = unsafe {
                SetPropW(
                    h,
                    key,
                    Some(HANDLE((((since << 1) | u64::from(on)) + 2) as *mut _)),
                )
            };
        }
        let t = (now.saturating_sub(since) as f32 / 150.0).min(1.0);
        if t < 1.0 {
            // SAFETY: the child repaints itself once more.
            let _ = unsafe { InvalidateRect(Some(h), None, false) };
        }
        let (from, to) = if on { (off_at, on_at) } else { (on_at, off_at) };
        from + ((to - from) as f32 * motion::smooth(t)).round() as i32
    };
    let title = RECT {
        right: pill.left - px(8),
        bottom: if tall { r.top + px(28) } else { r.bottom },
        ..r
    };
    // SAFETY: the DC and rectangle come from this custom-draw notification.
    unsafe {
        fill(dc, r, p.pal.card);
        let track = if on { p.pal.accent } else { p.pal.off };
        let knob = RECT {
            left: thumb,
            top: mid - px(8),
            right: thumb + knob_side,
            bottom: mid + px(8),
        };
        let focused = focus_ring(h);
        batch_gdi(dc, r, |b| {
            ringed_in(b, pill, px(12), track, focused, p);
            // One knob rule in every theme: the same knob on and off, only the track changes.
            let knob_colour = swap(p.pal.thumb);
            b.round(knob, px(8), knob_colour, knob_colour);
        });
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.font.into());
        write(dc, &text(h, 256), title, p.pal.text, DT_LEFT);
    }
}

/// A drop-down row, owner-drawn: the closed field and the list rows on the card, the row under the pointer a
/// solid accent capsule with dark ink.
fn draw_combo(item: &DRAWITEMSTRUCT, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let (dc, row) = (item.hDC, item.rcItem);
    let field = item.itemState.0 & ODS_COMBOBOXEDIT.0 != 0;
    let lit = !field && item.itemState.0 & ODS_SELECTED.0 != 0;
    let mut buffer = [0u16; 128];
    // SAFETY: the combo copies one row's text into a local buffer of the length it is given.
    let length = unsafe {
        SendMessageW(
            item.hwndItem,
            CB_GETLBTEXT,
            Some(WPARAM(item.itemID as usize)),
            Some(LPARAM(buffer.as_mut_ptr() as isize)),
        )
    }
    .0
    .clamp(0, 127) as usize;
    let (back, ink) = if lit {
        (p.pal.accent, p.pal.on_accent)
    } else {
        (p.pal.card, p.pal.text)
    };
    // SAFETY: the DC and rectangle come from this owner-draw message.
    unsafe {
        fill(dc, row, p.pal.card);
        if lit {
            let cell = RECT {
                left: row.left + px(3),
                right: row.right - px(3),
                ..row
            };
            round(dc, cell, (cell.bottom - cell.top) / 2, back, back);
        }
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.font.into());
        let at = RECT {
            left: row.left + px(10),
            ..row
        };
        if item.itemID != u32::MAX {
            write(
                dc,
                &String::from_utf16_lossy(&buffer[..length]),
                at,
                ink,
                DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
        }
    }
}

fn is_push(h: HWND) -> bool {
    // SAFETY: scalar query of a live child.
    let kind = (unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32) & 0xf;
    kind == BS_PUSHBUTTON as u32 || kind == BS_DEFPUSHBUTTON as u32
}

/// A push button as a capsule: the few starting actions are solid accent with dark ink, the rest a quiet fill;
/// lighter under the pointer, an accent ring while focused, dim text when disabled.
fn draw_push(note: &NMCUSTOMDRAW, p: &Paint) {
    let (dc, r, h) = (note.hdc, note.rc, note.hdr.hwndFrom);
    let state = note.uItemState.0;
    let primary = matches!(
        // SAFETY: scalar query of a live child.
        unsafe { GetDlgCtrlID(h) } as usize,
        START | PSTART | WDRANK
    );
    let (fill_colour, ink) = if state & CDIS_DISABLED.0 != 0 {
        (p.pal.off, p.pal.dim)
    } else if primary {
        (p.pal.accent, p.pal.on_accent)
    } else if state & (CDIS_HOT.0 | CDIS_SELECTED.0) != 0 {
        (p.pal.field, p.pal.text)
    } else {
        (p.pal.sel, p.pal.text)
    };
    // SAFETY: the DC and rectangle come from this custom-draw notification.
    unsafe {
        fill(
            dc,
            r,
            if GetDlgCtrlID(h) as usize == CLOSE {
                p.pal.side
            } else {
                p.pal.card
            },
        );
        ringed(dc, r, (r.bottom - r.top) / 2, fill_colour, focus_ring(h), p);
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.font.into());
        write(
            dc,
            &text(h, 256),
            r,
            ink,
            DT_CENTER | DT_NOPREFIX | DT_END_ELLIPSIS,
        );
    }
}

fn is_tab(h: HWND) -> bool {
    // SAFETY: scalar query of a live child.
    (unsafe { GetWindowLongPtrW(h, GWL_STYLE) } as u32) & 0xf == BS_AUTORADIOBUTTON as u32
}

/// A local tab (a native push-like radio underneath, for arrows, focus and its one Tab stop): a 32 high capsule
/// in a neutral fill, the current one lighter with the accent dot after its name.
fn draw_tab(note: &NMCUSTOMDRAW, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let (dc, r, h) = (note.hdc, note.rc, note.hdr.hwndFrom);
    let on = checked(h);
    let fill_colour = if on {
        p.pal.sel
    } else if note.uItemState.0 & CDIS_HOT.0 != 0 {
        p.pal.field
    } else {
        p.pal.card
    };
    let dot = RECT {
        left: r.right - px(18),
        top: (r.top + r.bottom) / 2 - px(3),
        right: r.right - px(12),
        bottom: (r.top + r.bottom) / 2 + px(3),
    };
    // SAFETY: the DC and rectangle come from this custom-draw notification.
    unsafe {
        fill(dc, r, p.pal.content);
        batch_gdi(dc, r, |b| {
            ringed_in(b, r, (r.bottom - r.top) / 2, fill_colour, focus_ring(h), p);
            if p.pal.line == p.pal.text {
                // High contrast: the chip is the window's colour, so it gets the text colour's outline.
                b.round(r, (r.bottom - r.top) / 2, u32::MAX, swap(p.pal.line));
            }
            if on {
                b.round(dot, px(3), swap(p.pal.accent), swap(p.pal.accent));
            }
        });
        SetBkMode(dc, TRANSPARENT);
        SelectObject(dc, p.font.into());
        let name = RECT {
            left: r.left + px(14),
            right: if on {
                dot.left - px(4)
            } else {
                r.right - px(14)
            },
            ..r
        };
        write(
            dc,
            &text(h, 64),
            name,
            if on { p.pal.seltext } else { p.pal.text },
            DT_LEFT | DT_NOPREFIX,
        );
    }
}

/// Dialog procedure: the settings colours, background and sidebar rows; everything else is the shared one.
pub(super) unsafe extern "system" fn settings_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> isize {
    let handled = PAINT.with(|cell| {
        let guard = cell.try_borrow().ok()?;
        let p = guard.as_ref()?;
        match message {
            WM_ERASEBKGND => {
                paint(HDC(wparam.0 as *mut _), hwnd, p);
                Some(1)
            }
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_MOUSEMOVE | WM_LBUTTONUP | WM_CANCELMODE
            | WM_CAPTURECHANGED => scroll::mouse(hwnd, message, lparam, &p.bar).then_some(1),
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
                let edit = message == WM_CTLCOLOREDIT;
                let dc = HDC(wparam.0 as *mut _);
                // SAFETY: the DC is the one this message carries; the child is live.
                unsafe {
                    let (text, back) = match GetDlgCtrlID(HWND(lparam.0 as *mut _)) as usize {
                        STATUS => (p.pal.dim, p.pal.side),
                        SIDE => (p.pal.text, p.pal.side),
                        CAPTION => (p.pal.dim, p.pal.card),
                        // An edit sits in its box, which is the control fill.
                        _ if edit => (p.pal.text, p.pal.well),
                        _ => (p.pal.text, p.pal.card),
                    };
                    SetTextColor(dc, rgb(text));
                    SetBkColor(dc, rgb(back));
                    SetDCBrushColor(dc, rgb(back));
                    Some(GetStockObject(DC_BRUSH).0 as isize)
                }
            }
            WM_NOTIFY => {
                // SAFETY: for WM_NOTIFY lparam is an NMHDR that lives through this call; only a custom-draw
                // notification (checked first) is an NMCUSTOMDRAW.
                if unsafe { (*(lparam.0 as *const NMHDR)).code } != NM_CUSTOMDRAW {
                    return None;
                }
                // SAFETY: the code says this is an NMCUSTOMDRAW.
                let note = unsafe { &*(lparam.0 as *const NMCUSTOMDRAW) };
                let from = note.hdr.hwndFrom;
                if note.dwDrawStage != CDDS_PREPAINT
                    || !(is_toggle(from) || is_push(from) || is_tab(from))
                {
                    return None;
                }
                if is_push(from) {
                    draw_push(note, p);
                } else if is_tab(from) {
                    draw_tab(note, p);
                } else if let Some(face) = p
                    .geometry
                    .displays
                    .get(note.hdr.idFrom.wrapping_sub(MONITOR))
                {
                    displays::draw(note, p, face);
                } else {
                    draw_toggle(note, p);
                }
                // SAFETY: the result of this notification (DWLP_MSGRESULT, offset 0), on our own dialog.
                unsafe {
                    SetWindowLongPtrW(hwnd, WINDOW_LONG_PTR_INDEX(0), CDRF_SKIPDEFAULT as isize)
                };
                Some(1)
            }
            WM_DRAWITEM => {
                // SAFETY: for WM_DRAWITEM lparam is a DRAWITEMSTRUCT that lives through this call.
                let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
                if item.CtlType == ODT_COMBOBOX {
                    draw_combo(item, p);
                } else if item.CtlType == ODT_BUTTON {
                    let note = NMCUSTOMDRAW {
                        hdr: NMHDR {
                            hwndFrom: item.hwndItem,
                            ..Default::default()
                        },
                        hdc: item.hDC,
                        rc: item.rcItem,
                        uItemState: if item.itemState.0 & ODS_SELECTED.0 != 0 {
                            CDIS_SELECTED
                        } else {
                            Default::default()
                        },
                        ..Default::default()
                    };
                    draw_push(&note, p);
                } else {
                    draw_side(item, p);
                }
                Some(1)
            }
            _ => None,
        }
    });
    match handled {
        Some(result) => result,
        // SAFETY: forwards the same arguments to the shared dialog procedure.
        None => unsafe { dialog_proc(hwnd, message, wparam, lparam) },
    }
}

/// Paint only the closed native combo; Windows retains its list, selection and mouse behavior.
pub(super) unsafe extern "system" fn combo_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if matches!(message, WM_PAINT | WM_PRINTCLIENT | WM_ERASEBKGND) {
        let handled = PAINT.with(|cell| {
            let guard = cell.try_borrow().ok()?;
            let p = guard.as_ref()?;
            if message == WM_ERASEBKGND {
                return Some(LRESULT(1));
            }
            // SAFETY: BeginPaint/EndPaint bracket this owned control's live DC.
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let dc = if message == WM_PRINTCLIENT {
                    HDC(wparam.0 as *mut _)
                } else {
                    BeginPaint(hwnd, &mut paint)
                };
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                paint_combo_buffered(dc, rect, |dc| {
                    fill(dc, rect, p.pal.card);
                    let radius = (rect.bottom - rect.top) / 2;
                    let px = |v: i32| (v as f64 * p.scale).round() as i32;
                    let glyph = RECT {
                        left: rect.right - px(22),
                        top: (rect.bottom - px(16)) / 2,
                        right: rect.right - px(6),
                        bottom: (rect.bottom + px(16)) / 2,
                    };
                    let focused = focus_ring(hwnd);
                    batch_gdi(dc, rect, |b| {
                        ringed_in(b, rect, radius, p.pal.sel, focused, p);
                        b.round(rect, radius, u32::MAX, swap(p.pal.field));
                        b.glyph(glyph, Glyph::Expand, swap(p.pal.text));
                    });
                    let name = RECT {
                        left: rect.left + px(10),
                        right: rect.right - px(24),
                        ..rect
                    };
                    let previous = SelectObject(dc, p.font.into());
                    write(
                        dc,
                        &text(hwnd, 128),
                        name,
                        p.pal.text,
                        DT_LEFT | DT_END_ELLIPSIS | DT_NOPREFIX,
                    );
                    SelectObject(dc, previous);
                });
                if message == WM_PAINT {
                    let _ = EndPaint(hwnd, &paint);
                }
            }
            Some(LRESULT(0))
        });
        if let Some(result) = handled {
            return result;
        }
    }
    // SAFETY: the native subclass chain owns all other combo messages.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn paint_combo_buffered(dc: HDC, rect: RECT, draw: impl FnOnce(HDC)) {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, SRCCOPY,
    };

    let (width, height) = (rect.right, rect.bottom);
    if dc.0.is_null() || width <= 0 || height <= 0 {
        return;
    }
    // SAFETY: the complete face is composed offscreen, then copied once with the destination clip intact.
    unsafe {
        let buffer = CreateCompatibleDC(Some(dc));
        if buffer.0.is_null() {
            draw(dc);
            return;
        }
        let bitmap = CreateCompatibleBitmap(dc, width, height);
        if bitmap.0.is_null() {
            draw(dc);
        } else {
            let previous = SelectObject(buffer, bitmap.into());
            draw(buffer);
            let _ = BitBlt(dc, 0, 0, width, height, Some(buffer), 0, 0, SRCCOPY);
            SelectObject(buffer, previous);
            let _ = DeleteObject(bitmap.into());
        }
        let _ = DeleteDC(buffer);
    }
}
