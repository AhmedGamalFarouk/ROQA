//! Painting for the Settings window: the fixed frame (rail strip, brand, heading, footer strip, scroll bar) on the
//! dialog, and the page (group headings, panels, field boxes) on the content window, in the content's own
//! coordinates, so scrolling never repaints either of them for position.

use super::*;

/// The clip box of `dc` limited to `client`, or all of `client` when the box is unusable.
fn dirty_in(dc: HDC, client: RECT) -> RECT {
    let mut dirty = RECT::default();
    // SAFETY: the DC Windows gave this message.
    let kind = unsafe { GetClipBox(dc, &mut dirty) };
    if kind == GDI_REGION_TYPE(GDI_ERROR)
        || kind == NULLREGION
        || dirty.right <= dirty.left
        || dirty.bottom <= dirty.top
    {
        return client;
    }
    RECT {
        left: dirty.left.max(client.left),
        top: dirty.top.max(client.top),
        right: dirty.right.min(client.right),
        bottom: dirty.bottom.min(client.bottom),
    }
}

fn touches(r: RECT, dirty: RECT) -> bool {
    r.right > dirty.left && r.left < dirty.right && r.bottom > dirty.top && r.top < dirty.bottom
}

/// The dialog's own parts: the page behind the viewport, the rail strip with the brand, the heading and its
/// caption, the footer strip and the scroll bar.
pub(super) fn paint(dc: HDC, window: HWND, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let mut client = RECT::default();
    // SAFETY: our own live window and the DC Windows gave this message.
    unsafe {
        let _ = GetClientRect(window, &mut client);
        let dirty = dirty_in(dc, client);
        fill(dc, dirty, p.pal.content);
        let f = &p.frame;
        // The rail strip is the full height; the footer strip runs from its edge to the window's.
        if !f.compact && dirty.left < f.rail.right {
            fill(
                dc,
                RECT {
                    right: f.rail.right.min(dirty.right),
                    ..dirty
                },
                p.pal.side,
            );
        }
        if touches(f.foot, dirty) {
            fill(dc, f.foot, p.pal.side);
        }
        SetBkMode(dc, TRANSPARENT);
        if !f.compact
            && touches(
                RECT {
                    top: 0,
                    bottom: f.rail.top,
                    ..f.rail
                },
                dirty,
            )
        {
            // The brand: two still eyes and the name, in the rail's top.
            let eye = |x: i32| RECT {
                left: px(x),
                top: px(22),
                right: px(x + 8),
                bottom: px(38),
            };
            batch_gdi(dc, dirty, |b| {
                for x in [22, 34] {
                    b.round(eye(x), px(4), swap(p.pal.accent), swap(p.pal.accent));
                }
            });
            SelectObject(dc, p.bold.into());
            let name = RECT {
                left: px(54),
                top: px(18),
                right: f.rail.right,
                bottom: px(42),
            };
            write(dc, "roqa", name, p.pal.text, DT_LEFT | DT_NOPREFIX);
        }
        if touches(f.title, dirty) && !f.compact {
            SelectObject(dc, p.title_font.into());
            write(dc, p.title, f.title, p.pal.text, DT_LEFT | DT_NOPREFIX);
        }
        if touches(f.caption, dirty) {
            SelectObject(dc, p.font.into());
            write(dc, p.caption, f.caption, p.pal.dim, DT_LEFT | DT_NOPREFIX);
        }
        if touches(f.search, dirty) {
            let focused = GetDlgItem(Some(window), search::EDIT as i32).is_ok_and(focus_ring);
            batch_gdi(dc, dirty, |batch| {
                ringed_in(batch, f.search, px(16), p.pal.well, focused, p);
                let left = f.search.left + px(7);
                let top = f.search.top + px(6);
                batch.glyph(
                    RECT {
                        left,
                        top,
                        right: left + px(20),
                        bottom: top + px(20),
                    },
                    Glyph::Search,
                    swap(p.pal.dim),
                );
            });
        }
        if p.bar.scrolls() && touches(p.bar.track, dirty) {
            batch_gdi(dc, dirty, |b| {
                scroll::draw_bar(b, &p.bar, p.pal.card, p.pal.off);
            });
        }
    }
}

/// The page, in content coordinates: group headings, panels and the boxes round edits. Only what the dirty
/// strip touches is drawn (a scroll uncovers a strip, not the page).
pub(super) fn page(dc: HDC, window: HWND, p: &Paint) {
    let px = |v: i32| (v as f64 * p.scale).round() as i32;
    let mut client = RECT::default();
    // SAFETY: our own live window and the DC Windows gave this message.
    unsafe {
        let _ = GetClientRect(window, &mut client);
        let dirty = dirty_in(dc, client);
        fill(dc, dirty, p.pal.content);
        SetBkMode(dc, TRANSPARENT);
        if let Some(results) = p
            .results
            .as_ref()
            .filter(|results| results.window == window)
        {
            results.paint(dc, client.right, dirty, p);
            return;
        }
        SelectObject(dc, p.bold.into());
        for (rect, text) in &p.geometry.labels {
            if touches(*rect, dirty) {
                write(dc, text, *rect, p.pal.text, DT_LEFT | DT_NOPREFIX);
            }
        }
        if let Some(hint) = p.geometry.display_hint.filter(|rect| touches(*rect, dirty)) {
            SelectObject(dc, p.display_meta.into());
            write(dc, displays::HINT, hint, p.pal.dim, DT_LEFT | DT_NOPREFIX);
        }
        let line = if p.pal.dark { p.pal.card } else { p.pal.line };
        batch_gdi(dc, dirty, |b| {
            for rect in &p.geometry.cards {
                if touches(*rect, dirty) {
                    b.round(*rect, px(16), swap(p.pal.card), swap(line));
                }
            }
            for field in &p.geometry.fields {
                if touches(*field, dirty) {
                    b.round(
                        *field,
                        (field.bottom - field.top) / 2,
                        swap(p.pal.well),
                        swap(p.pal.field),
                    );
                }
            }
        });
        if let Some(empty) = p
            .geometry
            .display_empty
            .filter(|rect| touches(*rect, dirty))
        {
            SelectObject(dc, p.font.into());
            write(
                dc,
                "No displays found.",
                empty,
                p.pal.dim,
                DT_LEFT | DT_NOPREFIX,
            );
        }
    }
}
