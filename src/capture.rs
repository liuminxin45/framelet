use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    UI::{HiDpi::*, WindowsAndMessaging::*},
};

pub fn init_dpi() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
impl std::str::FromStr for Region {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        let n: Vec<i32> = s
            .split(',')
            .map(str::parse)
            .collect::<std::result::Result<_, _>>()
            .context("region must be x,y,width,height in physical pixels")?;
        ensure!(
            n.len() == 4 && n[2] > 0 && n[3] > 0,
            "region must be x,y,width,height with positive dimensions"
        );
        Ok(Self {
            x: n[0],
            y: n[1],
            width: n[2] as u32,
            height: n[3] as u32,
        })
    }
}
pub fn desktop() -> Region {
    unsafe {
        Region {
            x: GetSystemMetrics(SM_XVIRTUALSCREEN),
            y: GetSystemMetrics(SM_YVIRTUALSCREEN),
            width: GetSystemMetrics(SM_CXVIRTUALSCREEN) as u32,
            height: GetSystemMetrics(SM_CYVIRTUALSCREEN) as u32,
        }
    }
}
pub fn validate(r: Region) -> Result<()> {
    let d = desktop();
    ensure!(
        r.width > 0
            && r.height > 0
            && r.width <= 8192
            && r.height <= 8192
            && u64::from(r.width) * u64::from(r.height) <= 16_777_216,
        "capture must be between 1 and 16 megapixels, at most 8192 per side"
    );
    ensure!(
        r.x >= d.x
            && r.y >= d.y
            && i64::from(r.x) + i64::from(r.width) <= i64::from(d.x) + i64::from(d.width)
            && i64::from(r.y) + i64::from(r.height) <= i64::from(d.y) + i64::from(d.height),
        "capture region lies outside virtual desktop {d:?}"
    );
    Ok(())
}

pub struct Grabber {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    pub region: Region,
    cursor: bool,
}
impl Grabber {
    pub fn new(region: Region, cursor: bool) -> Result<Self> {
        unsafe {
            validate(region)?;
            let screen = GetDC(null_mut());
            ensure!(!screen.is_null(), "GetDC failed: {}", GetLastError());
            let memory = CreateCompatibleDC(screen);
            if memory.is_null() {
                ReleaseDC(null_mut(), screen);
                bail!("CreateCompatibleDC failed")
            }
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            info.bmiHeader.biWidth = region.width as i32;
            info.bmiHeader.biHeight = -(region.height as i32);
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(screen, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() {
                DeleteDC(memory);
                ReleaseDC(null_mut(), screen);
                bail!("CreateDIBSection failed")
            }
            if bits.is_null() {
                DeleteObject(bitmap);
                DeleteDC(memory);
                ReleaseDC(null_mut(), screen);
                bail!("CreateDIBSection returned null pixels");
            }
            let old = SelectObject(memory, bitmap);
            if old.is_null() || old as isize == -1 {
                DeleteObject(bitmap);
                DeleteDC(memory);
                ReleaseDC(null_mut(), screen);
                bail!("SelectObject failed");
            }
            Ok(Self {
                screen,
                memory,
                bitmap,
                old,
                bits: bits.cast(),
                region,
                cursor,
            })
        }
    }
    pub fn rgb(&self) -> Result<Vec<u8>> {
        unsafe {
            let r = self.region;
            ensure!(
                BitBlt(
                    self.memory,
                    0,
                    0,
                    r.width as i32,
                    r.height as i32,
                    self.screen,
                    r.x,
                    r.y,
                    SRCCOPY | CAPTUREBLT
                ) != 0,
                "BitBlt failed: {}",
                GetLastError()
            );
            if self.cursor {
                let mut ci: CURSORINFO = std::mem::zeroed();
                ci.cbSize = size_of::<CURSORINFO>() as u32;
                if GetCursorInfo(&mut ci) != 0 && ci.flags == CURSOR_SHOWING {
                    let mut ii: ICONINFO = std::mem::zeroed();
                    if GetIconInfo(ci.hCursor, &mut ii) != 0 {
                        DrawIconEx(
                            self.memory,
                            ci.ptScreenPos.x - r.x - ii.xHotspot as i32,
                            ci.ptScreenPos.y - r.y - ii.yHotspot as i32,
                            ci.hCursor,
                            0,
                            0,
                            0,
                            null_mut(),
                            DI_NORMAL,
                        );
                        if !ii.hbmMask.is_null() {
                            DeleteObject(ii.hbmMask);
                        }
                        if !ii.hbmColor.is_null() {
                            DeleteObject(ii.hbmColor);
                        }
                    }
                }
            }
            ensure!(GdiFlush() != 0, "GdiFlush failed before pixel read");
            let src =
                std::slice::from_raw_parts(self.bits, r.width as usize * r.height as usize * 4);
            let mut rgb = Vec::with_capacity(src.len() / 4 * 3);
            for p in src.as_chunks::<4>().0 {
                rgb.extend_from_slice(&[p[2], p[1], p[0]]);
            }
            Ok(rgb)
        }
    }
}
impl Drop for Grabber {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.memory, self.old);
            DeleteObject(self.bitmap);
            DeleteDC(self.memory);
            ReleaseDC(null_mut(), self.screen);
        }
    }
}

#[derive(Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub region: Region,
    pub primary: bool,
}
pub fn monitors() -> Vec<MonitorInfo> {
    unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        unsafe {
            let mut info: MONITORINFOEXW = std::mem::zeroed();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(m, (&mut info as *mut MONITORINFOEXW).cast()) != 0 {
                let r = info.monitorInfo.rcMonitor;
                let out = &mut *(data as *mut Vec<MonitorInfo>);
                out.push(MonitorInfo {
                    name: String::from_utf16_lossy(&info.szDevice)
                        .trim_end_matches('\0')
                        .into(),
                    region: Region {
                        x: r.left,
                        y: r.top,
                        width: (r.right - r.left) as u32,
                        height: (r.bottom - r.top) as u32,
                    },
                    primary: info.monitorInfo.dwFlags & 1 != 0,
                });
            }
            1
        }
    }
    let mut out = Vec::new();
    unsafe {
        EnumDisplayMonitors(null_mut(), null(), Some(cb), &mut out as *mut _ as isize);
    }
    out
}
#[derive(Serialize)]
pub struct WindowInfo {
    pub handle: u64,
    pub title: String,
    pub region: Region,
}
pub fn windows() -> Vec<WindowInfo> {
    unsafe extern "system" fn cb(w: HWND, data: LPARAM) -> i32 {
        unsafe {
            if IsWindowVisible(w) == 0 || IsIconic(w) != 0 {
                return 1;
            }
            let mut title = [0u16; 1024];
            let n = GetWindowTextW(w, title.as_mut_ptr(), 1024);
            let mut r: RECT = std::mem::zeroed();
            if n > 0 && GetWindowRect(w, &mut r) != 0 && r.right > r.left && r.bottom > r.top {
                (&mut *(data as *mut Vec<WindowInfo>)).push(WindowInfo {
                    handle: w as u64,
                    title: String::from_utf16_lossy(&title[..n as usize]),
                    region: Region {
                        x: r.left,
                        y: r.top,
                        width: (r.right - r.left) as u32,
                        height: (r.bottom - r.top) as u32,
                    },
                });
            }
            1
        }
    }
    let mut out = Vec::new();
    unsafe {
        EnumWindows(Some(cb), &mut out as *mut _ as isize);
    }
    out
}
