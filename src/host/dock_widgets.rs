use super::*;
use crate::widgets;

impl Desktop {
    pub(super) fn refresh_stats(&mut self) -> u32 {
        let answer = widgets::system_answer();
        let Some(opened) = self
            .pages
            .system_opened
            .filter(|_| self.widget_popup_is(Button::System))
        else {
            self.pages.system_opened = None;
            return INFINITE;
        };
        if let Some((request, sample)) = answer
            && opened == request
        {
            self.pages.system = sample;
            self.refresh_widget_popup(Button::System);
        }
        let now = Instant::now();
        let counters = self.pages.system.counters;
        if let Some(due) = counters.deadline().filter(|&due| due > now) {
            return milliseconds_until(due, now);
        }
        if widgets::request_system(opened, counters, self.settings.widgets.disk_drive) {
            INFINITE
        } else {
            widgets::STATS_STEP.as_millis() as u32
        }
    }

    pub(super) fn visible_dock_widgets(&self) -> [bool; 6] {
        if !self.settings.dock {
            return [false; 6];
        }
        let mut shown = self.settings.dock_buttons.widgets;
        shown[5] &= self.timers.running();
        let mut remaining = self.settings.widget_limit;
        for show in &mut shown {
            *show &= remaining > 0;
            remaining -= usize::from(*show);
        }
        shown
    }

    pub(super) fn refresh_widget_readings(&mut self) -> Result<()> {
        let shown = self.visible_dock_widgets();
        let buttons = self.dock_buttons();
        let changed = self
            .surfaces
            .iter()
            .filter(|dock| dock.kind() == SurfaceKind::Dock)
            .any(|dock| {
                self.monitors
                    .iter()
                    .find(|monitor| monitor.device == dock.monitor)
                    .is_some_and(|monitor| {
                        dock.dock_buttons.widgets
                            != buttons
                                .fit_widgets(
                                    monitor.dock_room(self.settings.dock_style).width,
                                    dock.dpi,
                                )
                                .widgets
                    })
            });
        if let Some(Hit::Button(button)) =
            self.hover_popup.as_ref().and_then(|popup| popup.hover_hit)
            && let Some(index) = button.widget()
            && !shown[index]
        {
            self.hover_popup = None;
        }
        if !changed && !shown.contains(&true) {
            return Ok(());
        }
        let reads = std::array::from_fn(|index| {
            let button = Button::WIDGETS[index];
            if !shown[index] {
                return String::new();
            }
            match button {
                Button::Weather => self.pages.weather.temp.clone(),
                Button::Prayer => format!("{} {}", self.pages.prayer.next, self.pages.prayer.left),
                Button::Calendar => {
                    let date = prayer_os::local_now().ymd;
                    let weekday = (prayer_tab::day_number(date) + 4).rem_euclid(7) as usize;
                    format!(
                        "{} {}",
                        ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][weekday],
                        date.2
                    )
                }
                Button::Water => {
                    format!("{} of {}", self.water_today(), self.settings.water_cfg[0])
                }
                Button::Timer => self
                    .timers
                    .view(Instant::now())
                    .map_or_else(String::new, |timer| timer.pill),
                _ => String::new(),
            }
        });
        let repaint = self.pages.widget_readings != reads;
        self.pages.widget_readings = reads;
        if changed {
            self.relayout_docks()?;
        } else if repaint {
            for dock in self
                .surfaces
                .iter_mut()
                .filter(|surface| surface.kind() == SurfaceKind::Dock)
            {
                dock.live = true;
            }
        }
        if let Some(Hit::Button(button)) =
            self.hover_popup.as_ref().and_then(|popup| popup.hover_hit)
            && let Some(index) = button.widget()
            && shown[index]
            && (repaint || button == Button::Timer)
        {
            self.refresh_widget_popup(button);
        }
        Ok(())
    }

    pub(super) fn widget_popup_is(&self, button: Button) -> bool {
        self.hover_popup
            .as_ref()
            .is_some_and(|popup| popup.hover_hit == Some(Hit::Button(button)))
    }

    pub(super) fn show_widget_popup(&mut self, hwnd: HWND, button: Button) {
        self.pages.month = None;
        if button == Button::System {
            self.pages.system = Default::default();
            self.pages.system_opened = Some(Instant::now());
        }
        let (items, titles) = self.widget_rows(button);
        self.show_owned_dock_popup(hwnd, button, items, titles);
    }

    pub(super) fn show_owned_dock_popup(
        &mut self,
        hwnd: HWND,
        button: Button,
        items: Vec<Item>,
        titles: Vec<String>,
    ) {
        let Some((buttons, bounds, dpi, work)) = self.dock_at(hwnd).and_then(|dock| {
            self.monitors
                .iter()
                .find(|monitor| monitor.device == dock.monitor)
                .map(|monitor| (dock.dock_buttons, dock.bounds, dock.dpi, monitor.work))
        }) else {
            return;
        };
        if !buttons.on(button) {
            return;
        }
        let hit = Hit::Button(button);
        let (left, width) = dock::span(buttons, bounds.width, dpi, hit);
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            identity: Identity::Unavailable(0),
            windows: Vec::new(),
            items,
            dock_bounds: bounds,
            work,
            dpi,
            app: String::new(),
            titles,
            icons: Vec::new(),
            x: bounds.x + left + width / 2,
            below: None,
            return_to: None,
        };
        if let Ok(mut popup) = dock_menu::Menu::open_hover(&self.graphics, self.palette, spec) {
            popup.hover_hit = Some(hit);
            self.hover_popup = Some(popup);
        }
    }

    pub(super) fn refresh_widget_popup(&mut self, button: Button) {
        let (items, titles) = self.widget_rows(button);
        if let Some(popup) = &mut self.hover_popup {
            let _ = popup.update_rows(items, titles);
        }
    }

    fn widget_rows(&self, button: Button) -> (Vec<Item>, Vec<String>) {
        let mut rows = Vec::new();
        match button {
            Button::Weather => {
                let weather = &self.pages.weather;
                rows.push((
                    Item::Label,
                    weather.message.map_or_else(
                        || format!("{}  {}", weather.temp, weather.cond),
                        str::to_owned,
                    ),
                ));
                rows.extend(
                    weather
                        .hours
                        .iter()
                        .take(3)
                        .map(|hour| (Item::MoonDate, format!("{}\t{}", hour.label, hour.temp))),
                );
            }
            Button::Prayer => {
                let prayer = &self.pages.prayer;
                if let Some(message) = prayer.message {
                    rows.push((Item::Label, message.to_owned()));
                } else {
                    rows.extend(
                        prayer
                            .rows
                            .iter()
                            .take(5)
                            .map(|row| (Item::MoonDate, format!("{}\t{}", row.name, row.time))),
                    );
                }
            }
            Button::Calendar => {
                let today = prayer_os::local_now().ymd;
                let (year, month) = self.pages.month.unwrap_or((today.0 as u16, today.1 as u16));
                let sunday = self.settings.widgets.week_sunday;
                rows.push((
                    Item::Label,
                    format!("{} {year}", widgets::MONTHS[usize::from(month - 1)]),
                ));
                rows.push((
                    Item::CalendarWeek(None),
                    if sunday {
                        "Su\tMo\tTu\tWe\tTh\tFr\tSa"
                    } else {
                        "Mo\tTu\tWe\tTh\tFr\tSa\tSu"
                    }
                    .to_owned(),
                ));
                for week in widgets::month_grid(year, month, sunday).chunks_exact(7) {
                    let current = if (i32::from(year), u32::from(month)) == (today.0, today.1) {
                        week.iter()
                            .position(|&(day, inside)| inside && u32::from(day) == today.2)
                            .map(|index| index as u8)
                    } else {
                        None
                    };
                    rows.push((
                        Item::CalendarWeek(current),
                        week.iter()
                            .map(|(day, inside)| {
                                if *inside {
                                    day.to_string()
                                } else {
                                    String::new()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\t"),
                    ));
                }
                rows.push((
                    Item::Widget(dock_menu::WidgetAction::MonthBack),
                    "Previous month".into(),
                ));
                rows.push((
                    Item::Widget(dock_menu::WidgetAction::MonthNext),
                    "Next month".into(),
                ));
            }
            Button::Water => {
                let drank = self.water_today();
                let pct = (u16::from(drank) * 100 / self.settings.water_cfg[0]).min(100) as u8;
                rows.push((
                    Item::Limit(pct),
                    water::text(drank, &self.settings.water_cfg),
                ));
                rows.push((
                    Item::Widget(dock_menu::WidgetAction::WaterAdd),
                    "Add a glass".into(),
                ));
                if drank > 0 {
                    rows.push((
                        Item::Widget(dock_menu::WidgetAction::WaterRemove),
                        "Remove one".into(),
                    ));
                }
            }
            Button::Timer => {
                if let Some(view) = self.timers.view(Instant::now()).filter(|view| !view.done) {
                    let progress = view.total.saturating_sub(view.left) * 100 / view.total.max(1);
                    rows.push((Item::Limit(progress.min(100) as u8), view.panel));
                    rows.push((
                        Item::Widget(dock_menu::WidgetAction::TimerPause),
                        "Pause".into(),
                    ));
                    rows.push((
                        Item::Widget(dock_menu::WidgetAction::TimerExtend),
                        "Add 5 min".into(),
                    ));
                }
            }
            Button::System => {
                let system = &self.pages.system;
                let stats = system.counters;
                let bits = self.settings.widgets.net_bits;
                let meter = |usage: Option<&widgets::Usage>| {
                    usage.map_or_else(
                        || "Not available".to_owned(),
                        |usage| format!("{}% ? {}", usage.percent, usage.detail),
                    )
                };
                rows.push((
                    Item::MoonDate,
                    format!(
                        "Processor\t{}",
                        stats
                            .cpu
                            .map_or_else(|| "Measuring".to_owned(), |pct| format!("{pct}%"))
                    ),
                ));
                rows.push((
                    Item::MoonDate,
                    format!("Memory\t{}", meter(system.memory.as_ref())),
                ));
                rows.push((
                    Item::MoonDate,
                    format!("Disk\t{}", meter(system.disk.as_ref())),
                ));
                rows.push((
                    Item::MoonDate,
                    format!(
                        "Network\t{}",
                        stats.net.map_or_else(
                            || "Measuring".to_owned(),
                            |(down, up)| format!(
                                "{} / {}",
                                widgets::speed(down, bits),
                                widgets::speed(up, bits)
                            )
                        )
                    ),
                ));
            }
            _ => {}
        }
        if rows.is_empty() {
            rows.push((Item::Label, "Not available".into()));
        }
        rows.into_iter().unzip()
    }

    pub(super) fn press_widget(&mut self, action: dock_menu::WidgetAction) {
        use dock_menu::WidgetAction::*;
        let button = match action {
            WaterAdd | WaterRemove => {
                if action == WaterAdd {
                    self.water_drink();
                } else {
                    self.store.water.1 = self.water_today().saturating_sub(1);
                    self.pending.mark(Instant::now());
                }
                self.sync_water();
                Button::Water
            }
            MonthBack | MonthNext => {
                let today = prayer_os::local_now().ymd;
                let (year, month) = self.pages.month.unwrap_or((today.0 as u16, today.1 as u16));
                let next = (i32::from(year) * 12 + i32::from(month) - 1
                    + if action == MonthBack { -1 } else { 1 })
                .clamp(1601 * 12, 10_000 * 12 - 1);
                self.pages.month = Some(((next / 12) as u16, (next % 12 + 1) as u16));
                Button::Calendar
            }
            TimerPause | TimerExtend => {
                let now = Instant::now();
                if action == TimerPause {
                    self.timer_press(timer::Ctl::Pause, now);
                } else if self.timers.extend_soonest(now, timer::wall_now(), 300) {
                    self.timers
                        .write_to(&mut self.store, &mut self.pending, now);
                    self.request_notches(Repaint::Full, |_| true);
                }
                Button::Timer
            }
        };
        self.refresh_widget_popup(button);
    }
}
