//! Hover tooltips for the dock's taskbar buttons (features D2): one native tooltip control owned by the dock
//! window, one tool per button. The control subclasses the dock to watch the pointer and runs its own show
//! delay only while the pointer is over a tool, so the dock gains no timer, thread or polling. App slots keep
//! their DWM preview instead.
use crate::dock::{self, Button, Buttons};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, RECT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Controls::{
                ICC_TAB_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TOOLTIPS_CLASSW,
                TTF_SUBCLASS, TTM_ADDTOOLW, TTM_NEWTOOLRECTW, TTS_ALWAYSTIP, TTS_NOPREFIX,
                TTTOOLINFOW,
            },
            WindowsAndMessaging::{
                CW_USEDEFAULT, CreateWindowExW, SendMessageW, WINDOW_STYLE, WS_EX_TOPMOST, WS_POPUP,
            },
        },
    },
    core::{HSTRING, PWSTR, Result},
};

pub struct Tips {
    tip: HWND,
    owner: HWND,
    /// The tool texts; the control keeps pointers to them.
    labels: [HSTRING; 5],
}

impl Tips {
    /// The control and its five tools, all empty until `place`. The tooltip window is owned by `owner` and goes
    /// with it.
    pub fn create(owner: HWND) -> Result<Self> {
        let labels = Button::ALL.map(|button| HSTRING::from(button.label()));
        // SAFETY: standard comctl32 classes are initialized before the control is created; the tool texts
        // outlive the control (they are kept in the returned value, which the dock owns).
        unsafe {
            InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_TAB_CLASSES,
            })
            .ok()?;
            let tip = CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                None,
                WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(owner),
                None,
                Some(HINSTANCE(GetModuleHandleW(None)?.0)),
                None,
            )?;
            let tips = Self { tip, owner, labels };
            for button in Button::ALL {
                let info = tips.tool(button, RECT::default());
                tips.send(TTM_ADDTOOLW, &info);
            }
            Ok(tips)
        }
    }

    /// Puts each shown button's tool on its cell of a dock `width` by `height` pixels; an off button's tool is
    /// empty, so it never shows.
    pub fn place(&self, buttons: Buttons, width: i32, height: i32, dpi: u32) {
        for button in Button::ALL {
            let rect = if buttons.on(button) {
                let (left, cell) = dock::span(buttons, width, dpi, dock::Hit::Button(button));
                RECT {
                    left,
                    top: 0,
                    right: left + cell,
                    bottom: height,
                }
            } else {
                RECT::default()
            };
            self.send(TTM_NEWTOOLRECTW, &self.tool(button, rect));
        }
    }

    fn tool(&self, button: Button, rect: RECT) -> TTTOOLINFOW {
        TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_SUBCLASS,
            hwnd: self.owner,
            uId: button.index(),
            rect,
            lpszText: PWSTR(self.labels[button.index()].as_ptr().cast_mut()),
            ..Default::default()
        }
    }

    fn send(&self, message: u32, info: &TTTOOLINFOW) {
        // SAFETY: `info` is a fully initialized TTTOOLINFOW that outlives the synchronous call, to the control
        // this value owns; a failed tool is only a missing tooltip.
        unsafe {
            SendMessageW(
                self.tip,
                message,
                None,
                Some(LPARAM(std::ptr::from_ref(info) as isize)),
            );
        }
    }
}
