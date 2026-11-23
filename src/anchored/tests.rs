use super::*;
use crate::{edge::Hover, layout::SurfaceKind};
fn hwnd(id: usize) -> HWND {
    HWND(id as *mut _)
}
fn key(id: usize) -> WindowKey {
    WindowKey {
        handle: id,
        process: 42,
    }
}
fn request(owner: usize, now: Instant) -> Request {
    Request {
        owner: Owner(hwnd(owner)),
        kind: Kind::Start,
        since: 100,
        due: Some(now + OPEN_WAIT),
        window: None,
        tray_entered: false,
        anchor: (-900, 1000),
        work: Bounds {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
        },
    }
}
fn event_at(kind: u32, window: usize, time: u32) -> Event {
    Event {
        kind,
        window: hwnd(window),
        time,
    }
}
#[test]
fn native_popup_keeps_the_origin_through_pending_visible_and_exact_dismissal() {
    for surface in [SurfaceKind::Dock, SurfaceKind::Notch] {
        let now = Instant::now();
        let mut native = Native::default();
        let owner = if surface == SurfaceKind::Dock { 11 } else { 12 };
        assert!(native.begin(request(owner, now)));
        assert_eq!(native.owner(), Some(Owner(hwnd(owner))));
        assert_eq!(native.refresh(now), Some(now + OPEN_WAIT));
        let r = native.request.as_mut().unwrap();
        assert_eq!(
            r.observe(event_at(EVENT_OBJECT_SHOW, 50, 110), Some(key(50))),
            Transition::Visible
        );
        assert_eq!(native.refresh(now + Duration::from_secs(600)), None);
        assert_eq!(native.owner(), Some(Owner(hwnd(owner))));
        native.observe(event_at(EVENT_OBJECT_HIDE, 51, 610_100));
        assert_eq!(native.owner(), Some(Owner(hwnd(owner))));
        native.observe(event_at(EVENT_OBJECT_HIDE, 50, 610_101));
        assert_eq!(native.owner(), None);
    }
}
#[test]
fn only_the_shown_panel_of_the_same_owner_and_kind_is_closed_by_a_second_click() {
    let now = Instant::now();
    let mut native = Native::default();
    assert!(native.begin(request(11, now)));
    // Still opening: the click is left to `open`, which keeps the first request.
    assert_eq!(native.shown(Owner(hwnd(11)), Kind::Start), None);
    native.request.as_mut().unwrap().window = Some(key(50));
    assert_eq!(native.shown(Owner(hwnd(11)), Kind::Start), Some(key(50)));
    assert_eq!(native.shown(Owner(hwnd(12)), Kind::Start), None);
    assert_eq!(native.shown(Owner(hwnd(11)), Kind::Tray), None);
}
#[test]
fn pending_timeout_and_failed_input_release_the_owner() {
    let now = Instant::now();
    let mut native = Native::default();
    assert!(native.begin(request(11, now)));
    assert_eq!(native.refresh(now + OPEN_WAIT), None);
    assert_eq!(native.owner(), None);
    assert!(native.begin(request(12, now)));
    native.cancel(); // The same path as SendInput refusal.
    assert_eq!(native.owner(), None);
    assert!(!ACTIVE.get());
}
#[test]
fn concurrent_requests_keep_the_first_and_old_events_cannot_attach_to_the_next() {
    let now = Instant::now();
    let mut native = Native::default();
    assert!(native.begin(request(11, now)));
    assert!(!native.begin(request(12, now)));
    assert_eq!(native.owner(), Some(Owner(hwnd(11))));
    native.cancel();
    let mut next = request(12, now);
    next.since = 200;
    assert!(native.begin(next));
    let r = native.request.as_mut().unwrap();
    assert_eq!(
        r.observe(event_at(EVENT_OBJECT_SHOW, 50, 150), Some(key(50))),
        Transition::None
    );
    assert_eq!(r.window, None);
    assert_eq!(
        r.observe(event_at(EVENT_OBJECT_SHOW, 50, 250), Some(key(50))),
        Transition::Visible
    );
}
#[test]
fn late_or_unidentified_shows_do_not_acquire_an_indefinite_hold() {
    let now = Instant::now();
    let mut r = request(11, now);
    assert_eq!(
        r.observe(event_at(EVENT_OBJECT_SHOW, 50, 110), None),
        Transition::None
    );
    assert_eq!(
        r.observe(event_at(EVENT_OBJECT_SHOW, 50, 2101), Some(key(50))),
        Transition::None
    );
    assert_eq!(r.due, Some(now + OPEN_WAIT));
}
#[test]
fn shell_source_and_class_must_both_match_the_requested_kind() {
    assert!(!Kind::Tray.matches(Kind::Tray.class(), r"c:\other.exe"));
    assert!(!Kind::Tray.matches("Xaml_WindowedPopupClass", r"c:\windows\explorer.exe"));
    assert!(Kind::Tray.matches(Kind::Tray.class(), "explorer.exe"));
    assert!(Kind::Start.matches(Kind::Start.class(), "startmenuexperiencehost.exe"));
    assert!(!Kind::Start.matches(Kind::Start.class(), "searchhost.exe"));
    assert!(Kind::Search.matches(Kind::Search.class(), "searchhost.exe"));
}
#[test]
fn destroy_cloak_and_menu_end_release_only_the_identified_popup() {
    for kind in [
        EVENT_OBJECT_DESTROY,
        EVENT_OBJECT_CLOAKED,
        EVENT_SYSTEM_MENUPOPUPEND,
    ] {
        let mut r = request(11, Instant::now());
        r.observe(event_at(EVENT_OBJECT_SHOW, 50, 110), Some(key(50)));
        assert_eq!(r.observe(event_at(kind, 51, 120), None), Transition::None);
        assert_eq!(r.observe(event_at(kind, 50, 121), None), Transition::Closed);
    }
}
#[test]
fn second_monitor_owner_holds_only_its_surface_without_any_recurring_wake() {
    let now = Instant::now();
    let mut native = Native::default();
    native.begin(request(12, now));
    let (mut first, mut second) = (Hover::default(), Hover::default());
    let held = |id| native.owner() == Some(Owner(hwnd(id)));
    assert_eq!(
        first.tick_after(now, held(11), Duration::from_millis(300)),
        None
    );
    assert_eq!(
        second.tick_after(now, held(12), Duration::from_millis(300)),
        Some(true)
    );
    assert_eq!(second.deadline(), None);
    let r = native.request.as_mut().unwrap();
    r.observe(event_at(EVENT_OBJECT_SHOW, 50, 110), Some(key(50)));
    let later = now + Duration::from_secs(60);
    assert_eq!(
        second.tick_after(later, true, Duration::from_millis(300)),
        None
    );
    assert_eq!(second.deadline(), None);
    native.observe(event_at(EVENT_OBJECT_HIDE, 50, 60_100));
    assert_eq!(
        second.tick_after(later, native.owner().is_some(), Duration::from_millis(300)),
        None
    );
    assert_eq!(second.deadline(), Some(later + Duration::from_millis(300)));
    assert_eq!(
        second.tick_after(
            later + Duration::from_millis(300),
            false,
            Duration::from_millis(300)
        ),
        Some(false)
    );
}
#[test]
fn tray_position_stays_in_the_requested_non_primary_work_area() {
    let work = Bounds {
        x: -1920,
        y: -200,
        width: 1920,
        height: 1080,
    };
    let popup = RECT {
        left: 0,
        top: 0,
        right: 400,
        bottom: 500,
    };
    assert_eq!(tray_at(popup, (-900, 800), work), (-1100, 300));
    assert_eq!(tray_at(popup, (-1910, 200), work), (-1920, -200));
    assert_eq!(tray_at(popup, (-5, 800), work), (-400, 300));
}
#[test]
fn event_overflow_releases_the_hold_instead_of_losing_a_close_and_sticking() {
    let now = Instant::now();
    let mut native = Native::default();
    native.begin(request(12, now));
    for _ in 0..257 {
        event(EVENT_OBJECT_HIDE, hwnd(50), 110);
    }
    assert_eq!(native.refresh(now), None);
    assert_eq!(native.owner(), None);
}

#[test]
fn only_the_identified_window_can_request_repositioning_without_renewing_its_lifetime() {
    let mut r = request(11, Instant::now());
    r.observe(event_at(EVENT_OBJECT_SHOW, 50, 110), Some(key(50)));
    assert_eq!(
        r.observe(event_at(EVENT_OBJECT_LOCATIONCHANGE, 51, 120), None),
        Transition::None
    );
    assert_eq!(
        r.observe(event_at(EVENT_OBJECT_LOCATIONCHANGE, 50, 121), None),
        Transition::Moved
    );
    assert_eq!(r.due, None);
}
