//! Notch tabs (features tabs-eyes): the pure rules for which tabs exist, which one shows and when the
//! hanging handle shows. No window, timer or thread.

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
    Widgets,
}

/// One user switch per tab, in `Tab::ALL` order.
pub type Flags = [bool; 7];

impl Tab {
    pub const ALL: [Tab; 7] = [
        Tab::Media,
        Tab::Weather,
        Tab::Prayer,
        Tab::Timer,
        Tab::Shelf,
        Tab::Limits,
        Tab::Widgets,
    ];
    /// Every tab is on by default except Limits, which reads other tools' files and runs nothing until asked.
    pub const DEFAULT: Flags = [true, true, true, true, true, false, true];

    /// Every tab but Limits shows in the notch, hidden only by its Settings switch. Limits lives in the dock
    /// now; its switch shows the dock button.
    pub fn available(self) -> bool {
        self != Tab::Limits
    }
    pub fn index(self) -> usize {
        self as usize
    }
    pub fn name(self) -> &'static str {
        [
            "Media", "Weather", "Prayer", "Timer", "Shelf", "Limits", "Widgets",
        ][self.index()]
    }
    /// The store key suffix: `tab<key>=0|1`.
    pub fn key(self) -> &'static str {
        [
            "media", "weather", "prayer", "timer", "shelf", "limits", "widgets",
        ][self.index()]
    }
    pub fn from_key(key: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|tab| tab.key() == key)
    }
}

/// The tabs the bar shows, in order: available and switched on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Visible {
    tabs: [Tab; 7],
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

/// The plain Media page's line under the eyes (`cook/design/lumabar-redesign.md`): the next thing that matters,
/// from a tab that is on, and the tab a click opens. `None` keeps "Nothing playing". Built from views the host
/// already holds; nothing is fetched for it.
pub fn glance(
    tabs: &Tabs,
    timer: Option<&crate::timer::View>,
    prayer: &crate::prayer_tab::View,
    weather: &crate::weather::View,
) -> Option<(String, Tab)> {
    let timer = timer
        .filter(|view| tabs.on(Tab::Timer) && !view.done && !view.watch)
        .map(|view| {
            (
                format!("Timer, {} min left", view.left.div_ceil(60)),
                Tab::Timer,
            )
        });
    let ready = tabs.on(Tab::Prayer) && prayer.message.is_none() && !prayer.next.is_empty();
    let prayer = ready.then(|| {
        let row = prayer.rows.iter().find(|row| row.name == prayer.next);
        let text = match row {
            // An hour or more away ("in 2 h 10 min"): the clock time reads better.
            Some(row) if prayer.left.contains(" h") => format!("{} at {}", prayer.next, row.time),
            _ => format!("{} {}", prayer.next, prayer.left),
        };
        (text, Tab::Prayer)
    });
    let ready = tabs.on(Tab::Weather) && weather.message.is_none() && !weather.temp.is_empty();
    let weather = ready.then(|| {
        let text = format!("{}, {}", weather.temp, weather.cond.to_lowercase());
        (text, Tab::Weather)
    });
    timer.or(prayer).or(weather)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_glance_says_the_next_thing_from_a_tab_that_is_on() {
        let mut prayer = crate::prayer_tab::View {
            next: "Maghrib".into(),
            left: "in 12 min".into(),
            rows: vec![crate::prayer_tab::Row {
                name: "Maghrib",
                time: "5:01 pm".into(),
                current: false,
            }],
            ..Default::default()
        };
        let weather = crate::weather::View {
            temp: "29°".into(),
            cond: "Clear",
            ..Default::default()
        };
        let timer = crate::timer::View::paused(1050, 1500, String::new(), None);
        let all = Tabs::new([true; 7], Tab::Media);
        let text = |tabs: &Tabs, timer, prayer: &_| glance(tabs, timer, prayer, &weather);
        assert_eq!(
            text(&all, Some(&timer), &prayer),
            Some(("Timer, 18 min left".into(), Tab::Timer))
        );
        assert_eq!(
            text(&all, None, &prayer),
            Some(("Maghrib in 12 min".into(), Tab::Prayer))
        );
        prayer.left = "in 2 h 10 min".into();
        assert_eq!(text(&all, None, &prayer).unwrap().0, "Maghrib at 5:01 pm");
        // A switched-off tab never speaks; with none left the page keeps "Nothing playing".
        let mut flags = [true; 7];
        flags[Tab::Timer.index()] = false;
        flags[Tab::Prayer.index()] = false;
        let quiet = Tabs::new(flags, Tab::Media);
        assert_eq!(
            text(&quiet, Some(&timer), &prayer),
            Some(("29°, clear".into(), Tab::Weather))
        );
        flags[Tab::Weather.index()] = false;
        assert_eq!(text(&Tabs::new(flags, Tab::Media), None, &prayer), None);
    }

    #[test]
    fn weather_and_prayer_switch_independently() {
        let mut flags = [true; 7];
        flags[Tab::Prayer.index()] = false;
        let tabs = Tabs::new(flags, Tab::Media);
        assert!(tabs.on(Tab::Weather) && !tabs.on(Tab::Prayer));
        flags = [true; 7];
        flags[Tab::Weather.index()] = false;
        let tabs = Tabs::new(flags, Tab::Media);
        assert!(tabs.on(Tab::Prayer) && !tabs.on(Tab::Weather));
    }

    #[test]
    fn tabs_keep_their_order_and_limits_stays_out_of_the_notch() {
        let all = Visible::new([true; 7]);
        assert_eq!(
            all.as_slice(),
            [
                Tab::Media,
                Tab::Weather,
                Tab::Prayer,
                Tab::Timer,
                Tab::Shelf,
                Tab::Widgets
            ]
        );
        assert!(Tab::Weather.available() && Tab::Prayer.available() && !Tab::Limits.available());
        assert_eq!(
            Visible::new([false, true, true, true, true, false, false]).as_slice(),
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
        assert!(Visible::new([true; 7]).handle());
        assert!(Visible::new([true, true, false, false, false, false, false]).handle());
        assert!(!Visible::new([false; 7]).handle());
        // Prayer counts like any other tab.
        assert!(Visible::new([true, false, true, false, false, false, false]).handle());
    }

    #[test]
    fn the_live_tab_wins_then_the_last_used_one() {
        let visible = Visible::new([true; 7]);
        assert_eq!(pick(&visible, None, true, Tab::Shelf), Some(Tab::Media));
        assert_eq!(pick(&visible, None, false, Tab::Shelf), Some(Tab::Shelf));
        // An explicit pick during this open beats both.
        assert_eq!(
            pick(&visible, Some(Tab::Timer), true, Tab::Shelf),
            Some(Tab::Timer)
        );
        // A hidden tab is never shown: fall back.
        let no_media = Visible::new([false, true, true, true, true, false, false]);
        assert_eq!(pick(&no_media, None, true, Tab::Timer), Some(Tab::Timer));
        let no_prayer = Visible::new([false, true, false, true, true, false, false]);
        assert_eq!(
            pick(&no_prayer, None, false, Tab::Prayer),
            Some(Tab::Weather)
        );
        assert_eq!(
            pick(&no_media, Some(Tab::Media), false, Tab::Shelf),
            Some(Tab::Shelf)
        );
        assert_eq!(
            pick(&Visible::new([false; 7]), None, true, Tab::Media),
            None
        );
    }

    #[test]
    fn selecting_remembers_the_tab_and_reports_a_change() {
        let mut tabs = Tabs::new([true; 7], Tab::Media);
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
    fn a_file_dropped_on_the_notch_selects_shelf_unless_the_tab_is_off() {
        let mut tabs = Tabs::new([true; 7], Tab::Media);
        assert!(tabs.select(Tab::Shelf, true), "was Media, now Shelf");
        assert_eq!(tabs.active(true), Some(Tab::Shelf));
        assert!(!tabs.select(Tab::Shelf, true), "already there");
        let mut flags: Flags = [true; 7];
        flags[Tab::ALL.iter().position(|&t| t == Tab::Shelf).unwrap()] = false;
        let mut off = Tabs::new(flags, Tab::Media);
        assert!(
            !off.select(Tab::Shelf, true),
            "disabled: behaviour unchanged"
        );
        assert_eq!(off.active(true), Some(Tab::Media));
    }
}
