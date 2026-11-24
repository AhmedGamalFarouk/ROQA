//! The dock's menus and popups: quick settings, Places, stacks, item and app menus, and what a row does.
//! One `impl Desktop` block moved out of `host.rs` unchanged; the methods are `pub(super)` so the host reaches them.

use super::*;

impl Desktop {
    pub(super) fn hover_popup_message(&mut self, message: &MSG, catalog: &mut Catalog) -> bool {
        let Some(popup) = &mut self.hover_popup else {
            return false;
        };
        let slot = popup.slot;
        let hwnd = popup.dock();
        let origin = popup.hover_hit;
        match popup.message(message) {
            Event::Ignored => false,
            Event::Consumed => true,
            Event::Dismiss => {
                if message.message == WM_LBUTTONDOWN
                    && message.hwnd == hwnd
                    && let Some(Hit::Button(button)) = origin
                    && (button.widget().is_some()
                        || button == Button::Quick
                        || matches!(button, Button::Moon | Button::Volume | Button::Brightness))
                    && self
                        .dock_at(hwnd)
                        .and_then(|surface| surface.dock_hit(message.lParam, self.dock_slots()))
                        == origin
                {
                    self.swallow_up = Some((hwnd, button));
                    self.hover_spot = Some((
                        hwnd.0 as usize,
                        Hit::Button(button),
                        Instant::now() + Duration::from_secs(86_400),
                    ));
                }
                self.hover_popup = None;
                false
            }
            Event::Choose(Item::Member(index)) => {
                let member = self
                    .dock_items
                    .get(slot)
                    .and_then(|tile| tile.members.get(index))
                    .cloned();
                self.hover_popup = None;
                if let Some(member) = member {
                    self.open_member(&member);
                }
                true
            }
            Event::Choose(Item::MemberMenu(index)) => {
                let member = self
                    .dock_items
                    .get(slot)
                    .and_then(|tile| tile.members.get(index))
                    .cloned();
                self.hover_popup = None;
                if let Some(member) = member {
                    self.open_item_menu(hwnd, slot, member);
                }
                true
            }
            Event::Level(row, percent) => {
                self.set_level(row, percent);
                true
            }
            Event::Mute(row) => {
                self.mute_row(row);
                true
            }
            // The slider popups stay open: a device choice or a click on a note row does not close them.
            Event::Choose(Item::Device(index, _)) => {
                self.choose_device(index);
                true
            }
            Event::Choose(Item::Quick(action)) => {
                self.press_command(action, catalog);
                true
            }
            Event::Choose(Item::Widget(action)) => {
                self.press_widget(action);
                true
            }
            Event::Choose(Item::Label) => true,
            Event::Choose(_) => {
                self.hover_popup = None;
                true
            }
        }
    }
    fn refresh_battery_menu(&mut self) {
        let Some(mut menu) = self.menu.take() else {
            return;
        };
        if menu.items().first() != Some(&Item::Battery) {
            self.menu = Some(menu);
            return;
        }
        let titles = crate::device_battery::titles();
        if menu
            .relist(vec![Item::Battery; titles.len()], titles, Vec::new())
            .is_ok()
        {
            self.menu = Some(menu);
        } else {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
        }
    }
    /// Today's moon (`pages.moon`, for the dock button and its popup), rebuilt once the date or hemisphere
    /// changes; the docks repaint only when it did.
    pub(super) fn refresh_moon(&mut self) -> Result<()> {
        let today = prayer_os::local_now().ymd;
        let south = self
            .weather
            .place
            .as_ref()
            .is_some_and(|place| place.lat < 0);
        if (today, south) == (self.pages.moon.ymd, self.pages.moon.south) {
            return Ok(());
        }
        let day = crate::prayer_tab::moon_day(today, south, &prayer_os::hijri);
        if day != self.pages.moon {
            self.pages.moon = day;
            if self.settings.dock_moon {
                self.relayout_docks()?;
            }
        }
        Ok(())
    }
    /// A click on the moon, volume or brightness does what hover does: opens its popup now, or closes it and keeps
    /// it closed until the pointer leaves the button.
    fn toggle_popup(&mut self, hwnd: HWND, button: Button) {
        let (hit, now) = (Hit::Button(button), Instant::now());
        if self
            .hover_popup
            .as_ref()
            .is_some_and(|popup| popup.hover_hit == Some(hit) && !popup.closing())
        {
            self.hover_popup = None;
            self.hover_spot = Some((hwnd.0 as usize, hit, now + Duration::from_secs(86_400)));
        } else {
            self.hover_spot = Some((hwnd.0 as usize, hit, now));
            self.show_hover_popup(hwnd, hit);
        }
    }
    /// The moon's popup over its dock button: today's phase, lit share and Hijri date, then the next full moon,
    /// new moon and Hijri month.
    pub(super) fn show_moon_popup(&mut self, hwnd: HWND) {
        let Some((buttons, bounds, dpi, work)) = self.dock_at(hwnd).and_then(|dock| {
            self.monitors
                .iter()
                .find(|monitor| monitor.device == dock.monitor)
                .map(|monitor| (dock.dock_buttons, dock.bounds, dock.dpi, monitor.work))
        }) else {
            return;
        };
        let (day, moon) = (&self.pages.moon, self.pages.moon.moon);
        let dates = [("Full moon", moon.next_full), ("New moon", moon.next_new)]
            .map(|(label, ymd)| (label.to_owned(), ymd))
            .into_iter()
            .chain(day.month.clone());
        let titles: Vec<String> =
            std::iter::once(format!("{}\n{}% lit\n{}", moon.name, moon.pct, day.hijri))
                .chain(
                    dates.map(|(label, ymd)| {
                        format!("{label}\t{}", crate::prayer_tab::short_date(ymd))
                    }),
                )
                .collect();
        let items = std::iter::once(Item::Moon(moon.pct, moon.right))
            .chain(std::iter::repeat_n(Item::Pair, titles.len() - 1))
            .collect();
        let hit = Hit::Button(Button::Moon);
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
            app: "Moon".into(),
            titles,
            icons: Vec::new(),
            x: bounds.x + left + width / 2,
            below: None,
            return_to: None,
        };
        match dock_menu::Menu::open_hover(&self.graphics, self.palette, spec) {
            Ok(mut popup) => {
                popup.hover_hit = Some(hit);
                self.hover_popup = Some(popup);
            }
            Err(error) => eprintln!("Dock moon: {error}"),
        }
    }
    pub(super) fn open_windows_links(&mut self, hwnd: HWND) {
        self.hover_popup = None;
        self.hover_spot = None;
        self.open_status_menu(hwnd, Button::Start);
    }
    fn open_status_menu(&mut self, hwnd: HWND, button: Button) {
        self.dismiss_preview();
        self.close_menu();
        let Some(dock) = self.dock_at(hwnd).filter(|dock| dock.visible) else {
            return;
        };
        let (bounds, dpi, monitor, buttons) = (
            dock.bounds,
            dock.dpi,
            dock.monitor.clone(),
            dock.dock_buttons,
        );
        let return_to = dock::foreground_key();
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == monitor)
            .map(|m| m.work)
        else {
            return;
        };
        let (items, titles) = match button {
            Button::Start => (
                dock_menu::WindowsLink::ALL
                    .into_iter()
                    .map(Item::WindowsLink)
                    .collect(),
                Vec::new(),
            ),
            Button::Battery => {
                let titles = crate::device_battery::titles();
                (vec![Item::Battery; titles.len()], titles)
            }
            _ => return,
        };
        if items.is_empty() {
            return;
        }
        let (left, width) = dock::span(buttons, bounds.width, dpi, Hit::Button(button));
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            windows: Vec::new(),
            identity: Identity::Unavailable(0),
            items,
            dock_bounds: bounds,
            work,
            dpi,
            app: match button {
                Button::Start => "Quick links",
                Button::Battery => "Device batteries",
                _ => "Dock",
            }
            .into(),
            titles,
            icons: Vec::new(),
            x: bounds.x + left + width / 2,
            below: None,
            return_to,
        };
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => self.menu = Some(menu),
            Ok(_) => eprintln!("Dock status: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("Dock status: {error}"),
        }
    }
    /// The Places button's popup above it: the Recycle Bin and each drive as the shell reports them now, read on a
    /// worker (`stack_ready` opens it); a click opens that place in Explorer.
    pub(super) fn ask_places(&mut self, hwnd: HWND) {
        if self
            .pending_popup
            .is_some_and(|(_, due)| Instant::now() < due)
        {
            self.show_notice("Still reading. Try again.".into(), Instant::now());
            return;
        }
        self.dismiss_preview();
        self.close_menu();
        if crate::places::ask_places(hwnd.0 as isize) {
            self.pending_popup = Some((
                crate::anchored::Owner(hwnd),
                Instant::now() + crate::places::STALE,
            ));
        } else {
            self.show_notice("Still reading. Try again.".into(), Instant::now());
        }
    }
    /// Opens the Places popup with the rows a worker read (`places::ask_places`).
    fn open_places_menu(&mut self, hwnd: HWND, places: Vec<crate::places::Place>) {
        self.dismiss_preview();
        self.close_menu();
        let Some(dock) = self.dock_at(hwnd).filter(|dock| dock.visible) else {
            return;
        };
        let (bounds, dpi, monitor, buttons) = (
            dock.bounds,
            dock.dpi,
            dock.monitor.clone(),
            dock.dock_buttons,
        );
        let return_to = dock::foreground_key();
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == monitor)
            .map(|m| m.work)
        else {
            return;
        };
        let (left, width) = dock::span(buttons, bounds.width, dpi, Hit::Button(Button::Places));
        let (items, titles) = crate::places::rows(&places);
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            windows: Vec::new(),
            identity: Identity::Unavailable(0),
            items,
            dock_bounds: bounds,
            work,
            dpi,
            app: "Places".into(),
            titles,
            icons: Vec::new(),
            x: bounds.x + left + width / 2,
            below: None,
            return_to,
        };
        self.places = places;
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => self.menu = Some(menu),
            Ok(_) => eprintln!("Places: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("Places: {error}"),
        }
    }
    /// A click on a pinned path that may be a folder: a worker reads it (`stack_ready` goes on). One read at a time,
    /// so a click while one is out says so.
    pub(super) fn ask_stack(&mut self, hwnd: HWND, slot: usize, path: String) {
        if crate::places::ask(hwnd.0 as isize, slot, path) {
            self.pending_popup = Some((
                crate::anchored::Owner(hwnd),
                Instant::now() + crate::places::STALE,
            ));
        } else {
            self.show_notice("Still reading. Try again.".into(), Instant::now());
        }
    }
    /// What the worker found for a click on a pinned path (`places::ask`): a folder opens its stack unless the click is
    /// out of date (a later click, a menu opened since, too slow) or the slot holds another pin now, and a path that is
    /// not a folder is launched like any other pin.
    pub(super) fn stack_ready(&mut self, answer: crate::places::Answer) {
        let (current, slow) = (answer.current(), answer.slow());
        if current {
            self.pending_popup = None;
        }
        if let Some(places) = answer.places {
            if current {
                self.open_places_menu(HWND(answer.dock as *mut _), places);
            } else if slow {
                self.show_notice("Places were slow. Try again.".into(), Instant::now());
            }
            return;
        }
        let held = self
            .dock_items
            .get(answer.slot)
            .is_some_and(|item| item.identity == Identity::Executable(answer.folder.clone()));
        if !answer.current() || !held {
            if held && answer.slow() {
                self.show_notice("That folder was slow. Try again.".into(), Instant::now());
            }
            return;
        }
        let hwnd = HWND(answer.dock as *mut _);
        match answer.files {
            Some(_) if self.menu.is_some() => {}
            Some(files) => self.open_stack(hwnd, answer.slot, &answer.folder, files),
            None => self.run_pin(answer.slot, Some(&answer.folder)),
        }
    }
    /// Files dropped on a dock slot open with that app: its program is started with the paths. A group of apps,
    /// a folder and an app with no program path (a packaged one) are told so instead. A program on a network share is
    /// not looked at here: `dock::launch_with` starts it on the network worker. Folders dropped anywhere else on the
    /// dock (a gap, a button, a pinned folder) are pinned, each a stack.
    pub(super) fn drop_on_dock(&mut self, hwnd: HWND, x: i32, paths: &[String]) {
        let Some(dock) = self.dock_at(hwnd) else {
            return;
        };
        let hit = dock::hit_at(
            dock.dock_buttons,
            x - dock.bounds.x,
            dock.bounds.width,
            dock.dpi,
            self.dock_slots(),
        );
        let item = hit
            .and_then(Hit::slot)
            .and_then(|slot| self.dock_items.get(slot));
        let program = item
            .filter(|item| item.members.is_empty())
            .and_then(|item| match &item.identity {
                Identity::Executable(path) => Some(path.as_str()),
                _ => item.executable.as_deref(),
            })
            .filter(|path| {
                std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                    && (shelf_os::is_network(path) || std::path::Path::new(path).is_file())
            });
        let quoted: Vec<_> = paths.iter().map(|path| quote_path(path)).collect();
        if program.is_some_and(|program| dock::launch_with(program, Some(&quoted.join(" ")))) {
            return;
        }
        // A share is not touched here (it may stall the host): its stack's worker reads it on the first click.
        let folders = paths.iter().all(|path| {
            if shelf_os::is_network(path) {
                crate::places::may_be_folder(path)
            } else {
                std::path::Path::new(path).is_dir()
            }
        });
        if folders && program.is_none() {
            let mut full = false;
            self.change_pins(|pins| {
                paths.iter().fold(false, |added, path| {
                    add_pin(pins, Identity::Executable(path.clone()), &mut full) || added
                })
            });
            self.say_pins_full(full);
        } else if item.is_some() {
            self.show_notice("That app can't open files.".into(), Instant::now());
        }
    }
    /// A file drag moved over a dock (`x` is the screen column) or left it: the slot under it starts or restarts
    /// the raise delay; `refresh_raise` fires it.
    pub(super) fn drag_over_dock(&mut self, hwnd: HWND, x: Option<i32>) {
        let slots = self.dock_slots();
        let spot = x.and_then(|x| {
            let dock = self.dock_at(hwnd)?;
            match dock::hit_at(
                dock.dock_buttons,
                x - dock.bounds.x,
                dock.bounds.width,
                dock.dpi,
                slots,
            )? {
                Hit::Slot(slot) => Some((hwnd.0 as usize, slot)),
                Hit::Button(_) => None,
            }
        });
        self.raise
            .point_after(spot, Instant::now(), dock::RAISE_DELAY);
    }
    /// Runs the dock action of pinned slot `slot` (launch, focus or cycle); with `folder`, only while the slot still
    /// holds that path, since the click that asked may be a moment old.
    pub(super) fn run_pin(&mut self, slot: usize, folder: Option<&str>) {
        let Some(item) = self.dock_items.get(slot) else {
            return;
        };
        if folder.is_some_and(|folder| item.identity != Identity::Executable(folder.to_owned())) {
            return;
        }
        if let Some(action) = dock::action(item, dock::foreground_key()) {
            self.run_dock_action(action);
        }
    }
    /// Says a pin was refused because the dock holds `MAX_PINS` already (`add_pin`).
    pub(super) fn say_pins_full(&mut self, full: bool) {
        if full {
            self.show_notice("Dock is full. Unpin one first.".into(), Instant::now());
        }
    }
    /// The wheel over a dock's app slot steps through that app's windows: down to the next, up to the previous.
    pub(super) fn wheel_dock(&mut self, message: &MSG) {
        let delta = i32::from((message.wParam.0 >> 16) as u16 as i16);
        let steps = edge::wheel_for(&mut self.wheel, edge::Wheel::Dock, delta);
        let slots = self.dock_slots();
        let hovered = self
            .dock_at(message.hwnd)
            .and_then(|surface| match surface.role {
                Role::Dock {
                    hover: Some(cell), ..
                } => dock::cell(surface.dock_buttons, slots, cell).and_then(Hit::slot),
                _ => None,
            });
        let Some(item) = hovered.and_then(|slot| self.dock_items.get(slot)) else {
            return;
        };
        if steps != 0
            && let Some(key) = dock::wheel_target(item, dock::foreground_key(), steps < 0)
            && !dock::activate_on_wheel(key)
        {
            self.show_notice(
                "Can't switch. Try the Windows taskbar.".into(),
                Instant::now(),
            );
        }
    }
    /// A pinned folder's stack above its dock slot: its newest files as rows; a click opens that file and an empty
    /// folder opens in Explorer instead.
    fn open_stack(
        &mut self,
        hwnd: HWND,
        slot: usize,
        folder: &str,
        files: Vec<std::path::PathBuf>,
    ) {
        self.dismiss_preview();
        self.close_menu();
        let Some(dock) = self.dock_at(hwnd).filter(|dock| dock.visible) else {
            return;
        };
        let (bounds, dpi, monitor, buttons) = (
            dock.bounds,
            dock.dpi,
            dock.monitor.clone(),
            dock.dock_buttons,
        );
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == monitor)
            .map(|m| m.work)
        else {
            return;
        };
        if files.is_empty() {
            if !dock::launch(folder) {
                self.show_open_failed();
            }
            return;
        }
        let (left, width) = dock::span(buttons, bounds.width, dpi, Hit::Slot(slot));
        // Rows draw now; each file's icon is asked of the icon worker and fills in as it arrives.
        let size = self.icon_size().unwrap_or(40);
        let name = std::path::Path::new(folder).file_name().map_or_else(
            || "Folder".into(),
            |name| name.to_string_lossy().into_owned(),
        );
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            windows: Vec::new(),
            identity: Identity::Unavailable(0),
            items: (0..files.len()).map(Item::File).collect(),
            dock_bounds: bounds,
            work,
            dpi,
            app: name,
            titles: files
                .iter()
                .map(|file| {
                    file.file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
                })
                .collect(),
            icons: files
                .iter()
                .map(|file| self.stack_icon(file, size))
                .collect(),
            x: bounds.x + left + width / 2,
            below: None,
            return_to: dock::foreground_key(),
        };
        self.stack = files;
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => self.menu = Some(menu),
            Ok(_) => eprintln!("Folder stack: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("Folder stack: {error}"),
        }
    }
    /// A listed file's shell icon: cached, or queued on the icon worker (`None` until it arrives).
    pub(super) fn stack_icon(
        &mut self,
        file: &std::path::Path,
        size: u32,
    ) -> Option<Arc<IconPixels>> {
        let identity = Identity::Executable(file.to_string_lossy().into_owned());
        let sources = icons::sources(&identity, None);
        self.icons.icon(&identity, sources, size)
    }
    /// Icons arrived while a folder stack is open: its rows pick them up.
    pub(super) fn fill_stack_icons(&mut self) {
        let open = self.menu.as_ref().is_some_and(|menu| {
            menu.items()
                .iter()
                .any(|item| matches!(item, Item::File(_)))
        });
        let Some(size) = self.icon_size().filter(|_| open) else {
            return;
        };
        let files = self.stack.clone();
        let icons = files
            .iter()
            .map(|file| self.stack_icon(file, size))
            .collect();
        if self
            .menu
            .as_mut()
            .is_some_and(|menu| menu.set_icons(icons).is_err())
        {
            self.close_menu();
        }
    }
    /// Applies one quick setting, then shows the new state in the same popup.
    /// What the short workers left for the host (read on every pass): a folder or the Places rows (`places`), new radio
    /// states for an open quick settings popup in place (`quick::refresh`), and a failed network launch.
    pub(super) fn worker_answers(&mut self) {
        if crate::device_battery::changed() {
            self.refresh_battery_menu();
        }
        if let Some(answer) = crate::places::answer() {
            self.stack_ready(answer);
        }
        self.network_answers();
        if quick::changed() {
            if quick::airplane_fallback() {
                self.hover_popup = None;
                self.run_dock_action(dock::Action::Launch(
                    "ms-settings:network-airplanemode".into(),
                ));
            }
            self.refresh_quick_menu();
            quick::run_waiting();
        }
        if shelf_os::network_failed() {
            self.show_open_failed();
        }
    }
    /// A taskbar button: the shell shortcut it stands for (sent on this click only), or its own popup.
    pub(super) fn press_button(&mut self, hwnd: HWND, button: Button) {
        if button != Button::Places {
            crate::places::invalidate(); // Places asks, and an accepted ask supersedes the older answer itself
            self.pending_popup = None;
        }
        match (button, button.keys()) {
            (Button::Battery, _) => self.open_status_menu(hwnd, button),
            (Button::Limits, _) => self.start_limits(crate::limits_os::now(), true),
            (Button::Moon | Button::Volume | Button::Brightness, _) => {
                self.toggle_popup(hwnd, button)
            }
            (button, _) if button.widget().is_some() => self.toggle_popup(hwnd, button),
            (Button::Quick, _) => self.toggle_popup(hwnd, button),
            (Button::Places, _) => self.ask_places(hwnd),
            (Button::Tray, _) => self.open_flyout(hwnd, crate::anchored::Kind::Tray),
            (Button::Start, _) => self.open_flyout(hwnd, crate::anchored::Kind::Start),
            (Button::Search, _) => self.open_flyout(hwnd, crate::anchored::Kind::Search),
            (_, Some(keys)) => {
                if !dock::send_keys(keys) {
                    eprintln!("Dock button: Windows refused the key input");
                }
            }
            (_, None) => {}
        }
    }
    /// Middle-click on a dock slot launches a new instance when the app can be launched.
    pub(super) fn middle_click_slot(&mut self, slot: usize) {
        if let Some(item) = self.dock_items.get(slot)
            && let Some(launch) = crate::pins::launch_target(&item.identity)
        {
            self.run_dock_action(dock::Action::Launch(launch));
        }
    }
    /// Right-click on a dock slot opens its menu above it (a gap has none). It needs the
    /// foreground for dismissal; if Windows refuses, no menu opens rather than one that
    /// a click elsewhere could never dismiss.
    pub(super) fn open_menu(&mut self, hwnd: HWND, slot: Option<usize>, lparam: LPARAM) {
        // Outside an app slot (a gap) the dock offers the app menu instead.
        if slot.is_none_or(|slot| slot >= self.dock_items.len()) {
            return self.open_app_menu(hwnd, lparam);
        }
        self.dismiss_preview();
        self.close_menu();
        let Some((_, slot)) = self
            .dock_at(hwnd)
            .zip(slot)
            .filter(|(dock, _)| dock.visible)
        else {
            return;
        };
        let Some(item) = self.dock_items.get(slot).cloned() else {
            return;
        };
        self.open_item_menu(hwnd, slot, item);
    }
    pub(super) fn open_item_menu(&mut self, hwnd: HWND, slot: usize, item: DockItem) {
        self.hover_popup = None;
        let Some(dock) = self.dock_at(hwnd) else {
            return;
        };
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == dock.monitor)
            .map(|m| m.work)
        else {
            return;
        };
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot,
            identity: item.identity.clone(),
            windows: item.windows.clone(),
            items: dock_menu::items(&item, &self.pins),
            dock_bounds: dock.bounds,
            work,
            dpi: dock.dpi,
            app: crate::pins::display_name(&item.identity),
            titles: Vec::new(),
            icons: Vec::new(),
            x: dock::slot_center_x(
                dock.bounds.x,
                dock.dock_buttons,
                dock.bounds.width,
                slot,
                dock.dpi,
            ),
            below: None,
            return_to: dock::foreground_key(),
        };
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => self.menu = Some(menu),
            Ok(_) => eprintln!("Dock menu: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("Dock menu: {error}"),
        }
    }
    /// Right-click on the notch, or on the dock outside an app slot: Settings and Quit, below the notch or
    /// above the dock. Needs the foreground like the slot menu.
    pub(super) fn open_app_menu(&mut self, hwnd: HWND, lparam: LPARAM) {
        self.open_context_menu(hwnd, lparam, None);
    }

    pub(super) fn open_context_menu(
        &mut self,
        hwnd: HWND,
        lparam: LPARAM,
        shelf_path: Option<String>,
    ) {
        self.dismiss_preview();
        self.close_menu();
        let Some(surface) = self
            .surfaces
            .iter()
            .find(|s| s.window.0 == hwnd && s.visible)
        else {
            return;
        };
        let Some(work) = self
            .monitors
            .iter()
            .find(|m| m.device == surface.monitor)
            .map(|m| m.work)
        else {
            return;
        };
        let below = (surface.kind() == SurfaceKind::Notch).then(|| {
            let media = self.pages.track(&self.media).is_some();
            let shape = layout::notch_shape(surface.motion.value(Instant::now()), media);
            surface.bounds.y + (shape.height * surface.dpi as f32 / 96.0).round() as i32
        });
        let mut items = if shelf_path.is_some() {
            vec![Item::RemoveShelf]
        } else {
            dock_menu::APP_ITEMS.to_vec()
        };
        // The dock offers its Downloads stack until it is pinned.
        if surface.kind() == SurfaceKind::Dock
            && let Some(path) = crate::places::downloads()
            && self.pins.position(&Identity::Executable(path)).is_none()
        {
            items.insert(0, Item::PinDownloads);
        }
        let shelf_menu = surface.kind() == SurfaceKind::Notch && self.page() == Tab::Shelf;
        if shelf_menu && self.shelf.removed.is_some() {
            items.push(Item::UndoShelf);
        }
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            windows: Vec::new(),
            identity: shelf_path.map_or(Identity::Unavailable(0), Identity::Executable),
            items,
            dock_bounds: surface.bounds,
            work,
            dpi: surface.dpi,
            app: if shelf_menu { "Shelf" } else { "ROQA" }.into(),
            titles: Vec::new(),
            icons: Vec::new(),
            x: surface.bounds.x + point(lparam).0,
            below,
            return_to: dock::foreground_key(),
        };
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => {
                self.menu = Some(menu);
                // The panel the pointer had opened closes, so the menu does not sit on it.
                self.surfaces
                    .iter_mut()
                    .for_each(|s| s.set_notch_hover(false));
            }
            Ok(_) => eprintln!("App menu: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("App menu: {error}"),
        }
    }
    /// Ends the open menu, giving the foreground back to the window it came from if the menu still holds it.
    pub(super) fn close_menu(&mut self) {
        if self.pending_popup.take().is_some() {
            crate::places::invalidate();
        }
        if let Some(menu) = self.menu.take() {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
        }
    }
    /// Input for the open menu. Returns true when the message is consumed.
    pub(super) fn menu_message(&mut self, message: &MSG) -> bool {
        let Some(menu) = &mut self.menu else {
            return false;
        };
        let own = message.hwnd == menu.hwnd();
        let (dock, toggles) = (menu.dock(), toggled_by(menu.items()));
        match menu.message(message) {
            Event::Ignored => false,
            Event::Consumed | Event::Level(..) | Event::Mute(_) => true,
            Event::Dismiss => {
                let hit = self
                    .dock_at(message.hwnd)
                    .and_then(|surface| surface.dock_hit(message.lParam, self.dock_slots()));
                self.swallow_up = toggle_press(toggles, dock, message, hit);
                self.close_menu();
                own // a press on another ROQA window still reaches it
            }
            Event::Choose(item) => {
                self.run_menu(item);
                true
            }
        }
    }
    /// Takes the pending toggle-close: true when this release belongs to the press that closed the button's popup.
    pub(super) fn swallow_release(&mut self, hwnd: HWND, hit: Option<Hit>) -> bool {
        self.swallow_up
            .take()
            .is_some_and(|(dock, button)| dock == hwnd && hit == Some(Hit::Button(button)))
    }
    /// Does the chosen action on the slot's app, if the slot still holds it. A new instance leaves the
    /// foreground to the app it starts; the rest return it to the window the menu came from.
    pub(super) fn run_menu(&mut self, item: Item) {
        let Some(mut menu) = self.menu.take() else {
            return;
        };
        if item == Item::Rename {
            let name = self
                .pins
                .folder(&menu.identity)
                .map(|folder| folder.name.clone());
            if let Some(name) = name {
                if let Err(error) = menu.rename(&name) {
                    eprintln!("Folder name: {error}");
                }
                self.menu = Some(menu);
            }
            return;
        }
        if item == Item::Group {
            let choices: Vec<_> = self
                .dock_items
                .iter()
                .enumerate()
                .filter(|(_, target)| {
                    target.identity != menu.identity
                        && crate::pins::launch_target(&target.identity).is_some()
                })
                .collect();
            let labels = choices
                .iter()
                .map(|(_, target)| {
                    self.pins.folder(&target.identity).map_or_else(
                        || {
                            format!(
                                "New folder with {}",
                                crate::pins::display_name(&target.identity)
                            )
                        },
                        |folder| format!("Add to {}", folder.name),
                    )
                })
                .collect();
            if !choices.is_empty() {
                menu.targets = choices
                    .iter()
                    .map(|(_, target)| {
                        dock::pinned_as(target, &self.pins)
                            .unwrap_or_else(|| target.identity.clone())
                    })
                    .collect();
                let actions = (0..menu.targets.len()).map(Item::GroupTarget).collect();
                if let Err(error) = menu.relist(actions, labels, Vec::new()) {
                    eprintln!("Dock folder: {error}");
                }
                self.menu = Some(menu);
            } else {
                dock::restore_foreground(menu.hwnd(), menu.return_to);
                self.show_notice("Drag another app here to group it.".into(), Instant::now());
            }
            return;
        }
        let name = (item == Item::SaveName).then(|| menu.edited_name());
        if let Some(name) = name {
            self.change_pins(|pins| pins.rename(&menu.identity, &name));
            dock::restore_foreground(menu.hwnd(), menu.return_to);
            return;
        }
        if let Item::GroupTarget(index) = item {
            if let Some(target) = menu.targets.get(index).cloned() {
                let source = self
                    .dock_items
                    .iter()
                    .flat_map(|tile| std::iter::once(tile).chain(tile.members.iter()))
                    .find(|candidate| candidate.identity == menu.identity)
                    .and_then(|source| dock::pinned_as(source, &self.pins))
                    .unwrap_or_else(|| menu.identity.clone());
                self.change_pins(|pins| pins.group(&source, &target));
            }
            dock::restore_foreground(menu.hwnd(), menu.return_to);
            return;
        }
        // A new instance and Explorer take the foreground themselves.
        if !matches!(item, Item::NewInstance | Item::OpenFileLocation) {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
        }
        if menu.slot == dock_menu::APP {
            match item {
                Item::WindowsLink(link) => {
                    if !dock::launch(link.details().1) {
                        self.show_open_failed();
                    }
                }
                Item::Settings => {
                    if let Err(error) = self.open_settings() {
                        eprintln!("Settings: {error}");
                    }
                }
                // SAFETY: a scalar quit request to this thread's own message loop.
                Item::Quit => unsafe { PostQuitMessage(0) },
                Item::File(index) => {
                    if let Some(file) = self.stack.get(index)
                        && !dock::launch(&file.to_string_lossy())
                    {
                        eprintln!("Folder stack: the shell did not open {}", file.display());
                        self.show_open_failed();
                    }
                }
                Item::PinDownloads => {
                    if let Some(path) = crate::places::downloads() {
                        let mut full = false;
                        self.change_pins(|pins| {
                            add_pin(pins, Identity::Executable(path), &mut full)
                        });
                        self.say_pins_full(full);
                    }
                }
                Item::Place(index) => {
                    if let Some(place) = self.places.get(index)
                        && !dock::launch(&place.target)
                    {
                        eprintln!("Places: the shell did not open {}", place.target);
                        self.show_open_failed();
                    }
                }
                Item::Eject(index) => {
                    // A row that no longer matches the listed places does nothing; the worker checks the identity itself.
                    if let Some(volume) =
                        self.places.get(index).and_then(|place| place.eject.clone())
                    {
                        let letter = volume.letter;
                        let text = if crate::system_os::eject_volume_in_background(volume) {
                            format!("Ejecting {letter}:")
                        } else {
                            "Still ejecting another drive.".into()
                        };
                        self.show_notice(text, Instant::now());
                    }
                }
                Item::RemoveShelf => {
                    if let Identity::Executable(path) = &menu.identity {
                        self.remove_shelf_entry(path);
                    }
                }
                Item::UndoShelf => self.undo_shelf_removal(),
                _ => {}
            }
            return;
        }
        let target = self
            .dock_items
            .get(menu.slot)
            .and_then(|tile| {
                if menu.items().contains(&Item::RemoveMember) {
                    tile.members
                        .iter()
                        .find(|member| member.identity == menu.identity)
                } else {
                    Some(tile)
                }
            })
            .filter(|target| {
                dock_menu::describes(
                    &menu.identity,
                    menu.items(),
                    &menu.windows,
                    Some(target),
                    &self.pins,
                )
            })
            .cloned();
        let Some(target) = target else {
            return;
        };
        match item {
            Item::OpenFolder => {
                for member in &target.members {
                    self.open_member(member);
                }
            }
            Item::Dissolve => {
                let members: Vec<_> = target
                    .members
                    .iter()
                    .filter_map(|member| dock::pinned_as(member, &self.pins))
                    .collect();
                self.change_pins(|pins| {
                    members
                        .iter()
                        .fold(false, |changed, member| pins.ungroup(member) || changed)
                });
            }
            Item::RemoveMember => {
                let identity = dock::pinned_as(&target, &self.pins).unwrap_or(target.identity);
                self.change_pins(|pins| pins.ungroup(&identity));
            }
            Item::Close(key) => dock::close(&[key]),
            Item::CloseAll => dock::close(&menu.windows),
            Item::OnTop(key) => dock::set_on_top(key, true),
            Item::OffTop(key) => dock::set_on_top(key, false),
            Item::NewInstance => {
                if let Some(launch) = crate::pins::launch_target(&target.identity) {
                    self.run_dock_action(dock::Action::Launch(launch));
                }
            }
            Item::OpenFileLocation => {
                if !dock_menu::file_location(&target.identity)
                    .is_some_and(|exe| crate::shelf_os::reveal_local(&exe))
                {
                    self.show_open_failed();
                }
            }
            Item::Pin | Item::Unpin => {
                let identity = dock::pinned_as(&target, &self.pins).unwrap_or(target.identity);
                let mut full = false;
                self.change_pins(|pins| {
                    if item == Item::Unpin {
                        pins.remove(&identity)
                    } else {
                        add_pin(pins, identity, &mut full)
                    }
                });
                self.say_pins_full(full);
            }
            Item::Hide => self.change_pins(|pins| pins.hide(target.identity)),
            Item::Settings
            | Item::Quit
            | Item::Quick(_)
            | Item::QuickTile(..)
            | Item::WindowsLink(_)
            | Item::Place(_)
            | Item::Eject(_)
            | Item::File(_)
            | Item::PinDownloads
            | Item::RemoveShelf
            | Item::UndoShelf
            | Item::Group
            | Item::GroupTarget(_)
            | Item::Rename
            | Item::SaveName
            | Item::Member(_)
            | Item::MemberMenu(_)
            | Item::Label
            | Item::Battery
            | Item::Limit(_)
            | Item::Moon(..)
            | Item::Pair
            | Item::CalendarWeek(_)
            | Item::Widget(_)
            | Item::Level(..)
            | Item::Device(..) => {}
        }
    }
    /// Ends the open menu once it no longer describes the dock (its slot changed, or a full-screen app hid
    /// the dock), and repaints it when its highlight moved. Nothing open costs one check.
    pub(super) fn refresh_menu(&mut self) {
        let Some(menu) = &mut self.menu else {
            return;
        };
        let shown = self
            .surfaces
            .iter()
            .any(|s| s.window.0 == menu.dock() && s.visible);
        // The app menu (`dock_menu::APP`) describes no dock item, so only its surface being shown matters.
        let current = shown
            && (menu.slot == dock_menu::APP
                || dock_menu::describes(
                    &menu.identity,
                    menu.items(),
                    &menu.windows,
                    self.dock_items.get(menu.slot),
                    &self.pins,
                ));
        if !current || menu.paint().is_err() {
            self.close_menu();
        }
    }
}

/// `path` as one command-line argument: quoted, with a trailing backslash doubled so it cannot escape the closing
/// quote (a dropped drive root `E:\` would otherwise swallow the arguments after it).
fn quote_path(path: &str) -> String {
    let tail = if path.ends_with('\\') { "\\" } else { "" };
    format!("\"{path}{tail}\"")
}

/// The dock button whose own popup lists these rows, so a second click on it closes that popup.
fn toggled_by(items: &[Item]) -> Option<Button> {
    match items.first()? {
        Item::Quick(_) => Some(Button::Quick),
        Item::Battery => Some(Button::Battery),
        Item::Place(_) => Some(Button::Places),
        _ => None,
    }
}
/// A left press on the very button (on the very dock) that opened the popup: its release must not reopen it.
fn toggle_press(
    toggles: Option<Button>,
    dock: HWND,
    message: &MSG,
    hit: Option<Hit>,
) -> Option<(HWND, Button)> {
    toggles
        .filter(|&button| {
            message.message == WM_LBUTTONDOWN
                && message.hwnd == dock
                && hit == Some(Hit::Button(button))
        })
        .map(|button| (dock, button))
}
/// Adds a pin at the end. A refusal at the `MAX_PINS` cap sets `full`, so the caller says so (`say_pins_full`)
/// instead of failing silently; a duplicate or unpinnable target stays quiet.
pub(super) fn add_pin(pins: &mut Pins, identity: Identity, full: &mut bool) -> bool {
    match pins.add(identity) {
        Ok(_) => true,
        Err(error) => {
            *full |= error == crate::pins::PinError::Full;
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::{Identity, Pins, add_pin, quote_path, toggle_press, toggled_by};
    use crate::dock::{Button, Hit};
    use crate::dock_menu::{Item, Quick};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{MSG, WM_LBUTTONDOWN, WM_RBUTTONDOWN};

    #[test]
    fn a_second_press_on_the_same_button_closes_its_popup() {
        let (dock, other) = (HWND(1 as _), HWND(2 as _));
        let press = |hwnd, message| MSG {
            hwnd,
            message,
            ..Default::default()
        };
        let quick = toggled_by(&[Item::Quick(Quick::Mute)]);
        let at = |button| Some(Hit::Button(button));
        let left = press(dock, WM_LBUTTONDOWN);
        assert_eq!(
            toggle_press(quick, dock, &left, at(Button::Quick)),
            Some((dock, Button::Quick))
        );
        // Another button switches menus, another dock or a right press opens nothing new here.
        assert_eq!(toggle_press(quick, dock, &left, at(Button::Battery)), None);
        let elsewhere = press(other, WM_LBUTTONDOWN);
        assert_eq!(
            toggle_press(quick, dock, &elsewhere, at(Button::Quick)),
            None
        );
        let right = press(dock, WM_RBUTTONDOWN);
        assert_eq!(toggle_press(quick, dock, &right, at(Button::Quick)), None);
        // An app's own menu is not a button popup.
        assert_eq!(toggled_by(&[Item::Pin]), None);
        assert_eq!(toggled_by(&[Item::Battery]), Some(Button::Battery));
        assert_eq!(toggled_by(&[Item::Place(0)]), Some(Button::Places));
    }

    #[test]
    fn a_dropped_path_is_one_argument() {
        assert_eq!(quote_path(r"C:\a b\c.txt"), r#""C:\a b\c.txt""#);
        assert_eq!(quote_path(r"E:\"), r#""E:\\""#);
    }
    #[test]
    fn a_pin_refused_at_the_cap_is_reported_and_leaves_the_pins_alone() {
        let (mut pins, mut full) = (Pins::default(), false);
        let app = |id: String| Identity::Application(id);
        for id in 0..crate::pins::MAX_PINS {
            assert!(add_pin(&mut pins, app(id.to_string()), &mut full));
        }
        // A duplicate stays quiet; only the cap asks for the notice.
        assert!(!add_pin(&mut pins, app("0".into()), &mut full) && !full);
        let before = pins.clone();
        assert!(!add_pin(&mut pins, app("one-too-many".into()), &mut full) && full);
        assert_eq!(pins, before);
    }
}
