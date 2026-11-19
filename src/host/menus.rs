//! The dock's menus and popups: quick settings, Places, stacks, item and app menus, and what a row does.
//! One `impl Desktop` block moved out of `host.rs` unchanged; the methods are `pub(super)` so the host reaches them.

use super::*;

impl Desktop {
    /// The quick settings rows right now: what Windows reports on this machine, each row's text being the
    /// setting's state. Volume steps are 10 points; Core Audio loads on first use.
    pub(super) fn quick_rows(&mut self) -> (Vec<Item>, Vec<String>) {
        use dock_menu::Quick;
        self.audio.start();
        self.audio.refresh();
        let mut rows = Vec::new();
        if let Some(level) = self.audio.level() {
            rows.push((
                Quick::Mute,
                if level.muted {
                    "Sound muted".to_owned()
                } else {
                    format!("Sound on, {}%", level.percent)
                },
            ));
            rows.push((Quick::VolumeUp, "Volume up".to_owned()));
            rows.push((Quick::VolumeDown, "Volume down".to_owned()));
        }
        for (row, kind, name) in [
            (Quick::WiFi, quick::Kind::WiFi, "Wi-Fi"),
            (Quick::Bluetooth, quick::Kind::Bluetooth, "Bluetooth"),
        ] {
            if let Some(on) = quick::radio_on(kind) {
                rows.push((row, format!("{name} {}", if on { "on" } else { "off" })));
            }
        }
        if let Some(mode) = quick::mode() {
            let battery = power::Raw::read().and_then(power::Raw::summary);
            rows.push((
                Quick::Power,
                battery.map_or_else(
                    || mode.label().to_owned(),
                    |battery| format!("{}, {}", mode.label(), battery.to_lowercase()),
                ),
            ));
        }
        rows.push((
            Quick::KeepAwake,
            format!("Keep awake {}", if self.keep_awake { "on" } else { "off" }),
        ));
        let light = self.settings.theme == crate::theme::Theme::Light;
        rows.push((
            Quick::Theme,
            format!("Theme {}", if light { "light" } else { "dark" }),
        ));
        rows.push((Quick::Lock, "Lock".to_owned()));
        rows.push((Quick::Sleep, "Sleep".to_owned()));
        rows.into_iter()
            .map(|(row, text)| (Item::Quick(row), text))
            .unzip()
    }
    /// The sliders button's popup above it. It stays open while settings change; each press updates the
    /// row texts in place.
    pub(super) fn open_quick_menu(&mut self, hwnd: HWND) {
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
        let (items, titles) = self.quick_rows();
        if items.is_empty() {
            return;
        }
        let (left, width) = dock::span(buttons, bounds.width, dpi, Hit::Button(Button::Quick));
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            windows: Vec::new(),
            identity: Identity::Unavailable(0),
            items,
            dock_bounds: bounds,
            work,
            dpi,
            app: "Quick settings".into(),
            titles,
            icons: Vec::new(),
            x: bounds.x + left + width / 2,
            below: None,
            return_to,
        };
        match dock_menu::Menu::open(&self.graphics, self.palette, spec) {
            Ok(menu) if take_foreground(menu.hwnd()) => self.menu = Some(menu),
            Ok(_) => eprintln!("Quick settings: Windows kept another app in front; not opened"),
            Err(error) => eprintln!("Quick settings: {error}"),
        }
    }
    /// The Places button's popup above it: the Recycle Bin and each drive as the shell reports them now;
    /// a click opens that place in Explorer.
    pub(super) fn open_places_menu(&mut self, hwnd: HWND) {
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
        let places = crate::places::list();
        let (left, width) = dock::span(buttons, bounds.width, dpi, Hit::Button(Button::Places));
        let spec = dock_menu::Spec {
            dock: hwnd,
            slot: dock_menu::APP,
            windows: Vec::new(),
            identity: Identity::Unavailable(0),
            items: (0..places.len()).map(Item::Place).collect(),
            dock_bounds: bounds,
            work,
            dpi,
            app: "Places".into(),
            titles: places.iter().map(|place| place.text.clone()).collect(),
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
    /// A pinned folder's stack above its dock slot: its newest files as rows, read now; a click opens that file
    /// and an empty folder opens in Explorer instead.
    pub(super) fn open_stack(&mut self, hwnd: HWND, slot: usize, folder: &str) {
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
        let files = crate::places::newest_files(std::path::Path::new(folder), crate::places::STACK);
        if files.is_empty() {
            dock::launch(folder);
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
    pub(super) fn run_quick(
        &mut self,
        mut menu: dock_menu::Menu,
        row: dock_menu::Quick,
        catalog: &mut Catalog,
    ) {
        use dock_menu::Quick;
        let step = |level: Option<audio::Level>, up: bool| {
            level.map(|level| {
                let percent = i32::from(level.percent) + if up { 10 } else { -10 };
                percent.clamp(0, 100) as f32 / 100.0
            })
        };
        match row {
            Quick::Mute => self.audio.toggle_mute(),
            Quick::VolumeUp | Quick::VolumeDown => {
                if let Some(volume) = step(self.audio.level(), row == Quick::VolumeUp) {
                    self.audio.set(volume);
                }
            }
            Quick::WiFi => {
                quick::toggle_radio(quick::Kind::WiFi);
            }
            Quick::Bluetooth => {
                quick::toggle_radio(quick::Kind::Bluetooth);
            }
            Quick::Power => quick::next_mode(),
            Quick::KeepAwake => {
                self.keep_awake = !self.keep_awake;
                crate::system_os::keep_awake(self.keep_awake);
            }
            Quick::Theme => {
                use crate::theme::Theme;
                let theme = if self.settings.theme == Theme::Light {
                    self.dark_theme
                } else {
                    self.dark_theme = self.settings.theme;
                    Theme::Light
                };
                let next = Settings {
                    theme,
                    ..self.settings.clone()
                };
                // A new theme rebuilds every surface, popup included, so nothing is listed again.
                dock::restore_foreground(menu.hwnd(), menu.return_to);
                if let Err(error) = self.apply_settings(next, catalog) {
                    eprintln!("Theme: {error}");
                }
                // An open Settings window shows the theme it last knew.
                if let Some(window) = &mut self.settings_window
                    && let Err(error) = window.set_theme(self.settings.theme)
                {
                    eprintln!("Settings theme: {error}");
                }
                return;
            }
            Quick::Lock | Quick::Sleep => {
                if row == Quick::Lock {
                    crate::system_os::lock();
                } else {
                    crate::system_os::sleep();
                }
                // Nothing to show after it: the popup closes instead of listing the new state.
                dock::restore_foreground(menu.hwnd(), menu.return_to);
                return;
            }
        }
        let (items, titles) = self.quick_rows();
        if menu.relist(items, titles, Vec::new()).is_ok() {
            self.menu = Some(menu);
        } else {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
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
        if let Some(menu) = self.menu.take() {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
        }
    }
    /// Input for the open menu. Returns true when the message is consumed.
    pub(super) fn menu_message(&mut self, message: &MSG, catalog: &mut Catalog) -> bool {
        let Some(menu) = &mut self.menu else {
            return false;
        };
        let own = message.hwnd == menu.hwnd();
        match menu.message(message) {
            Event::Ignored => false,
            Event::Consumed => true,
            Event::Dismiss => {
                self.close_menu();
                own // a press on another ROQA window still reaches it
            }
            Event::Choose(item) => {
                self.run_menu(item, catalog);
                true
            }
        }
    }
    /// Does the chosen action on the slot's app, if the slot still holds it. A new instance leaves the
    /// foreground to the app it starts; the rest return it to the window the menu came from.
    pub(super) fn run_menu(&mut self, item: Item, catalog: &mut Catalog) {
        let Some(mut menu) = self.menu.take() else {
            return;
        };
        if let Item::Quick(row) = item {
            self.run_quick(menu, row, catalog);
            return;
        }
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
        // A new instance takes the foreground itself.
        if !matches!(item, Item::NewInstance) {
            dock::restore_foreground(menu.hwnd(), menu.return_to);
        }
        if menu.slot == dock_menu::APP {
            match item {
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
                    }
                }
                Item::PinDownloads => {
                    if let Some(path) = crate::places::downloads() {
                        self.change_pins(|pins| pins.add(Identity::Executable(path)).is_ok());
                    }
                }
                Item::Place(index) => {
                    if let Some(place) = self.places.get(index)
                        && !dock::launch(&place.target)
                    {
                        eprintln!("Places: the shell did not open {}", place.target);
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
            Item::Pin | Item::Unpin => {
                let identity = dock::pinned_as(&target, &self.pins).unwrap_or(target.identity);
                self.change_pins(|pins| {
                    if item == Item::Unpin {
                        pins.remove(&identity)
                    } else {
                        pins.add(identity).is_ok()
                    }
                });
            }
            Item::Hide => self.change_pins(|pins| pins.hide(target.identity)),
            Item::Settings
            | Item::Quit
            | Item::Quick(_)
            | Item::Place(_)
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
            | Item::Label => {}
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
