//! The Settings window built for real but never shown: the nested viewport and content windows, notification
//! routing through them, Tab order, route switching and what a scroll does (and does not). Nothing here touches the
//! owner's settings or their screen; the window is hidden and destroyed with the test.

use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::GetFocus;

fn monitor(n: i32, x: i32) -> Monitor {
    let bounds = |x: i32| Bounds {
        x,
        y: 0,
        width: 1920,
        height: 1080,
    };
    Monitor {
        device: format!("\\\\.\\DISPLAY{n}"),
        work: bounds(x),
        rect: bounds(x),
        dpi: 120,
        taskbar: 0,
    }
}

/// A hidden window in a work area `width` by `height` (a small one makes the compact layout and a long scroll).
/// Setting the starting values posts the edits' change notes: they are taken off the queue, so a test sees only
/// what it causes.
fn sample(width: i32, height: i32) -> SettingsWindow {
    let window = SettingsWindow::open(
        &Settings::default(),
        &[monitor(1, 0), monitor(2, 1920)],
        1.0,
        Bounds {
            x: 0,
            y: 0,
            width,
            height,
        },
    )
    .expect("the window builds");
    drain(window.window);
    window
}

fn page_of(id: usize) -> usize {
    PAGES
        .iter()
        .position(|p| p.cards.iter().any(|c| c.rows.iter().any(|r| r.id == id)))
        .expect("a page holds it")
}

fn rect(h: HWND) -> RECT {
    let mut r = RECT::default();
    // SAFETY: a live window of the test.
    unsafe { GetWindowRect(h, &mut r).expect("a window rectangle") };
    r
}

fn parent(h: HWND) -> HWND {
    // SAFETY: a scalar query of a live window.
    unsafe { GetParent(h) }.expect("a parent")
}

fn visible_style(h: HWND) -> bool {
    // SAFETY: a scalar query of a live window.
    unsafe { GetWindowLongPtrW(h, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0 }
}

/// The posted messages of `window` that are Settings' own (actions and scrolls), taken from the queue.
fn drain(window: HWND) -> Vec<MSG> {
    let mut found = Vec::new();
    let mut message = MSG::default();
    // SAFETY: reads this thread's queue for our own window.
    while unsafe {
        PeekMessageW(
            &mut message,
            Some(window),
            ACTION_FIRST,
            SCROLL_LAST,
            PM_REMOVE,
        )
    }
    .as_bool()
    {
        found.push(message);
    }
    found
}
const ACTION_FIRST: u32 = crate::dialog::ACTION;
const SCROLL_LAST: u32 = crate::dialog::SCROLL;

#[test]
fn a_dropdown_prints_only_inside_the_dirty_strip_without_native_erase() {
    let mut window = sample(720, 584);
    window
        .show_page(page_of(NETU))
        .expect("the notch page opens");
    let combo = window.h(NETU);
    let bounds = rect(combo);
    let (width, height) = (bounds.right - bounds.left, bounds.bottom - bounds.top);
    let untouched = 0x00ff00ff;
    let expected = PAINT.with(|paint| paint.borrow().as_ref().expect("paint data").pal.card);
    let mut observed = None;
    crate::graphics::sample(width.max(height), |dc, surface| {
        // SAFETY: a live test-owned control and offscreen DC; the sample helper releases the renderer.
        unsafe {
            fill(dc, surface, untouched);
            let erased = SendMessageW(combo, WM_ERASEBKGND, Some(WPARAM(dc.0 as usize)), None);
            IntersectClipRect(dc, 0, 0, width, 3);
            SendMessageW(
                combo,
                WM_PRINTCLIENT,
                Some(WPARAM(dc.0 as usize)),
                Some(LPARAM(PRF_CLIENT as isize)),
            );
            let corner = GetPixel(dc, 0, 0);
            SelectClipRgn(dc, None);
            observed = Some((erased.0, corner, GetPixel(dc, 0, height - 1)));
        }
    });
    assert_eq!(observed, Some((1, rgb(expected), rgb(untouched))));
}

/// Where `h` is inside its page (screen position less the page's), which a scroll must never change.
fn within_page(w: &SettingsWindow, h: HWND) -> (i32, i32) {
    let (r, c) = (rect(h), rect(w.contents[w.page]));
    (r.left - c.left, r.top - c.top)
}

#[test]
fn every_control_is_a_child_of_its_pages_content_window_inside_the_viewport() {
    let w = sample(1400, 900);
    assert_eq!(parent(w.viewport), w.window);
    for (index, &content) in w.contents.iter().enumerate() {
        assert_eq!(parent(content), w.viewport, "page {index}");
    }
    for item in &w.items {
        let content = w.contents[usize::from(item.page)];
        for h in [item.h, item.label, item.cap] {
            if !h.0.is_null() {
                assert_eq!(parent(h), content, "control {}", item.id);
            }
        }
    }
    // The rail, picker, tabs, footer and Close are the dialog's own: they never move with the page.
    for h in [w.side, w.nav, w.search.edit, w.status, w.close] {
        assert_eq!(parent(h), w.window);
    }
    // The viewport sits where the frame says, and the page's window is as wide as it.
    let (v, f) = (rect(w.viewport), w.frame.viewport);
    assert_eq!(v.right - v.left, f.right - f.left);
    assert_eq!(v.bottom - v.top, f.bottom - f.top);
    assert_eq!(
        rect(w.contents[0]).right - rect(w.contents[0]).left,
        f.right - f.left
    );
}

#[test]
fn every_page_has_a_row_for_each_connected_display() {
    let w = sample(1400, 900);
    let shown: Vec<_> = w
        .items
        .iter()
        .filter(|i| (MONITOR..MONITOR + MAX_OFF).contains(&i.id))
        .collect();
    assert_eq!(shown.len(), 2);
    assert!(
        shown
            .iter()
            .all(|i| i.page == 0 && usize::from(i.card) == DISPLAYS_CARD)
    );
    assert!(text(shown[0].h, 256).starts_with("Display 1, primary, 1920 by 1080 at 125%"));
}

#[test]
fn display_tiles_follow_physical_order_and_the_last_active_tile_stays_on() {
    let mut window = sample(1400, 900);
    window
        .refresh_monitors(&[monitor(1, 0), monitor(2, -1920)])
        .unwrap();
    let primary = window.h(MONITOR);
    let secondary = window.h(MONITOR + 1);
    let (primary_rect, secondary_rect) = (rect(primary), rect(secondary));
    assert!(
        secondary_rect.right < primary_rect.left,
        "physical left precedes primary"
    );
    assert_eq!(primary_rect.top, secondary_rect.top);
    let order = tab_order(&window);
    let display_id = (MONITOR + 1).to_string();
    let primary_id = MONITOR.to_string();
    assert!(
        order.iter().position(|id| *id == display_id).unwrap()
            < order.iter().position(|id| *id == primary_id).unwrap()
    );
    for tile in [primary, secondary] {
        // SAFETY: activate one native checkbox on our own test window.
        unsafe { SendMessageW(tile, BM_CLICK, None, None) };
        for message in drain(window.window) {
            window.message(&message).unwrap();
        }
    }
    assert!(!checked(primary));
    assert!(checked(secondary));
    assert_eq!(text(window.status, 128), "Keep at least one monitor on.");
}

/// The Tab stops in order from the first, as ids (the rail, the tabs and Close are named).
fn tab_order(w: &SettingsWindow) -> Vec<String> {
    let mut order = Vec::new();
    let mut at: Option<HWND> = None;
    for _ in 0..64 {
        // SAFETY: the dialog manager's own query on our live dialog.
        let next = unsafe { GetNextDlgTabItem(w.window, at, false) }.expect("a next stop");
        if Some(next) == order.first().map(|(h, _)| *h) {
            break;
        }
        let name = if next == w.side {
            "rail".to_owned()
        } else if next == w.nav {
            "picker".to_owned()
        } else if next == w.search.edit {
            "search".to_owned()
        } else if next == w.close {
            "close".to_owned()
        } else if let Some(tab) = w.tabs.iter().position(|t| *t == next) {
            format!("tab {}", PAGES[tab].name)
        } else if let Some(item) = w.items.iter().find(|i| i.h == next) {
            item.id.to_string()
        } else {
            format!("unknown {next:?}")
        };
        order.push((next, name));
        at = Some(next);
    }
    order.into_iter().map(|(_, name)| name).collect()
}

#[test]
fn tab_order_is_the_rail_search_local_tab_page_then_close() {
    let mut w = sample(1400, 900);
    let ids = |list: &[usize]| list.iter().map(ToString::to_string).collect::<Vec<_>>();
    let mut want: Vec<String> = vec!["rail".into(), "search".into()];
    want.extend(ids(&[
        DOCK,
        NOTCH,
        MONITOR,
        MONITOR + 1,
        AUTOSTART,
        HIDE_CAPTURE,
        FSHIDE,
        RSOUNDS,
    ]));
    want.push("close".into());
    assert_eq!(
        tab_order(&w),
        want,
        "General: no tab, only its own controls"
    );

    // Dock's Apps page: one stop in the tabs (the current one), then only that page's controls.
    w.show_page(page_of(PIN)).unwrap();
    let mut want: Vec<String> = vec!["rail".into(), "search".into(), "tab Apps".into()];
    want.extend(ids(&[PIN, EARLIER, LATER, HIDDEN, SHOW]));
    want.push("close".into());
    assert_eq!(tab_order(&w), want);

    // A disabled control is not a stop (an empty pin list disables its three).
    w.set_pins(&[], 0);
    let mut want: Vec<String> = vec!["rail".into(), "search".into(), "tab Apps".into()];
    want.extend(ids(&[HIDDEN, SHOW]));
    want.push("close".into());
    assert_eq!(tab_order(&w), want);
}

#[test]
fn clearing_search_restores_the_page_scroll_and_status() {
    let mut window = sample(600, 472);
    window.show_page(page_of(PBROWSE)).unwrap();
    window.jump(window.scroll_max).unwrap();
    window.say("Keep this message.").unwrap();
    let before = (window.page, window.scroll);
    set_text(window.search.edit, "sound").unwrap();
    window.search_changed().unwrap();
    assert_eq!(tab_order(&window), ["picker", "search", "close"]);
    assert_eq!(window.scrolling_body(), window.search.body);
    set_text(window.search.edit, " ").unwrap();
    window.search_changed().unwrap();
    assert_eq!((window.page, window.scroll), before);
    assert_eq!(text(window.status, 64), "Keep this message.");
}

#[test]
fn search_enter_reveals_the_exact_setting_without_applying_it() {
    let mut window = sample(720, 584);
    set_text(window.search.edit, "finish sound").unwrap();
    window.open_search_hit(None).unwrap();
    let finish = PAGES[window.page]
        .cards
        .iter()
        .flat_map(|card| card.rows)
        .find(|row| row.title == "Finish sound")
        .expect("timer finish sound");
    assert!(checked(window.h(finish.id)) == flag(&mut window.current, finish.id).copied().unwrap());
    assert!(!window.search.is_open() && window.take_change().is_none());
}

#[test]
fn enter_in_empty_results_does_not_close_settings() {
    let mut window = sample(600, 472);
    set_text(window.search.edit, "no-such-setting").unwrap();
    let enter = MSG {
        hwnd: window.search.edit,
        message: WM_KEYDOWN,
        wParam: WPARAM(13),
        ..Default::default()
    };
    assert!(window.key(&enter).unwrap());
    assert!(window.search.is_open() && !window.closed());
}

#[test]
fn a_result_click_cannot_land_on_a_reordered_result() {
    let mut window = sample(720, 584);
    set_text(window.search.edit, "finish sound").unwrap();
    window.search_changed().unwrap();
    let clicked = PAINT.with(|paint| {
        paint
            .borrow()
            .as_ref()
            .unwrap()
            .results
            .as_ref()
            .unwrap()
            .click(window.search.body, 10)
            .unwrap()
    });
    set_text(window.search.edit, "reminder sounds").unwrap();
    window.open_search_hit(Some(clicked)).unwrap();
    assert!(window.search.is_open());
    assert!(window.take_change().is_none());
}

#[test]
fn a_click_reaches_the_dialog_through_both_nested_windows_and_applies_once() {
    let mut w = sample(1400, 900);
    assert!(w.current.dock, "the default shows the dock");
    // SAFETY: a button click on our own live control; its parent is the content window.
    unsafe { SendMessageW(w.h(DOCK), BM_CLICK, None, None) };
    let posted = drain(w.window);
    assert_eq!(
        posted.len(),
        1,
        "one action, from the content window's forward"
    );
    assert_eq!(posted[0].message, crate::dialog::ACTION);
    assert_eq!(posted[0].wParam.0 & 0xffff, DOCK);
    assert_eq!((posted[0].wParam.0 >> 16) as u32, BN_CLICKED);
    assert!(w.message(&posted[0]).unwrap());
    let change = w.take_change().expect("the switch applied");
    assert!(!change.dock && change.notch, "only the dock changed");
    assert!(w.take_change().is_none());
}

#[test]
fn a_drop_down_choice_and_an_edit_commit_through_the_same_route() {
    let mut w = sample(1400, 900);
    // SAFETY: selecting a row of our own combo box and sending the notification its parent would get.
    unsafe {
        SendMessageW(w.h(ANIM), CB_SETCURSEL, Some(WPARAM(1)), None);
        SendMessageW(
            parent(w.h(ANIM)),
            WM_COMMAND,
            Some(WPARAM(ANIM | (CBN_SELCHANGE as usize) << 16)),
            Some(LPARAM(w.h(ANIM).0 as isize)),
        );
    }
    let posted = drain(w.window);
    assert_eq!(posted.len(), 1);
    w.message(&posted[0]).unwrap();
    assert!(w.take_change().expect("applied").anim_fast);
}

#[test]
fn scrolling_moves_the_page_window_only_and_applies_nothing() {
    // A short work area: the compact layout, a short viewport, a long Prayer page.
    let mut w = sample(1000, 560);
    assert!(w.frame.compact);
    w.show_page(page_of(PSHIFT)).unwrap();
    assert!(w.scroll_max > 0, "the page overflows");
    let before: Vec<_> = w
        .items
        .iter()
        .filter(|i| usize::from(i.page) == w.page)
        .map(|i| (i.id, within_page(&w, i.h)))
        .collect();
    let settings = w.current.clone();
    let viewport_top = rect(w.viewport).top;
    for _ in 0..6 {
        w.scroll_message(WHEEL, -120).unwrap();
    }
    assert!(w.scroll > 0 && w.scroll <= w.scroll_max);
    assert_eq!(
        rect(w.contents[w.page]).top - viewport_top,
        -w.scroll,
        "one window moved by exactly the offset"
    );
    let after: Vec<_> = w
        .items
        .iter()
        .filter(|i| usize::from(i.page) == w.page)
        .map(|i| (i.id, within_page(&w, i.h)))
        .collect();
    assert_eq!(before, after, "no control moved inside its page");
    // Scrolling reads and writes no settings and posts nothing for the loop to act on.
    assert!(w.take_change().is_none() && w.current == settings);
    assert!(drain(w.window).is_empty());
    assert_eq!(
        text(w.status, 128),
        DEFAULT_STATUS,
        "the footer is untouched"
    );
}

#[test]
fn the_wheel_stops_at_both_ends_and_the_first_turn_back_moves_at_once() {
    let mut w = sample(1000, 560);
    w.show_page(page_of(PSHIFT)).unwrap();
    for _ in 0..200 {
        w.scroll_message(WHEEL, -120).unwrap();
    }
    assert_eq!(w.scroll, w.scroll_max, "the end holds");
    assert_eq!(w.wheel_remainder, 0, "nothing is carried past the end");
    w.scroll_message(WHEEL, -30).unwrap();
    assert_eq!(w.scroll, w.scroll_max);
    w.scroll_message(WHEEL, 120).unwrap();
    assert!(w.scroll < w.scroll_max, "the first turn back moves");
    for _ in 0..200 {
        w.scroll_message(WHEEL, 120).unwrap();
    }
    assert_eq!(w.scroll, 0);
    // The thumb and the track agree with the offset the page stands at.
    w.jump(w.scroll_max / 2).unwrap();
    let bar = PAINT
        .with(|p| p.borrow().as_ref().map(|p| p.bar))
        .expect("painted");
    assert_eq!(bar.scroll, w.scroll);
    assert_eq!(bar.max(), w.scroll_max);
}

#[test]
fn a_page_keeps_its_offset_when_you_leave_and_return_and_only_it_shows() {
    let mut w = sample(1000, 560);
    let prayer = page_of(PSHIFT);
    w.show_page(prayer).unwrap();
    w.jump(37).unwrap();
    w.show_route(0).unwrap();
    assert_eq!(w.scroll, 0, "General has its own place");
    let shown: Vec<_> = (0..PAGES.len())
        .filter(|&i| visible_style(w.contents[i]))
        .collect();
    assert_eq!(shown, [0], "never two live pages");
    w.show_route(usize::from(PAGES[prayer].route)).unwrap();
    assert_eq!(w.scroll, 37);
    assert_eq!(w.page, prayer);
    // The destination remembers the local page it was left on.
    let apps = page_of(PIN);
    w.show_page(apps).unwrap();
    w.show_route(0).unwrap();
    w.show_route(2).unwrap();
    assert_eq!(w.page, apps);
}

#[test]
fn a_page_change_clears_the_footer_and_a_refused_value_keeps_it_until_then() {
    let mut w = sample(1400, 900);
    w.say("Keep at least one monitor on.").unwrap();
    assert_eq!(text(w.status, 128), "Keep at least one monitor on.");
    w.show_route(1).unwrap();
    assert_eq!(text(w.status, 128), DEFAULT_STATUS);
}

#[test]
fn the_rail_gives_way_to_a_picker_below_the_rail_size() {
    let wide = sample(1400, 900);
    assert!(!wide.frame.compact);
    assert!(visible_style(wide.side) && !visible_style(wide.nav));
    let small = sample(700, 560);
    assert!(small.frame.compact);
    assert!(!visible_style(small.side) && visible_style(small.nav));
}

#[test]
fn focus_in_a_hidden_page_does_not_stay_there() {
    // Focus belongs on a page's own control only while that page shows; leaving moves it to the rail's stop.
    let mut w = sample(1400, 900);
    // SAFETY: a focus move on our own hidden window's control (it may fail while the window is hidden).
    let moved = unsafe { SetFocus(Some(w.h(DOCK))) }.is_ok();
    w.show_route(2).unwrap();
    // SAFETY: a scalar query.
    let focus = unsafe { GetFocus() };
    assert!(!moved || focus != w.h(DOCK));
}

#[test]
fn a_closed_drop_down_is_the_box_high_and_its_list_shows_at_most_eight_rows() {
    let mut w = sample(1400, 900);
    w.show_route(1).unwrap();
    let scale = w.font.as_ref().map(|f| f.scale).unwrap();
    let px = |v: i32| (f64::from(v) * scale).round() as i32;
    let h = w.h(ANIM);
    for n in 0..40 {
        add_row(h, &format!("row {n}"));
    }
    w.layout().unwrap();
    let (mut client, mut list) = (RECT::default(), RECT::default());
    // SAFETY: our own live combo box and a rectangle of ours.
    unsafe {
        GetClientRect(h, &mut client).unwrap();
        SendMessageW(
            h,
            CB_GETDROPPEDCONTROLRECT,
            None,
            Some(LPARAM(&mut list as *mut RECT as isize)),
        );
    }
    assert_eq!(
        client.bottom,
        px(32),
        "the box is 32 DIPs, with no region to cut it"
    );
    assert_eq!(rect(h).bottom - rect(h).top, px(32));
    let rows = (list.bottom - list.top - px(32)) / px(36);
    assert!(
        rows <= 8,
        "a long list scrolls after eight rows, not {rows}"
    );
    assert!(rows >= 7, "and shows them whole");
}
