use crate::{capture::Region, encode::Stats};
use anyhow::{Result, ensure};
use std::{
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{Console::*, LibraryLoader::*},
    UI::{Controls::Dialogs::*, Controls::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};
const RECORD: u16 = 101;
const PAUSE: u16 = 102;
const FRAME: u16 = 103;
const SAVE: u16 = 104;
const FPS: u16 = 105;
const CURSOR: u16 = 106;
const FOLDER: u16 = 107;
const FOOT: i32 = 100;
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
enum Cmd {
    Pause(bool),
    Frame,
    Stop,
}
struct Session {
    tx: Sender<Cmd>,
    rx: Receiver<std::result::Result<Stats, String>>,
    count: Arc<AtomicU64>,
    paused: bool,
    stopping: bool,
}
struct App {
    session: Option<Session>,
    last: Option<PathBuf>,
    status: String,
    font: HFONT,
    controls: Vec<HWND>,
    folder: PathBuf,
}
struct FontGuard(HFONT);
impl Drop for FontGuard {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}
pub fn run() -> Result<()> {
    unsafe {
        FreeConsole();
        let instance = GetModuleHandleW(null());
        let class = wide("FrameletCaptureFrame");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(proc),
            hInstance: instance,
            hIcon: LoadIconW(instance, wide("FRAMELET").as_ptr()),
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        ensure!(RegisterClassW(&wc) != 0, "RegisterClass failed");
        let folder = PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into()))
            .join("Framelet")
            .join("Captures");
        std::fs::create_dir_all(&folder)?;
        let font = CreateFontW(
            -14,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            0,
            0,
            CLEARTYPE_QUALITY as u32,
            0,
            wide("Segoe UI").as_ptr(),
        );
        ensure!(!font.is_null(), "CreateFont failed");
        let _font_guard = FontGuard(font);
        let mut app = Box::new(App {
            session: None,
            last: None,
            status: "Ready. Move or resize the frame over your screen.".into(),
            font,
            controls: Vec::new(),
            folder,
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            class.as_ptr(),
            wide("Framelet  /  Screen to GIF").as_ptr(),
            WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX,
            180,
            160,
            680,
            480,
            null_mut(),
            null_mut(),
            instance,
            (&mut *app as *mut App).cast(),
        );
        ensure!(!hwnd.is_null(), "CreateWindow failed");
        if std::env::var_os("FRAMELET_CAPTURE_UI").is_none() {
            SetWindowDisplayAffinity(hwnd, 0x11);
        } // QA override permits screenshots of the frame itself.
        ShowWindow(hwnd, SW_SHOWNORMAL);
        UpdateWindow(hwnd);
        if SetTimer(hwnd, 1, 150, None) == 0 {
            DestroyWindow(hwnd);
            anyhow::bail!("GUI status timer unavailable");
        }
        if RegisterHotKey(hwnd, 1, MOD_NOREPEAT, 0x77) == 0 {
            crate::runtime::report("gui.hotkey", &anyhow::anyhow!("F8 registration failed"));
            app.status = "F8 unavailable; use Record button".into();
        } // F8: global start/stop
        let mut msg: MSG = std::mem::zeroed();
        loop {
            let received = GetMessageW(&mut msg, null_mut(), 0, 0);
            ensure!(received >= 0, "GetMessage failed: {}", GetLastError());
            if received == 0 {
                break;
            }

            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if let Some(s) = app.session.take() {
            let _ = s.tx.send(Cmd::Stop);
            let _ = s.rx.recv_timeout(Duration::from_secs(30));
        }
        Ok(())
    }
}
unsafe fn app(hwnd: HWND) -> &'static mut App {
    unsafe { &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App) }
}
unsafe fn child(hwnd: HWND, class: &str, text: &str, id: u16, style: u32) -> HWND {
    unsafe {
        let c = CreateWindowExW(
            0,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | style,
            0,
            0,
            10,
            10,
            hwnd,
            id as usize as HMENU,
            GetModuleHandleW(null()),
            null(),
        );
        SendMessageW(c, WM_SETFONT, app(hwnd).font as usize, 1);
        app(hwnd).controls.push(c);
        c
    }
}
unsafe fn text(hwnd: HWND, id: u16, s: &str) {
    unsafe {
        SetWindowTextW(GetDlgItem(hwnd, id as i32), wide(s).as_ptr());
    }
}
unsafe fn layout(hwnd: HWND) {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        let y = r.bottom - FOOT;
        let positions = [
            (RECORD, 12, 94),
            (PAUSE, 114, 80),
            (FRAME, 202, 80),
            (SAVE, 290, 94),
            (FOLDER, 392, 108),
        ];
        for (id, x, w) in positions {
            MoveWindow(GetDlgItem(hwnd, id as i32), x, y + 12, w, 30, 1);
        }
        MoveWindow(GetDlgItem(hwnd, FPS as i32), 12, y + 54, 80, 180, 1);
        MoveWindow(GetDlgItem(hwnd, CURSOR as i32), 108, y + 54, 120, 26, 1);
        // Remove the viewport from the window region: real click-through, no color key.
        let mut outer: RECT = std::mem::zeroed();
        GetWindowRect(hwnd, &mut outer);
        let mut origin = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut origin);
        let dx = origin.x - outer.left;
        let dy = origin.y - outer.top;
        let full = CreateRectRgn(0, 0, outer.right - outer.left, outer.bottom - outer.top);
        let hole = CreateRectRgn(dx + 5, dy + 5, dx + r.right - 5, dy + y - 5);
        CombineRgn(full, full, hole, RGN_DIFF);
        DeleteObject(hole);
        if SetWindowRgn(hwnd, full, 1) == 0 {
            DeleteObject(full);
        }
        InvalidateRect(hwnd, null(), 1);
    }
}
unsafe fn region(hwnd: HWND) -> Region {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut r);
        let mut p = POINT { x: 5, y: 5 };
        ClientToScreen(hwnd, &mut p);
        Region {
            x: p.x,
            y: p.y,
            width: (r.right - 10).max(1) as u32,
            height: (r.bottom - FOOT - 10).max(1) as u32,
        }
    }
}
fn start_worker(r: Region, fps: u32, cursor: bool, path: PathBuf, paused: bool) -> Result<Session> {
    let (tx, commands) = mpsc::channel();
    let (done, rx) = mpsc::channel();
    let count = Arc::new(AtomicU64::new(0));
    let progress = count.clone();
    std::thread::Builder::new()
        .name("framelet-gui-capture".into())
        .spawn(move || {
            let result=crate::runtime::guard("gui.recording",||{
            let args:crate::recording::RecordArgs=serde_json::from_value(serde_json::json!({"region":r,"fps":fps,"cursor":cursor,"output":path,"duration":300}))?;
            let control=crate::recording::Control::default();control.paused.store(paused,Ordering::Relaxed);control.single.store(paused,Ordering::Relaxed);
            crate::recording::run(&args,&control,||{
                progress.store(control.samples.load(Ordering::Relaxed),Ordering::Relaxed);
                let mut stop=false;for command in commands.try_iter(){match command{Cmd::Stop=>stop=true,Cmd::Pause(p)=>control.paused.store(p,Ordering::Relaxed),Cmd::Frame=>control.single.store(true,Ordering::Relaxed)}}
                stop
            })
        });
        let _ = done.send(result.map_err(|e| {
                crate::runtime::report("gui.recording", &e);
                format!("{e:#}")
            }));
        })?;
    Ok(Session {
        tx,
        rx,
        count,
        paused,
        stopping: false,
    })
}
unsafe fn start(hwnd: HWND, paused: bool) {
    unsafe {
        let a = app(hwnd);
        if a.session.is_some() {
            return;
        }
        let r = region(hwnd);
        if let Err(e) = crate::capture::validate(r) {
            a.status = e.to_string();
            return;
        }
        let idx = SendMessageW(GetDlgItem(hwnd, FPS as i32), CB_GETCURSEL, 0, 0);
        let fps = match idx {
            0 => 10,
            1 => 16,
            2 => 20,
            3 => 25,
            4 => 30,
            5 => 50,
            6 => 60,
            _ => 30,
        };
        let cursor = SendMessageW(GetDlgItem(hwnd, CURSOR as i32), BM_GETCHECK, 0, 0)
            == BST_CHECKED as isize;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = a.folder.join(format!("framelet-{stamp}.gif"));
        match start_worker(r, fps, cursor, path, paused) {
            Ok(worker) => a.session = Some(worker),
            Err(e) => {
                crate::runtime::report("gui.spawn", &e);
                a.status = format!("Cannot start: {e}");
                return;
            }
        }
        a.status = "Recording".into();
        text(hwnd, RECORD, "Stop  [F8]");
        text(hwnd, PAUSE, if paused { "Resume" } else { "Pause" });
        EnableWindow(GetDlgItem(hwnd, SAVE as i32), 0);
        EnableWindow(GetDlgItem(hwnd, FPS as i32), 0);
        EnableWindow(GetDlgItem(hwnd, CURSOR as i32), 0);
    }
}
unsafe fn command(hwnd: HWND, id: u16) {
    unsafe {
        match id {
            RECORD => {
                if let Some(s) = &mut app(hwnd).session {
                    if !s.stopping {
                        s.stopping = true;
                        let _ = s.tx.send(Cmd::Stop);
                        app(hwnd).status = "Finalizing GIF...".into();
                    }
                } else {
                    start(hwnd, false)
                }
            }
            PAUSE => {
                if let Some(s) = &mut app(hwnd).session
                    && !s.stopping
                {
                    s.paused = !s.paused;
                    let _ = s.tx.send(Cmd::Pause(s.paused));
                    text(hwnd, PAUSE, if s.paused { "Resume" } else { "Pause" });
                }
            }
            FRAME => {
                if let Some(s) = &app(hwnd).session {
                    let _ = s.tx.send(Cmd::Frame);
                } else {
                    start(hwnd, true)
                }
            }
            SAVE => {
                if let Some(path) = app(hwnd).last.clone() {
                    let mut buffer = [0u16; 32768];
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let n = wide(&name);
                    buffer[..n.len()].copy_from_slice(&n);
                    let filter = wide("Animated GIF (*.gif)\0*.gif\0");
                    let mut dialog: OPENFILENAMEW = std::mem::zeroed();
                    dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
                    dialog.hwndOwner = hwnd;
                    dialog.lpstrFilter = filter.as_ptr();
                    dialog.lpstrFile = buffer.as_mut_ptr();
                    dialog.nMaxFile = buffer.len() as u32;
                    dialog.Flags = OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
                    let ext = wide("gif");
                    dialog.lpstrDefExt = ext.as_ptr();
                    if GetSaveFileNameW(&mut dialog) != 0 {
                        let end = buffer.iter().position(|v| *v == 0).unwrap_or(buffer.len());
                        let dest = PathBuf::from(String::from_utf16_lossy(&buffer[..end]));
                        let result = (|| -> Result<Vec<String>> {
                            crate::timeline::available(&dest)?;
                            let timeline = crate::timeline::load(&path)?;
                            let _pending = if timeline.is_some() {
                                Some(crate::transaction::begin(&dest)?)
                            } else {
                                None
                            };
                            let warnings = crate::storage::copy(&path, &dest)?;
                            if let Some(data) = timeline {
                                crate::timeline::publish(&dest, &data)?;
                                crate::transaction::complete(&dest)?;
                            }
                            Ok(warnings)
                        })();
                        app(hwnd).status = match result {
                            Ok(warnings) => format!(
                                "Saved: {}{}",
                                dest.display(),
                                if warnings.is_empty() {
                                    String::new()
                                } else {
                                    format!(" / warning: {}", warnings.join("; "))
                                }
                            ),
                            Err(e) => {
                                crate::runtime::report("gui.save_as", &e);
                                format!("Save failed (check saved files): {e}")
                            }
                        };
                    }
                }
            }
            FOLDER => {
                if let Err(e) = std::process::Command::new("explorer.exe")
                    .arg(&app(hwnd).folder)
                    .spawn()
                {
                    crate::runtime::report("gui.open_folder", &e.into());
                    app(hwnd).status = "Cannot open folder; inspect diagnostics".into();
                }
            }
            _ => {}
        }
        InvalidateRect(hwnd, null(), 1);
    }
}
thread_local! {static IN_CALLBACK:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
struct CallbackGuard;
impl Drop for CallbackGuard {
    fn drop(&mut self) {
        IN_CALLBACK.with(|c| c.set(false));
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    // SendMessage/modal dialogs re-enter synchronously. Do not create another App
    // reference while the outer callback may still hold a mutable reference.
    if IN_CALLBACK.with(|c| c.replace(true)) {
        return unsafe {
            match msg {
                WM_CLOSE | WM_COMMAND | WM_HOTKEY | WM_TIMER => 0,
                WM_DESTROY => {
                    UnregisterHotKey(hwnd, 1);
                    KillTimer(hwnd, 1);
                    PostQuitMessage(0);
                    0
                }
                _ => DefWindowProcW(hwnd, msg, w, l),
            }
        };
    }
    let _guard = CallbackGuard;
    match crate::runtime::guard("gui.callback", || Ok(unsafe { proc_impl(hwnd, msg, w, l) })) {
        Ok(v) => v,
        Err(e) => {
            crate::runtime::report("gui.callback", &e);
            unsafe {
                if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0 {
                    app(hwnd).status = "Operation failed; inspect diagnostics".into();
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
    }
}
unsafe fn proc_impl(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let cs = &*(l as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        }
        if GetWindowLongPtrW(hwnd, GWLP_USERDATA) == 0 {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        match msg {
            WM_CREATE => {
                child(
                    hwnd,
                    "BUTTON",
                    "Record  [F8]",
                    RECORD,
                    BS_DEFPUSHBUTTON as u32,
                );
                child(hwnd, "BUTTON", "Pause", PAUSE, 0);
                child(hwnd, "BUTTON", "Frame", FRAME, 0);
                child(hwnd, "BUTTON", "Save as...", SAVE, 0);
                child(hwnd, "BUTTON", "Open folder", FOLDER, 0);
                let combo = child(
                    hwnd,
                    "COMBOBOX",
                    "",
                    FPS,
                    CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
                );
                for s in [
                    "10 fps", "16 fps", "20 fps", "25 fps", "30 fps", "50 fps", "60 fps",
                ] {
                    SendMessageW(combo, CB_ADDSTRING, 0, wide(s).as_ptr() as isize);
                }
                SendMessageW(combo, CB_SETCURSEL, 4, 0);
                child(
                    hwnd,
                    "BUTTON",
                    "Capture cursor",
                    CURSOR,
                    BS_AUTOCHECKBOX as u32,
                );
                EnableWindow(GetDlgItem(hwnd, SAVE as i32), 0);
                if app(hwnd).controls.iter().any(|h| h.is_null()) {
                    crate::runtime::report(
                        "gui.controls",
                        &anyhow::anyhow!("CreateWindowEx failed for a control"),
                    );
                    return -1;
                }
                layout(hwnd);
                0
            }
            WM_SIZE | WM_DPICHANGED => {
                layout(hwnd);
                0
            }
            WM_GETMINMAXINFO => {
                let m = &mut *(l as *mut MINMAXINFO);
                m.ptMinTrackSize = POINT { x: 560, y: 250 };
                0
            }
            WM_COMMAND => {
                command(hwnd, (w & 0xffff) as u16);
                0
            }
            WM_HOTKEY => {
                command(hwnd, RECORD);
                0
            }
            WM_SYSCOMMAND => {
                if (w as u32 & 0xfff0) == SC_CLOSE {
                    PostMessageW(hwnd, WM_CLOSE, 0, 0);
                    return 0;
                }
                if app(hwnd).session.is_some()
                    && ((w as u32 & 0xfff0) == SC_MOVE || (w as u32 & 0xfff0) == SC_SIZE)
                {
                    return 0;
                }
                DefWindowProcW(hwnd, msg, w, l)
            }
            WM_TIMER => {
                let completion = app(hwnd)
                    .session
                    .as_ref()
                    .and_then(|s| match s.rx.try_recv() {
                        Ok(v) => Some(v),
                        Err(mpsc::TryRecvError::Empty) => None,
                        Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                            "Recording worker disconnected; inspect diagnostics and output files"
                                .into(),
                        )),
                    });
                if let Some(result) = completion {
                    app(hwnd).session = None;
                    SetFocus(GetDlgItem(hwnd, RECORD as i32));
                    text(hwnd, RECORD, "Record  [F8]");
                    text(hwnd, PAUSE, "Pause");
                    EnableWindow(GetDlgItem(hwnd, FPS as i32), 1);
                    EnableWindow(GetDlgItem(hwnd, CURSOR as i32), 1);
                    match result {
                        Ok(s) => {
                            app(hwnd).status = format!(
                                "Saved {} frames / {:.1}s / {:.0} KB",
                                s.frames,
                                s.duration_ms as f64 / 1000.0,
                                s.bytes as f64 / 1024.0
                            );
                            if !s.warnings.is_empty() {
                                app(hwnd)
                                    .status
                                    .push_str(&format!(" / warning: {}", s.warnings.join("; ")));
                            }
                            app(hwnd).last = Some(s.output);
                            EnableWindow(GetDlgItem(hwnd, SAVE as i32), 1);
                        }
                        Err(e) => {
                            crate::runtime::report("gui.worker", &anyhow::anyhow!("{e}"));
                            app(hwnd).status = e;
                        }
                    }
                } else if let Some(s) = &app(hwnd).session
                    && !s.stopping
                {
                    app(hwnd).status = format!(
                        "{}  /  {} captures  /  F8 to finish",
                        if s.paused { "Paused" } else { "Recording" },
                        s.count.load(Ordering::Relaxed)
                    );
                }
                EnableWindow(
                    GetDlgItem(hwnd, PAUSE as i32),
                    i32::from(app(hwnd).session.as_ref().is_some_and(|s| !s.stopping)),
                );
                InvalidateRect(hwnd, null(), 0);
                0
            }
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let brush = CreateSolidBrush(0x00f6f7f9);
                FillRect(dc, &r, brush);
                DeleteObject(brush);
                let pen = CreatePen(
                    PS_SOLID,
                    2,
                    if app(hwnd).session.is_some() {
                        0x002626dc
                    } else {
                        0x00a6978b
                    },
                );
                let old = SelectObject(dc, pen);
                let oldbrush = SelectObject(dc, GetStockObject(NULL_BRUSH));
                Rectangle(dc, 2, 2, r.right - 2, r.bottom - FOOT - 2);
                SelectObject(dc, oldbrush);
                SelectObject(dc, old);
                DeleteObject(pen);
                SelectObject(dc, app(hwnd).font);
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, 0x0050473f);
                let reg = region(hwnd);
                let label = format!(
                    "{} x {} px   |   {}",
                    reg.width,
                    reg.height,
                    app(hwnd).status
                );
                let label = wide(&label);
                let mut tr = RECT {
                    left: 240,
                    top: r.bottom - FOOT + 54,
                    right: r.right - 12,
                    bottom: r.bottom - 10,
                };
                DrawTextW(
                    dc,
                    label.as_ptr(),
                    -1,
                    &mut tr,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                EndPaint(hwnd, &ps);
                0
            }
            WM_CLOSE => {
                if let Some(s) = &mut app(hwnd).session {
                    if !s.stopping {
                        s.stopping = true;
                        let _ = s.tx.send(Cmd::Stop);
                    }
                    app(hwnd).status = "Finishing recording. Close again after it is saved.".into();
                    0
                } else {
                    DestroyWindow(hwnd);
                    0
                }
            }
            WM_DESTROY => {
                UnregisterHotKey(hwnd, 1);
                KillTimer(hwnd, 1);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
