//! Notch tabs (features tabs-eyes): the pure rules for which tabs exist, which one shows and when the
//! hanging handle opens. No window, timer or thread: the host feeds it pointer and focus facts and asks for
//! one deadline (`Handle::due`).

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Media,
    Weather,
    Prayer,
    Timer,
    Shelf,
    /// Plan-limit percentages for Claude, Codex and Antigravity; off until the user switches it on.
    Limits,
}

/// One user switch per tab, in `Tab::ALL` order.
pub type Flags = [bool; 6];

impl Tab {
    pub const ALL: [Tab; 6] = [
        Tab::Media,
        Tab::Weather,
        Tab::Prayer,
        Tab::Timer,
        Tab::Shelf,
        Tab::Limits,
    ];
    /// Every tab is on by default except Limits, which reads other tools' files and runs nothing until asked.
    pub const DEFAULT: Flags = [true, true, true, true, true, false];

    /// Every tab ships; a tab is hidden only by its Settings switch.
    pub fn available(self) -> bool {
        true
    }
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        ["Media", "Weather", "Prayer", "Timer", "Shelf", "Limits"][self.index()]
    }
    /// The store key suffix: `tab<key>=0|1`.
    pub fn key(self) -> &'static str {
        ["media", "weather", "prayer", "timer", "shelf", "limits"][self.index()]
    }
    pub fn from_key(key: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|tab| tab.key() == key)
    }
}

/// The tabs the bar shows, in order: available and switched on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    tabs: [Tab; 6],
    len: usize,
}

impl Visible {
    pub fn new(flags: Flags) -> Self {
        let mut visible = Self {
            tabs: Tab::ALL,
            len: 0,
        };
        for tab in Tab::ALL {
            if tab.available() && flags[tab.index()] {
                visible.tabs[visible.len] = tab;
                visible.len += 1;
            }
        }
        visible
    }
    pub fn as_slice(&self) -> &[Tab] {
        &self.tabs[..self.len]
    }
    pub fn position(&self, tab: Tab) -> Option<usize> {
        self.as_slice().iter().position(|&t| t == tab)
    }
    pub fn contains(&self, tab: Tab) -> bool {
        self.position(tab).is_some()
    }
    /// With one tab left there is nothing to switch to, so the handle is not drawn at all.
    pub fn handle(&self) -> bool {
        self.len >= 2
    }
    /// The neighbour of `tab`, wrapping; `tab` itself with one tab or when it is not shown.
    pub fn step(&self, tab: Tab, forward: bool) -> Tab {
        let tabs = self.as_slice();
        match self.position(tab) {
            Some(at) if tabs.len() > 1 => {
                tabs[(at + if forward { 1 } else { tabs.len() - 1 }) % tabs.len()]
            }
            _ => tab,
        }
    }
}

/// The tab to show: the user's pick during this open, else Media while a session exists, else the last
/// one used, else the first shown. `None` only when every tab is off.
pub fn pick(visible: &Visible, choice: Option<Tab>, media_live: bool, last: Tab) -> Option<Tab> {
    let shown = |tab: Tab| visible.contains(tab).then_some(tab);
    choice
        .and_then(shown)
        .or_else(|| media_live.then_some(Tab::Media).and_then(shown))
        .or_else(|| shown(last))
        .or_else(|| visible.as_slice().first().copied())
}

/// What the host keeps: switches, the open pick and the last used tab (persisted).
#[derive(Clone, Copy, Debug)]
pub struct Tabs {
    pub flags: Flags,
    pub choice: Option<Tab>,
    pub last: Tab,
}

impl Tabs {
    pub fn new(flags: Flags, last: Tab) -> Self {
        Self {
            flags,
            choice: None,
            last,
        }
    }
    pub fn visible(&self) -> Visible {
        Visible::new(self.flags)
    }
    pub fn on(&self, tab: Tab) -> bool {
        tab.available() && self.flags[tab.index()]
    }
    pub fn active(&self, media_live: bool) -> Option<Tab> {
        pick(&self.visible(), self.choice, media_live, self.last)
    }
    /// The user (or a finished timer) picked `tab`; true when that changes what shows.
    pub fn select(&mut self, tab: Tab, media_live: bool) -> bool {
        if !self.on(tab) {
            return false;
        }
        let before = self.active(media_live);
        self.choice = Some(tab);
        self.last = tab;
        before != Some(tab)
    }
}

/// How long the dots take to grow into icons, and how long the pointer may be away before they shrink.
pub const EXPAND: Duration = Duration::from_millis(240);
pub const COLLAPSE_DELAY: Duration = Duration::from_millis(450);

/// The handle's open/closed intent. Opens at once on hover or focus; closes `COLLAPSE_DELAY` after both end.
#[derive(Clone, Copy, Debug, Default)]
pub struct Handle {
    open: bool,
    leave_at: Option<Instant>,
}

impl Handle {
    /// `want`: the pointer is over the handle or a tab has keyboard focus. Returns the new target (open?)
    /// when it changed now.
    pub fn set(&mut self, want: bool, now: Instant) -> Option<bool> {
        if want {
            self.leave_at = None;
            return (!self.open).then(|| {
                self.open = true;
                true
            });
        }
        if self.open && self.leave_at.is_none() {
            self.leave_at = Some(now + COLLAPSE_DELAY);
        }
        None
    }
    /// Closes when the delay has passed; returns the new target when it changed.
    pub fn tick(&mut self, now: Instant) -> Option<bool> {
        if self.leave_at.is_some_and(|at| now >= at) {
            self.leave_at = None;
            self.open = false;
            return Some(false);
        }
        None
    }
    /// The only wake the handle ever asks for.
    pub fn due(&self) -> Option<Instant> {
        self.leave_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weather_and_prayer_switch_independently() {
        let mut flags = [true; 6];
        flags[Tab::Prayer.index()] = false;
        let tabs = Tabs::new(flags, Tab::Media);
        assert!(tabs.on(Tab::Weather) && !tabs.on(Tab::Prayer));
        flags = [true; 6];
        flags[Tab::Weather.index()] = false;
        let tabs = Tabs::new(flags, Tab::Media);
        assert!(tabs.on(Tab::Prayer) && !tabs.on(Tab::Weather));
    }

    #[test]
    fn tabs_keep_their_order_and_every_tab_ships() {
        let all = Visible::new([true; 6]);
        assert_eq!(
            all.as_slice(),
            [
                Tab::Media,
                Tab::Weather,
                Tab::Prayer,
                Tab::Timer,
                Tab::Shelf,
                Tab::Limits
            ]
        );
        assert!(Tab::Weather.available() && Tab::Prayer.available());
        assert_eq!(
            Visible::new([false, true, true, true, true, false]).as_slice(),
            [Tab::Weather, Tab::Prayer, Tab::Timer, Tab::Shelf]
        );
        assert_eq!(all.position(Tab::Shelf), Some(4));
        assert_eq!(all.position(Tab::Prayer), Some(2));
        for tab in Tab::ALL {
            assert_eq!(Tab::from_key(tab.key()), Some(tab));
        }
        assert_eq!(Tab::from_key("nope"), None);
    }

    #[test]
    fn the_handle_needs_two_tabs() {
        assert!(Visible::new([true; 6]).handle());
        assert!(Visible::new([true, true, false, false, false, false]).handle());
        assert!(!Visible::new([false; 6]).handle());
        // Prayer counts like any other tab.
        assert!(Visible::new([true, false, true, false, false, false]).handle());
    }

    #[test]
    fn the_live_tab_wins_then_the_last_used_one() {
        let visible = Visible::new([true; 6]);
        assert_eq!(pick(&visible, None, true, Tab::Shelf), Some(Tab::Media));
        assert_eq!(pick(&visible, None, false, Tab::Shelf), Some(Tab::Shelf));
        // An explicit pick during this open beats both.
        assert_eq!(
            pick(&visible, Some(Tab::Timer), true, Tab::Shelf),
            Some(Tab::Timer)
        );
        // A hidden tab is never shown: fall back.
        let no_media = Visible::new([false, true, true, true, true, false]);
        assert_eq!(pick(&no_media, None, true, Tab::Timer), Some(Tab::Timer));
        let no_prayer = Visible::new([false, true, false, true, true, false]);
        assert_eq!(
            pick(&no_prayer, None, false, Tab::Prayer),
            Some(Tab::Weather)
        );
        assert_eq!(
            pick(&no_media, Some(Tab::Media), false, Tab::Shelf),
            Some(Tab::Shelf)
        );
        assert_eq!(
            pick(&Visible::new([false; 6]), None, true, Tab::Media),
            None
        );
    }

    #[test]
    fn selecting_remembers_the_tab_and_reports_a_change() {
        let mut tabs = Tabs::new([true; 6], Tab::Media);
        assert_eq!(tabs.active(false), Some(Tab::Media));
        assert!(tabs.select(Tab::Shelf, false));
        assert_eq!(
            (tabs.active(true), tabs.last),
            (Some(Tab::Shelf), Tab::Shelf)
        );
        assert!(
            !tabs.select(Tab::Shelf, true),
            "same tab: nothing to repaint"
        );
        tabs.flags[Tab::Timer.index()] = false;
        assert!(
            !tabs.select(Tab::Timer, true),
            "a switched-off tab cannot be picked"
        );
        // Closing the panel forgets the pick, not the last-used tab.
        tabs.choice = None;
        assert_eq!(tabs.active(false), Some(Tab::Shelf));
        assert_eq!(tabs.active(true), Some(Tab::Media));
    }

    #[test]
    fn stepping_wraps_over_the_shown_tabs_only() {
        // Limits is off by default: the wrap closes at Shelf. With it on, Limits is last.
        let on = Visible::new([true; 6]);
        assert_eq!(on.step(Tab::Shelf, true), Tab::Limits);
        assert_eq!(on.step(Tab::Limits, true), Tab::Media);
        let visible = Visible::new(Tab::DEFAULT);
        assert_eq!(visible.step(Tab::Media, true), Tab::Weather);
        assert_eq!(visible.step(Tab::Weather, true), Tab::Prayer);
        assert_eq!(visible.step(Tab::Prayer, true), Tab::Timer);
        assert_eq!(visible.step(Tab::Shelf, true), Tab::Media);
        assert_eq!(visible.step(Tab::Media, false), Tab::Shelf);
        let one = Visible::new([true, false, false, false, false, false]);
        assert_eq!(one.step(Tab::Media, true), Tab::Media);
        let no_prayer = Visible::new([true, true, false, true, true, false]);
        assert_eq!(no_prayer.step(Tab::Prayer, true), Tab::Prayer, "not shown");
    }

    #[test]
    fn a_file_dropped_on_the_notch_selects_shelf_unless_the_tab_is_off() {
        let mut tabs = Tabs::new([true; 6], Tab::Media);
        assert!(tabs.select(Tab::Shelf, true), "was Media, now Shelf");
        assert_eq!(tabs.active(true), Some(Tab::Shelf));
        assert!(!tabs.select(Tab::Shelf, true), "already there");
        let mut flags: Flags = [true; 6];
        flags[Tab::ALL.iter().position(|&t| t == Tab::Shelf).unwrap()] = false;
        let mut off = Tabs::new(flags, Tab::Media);
        assert!(
            !off.select(Tab::Shelf, true),
            "disabled: behaviour unchanged"
        );
        assert_eq!(off.active(true), Some(Tab::Media));
    }

    #[test]
    fn the_handle_opens_at_once_and_closes_after_the_delay() {
        let t0 = Instant::now();
        let mut handle = Handle::default();
        assert_eq!(handle.due(), None, "closed and idle asks for no wake");
        assert_eq!(handle.set(false, t0), None);
        assert_eq!(handle.due(), None);
        assert_eq!(handle.set(true, t0), Some(true));
        assert_eq!(handle.set(true, t0), None, "already open");
        // Leaving arms one deadline, once; more leaves do not push it out.
        assert_eq!(handle.set(false, t0), None);
        let due = handle.due().unwrap();
        assert_eq!(due, t0 + COLLAPSE_DELAY);
        handle.set(false, t0 + Duration::from_millis(100));
        assert_eq!(handle.due(), Some(due));
        assert_eq!(handle.tick(due - Duration::from_millis(1)), None);
        assert_eq!(handle.tick(due), Some(false));
        assert_eq!(handle.due(), None);
        assert_eq!(handle.tick(due + Duration::from_secs(9)), None);
    }

    #[test]
    fn coming_back_in_time_or_holding_focus_keeps_the_handle_open() {
        let t0 = Instant::now();
        let mut handle = Handle::default();
        handle.set(true, t0);
        handle.set(false, t0);
        assert_eq!(handle.set(true, t0 + Duration::from_millis(200)), None);
        assert_eq!(handle.due(), None, "returning cancels the collapse");
        assert_eq!(handle.tick(t0 + Duration::from_secs(5)), None);
        // Reopening after a close works again.
        handle.set(false, t0);
        let due = handle.due().unwrap();
        handle.tick(due);
        assert_eq!(handle.set(true, due), Some(true));
    }
}
