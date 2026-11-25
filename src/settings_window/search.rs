use super::*;
use std::rc::Rc;

impl Results {
    pub fn height(&self) -> i32 {
        self.row * self.hits.len().max(1) as i32 + self.pad * 2
    }

    pub fn click(&self, window: HWND, y: i32) -> Option<(usize, isize)> {
        if self.window != window {
            return None;
        }
        let index = usize::try_from(y - self.pad).ok()? / self.row as usize;
        (index < self.hits.len()).then_some((index, self.revision))
    }

    pub fn paint(&self, dc: HDC, width: i32, dirty: RECT, paint: &Paint) {
        let px = |dip| (f64::from(dip) * paint.scale).round() as i32;
        let panel = RECT {
            left: 0,
            top: self.pad,
            right: width,
            bottom: self.height() - self.pad,
        };
        batch_gdi(dc, dirty, |batch| {
            batch.round(panel, px(16), swap(paint.pal.card), swap(paint.pal.card));
            if self.active < self.hits.len() {
                let top = self.pad + self.active as i32 * self.row;
                let mid = top + self.row / 2;
                batch.round(
                    RECT {
                        left: px(4),
                        top: top + px(3),
                        right: width - px(4),
                        bottom: top + self.row - px(3),
                    },
                    px(12),
                    swap(paint.pal.sel),
                    swap(paint.pal.sel),
                );
                let dot = swap(paint.pal.accent);
                batch.round(
                    RECT {
                        left: px(13),
                        top: mid - px(3),
                        right: px(19),
                        bottom: mid + px(3),
                    },
                    px(3),
                    dot,
                    dot,
                );
            }
        });
        if self.hits.is_empty() {
            // SAFETY: the paint handler owns the live DC and fonts for this call.
            unsafe {
                SelectObject(dc, paint.font.into());
                write(
                    dc,
                    "No settings found.",
                    RECT {
                        left: px(14),
                        ..panel
                    },
                    paint.pal.dim,
                    DT_LEFT | DT_NOPREFIX,
                );
            }
        }
        for (index, hit) in self.hits.iter().enumerate() {
            let top = self.pad + index as i32 * self.row;
            if top + self.row <= dirty.top || top >= dirty.bottom {
                continue;
            }
            let label = RECT {
                left: px(28),
                top: top + px(7),
                right: width - px(12),
                bottom: top + px(if self.wrap { 43 } else { 26 }),
            };
            let path = RECT {
                top: label.bottom,
                bottom: top + self.row - px(6),
                ..label
            };
            // SAFETY: the paint handler owns the live DC and fonts for this call.
            unsafe {
                SelectObject(dc, paint.bold.into());
                draw_line(dc, &hit.label, label, paint.pal.text, self.wrap);
                SelectObject(dc, paint.display_meta.into());
                draw_line(dc, &hit.path, path, paint.pal.dim, self.wrap);
            }
        }
    }
}

unsafe fn draw_line(dc: HDC, text: &str, mut rect: RECT, colour: u32, wrap: bool) {
    let flags = DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS;
    // SAFETY: the caller supplies the live paint DC; the text and rectangle are local.
    unsafe {
        if !wrap {
            write(dc, text, rect, colour, flags);
        } else {
            let mut words: Vec<_> = text.encode_utf16().collect();
            let reading = if GetLayout(dc) & LAYOUT_RTL.0 != 0 {
                DT_RTLREADING
            } else {
                DRAW_TEXT_FORMAT(0)
            };
            SetTextColor(dc, rgb(colour));
            DrawTextW(dc, &mut words, &mut rect, flags | reading | DT_WORDBREAK);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_labels_beat_label_terms_and_path_matches() {
        assert_eq!(rank("sound", &["sound"], "Sound", ["", "", ""]), Some(0));
        assert_eq!(
            rank("sound", &["sound"], "Finish sound", ["", "", ""]),
            Some(1)
        );
        assert_eq!(
            rank("sound", &["sound"], "Play", ["Prayer", "", "Sound"]),
            Some(2)
        );
    }

    #[test]
    fn terms_match_across_fields_but_not_in_captions() {
        let hits = matches("TIMER finish", &[]);
        assert!(hits.iter().any(|hit| hit.label == "Finish sound"));
        assert!(matches("recordings", &[]).is_empty());
    }

    #[test]
    fn display_results_follow_device_identity_not_tile_number() {
        let devices = [String::from("first"), String::from("second")];
        let before = matches("display 2", &devices);
        let after = matches("display 1", &devices[1..]);
        assert!(before[0].target == after[0].target);
        assert!(matches("second", &devices).is_empty());
    }
}

pub(super) const EDIT: usize = 96;
pub(super) const OPEN: u32 = WM_APP + 67;
const LIMIT: usize = 128;

thread_local! {
    static COMPOSING: Cell<HWND> = const { Cell::new(HWND(std::ptr::null_mut())) };
}

pub(super) fn composing(edit: HWND) -> bool {
    COMPOSING.with(|current| current.get() == edit)
}

unsafe extern "system" fn edit_proc(
    edit: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    match message {
        WM_IME_STARTCOMPOSITION => COMPOSING.with(|current| current.set(edit)),
        WM_IME_ENDCOMPOSITION | WM_NCDESTROY if composing(edit) => {
            COMPOSING.with(|current| current.set(HWND::default()));
        }
        _ => {}
    }
    // SAFETY: forwards the native subclass message unchanged.
    unsafe { DefSubclassProc(edit, message, wparam, lparam) }
}

#[derive(PartialEq, Eq)]
enum Target {
    Setting(usize),
    Display(String),
}

struct Hit {
    target: Target,
    label: String,
    location: [&'static str; 3],
    path: String,
}

#[derive(Default)]
pub(super) struct Search {
    pub edit: HWND,
    pub body: HWND,
    query: String,
    saved_scroll: Option<(i32, i32)>,
    hits: Rc<[Hit]>,
    active: usize,
    revision: isize,
}

pub(super) struct Results {
    pub window: HWND,
    hits: Rc<[Hit]>,
    active: usize,
    revision: isize,
    row: i32,
    pad: i32,
    wrap: bool,
}

fn rank(query: &str, terms: &[&str], label: &str, path: [&str; 3]) -> Option<u8> {
    let label = label.to_lowercase();
    if label == query {
        return Some(0);
    }
    if terms.iter().all(|term| label.contains(term)) {
        return Some(1);
    }
    let path = path.map(str::to_lowercase);
    terms
        .iter()
        .all(|term| label.contains(term) || path.iter().any(|part| part.contains(term)))
        .then_some(2)
}

fn matches(query: &str, devices: &[String]) -> Rc<[Hit]> {
    let query = query.to_lowercase();
    let terms: Vec<_> = query.split_whitespace().collect();
    let mut hits = Vec::new();
    for page in PAGES {
        let route = ROUTES[usize::from(page.route)].name;
        for (index, card) in page.cards.iter().enumerate() {
            let location = [route, page.name, card.label];
            let mut add = |target, label: &str| {
                if let Some(rank) = rank(&query, &terms, label, location) {
                    hits.push((
                        rank,
                        Hit {
                            target,
                            label: label.to_owned(),
                            location,
                            path: String::new(),
                        },
                    ));
                }
            };
            for row in card
                .rows
                .iter()
                .filter(|row| row.kind != Kind::Note && !row.title.is_empty())
            {
                add(Target::Setting(row.id), row.title);
            }
            if page.route == 0 && index == DISPLAYS_CARD {
                for (index, device) in devices.iter().enumerate() {
                    add(
                        Target::Display(device.clone()),
                        &format!("Display {}", index + 1),
                    );
                }
            }
        }
    }
    hits.sort_by_key(|(rank, _)| *rank);
    for index in 0..hits.len() {
        let hit = &hits[index].1;
        let [route, tab, group] = hit.location;
        let repeated = hits.iter().any(|(_, other)| {
            other.target != hit.target
                && other.label == hit.label
                && other.location[..2] == hit.location[..2]
        });
        hits[index].1.path = [route, tab, if repeated { group } else { "" }]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" / ");
    }
    hits.into_iter().map(|(_, hit)| hit).collect()
}

impl Search {
    pub fn is_open(&self) -> bool {
        self.saved_scroll.is_some()
    }

    pub fn results(&self, scale: f64, wrap: bool) -> Option<Results> {
        let px = |dip| (f64::from(dip) * scale).round() as i32;
        self.is_open().then(|| Results {
            window: self.body,
            hits: Rc::clone(&self.hits),
            active: self.active,
            revision: self.revision,
            row: px(if wrap { 76 } else { 52 }),
            pad: px(4),
            wrap,
        })
    }
}

impl SettingsWindow {
    pub(super) fn build_search(&mut self) -> Result<()> {
        self.search.edit = control(
            self.window,
            w!("Edit"),
            "",
            EDIT,
            ES_AUTOHSCROLL as u32 | WS_GROUP.0,
            WINDOW_EX_STYLE(0),
        )?;
        // SAFETY: our new edit owns the subclass and copies the static cue synchronously.
        unsafe {
            SetWindowSubclass(self.search.edit, Some(edit_proc), 1, 0).ok()?;
            SendMessageW(self.search.edit, EM_SETLIMITTEXT, Some(WPARAM(LIMIT)), None);
            SendMessageW(
                self.search.edit,
                EM_SETCUEBANNER,
                Some(WPARAM(1)),
                Some(LPARAM(w!("Search settings").as_ptr() as isize)),
            );
        }
        Ok(())
    }

    pub(super) fn search_changed(&mut self) -> Result<()> {
        let typed = text(self.search.edit, LIMIT + 1);
        let query = typed.trim();
        if query == self.search.query {
            return Ok(());
        }
        if query.is_empty() {
            self.leave_search(false)?;
        } else {
            scroll::release();
            if !self.search.is_open() {
                self.search.saved_scroll = Some((self.scroll, self.wheel_remainder));
                self.select_route();
            }
            self.search.query = query.to_owned();
            self.wheel_remainder = 0;
            self.refresh_search(false);
        }
        self.layout()
    }

    pub(super) fn refresh_search(&mut self, preserve: bool) {
        if !self.search.is_open() {
            return;
        }
        let target = self
            .search
            .hits
            .get(self.search.active)
            .filter(|_| preserve);
        let hits = matches(&self.search.query, &self.devices);
        let next = target.and_then(|old| hits.iter().position(|hit| hit.target == old.target));
        if next.is_none() {
            self.scroll = 0;
        }
        self.search.active = next.unwrap_or(0);
        self.search.hits = hits;
        self.search.revision = self.search.revision.wrapping_add(1);
    }

    pub(super) fn leave_search(&mut self, clear_edit: bool) -> Result<()> {
        if let Some((scroll, wheel)) = self.search.saved_scroll.take() {
            scroll::release();
            self.scroll = scroll;
            self.wheel_remainder = wheel;
            self.search.query.clear();
            self.search.hits = Rc::default();
            self.select_route();
        }
        if clear_edit && !text(self.search.edit, LIMIT + 1).is_empty() {
            set_text(self.search.edit, "")?;
        }
        Ok(())
    }

    pub(super) fn move_search(&mut self, down: bool) -> Result<()> {
        self.search_changed()?;
        let active = if down {
            (self.search.active + 1).min(self.search.hits.len().saturating_sub(1))
        } else {
            self.search.active.saturating_sub(1)
        };
        if active != self.search.active {
            self.search.active = active;
            PAINT.with(|cell| {
                if let Some(results) = cell
                    .borrow_mut()
                    .as_mut()
                    .and_then(|paint| paint.results.as_mut())
                {
                    results.active = active;
                }
            });
            self.reveal_search()?;
            // SAFETY: our own live result window.
            unsafe {
                let _ = InvalidateRect(Some(self.search.body), None, true);
            }
        }
        Ok(())
    }

    pub(super) fn reveal_search(&mut self) -> Result<()> {
        let row = PAINT.with(|cell| {
            let paint = cell.borrow();
            let results = paint.as_ref()?.results.as_ref()?;
            (!results.hits.is_empty()).then_some((results.pad, results.row))
        });
        if let Some((pad, row)) = row {
            let top = pad + self.search.active as i32 * row;
            let view = self.frame.viewport.bottom - self.frame.viewport.top;
            let offset = keyboard::into_view(top - self.scroll, top + row - self.scroll, view, pad);
            self.jump(self.scroll + offset)?;
        }
        Ok(())
    }

    pub(super) fn open_search_hit(&mut self, click: Option<(usize, isize)>) -> Result<()> {
        self.search_changed()?;
        let index = match click {
            Some((index, revision)) if revision == self.search.revision => index,
            Some(_) => return Ok(()),
            None => self.search.active,
        };
        let Some(hit) = self.search.hits.get(index) else {
            return Ok(());
        };
        let id = match &hit.target {
            Target::Setting(id) => Some(*id),
            Target::Display(device) => self
                .devices
                .iter()
                .position(|present| present == device)
                .map(|index| MONITOR + index),
        };
        let Some((page, focus)) = id
            .and_then(|id| self.items.iter().find(|item| item.id == id))
            .map(|item| (usize::from(item.page), item.h))
        else {
            self.refresh_search(true);
            return self.layout();
        };
        self.show_page(page)?;
        // SAFETY: these controls belong to the live page and focus does not invoke them.
        unsafe {
            let focus_to =
                if windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(focus).as_bool() {
                    focus
                } else if !self.tabs[page].0.is_null() {
                    self.tabs[page]
                } else if self.frame.compact {
                    self.nav
                } else {
                    self.side
                };
            let _ = SetFocus(Some(focus_to));
        }
        self.reveal_control(focus)
    }
}
