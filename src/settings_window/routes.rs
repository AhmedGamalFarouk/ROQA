//! What Settings shows: ten destinations (the rail), each with one or more short local pages, each page with
//! its groups and rows. Every setting appears once; the monitors are added at runtime after `NOTCH`.

use super::*;

/// How a row's control looks: the title is the checkbox text, or the label before the control.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Check,
    /// A drop-down list of this many DIPs wide.
    Combo(i32),
    Edit(i32),
    /// A push button; the row caption is the text on its left.
    Btn,
    /// Text only.
    Note,
}
pub(super) struct Row {
    pub id: usize,
    pub kind: Kind,
    pub title: &'static str,
    pub caption: &'static str,
}
pub(super) struct Card {
    pub label: &'static str,
    pub rows: &'static [Row],
}
/// One short page: the groups of a destination, or one local tab of it.
pub(super) struct Page {
    pub route: u8,
    /// The local tab's name; empty when the destination has one page only.
    pub name: &'static str,
    pub cards: &'static [Card],
}
/// A destination in the rail.
pub(super) struct Route {
    pub name: &'static str,
    pub caption: &'static str,
    pub glyph: Glyph,
}
const fn r(id: usize, kind: Kind, title: &'static str, caption: &'static str) -> Row {
    Row {
        id,
        kind,
        title,
        caption,
    }
}
const C: Kind = Kind::Check;

/// The rail, in its fixed order. The first is where Settings opens.
pub(super) const ROUTES: [Route; 10] = [
    Route {
        name: "General",
        caption: "Where ROQA shows up.",
        glyph: Glyph::Gear,
    },
    Route {
        name: "Appearance",
        caption: "Make ROQA feel like yours.",
        glyph: Glyph::Sliders,
    },
    Route {
        name: "Dock",
        caption: "Choose what stays close.",
        glyph: Glyph::TaskView,
    },
    Route {
        name: "Notch",
        caption: "Choose what your notch shows.",
        glyph: Glyph::Notch,
    },
    Route {
        name: "Timers & water",
        caption: "Set up timers and reminders.",
        glyph: Glyph::Timer,
    },
    Route {
        name: "Weather",
        caption: "Set your location and units.",
        glyph: Glyph::PartlyCloudy,
    },
    Route {
        name: "Prayer",
        caption: "Tune prayer times and reminders.",
        glyph: Glyph::Prayer,
    },
    Route {
        name: "Limits & agents",
        caption: "Manage limits and agent hooks.",
        glyph: Glyph::Limits,
    },
    Route {
        name: "Hotkeys",
        caption: "Choose which shortcuts ROQA uses.",
        glyph: Glyph::Keys,
    },
    Route {
        name: "About",
        caption: "Version, diagnostics and defaults.",
        glyph: Glyph::Info,
    },
];

/// The pages in rail order, then local-tab order (a page's index is what `Item::page` holds).
pub(super) const PAGES: &[Page] = &[
    Page {
        route: 0,
        name: "",
        cards: &[
            Card {
                label: "Show ROQA",
                rows: &[
                    r(DOCK, C, "Show the dock", ""),
                    r(NOTCH, C, "Show the notch", ""),
                ],
            },
            Card {
                label: "Displays",
                rows: &[],
            },
            Card {
                label: "Behaviour",
                rows: &[
                    r(
                        AUTOSTART,
                        C,
                        "Start with Windows",
                        "Open ROQA when you sign in",
                    ),
                    r(
                        HIDE_CAPTURE,
                        C,
                        "Hide from screen sharing",
                        "Recordings and shared screens skip ROQA",
                    ),
                    r(
                        FSHIDE,
                        C,
                        "Hide in full screen",
                        "Hide the dock and notch over a full-screen app",
                    ),
                    r(
                        RSOUNDS,
                        C,
                        "Reminder sounds",
                        "Master switch for the timer and prayer sounds",
                    ),
                ],
            },
        ],
    },
    Page {
        route: 1,
        name: "",
        cards: &[
            Card {
                label: "Theme",
                rows: &[
                    r(THEME, Kind::Combo(170), "Theme", ""),
                    r(
                        MATCH_ACCENT,
                        C,
                        "Match Windows accent colour",
                        "Tint progress bars and active tabs with the system accent",
                    ),
                ],
            },
            Card {
                label: "Motion",
                rows: &[r(ANIM, Kind::Combo(120), "Animation speed", "")],
            },
            Card {
                label: "Edge timing",
                rows: &[
                    r(
                        REVEAL,
                        Kind::Combo(120),
                        "Reveal delay",
                        "How long to rest at the edge",
                    ),
                    r(
                        HIDE,
                        Kind::Combo(120),
                        "Hide delay",
                        "How long before it slides away",
                    ),
                ],
            },
        ],
    },
    Page {
        route: 2,
        name: "Layout",
        cards: &[
            Card {
                label: "Dock layout",
                rows: &[
                    r(DOCKSTYLE, Kind::Combo(170), "Dock style", ""),
                    r(
                        DOCK_LEFT,
                        C,
                        "Icons at the start",
                        "Instead of the middle of a full-width dock",
                    ),
                    r(
                        DOCK_BADGES,
                        C,
                        "Show estimated notification badges",
                        "Counts are inferred from window titles",
                    ),
                    r(
                        DOCK_HIDE,
                        C,
                        "Hide behind windows",
                        "Rest the pointer at the bottom edge to bring it back",
                    ),
                ],
            },
            Card {
                label: "Windows",
                rows: &[
                    r(DESKTOP, C, "Show only windows on this desktop", ""),
                    r(HIDE_TASKBAR, C, "Hide Windows taskbar", ""),
                ],
            },
        ],
    },
    Page {
        route: 2,
        name: "Buttons",
        cards: &[
            Card {
                label: "Dock buttons",
                rows: &[
                    r(DBTN, C, "Start button", ""),
                    r(DBTN + 1, C, "Search button", ""),
                    r(DBTN + 2, C, "Task View button", ""),
                    r(DBTN + 3, C, "Show desktop pill", ""),
                    r(DBTN + 4, C, "Command center button", ""),
                    r(DBTN + 5, C, "Drives and Recycle Bin button", ""),
                    r(DBTN + 6, C, "Hidden icons button", ""),
                ],
            },
            Card {
                label: "Volume and brightness",
                rows: &[
                    r(EDGE_VOLUME, C, "Volume button on the dock", ""),
                    r(EDGE_BRIGHTNESS, C, "Brightness button on the dock", ""),
                ],
            },
        ],
    },
    Page {
        route: 2,
        name: "Widgets",
        cards: &[Card {
            label: "Dock widgets",
            rows: &[
                r(
                    WIDGET_LIMIT,
                    Kind::Combo(120),
                    "Maximum visible widgets",
                    "In order, up to this many",
                ),
                r(WIDGET_FIRST, C, "Weather", ""),
                r(WIDGET_FIRST + 1, C, "Prayer", ""),
                r(WIDGET_FIRST + 2, C, "Calendar", ""),
                r(WIDGET_FIRST + 3, C, "Water", ""),
                r(WIDGET_FIRST + 4, C, "System", ""),
                r(WIDGET_FIRST + 5, C, "Timer", "While a countdown runs"),
            ],
        }],
    },
    Page {
        route: 2,
        name: "Apps",
        cards: &[
            Card {
                label: "Pinned apps",
                rows: &[
                    r(PIN, Kind::Combo(200), "Pin to move", ""),
                    r(EARLIER, Kind::Btn, "Move pin earlier", ""),
                    r(LATER, Kind::Btn, "Move pin later", ""),
                ],
            },
            Card {
                label: "Hidden apps",
                rows: &[
                    r(HIDDEN, Kind::Combo(200), "Hidden app", ""),
                    r(SHOW, Kind::Btn, "Show in dock", ""),
                ],
            },
        ],
    },
    Page {
        route: 3,
        name: "Behaviour",
        cards: &[
            Card {
                label: "Opening",
                rows: &[
                    r(
                        NSIZE,
                        Kind::Combo(120),
                        "Notch size",
                        "Auto is the designed size",
                    ),
                    r(
                        NOPEN,
                        Kind::Combo(120),
                        "Open the notch",
                        "By hovering or by a click",
                    ),
                    r(
                        NOTCH_HIDE,
                        C,
                        "Hide behind windows",
                        "Rest the pointer at the top edge to bring it back",
                    ),
                ],
            },
            Card {
                label: "In the notch",
                rows: &[
                    r(EYES, C, "Mascot eyes", ""),
                    r(PRIVACY, C, "Privacy indicator", ""),
                    r(NOTIFS, C, "Show Windows notifications", ""),
                    r(CAPS_PILL, C, "Caps Lock pill", ""),
                ],
            },
            Card {
                label: "Shelf",
                rows: &[
                    r(CLIP, C, "Keep copied text on the Shelf", ""),
                    r(SHELF, Kind::Edit(70), "Shelf size", "Entries kept"),
                ],
            },
        ],
    },
    Page {
        route: 3,
        name: "Tabs and tiles",
        cards: &[
            Card {
                label: "Tabs",
                rows: &[
                    r(TAB, C, "Media tab", ""),
                    r(TAB + 1, C, "Weather tab", ""),
                    r(TAB + 2, C, "Prayer tab", ""),
                    r(TAB + 3, C, "Timer tab", ""),
                    r(TAB + 4, C, "Shelf tab", ""),
                ],
            },
            Card {
                label: "Calendar, system and battery",
                rows: &[
                    r(NETU, Kind::Combo(160), "Network speed", ""),
                    r(DISKD, Kind::Combo(160), "Disk tile", ""),
                    r(WEEK1, Kind::Combo(160), "Week starts on", ""),
                    r(LOWAT, Kind::Combo(160), "Low battery at", ""),
                    r(PILLLOW, C, "Low battery bolt in the pill", ""),
                ],
            },
        ],
    },
    Page {
        route: 4,
        name: "Timer",
        cards: &[Card {
            label: "Timer",
            rows: &[
                r(SOUND, C, "Finish sound", "Play a sound when a timer ends"),
                r(
                    TIMER,
                    Kind::Edit(120),
                    "Timer length",
                    "Minutes, m:ss or h:mm:ss",
                ),
                r(START, Kind::Btn, "Start timer", ""),
            ],
        }],
    },
    Page {
        route: 4,
        name: "Pomodoro",
        cards: &[
            Card {
                label: "Timing",
                rows: &[
                    r(PCFG, Kind::Edit(70), pomo::LABELS[0], ""),
                    r(PCFG + 1, Kind::Edit(70), pomo::LABELS[1], ""),
                    r(PCFG + 2, Kind::Edit(70), pomo::LABELS[2], ""),
                    r(PCFG + 3, Kind::Edit(70), pomo::LABELS[3], ""),
                ],
            },
            Card {
                label: "Pomodoro",
                rows: &[
                    r(PSTART, Kind::Btn, "Start / pause", "Pomodoro"),
                    r(PSKIP, Kind::Btn, "Skip", "Skip to the next phase"),
                ],
            },
        ],
    },
    Page {
        route: 4,
        name: "Water",
        cards: &[
            Card {
                label: "Reminders",
                rows: &[
                    r(
                        WATER,
                        C,
                        "Water reminders",
                        "A reminder to drink during the active hours",
                    ),
                    r(WCFG, Kind::Edit(70), water::LABELS[0], ""),
                    r(WCFG + 1, Kind::Edit(70), water::LABELS[1], ""),
                    r(WCFG + 2, Kind::Edit(70), water::LABELS[2], ""),
                    r(WCFG + 3, Kind::Edit(70), water::LABELS[3], ""),
                    r(WCFG + 4, Kind::Edit(70), water::LABELS[4], ""),
                ],
            },
            Card {
                label: "Today",
                rows: &[r(WDRANK, Kind::Btn, "I drank a glass", "Today")],
            },
        ],
    },
    Page {
        route: 5,
        name: "",
        cards: &[
            Card {
                label: "Location",
                rows: &[
                    r(DETECT, C, "Detect location automatically", ""),
                    r(
                        CITY,
                        Kind::Edit(200),
                        "Weather city",
                        "Leave empty for no weather",
                    ),
                ],
            },
            Card {
                label: "Units",
                rows: &[r(UNITS, Kind::Combo(170), "Temperature units", "")],
            },
        ],
    },
    Page {
        route: 6,
        name: "",
        cards: &[
            Card {
                label: "Reminders",
                rows: &[
                    r(PRAYER, C, "Prayer reminders", ""),
                    r(PMETHOD, Kind::Combo(240), "Prayer method", ""),
                    r(PLEAD, Kind::Combo(170), "Heads-up before the prayer", ""),
                    r(
                        PPAUSE,
                        C,
                        "Pause media at prayer time",
                        "Pauses a playing player (Media tab on), never resumes",
                    ),
                ],
            },
            Card {
                label: "Dock",
                rows: &[r(PMOON, C, "Moon on the dock", "")],
            },
            Card {
                label: "Time adjustments",
                rows: &[
                    r(PSHIFT, Kind::Edit(70), "Fajr shift", "Minutes, -30 to 30"),
                    r(
                        PSHIFT + 1,
                        Kind::Edit(70),
                        "Dhuhr shift",
                        "Minutes, -30 to 30",
                    ),
                    r(
                        PSHIFT + 2,
                        Kind::Edit(70),
                        "Asr shift",
                        "Minutes, -30 to 30",
                    ),
                    r(
                        PSHIFT + 3,
                        Kind::Edit(70),
                        "Maghrib shift",
                        "Minutes, -30 to 30",
                    ),
                    r(
                        PSHIFT + 4,
                        Kind::Edit(70),
                        "Isha shift",
                        "Minutes, -30 to 30",
                    ),
                ],
            },
            Card {
                label: "Sound",
                rows: &[
                    r(PSOUND, Kind::Combo(230), "Prayer sound", ""),
                    r(PBROWSE, Kind::Btn, "Browse...", "Choose a sound file"),
                    r(
                        PTEST,
                        Kind::Btn,
                        "Test sound",
                        "Plays even when Reminder sounds is off",
                    ),
                ],
            },
        ],
    },
    Page {
        route: 7,
        name: "Limits",
        cards: &[
            Card {
                label: "Dock",
                rows: &[r(TAB + 5, C, "Show AI limits in the dock", "")],
            },
            Card {
                label: "Permissions",
                rows: &[
                    r(LIMCODEX, C, "Run Codex on refresh", ""),
                    r(LIMAGY, C, "Run Antigravity on refresh", ""),
                ],
            },
            Card {
                label: "Display",
                rows: &[r(LIMGROUP, Kind::Combo(240), "Antigravity models", "")],
            },
            Card {
                label: "Claude Code",
                rows: &[
                    r(
                        LIMSNIP,
                        Kind::Edit(220),
                        "Statusline command",
                        "Optional: copy into settings.json",
                    ),
                    r(LIMCOPY, Kind::Btn, "Copy", "Copy the whole command"),
                ],
            },
        ],
    },
    Page {
        route: 7,
        name: "Agent status",
        cards: &[Card {
            label: "Hooks",
            rows: &[
                r(
                    AGENT_ON,
                    C,
                    "Show agent status",
                    "The eyes and pill show a coding agent working",
                ),
                r(ADD_ALL, Kind::Btn, "Add all", "Every detected agent"),
                r(AGENT_BTN, Kind::Btn, "Add hook", "Claude Code"),
                r(AGENT_BTN + 1, Kind::Btn, "Add hook", "Codex"),
                r(AGENT_BTN + 2, Kind::Btn, "Add hook", "Gemini CLI"),
                r(AGENT_BTN + 3, Kind::Btn, "Add hook", "Antigravity"),
                r(AGENT_BTN + 4, Kind::Btn, "Add hook", "Cursor"),
                r(AGENT_BTN + 5, Kind::Btn, "Add hook", "Copilot CLI"),
                r(AGENT_BTN + 6, Kind::Btn, "Add hook", "opencode"),
                r(
                    ASK_BTN,
                    Kind::Btn,
                    "Add hook",
                    "Allow or deny Claude's questions in the notch",
                ),
            ],
        }],
    },
    Page {
        route: 8,
        name: "",
        cards: &[
            Card {
                label: "Shortcuts",
                rows: &[
                    r(HOTKEY, C, "Open or close the notch", "Ctrl+Alt+N"),
                    r(HOTKEY + 1, C, "Open dock apps by number", "Ctrl+Alt+1 to 9"),
                    r(HOTKEY + 2, C, "Hide or show the dock", "Ctrl+Alt+D"),
                    r(HOTKEY + 3, C, "Open hidden icons", "Ctrl+Alt+B"),
                    r(HOTKEY + 4, C, "Mute the microphone", "Ctrl+Alt+M"),
                ],
            },
            Card {
                label: "Why Ctrl and Alt",
                rows: &[r(
                    LABEL,
                    Kind::Note,
                    "Windows keeps Win+number, so these use Ctrl+Alt.",
                    "",
                )],
            },
        ],
    },
    Page {
        route: 9,
        name: "",
        cards: &[
            Card {
                label: "ROQA",
                rows: &[r(
                    LABEL,
                    Kind::Note,
                    concat!(
                        "ROQA ",
                        env!("CARGO_PKG_VERSION"),
                        " (",
                        env!("ROQA_COMMIT"),
                        ")"
                    ),
                    "",
                )],
            },
            Card {
                label: "Diagnostics",
                rows: &[r(
                    DIAGNOSTICS,
                    Kind::Edit(220),
                    "Report file",
                    "Leave empty to turn off",
                )],
            },
            Card {
                label: "Reset",
                rows: &[r(
                    RESET,
                    Kind::Btn,
                    "Reset to defaults",
                    "Every setting returns to its default",
                )],
            },
        ],
    },
];

/// The pages of destination `route`, as indices into `PAGES` (consecutive, in tab order).
pub(super) fn pages_of(route: usize) -> std::ops::Range<usize> {
    let first = PAGES.iter().position(|p| usize::from(p.route) == route);
    let count = PAGES
        .iter()
        .filter(|p| usize::from(p.route) == route)
        .count();
    first.map_or(0..0, |f| f..f + count)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row id, in order, by destination: the spec's route map. A setting that moves routes fails here.
    fn spec() -> Vec<(&'static str, Vec<usize>)> {
        let range = |first: usize, n: usize| (first..first + n).collect::<Vec<_>>();
        let cat = |parts: &[Vec<usize>]| parts.concat();
        vec![
            (
                "General",
                cat(&[
                    vec![DOCK, NOTCH],
                    vec![AUTOSTART, HIDE_CAPTURE, FSHIDE, RSOUNDS],
                ]),
            ),
            ("Appearance", vec![THEME, MATCH_ACCENT, ANIM, REVEAL, HIDE]),
            (
                "Dock",
                cat(&[
                    vec![
                        DOCKSTYLE,
                        DOCK_LEFT,
                        DOCK_BADGES,
                        DOCK_HIDE,
                        DESKTOP,
                        HIDE_TASKBAR,
                    ],
                    range(DBTN, 7),
                    vec![EDGE_VOLUME, EDGE_BRIGHTNESS, WIDGET_LIMIT],
                    range(WIDGET_FIRST, 6),
                    vec![PIN, EARLIER, LATER, HIDDEN, SHOW],
                ]),
            ),
            (
                "Notch",
                cat(&[
                    vec![
                        NSIZE, NOPEN, NOTCH_HIDE, EYES, PRIVACY, NOTIFS, CAPS_PILL, CLIP, SHELF,
                    ],
                    range(TAB, 5),
                    vec![NETU, DISKD, WEEK1, LOWAT, PILLLOW],
                ]),
            ),
            (
                "Timers & water",
                cat(&[
                    vec![SOUND, TIMER, START],
                    range(PCFG, 4),
                    vec![PSTART, PSKIP, WATER],
                    range(WCFG, 5),
                    vec![WDRANK],
                ]),
            ),
            ("Weather", vec![DETECT, CITY, UNITS]),
            (
                "Prayer",
                cat(&[
                    vec![PRAYER, PMETHOD, PLEAD, PPAUSE, PMOON],
                    range(PSHIFT, 5),
                    vec![PSOUND, PBROWSE, PTEST],
                ]),
            ),
            (
                "Limits & agents",
                cat(&[
                    vec![
                        TAB + 5,
                        LIMCODEX,
                        LIMAGY,
                        LIMGROUP,
                        LIMSNIP,
                        LIMCOPY,
                        AGENT_ON,
                        ADD_ALL,
                    ],
                    range(AGENT_BTN, 7),
                    vec![ASK_BTN],
                ]),
            ),
            ("Hotkeys", range(HOTKEY, 5)),
            ("About", vec![DIAGNOSTICS, RESET]),
        ]
    }

    fn ids_of(route: usize) -> Vec<usize> {
        pages_of(route)
            .flat_map(|p| PAGES[p].cards)
            .flat_map(|c| c.rows)
            .filter(|r| r.kind != Kind::Note)
            .map(|r| r.id)
            .collect()
    }

    #[test]
    fn every_old_setting_is_on_its_specified_route_in_order() {
        let spec = spec();
        assert_eq!(spec.len(), ROUTES.len());
        for (route, (name, ids)) in spec.iter().enumerate() {
            assert_eq!(ROUTES[route].name, *name);
            assert_eq!(&ids_of(route), ids, "{name}");
        }
        // The map covers all 112 mapped controls (the monitors are runtime rows, one per display).
        assert_eq!(spec.iter().map(|(_, ids)| ids.len()).sum::<usize>(), 112);
    }

    #[test]
    fn routes_and_their_local_pages_are_consecutive_and_short() {
        let mut last = 0;
        for (index, page) in PAGES.iter().enumerate() {
            let route = usize::from(page.route);
            assert!(
                route == last || route == last + 1,
                "page {index} jumps routes"
            );
            last = route;
        }
        assert_eq!(last + 1, ROUTES.len());
        for (route, name) in ROUTES.iter().map(|r| r.name).enumerate() {
            let pages = pages_of(route);
            assert!(!pages.is_empty() && pages.len() <= 4, "{name}");
            // A destination with one page has no local tab name; with several, every page is named.
            let named = pages.clone().all(|p| !PAGES[p].name.is_empty());
            assert_eq!(named, pages.len() > 1, "{name}");
        }
    }
}
