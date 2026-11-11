use crate::{
    dock::{foreground_key, restore_foreground, valid_target},
    native_window,
    picker_font::PickerFont,
    text_settings::TextSettings,
    window_model::{WindowKey, WindowModel},
};
use windows::{
    Win32::{
        Foundation::{E_FAIL, HINSTANCE, HWND, LPARAM, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::*,
            HiDpi::GetDpiForWindow,
            Input::KeyboardAndMouse::{EnableWindow, SetFocus},
            WindowsAndMessaging::*,
        },
    },
    core::{Error, PCWSTR, Result, w},
};

pub(crate) const ACTION: u32 = WM_APP + 40;
pub(crate) const RELAYOUT: u32 = WM_APP + 41;
const FILTER: usize = 100;
const LIST: usize = 101;
const STATUS: usize = 102;
const OPEN: usize = 1;
pub(crate) const CLOSE: usize = 2;
pub(crate) const DIALOG_STYLE: u32 = WS_POPUP.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_BORDER.0;

#[repr(C, align(4))]
struct EmptyDialog {
    header: DLGTEMPLATE,
    menu: u16,
    class: u16,
    title: u16,
}

#[derive(Clone, PartialEq, Eq)]
struct Row {
    key: WindowKey,
    title: String,
}

pub struct Picker {
    window: HWND,
    filter: HWND,
    list: HWND,
    status: HWND,
    open: HWND,
    close: HWND,
    label: HWND,
    font: Option<PickerFont>,
    text: Option<TextSettings>,
    generation: u64,
    items: Vec<Row>,
    rows: Vec<Row>,
    return_to: Option<WindowKey>,
    closed: bool,
    /// Windows refused foreground; keep saying so until the user focuses the picker.
    denied: bool,
}
impl Picker {
    pub fn create(model: &WindowModel) -> Result<Self> {
        let window = empty_dialog(440, 280)?;
        let mut picker = Self {
            window,
            filter: HWND::default(),
            list: HWND::default(),
            status: HWND::default(),
            open: HWND::default(),
            close: HWND::default(),
            label: HWND::default(),
            font: None,
            text: None,
            generation: 0,
            items: Vec::new(),
            rows: Vec::new(),
            return_to: foreground_key(),
            closed: false,
            denied: false,
        };
        picker.create_controls()?;
        picker.text = Some(TextSettings::create()?);
        picker.relayout()?;
        picker.synchronize(model)?;
        // SAFETY: explicit user request only. Windows retains its normal foreground restrictions.
        unsafe {
            SetWindowTextW(window, w!("ROQA Windows"))?;
            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            if SetForegroundWindow(window).as_bool() {
                SetFocus(Some(picker.filter))?;
            } else {
                picker.denied = true;
                picker.refilter()?;
            }
        }
        Ok(picker)
    }
    fn create_controls(&mut self) -> Result<()> {
        self.label = control(
            self.window,
            w!("Static"),
            "&Filter windows",
            99,
            0,
            WS_EX_NOPARENTNOTIFY,
        )?;
        self.filter = control(
            self.window,
            w!("Edit"),
            "",
            FILTER,
            ES_AUTOHSCROLL as u32 | WS_BORDER.0 | WS_TABSTOP.0,
            WS_EX_CLIENTEDGE,
        )?;
        self.list = control(
            self.window,
            w!("ListBox"),
            "",
            LIST,
            LBS_NOTIFY as u32
                | LBS_NOINTEGRALHEIGHT as u32
                | WS_VSCROLL.0
                | WS_HSCROLL.0
                | WS_TABSTOP.0,
            WS_EX_CLIENTEDGE,
        )?;
        self.status = control(
            self.window,
            w!("Static"),
            "",
            STATUS,
            0,
            WS_EX_NOPARENTNOTIFY,
        )?;
        self.open = control(
            self.window,
            w!("Button"),
            "&Open window",
            OPEN,
            BS_DEFPUSHBUTTON as u32 | WS_TABSTOP.0,
            WINDOW_EX_STYLE(0),
        )?;
        self.close = control(
            self.window,
            w!("Button"),
            "&Close",
            CLOSE,
            WS_TABSTOP.0,
            WINDOW_EX_STYLE(0),
        )?;
        // SAFETY: these messages target our standard edit/dialog controls; scalar parameters only.
        unsafe {
            SendMessageW(
                self.filter,
                EM_SETLIMITTEXT,
                Some(WPARAM(256)),
                Some(LPARAM(0)),
            );
            SendMessageW(
                self.window,
                DM_SETDEFID,
                Some(WPARAM(OPEN)),
                Some(LPARAM(0)),
            );
        }
        Ok(())
    }
    pub fn synchronize(&mut self, model: &WindowModel) -> Result<()> {
        // SAFETY: scalar foreground comparison with our own live dialog.
        if self.denied && unsafe { GetForegroundWindow() } == self.window {
            self.denied = false;
            self.refilter()?;
        }
        if self.text.as_ref().is_some_and(TextSettings::take_changed) {
            self.relayout()?;
        }
        if self.generation == model.generation {
            return Ok(());
        }
        self.generation = model.generation;
        self.items = model
            .windows
            .iter()
            .map(|item| Row {
                key: item.key,
                title: item.title.clone(),
            })
            .collect();
        self.refilter()?;
        if model.incomplete.is_some() {
            self.set_status("Refreshing windows. Showing the last complete list.")?;
        }
        Ok(())
    }
    fn selected(&self) -> Option<WindowKey> {
        // SAFETY: only reads scalar selection index from our list box.
        let index = unsafe { SendMessageW(self.list, LB_GETCURSEL, None, None) }.0;
        if index < 0 {
            None
        } else {
            self.rows.get(index as usize).map(|row| row.key)
        }
    }
    fn refilter(&mut self) -> Result<()> {
        let query = window_text(self.filter).to_lowercase();
        let rows: Vec<_> = self
            .items
            .iter()
            .filter(|row| row.title.to_lowercase().contains(&query))
            .cloned()
            .collect();
        if rows != self.rows {
            let selected = self.selected();
            // SAFETY: our list box copies each terminated string synchronously; no foreign app messages.
            unsafe {
                SendMessageW(self.list, LB_RESETCONTENT, None, None);
                for row in &rows {
                    let title = wide(&row.title);
                    let result = SendMessageW(
                        self.list,
                        LB_ADDSTRING,
                        None,
                        Some(LPARAM(title.as_ptr() as isize)),
                    )
                    .0;
                    if result < 0 {
                        return Err(Error::new(E_FAIL, "Cannot populate the window list."));
                    }
                }
                let index = selected
                    .and_then(|key| rows.iter().position(|row| row.key == key))
                    .unwrap_or(0);
                SendMessageW(self.list, LB_SETCURSEL, Some(WPARAM(index)), None);
            }
            self.rows = rows;
        }
        // SAFETY: our button is live; enable state follows the actual filtered result even for the initial empty list.
        unsafe {
            let _ = EnableWindow(self.open, !self.rows.is_empty());
        }
        let hint = if self.denied {
            "Windows kept your current app in front. Click this window to use the keyboard."
        } else {
            "Filter, choose a window, then press Enter."
        };
        self.set_status(&format!("{} windows. {hint}", self.rows.len()))
    }
    fn set_status(&self, text: &str) -> Result<()> {
        let text = wide(text);
        // SAFETY: standard static control synchronously copies the terminated string.
        unsafe { SetWindowTextW(self.status, PCWSTR(text.as_ptr())) }
    }
    pub fn message(&mut self, message: &MSG) -> Result<bool> {
        if message.hwnd != self.window {
            return Ok(false);
        }
        match message.message {
            ACTION => {
                let id = message.wParam.0 & 0xffff;
                let notification = (message.wParam.0 >> 16) as u16;
                match id {
                    FILTER if notification as u32 == EN_CHANGE => self.refilter()?,
                    LIST if notification as u32 == LBN_DBLCLK => self.activate()?,
                    OPEN => self.activate()?,
                    CLOSE => self.dismiss(),
                    _ => {}
                }
            }
            RELAYOUT => self.relayout()?,
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn activate(&mut self) -> Result<()> {
        let Some(key) = self.selected() else {
            return self.set_status("Choose a window first.");
        };
        if !valid_target(key) {
            return self.set_status("That window has closed or changed. Choose another window.");
        }
        // SAFETY: revalidated foreign HWND; asynchronous restore avoids blocking on its message procedure.
        unsafe {
            let window = HWND(key.handle as *mut _);
            // A failed restore falls through to the foreground attempt and its status message; never exits the host.
            if IsIconic(window).as_bool() {
                let _ = ShowWindowAsync(window, SW_RESTORE);
            }
            if SetForegroundWindow(window).as_bool() {
                self.closed = true;
            } else {
                self.set_status("Windows could not activate that window. Try its taskbar button.")?;
            }
        }
        Ok(())
    }
    fn dismiss(&mut self) {
        self.closed = true;
        // Return focus only after explicit dismissal while this panel still owns the foreground.
        restore_foreground(self.window, self.return_to);
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn keyboard(&self, message: &MSG) -> bool {
        // SAFETY: no Rust state is exposed to the reentrant dialog procedure; it posts scalar actions only.
        unsafe { IsDialogMessageW(self.window, message).as_bool() }
    }
    fn relayout(&mut self) -> Result<()> {
        let work = native_window::primary_work_area()?;
        // SAFETY: owned dialog is live; SPI fills correctly sized writable local storage.
        let dpi = unsafe { GetDpiForWindow(self.window) }.max(96);
        let factor = self
            .text
            .as_ref()
            .ok_or_else(|| Error::new(E_FAIL, "Text settings unavailable."))?
            .factor()?;
        let font = PickerFont::create(dpi, factor)?;
        let scale = font.scale;
        let children = [
            self.label,
            self.filter,
            self.list,
            self.status,
            self.open,
            self.close,
        ];
        for child in children {
            // SAFETY: child controls are live and borrow this font until replaced or destroyed.
            unsafe {
                SendMessageW(
                    child,
                    WM_SETFONT,
                    Some(WPARAM(font.handle.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
        }
        self.font = Some(font);
        self.position_controls(scale, work)
    }
    fn position_controls(&self, scale: f64, work: crate::layout::Bounds) -> Result<()> {
        let px = |value: f64| (value * scale).round() as i32;
        let width = px(640.0).min(work.width);
        let height = px(440.0).min(work.height);
        // SAFETY: positions only the owned dialog/children; resizing is explicitly nonactivating.
        unsafe {
            SetWindowPos(
                self.window,
                None,
                work.x + (work.width - width) / 2,
                work.y + (work.height - height) / 2,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )?;
            let mut client = RECT::default();
            GetClientRect(self.window, &mut client)?;
            let margin = px(16.0);
            let gap = px(8.0);
            let line = px(30.0);
            let inner = (client.right - margin * 2).max(1);
            let list_y = margin + px(20.0) + gap + line + gap;
            let buttons_y = client.bottom - margin - line;
            let status_y = buttons_y - gap - px(42.0);
            let rows = [
                (self.label, margin, margin, inner, px(20.0)),
                (self.filter, margin, margin + px(20.0) + gap, inner, line),
                (
                    self.list,
                    margin,
                    list_y,
                    inner,
                    (status_y - gap - list_y).max(line),
                ),
                (self.status, margin, status_y, inner, px(42.0)),
                (self.open, margin, buttons_y, px(156.0), line),
                (
                    self.close,
                    client.right - margin - px(100.0),
                    buttons_y,
                    px(100.0),
                    line,
                ),
            ];
            for (child, x, y, w, h) in rows {
                SetWindowPos(child, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE)?;
            }
            SendMessageW(
                self.list,
                LB_SETITEMHEIGHT,
                Some(WPARAM(0)),
                Some(LPARAM(px(24.0) as isize)),
            );
            SendMessageW(
                self.list,
                LB_SETHORIZONTALEXTENT,
                Some(WPARAM(px(1600.0) as usize)),
                None,
            );
        }
        Ok(())
    }
}
/// A visible child control of `parent` (standard classes only); the settings window shares it.
pub(crate) fn control(
    parent: HWND,
    class: PCWSTR,
    title: &str,
    id: usize,
    style: u32,
    extended: WINDOW_EX_STYLE,
) -> Result<HWND> {
    let title = wide(title);
    // SAFETY: parent lives throughout synchronous creation; class/title strings are terminated and not retained.
    unsafe {
        CreateWindowExW(
            extended,
            class,
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | style),
            0,
            0,
            1,
            1,
            Some(parent),
            Some(HMENU(id as *mut _)),
            Some(HINSTANCE(GetModuleHandleW(None)?.0)),
            None,
        )
    }
}
/// An empty standard dialog (caption and close box) whose commands `dialog_proc` posts to the host loop.
/// The settings window reuses it.
pub(crate) fn empty_dialog(cx: i16, cy: i16) -> Result<HWND> {
    // SAFETY: initialize standard themed classes before creating this process's first interactive controls.
    unsafe {
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES,
        })
        .ok()?;
    }
    let template = EmptyDialog {
        header: DLGTEMPLATE {
            style: DIALOG_STYLE,
            cx,
            cy,
            ..Default::default()
        },
        menu: 0,
        class: 0,
        title: 0,
    };
    // SAFETY: properly aligned, zero-child standard template remains live throughout synchronous creation.
    unsafe {
        CreateDialogIndirectParamW(
            Some(HINSTANCE(GetModuleHandleW(None)?.0)),
            &template.header,
            None,
            Some(dialog_proc),
            LPARAM(0),
        )
    }
}
impl Drop for Picker {
    fn drop(&mut self) {
        self.text.take();
        // SAFETY: sole owner; destroying the parent synchronously destroys all child controls before the font drops.
        unsafe {
            if let Err(error) = DestroyWindow(self.window) {
                eprintln!("Window picker cleanup: {error}");
            }
        }
    }
}
pub(crate) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn window_text(window: HWND) -> String {
    let mut text = [0u16; 257];
    // SAFETY: own bounded edit control; Windows writes at most the provided slice capacity.
    let length = unsafe { GetWindowTextW(window, &mut text) }.max(0) as usize;
    String::from_utf16_lossy(&text[..length])
}
unsafe extern "system" fn dialog_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    _: LPARAM,
) -> isize {
    let (kind, argument) = match message {
        WM_COMMAND => (ACTION, wparam),
        WM_CLOSE => (ACTION, WPARAM(CLOSE)),
        WM_DPICHANGED | WM_SETTINGCHANGE => (RELAYOUT, WPARAM(0)),
        _ => return 0,
    };
    // SAFETY: no Rust pointers/callback state. All mutation happens later in the outer loop, never reentrantly.
    if unsafe { PostMessageW(Some(hwnd), kind, argument, LPARAM(0)) }.is_err() {
        // SAFETY: orderly failure on the owning UI thread.
        unsafe {
            PostQuitMessage(71);
        }
    }
    1
}
