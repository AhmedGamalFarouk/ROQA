use super::*;
use crate::{
    dock_menu::Quick,
    quick::networks::{self, Page},
};

#[derive(Default)]
pub(super) struct Center {
    page: Option<Page>,
    wifi: Option<Vec<networks::Row>>,
    rows: Option<Vec<networks::Row>>,
}

impl Desktop {
    pub(super) fn show_command_center(&mut self, hwnd: HWND) {
        self.dismiss_preview();
        self.center = Center::default();
        self.audio.start();
        self.audio.refresh();
        quick::refresh(None);
        self.prepare_command_brightness(hwnd);
        if quick::radio_on(quick::Kind::WiFi) == Some(true) {
            networks::request(Page::WiFi, None);
        }
        let (items, titles) = self.command_rows();
        self.show_owned_dock_popup(hwnd, Button::Quick, items, titles);
    }

    pub(super) fn command_open(&self) -> bool {
        self.hover_popup
            .as_ref()
            .is_some_and(|popup| popup.hover_hit == Some(Hit::Button(Button::Quick)))
    }

    pub(super) fn command_rows(&self) -> (Vec<Item>, Vec<String>) {
        let mut rows = Vec::new();
        if let Some(page) = self.center.page {
            let heading = if page == Page::WiFi {
                "Saved networks in range"
            } else {
                "Paired devices"
            };
            rows.push((Item::Quick(Quick::Back), "Back".into()));
            rows.push((Item::Label, heading.into()));
            match &self.center.rows {
                None => rows.push((Item::Label, "Reading devices".into())),
                Some(found) if found.is_empty() => rows.push((
                    Item::Label,
                    if page == Page::WiFi {
                        "No saved networks nearby"
                    } else {
                        "No paired devices reported"
                    }
                    .into(),
                )),
                Some(found) => rows.extend(found.iter().enumerate().map(|(index, row)| {
                    if row.can_connect() {
                        (
                            Item::Quick(Quick::Connect(index)),
                            format!("{}  ?  {}", row.name, row.detail),
                        )
                    } else {
                        (Item::MoonDate, format!("{}\t{}", row.name, row.detail))
                    }
                })),
            }
            rows.push(if page == Page::WiFi {
                (Item::Quick(Quick::WifiSettings), "More networks".into())
            } else {
                (Item::Quick(Quick::BluetoothSettings), "Add a device".into())
            });
            return rows.into_iter().unzip();
        }
        let mut tile = |action, name, state: &str, selected, detail| {
            rows.push((
                Item::QuickTile(action, selected, detail),
                format!("{name}\t{state}"),
            ));
        };
        let wifi_display = self
            .center
            .wifi
            .as_ref()
            .and_then(|rows| rows.iter().find(|row| row.connected))
            .map(|row| row.name.as_str());
        if let Some(on) = quick::radio_on(quick::Kind::WiFi) {
            tile(
                Quick::WiFi,
                "Wi-Fi",
                if on {
                    wifi_display.unwrap_or("On")
                } else {
                    "Off"
                },
                on,
                on,
            );
        }
        if let Some(on) = quick::radio_on(quick::Kind::Bluetooth) {
            tile(
                Quick::Bluetooth,
                "Bluetooth",
                if on { "On" } else { "Off" },
                on,
                on,
            );
        }
        let airplane = quick::radio_on(quick::Kind::Airplane);
        tile(
            Quick::Airplane,
            "Airplane",
            match airplane {
                Some(true) => "On",
                Some(false) => "Off",
                None => "Open settings",
            },
            airplane == Some(true),
            false,
        );
        tile(Quick::Hotspot, "Hotspot", "Open settings", false, false);
        tile(
            Quick::KeepAwake,
            "Keep awake",
            if self.keep_awake { "On" } else { "Off" },
            self.keep_awake,
            false,
        );
        let light = self.settings.theme == crate::theme::Theme::Light;
        tile(
            Quick::Theme,
            "Theme",
            if light { "Light" } else { "Dark" },
            light,
            false,
        );
        if let Some(level) = self.audio.level() {
            rows.push((Item::Level(level.percent, level.muted), "Volume".into()));
            rows.push((
                Item::Quick(Quick::Mute),
                if level.muted { "Unmute" } else { "Mute" }.into(),
            ));
        } else {
            rows.push((Item::Label, "No sound device".into()));
        }
        rows.push(self.command_brightness());
        if let Some(mode) = quick::mode() {
            let title = power::Raw::read()
                .and_then(|battery| {
                    let summary = battery.summary()?;
                    Some(
                        battery
                            .left
                            .filter(|_| battery.ac == 0)
                            .map_or(summary.clone(), |secs| {
                                format!("{summary}, {} min", secs.div_ceil(60))
                            }),
                    )
                })
                .map_or_else(
                    || mode.label().to_owned(),
                    |battery| format!("{} ? {battery}", mode.label()),
                );
            rows.push((Item::Quick(Quick::Power), title));
        }
        rows.push((Item::Quick(Quick::Lock), "Lock".into()));
        rows.push((Item::Quick(Quick::Sleep), "Sleep".into()));
        rows.into_iter().unzip()
    }

    pub(super) fn refresh_quick_menu(&mut self) {
        if !self.command_open() {
            return;
        }
        let (items, titles) = self.command_rows();
        if let Some(popup) = &mut self.hover_popup
            && !popup.held()
        {
            let _ = popup.update_rows(items, titles);
        }
    }

    pub(super) fn network_answers(&mut self) {
        let Some((page, rows)) = networks::answer() else {
            return;
        };
        if !self.command_open() {
            return;
        }
        if page == Page::WiFi {
            self.center.wifi = Some(rows.clone());
        }
        if self.center.page.is_none() {
            self.refresh_quick_menu();
        }
        if self.center.page == Some(page) {
            self.center.rows = Some(rows);
            self.refresh_quick_menu();
        } else if let Some(current) = self.center.page
            && self.center.rows.is_none()
        {
            networks::request(current, None);
        }
    }

    pub(super) fn press_command(&mut self, action: Quick, catalog: &mut Catalog) {
        let busy = |desktop: &mut Self| {
            desktop.show_notice("Still busy. Try again.".into(), Instant::now())
        };
        let launch = |desktop: &mut Self, target: &str| {
            desktop.hover_popup = None;
            desktop.run_dock_action(dock::Action::Launch(target.into()));
        };
        match action {
            Quick::Mute => self.audio.toggle_mute(),
            Quick::WiFi | Quick::Bluetooth | Quick::Airplane => {
                let kind = match action {
                    Quick::WiFi => quick::Kind::WiFi,
                    Quick::Bluetooth => quick::Kind::Bluetooth,
                    _ => quick::Kind::Airplane,
                };
                if kind == quick::Kind::Airplane && quick::radio_on(kind).is_none() {
                    return launch(self, "ms-settings:network-airplanemode");
                }
                if !quick::refresh(Some(kind)) {
                    busy(self);
                }
            }
            Quick::Hotspot => return launch(self, "ms-settings:network-mobilehotspot"),
            Quick::WifiSettings => return launch(self, "ms-availablenetworks:"),
            Quick::BluetoothSettings => return launch(self, "ms-settings:bluetooth"),
            Quick::WifiPage | Quick::BluetoothPage => {
                let page = if action == Quick::WifiPage {
                    Page::WiFi
                } else {
                    Page::Bluetooth
                };
                self.center.page = Some(page);
                self.center.rows = None;
                networks::request(page, None);
            }
            Quick::Connect(index) => {
                if let Some(row) = self
                    .center
                    .rows
                    .as_ref()
                    .and_then(|rows| rows.get(index))
                    .filter(|row| row.can_connect())
                    && !networks::request(Page::WiFi, Some(row))
                {
                    busy(self);
                }
            }
            Quick::Back => {
                self.center.page = None;
                self.center.rows = None;
            }
            Quick::Power => quick::next_mode(),
            Quick::KeepAwake => {
                self.keep_awake = !self.keep_awake;
                crate::system_os::keep_awake(self.keep_awake);
            }
            Quick::Theme => {
                let new_theme = if self.settings.theme == crate::theme::Theme::Light {
                    self.dark_theme
                } else {
                    self.dark_theme = self.settings.theme;
                    crate::theme::Theme::Light
                };
                let next = Settings {
                    theme: new_theme,
                    ..self.settings.clone()
                };
                if let Err(error) = self.apply_settings(next, catalog) {
                    eprintln!("Theme: {error}");
                }
                if let Some(window) = &mut self.settings_window {
                    let _ = window.set_theme(self.settings.theme);
                }
                return;
            }
            Quick::Lock | Quick::Sleep => {
                self.hover_popup = None;
                if action == Quick::Lock {
                    crate::system_os::lock();
                } else {
                    crate::system_os::sleep();
                }
                return;
            }
        }
        self.refresh_quick_menu();
    }
}
