use crate::capture::Region;
use anyhow::{Context as _, Result, ensure};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use windows_capture::{
    capture::{CaptureControl, Context, GraphicsCaptureApiHandler},
    frame::Frame,
    graphics_capture_api::InternalCaptureControl,
    settings::*,
    window::Window,
};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
    UI::WindowsAndMessaging::{IsIconic, IsWindow, IsWindowVisible},
};

pub fn region(handle: u64) -> Result<Region> {
    let hwnd = usize::try_from(handle)? as HWND;
    unsafe {
        ensure!(
            !hwnd.is_null()
                && IsWindow(hwnd) != 0
                && IsWindowVisible(hwnd) != 0
                && IsIconic(hwnd) == 0,
            "WGC target is invalid, hidden or minimized"
        );
        let mut rect = RECT::default();
        ensure!(
            DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                (&mut rect as *mut RECT).cast(),
                size_of::<RECT>() as u32
            ) >= 0,
            "cannot query WGC window bounds"
        );
        Ok(Region {
            x: rect.left,
            y: rect.top,
            width: u32::try_from(i64::from(rect.right) - i64::from(rect.left))?,
            height: u32::try_from(i64::from(rect.bottom) - i64::from(rect.top))?,
        })
    }
}

type Latest = Arc<Mutex<Option<(u64, Vec<u8>)>>>;
struct Handler {
    latest: Latest,
    expected: Region,
    scratch: Vec<u8>,
}
impl GraphicsCaptureApiHandler for Handler {
    type Flags = (Latest, Region);
    type Error = anyhow::Error;

    fn new(context: Context<Self::Flags>) -> Result<Self> {
        let (latest, expected) = context.flags;
        Ok(Self {
            latest,
            expected,
            scratch: Vec::new(),
        })
    }

    fn on_frame_arrived(&mut self, frame: &mut Frame, _: InternalCaptureControl) -> Result<()> {
        ensure!(
            (frame.width(), frame.height()) == (self.expected.width, self.expected.height),
            "WGC surface dimensions changed or differ from the initial window bounds"
        );
        let data = frame.buffer()?;
        let pixels = data.as_nopadding_buffer(&mut self.scratch);
        let mut lock = self
            .latest
            .lock()
            .map_err(|_| anyhow::anyhow!("WGC frame buffer unavailable"))?;
        let (generation, rgb) = lock.get_or_insert_with(Default::default);
        *generation += 1;
        rgb.clear();
        rgb.reserve(pixels.len() / 4 * 3);
        for p in pixels.as_chunks::<4>().0 {
            rgb.extend_from_slice(&[p[2], p[1], p[0]]);
        }
        Ok(())
    }
}

pub struct WindowSource {
    handle: u64,
    expected: Region,
    latest: Latest,
    control: Option<CaptureControl<Handler, anyhow::Error>>,
    last_generation: std::cell::Cell<u64>,
}
impl WindowSource {
    pub fn new(handle: u64, expected: Region, cursor: bool) -> Result<Self> {
        ensure!(
            region(handle)? == expected,
            "WGC window bounds changed before capture"
        );
        let latest = Latest::default();
        let settings = Settings::new(
            Window::from_raw_hwnd(usize::try_from(handle)? as HWND),
            if cursor {
                CursorCaptureSettings::WithCursor
            } else {
                CursorCaptureSettings::WithoutCursor
            },
            DrawBorderSettings::Default,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Bgra8,
            (latest.clone(), expected),
        );
        let control = Handler::start_free_threaded(settings)
            .map_err(|e| anyhow::anyhow!("WGC window capture initialization failed: {e:?}"))?;
        Ok(Self {
            handle,
            expected,
            latest,
            control: Some(control),
            last_generation: std::cell::Cell::new(0),
        })
    }

    pub fn rgb_into(&self, pixels: &mut Vec<u8>) -> Result<bool> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            ensure!(
                region(self.handle)? == self.expected,
                "WGC window moved or resized; the original timeline bounds no longer apply"
            );
            ensure!(
                !self
                    .control
                    .as_ref()
                    .context("WGC capture unavailable")?
                    .is_finished(),
                "WGC session ended"
            );
            if let Some((generation, rgb)) = self
                .latest
                .lock()
                .map_err(|_| anyhow::anyhow!("WGC buffer unavailable"))?
                .as_ref()
            {
                pixels.clear();
                pixels.extend_from_slice(rgb);
                return Ok(self.last_generation.replace(*generation) == *generation);
            }
            ensure!(
                Instant::now() < deadline,
                "WGC did not provide the first frame within two seconds"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for WindowSource {
    fn drop(&mut self) {
        if let Some(control) = self.control.take() {
            let _ = control.stop();
        }
    }
}
