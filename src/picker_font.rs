use std::mem::size_of;
use windows::{
    Win32::{
        Foundation::E_FAIL,
        Graphics::Gdi::{CreateFontIndirectW, DeleteObject, HFONT},
        UI::{
            HiDpi::SystemParametersInfoForDpi,
            WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS},
        },
    },
    core::{Error, Result},
};

pub struct PickerFont {
    pub handle: HFONT,
    pub scale: f64,
}
impl PickerFont {
    pub fn create(dpi: u32, factor: f64) -> Result<Self> {
        if !factor.is_finite() || !(1.0..=2.25).contains(&factor) {
            return Err(Error::new(E_FAIL, "Unsupported system text scale."));
        }
        let scale = dpi as f64 / 96.0 * factor;
        let mut metrics = NONCLIENTMETRICSW {
            cbSize: size_of::<NONCLIENTMETRICSW>() as u32,
            ..Default::default()
        };
        // SAFETY: reads current system font into initialized local storage, without changing global settings.
        unsafe {
            SystemParametersInfoForDpi(
                SPI_GETNONCLIENTMETRICS.0,
                metrics.cbSize,
                Some((&mut metrics as *mut NONCLIENTMETRICSW).cast()),
                0,
                dpi,
            )?;
        }
        metrics.lfMessageFont.lfHeight = -(15.0 * scale).round() as i32;
        // SAFETY: fully initialized font description; returned font is uniquely owned.
        let handle = unsafe { CreateFontIndirectW(&metrics.lfMessageFont) };
        if handle.0.is_null() {
            Err(Error::from_thread())
        } else {
            Ok(Self { handle, scale })
        }
    }
}
impl Drop for PickerFont {
    fn drop(&mut self) {
        // SAFETY: controls are destroyed or already use the replacement before this owned font is deleted.
        unsafe {
            let _ = DeleteObject(self.handle.into());
        }
    }
}
