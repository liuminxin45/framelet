use super::model::*;
use crate::{capture::Region, encode::Stats};
use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::JoinHandle,
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{SystemInformation::GetSystemTimePreciseAsFileTime, Threading::GetCurrentThreadId},
    UI::WindowsAndMessaging::*,
};
pub(super) fn utc_us() -> u64 {
    unsafe {
        let mut t = std::mem::zeroed::<FILETIME>();
        GetSystemTimePreciseAsFileTime(&mut t);
        ((t.dwHighDateTime as u64) << 32 | t.dwLowDateTime as u64)
            .saturating_sub(116444736000000000)
            / 10
    }
}
pub(super) struct Shared {
    pub(super) start: Instant,
    pub(super) next_id: AtomicU64,
    pub(super) region: Region,
    pub(super) masks: Vec<Region>,
    pub(super) events: Mutex<Vec<Event>>,
    pub(super) dropped: AtomicU64,
    pub(super) stopping: AtomicBool,
    pub(super) callback_failed: AtomicBool,
}
impl Shared {
    pub(super) fn point(&self, x: i32, y: i32) -> Option<[i32; 2]> {
        let x = x as i64 - self.region.x as i64;
        let y = y as i64 - self.region.y as i64;
        if x < 0 || y < 0 || x >= self.region.width as i64 || y >= self.region.height as i64 {
            return None;
        }
        if self.masks.iter().any(|m| {
            x >= m.x as i64
                && y >= m.y as i64
                && x < (m.x as i64 + m.width as i64)
                && y < (m.y as i64 + m.height as i64)
        }) {
            return None;
        }
        Some([x as i32, y as i32])
    }
}
thread_local! { static HOOK:RefCell<Option<Arc<Shared>>>=const{RefCell::new(None)}; }
unsafe extern "system" fn mouse(code: i32, w: usize, l: isize) -> isize {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        mouse_impl(code, w, l)
    }));
    match result {
        Ok(v) => v,
        Err(p) => {
            std::mem::forget(p);
            let _ = HOOK.try_with(|slot| {
                if let Ok(slot) = slot.try_borrow()
                    && let Some(s) = slot.as_ref()
                {
                    s.callback_failed.store(true, Ordering::Relaxed);
                    s.stopping.store(true, Ordering::Relaxed);
                }
            });
            unsafe { CallNextHookEx(std::ptr::null_mut(), code, w, l) }
        }
    }
}
unsafe fn mouse_impl(code: i32, w: usize, l: isize) -> isize {
    if code >= 0 && l != 0 {
        HOOK.with(|slot| {
            if let Ok(slot) = slot.try_borrow()
                && let Some(s) = slot.as_ref()
            {
                let m = unsafe { &*(l as *const MSLLHOOKSTRUCT) };
                let point = s.point(m.pt.x, m.pt.y);
                // Never retain outside-region or masked coordinates/events.
                if point.is_none() && w as u32 != WM_MOUSEMOVE {
                    return;
                }
                let now = s.start.elapsed().as_micros() as u64;
                if let Ok(mut events) = s.events.try_lock() {
                    if w as u32 == WM_MOUSEMOVE
                        && events.last().is_some_and(|e| {
                            e.kind == WM_MOUSEMOVE && now.saturating_sub(e.mono_us) < 16_000
                        })
                    {
                        return;
                    }
                    if events.len() < MAX_EVENTS {
                        let seq = events.len() as u64;
                        events.push(Event {
                            id: Some(s.next_id.fetch_add(1, Ordering::Relaxed)),
                            device: Device::Mouse,
                            vk: None,
                            scan: None,
                            flags: 0,
                            key_name: None,
                            sample_id: None,
                            offset_us: None,
                            seq,
                            utc_us: utc_us(),
                            mono_us: now,
                            kind: w as u32,
                            point,
                            injected: m.flags & LLMHF_INJECTED != 0,
                            wheel: ((m.mouseData >> 16) as u16) as i16,
                            media_us: None,
                            gif_frame: None,
                        });
                    } else {
                        s.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                } else {
                    s.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        });
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, w, l) }
}
unsafe extern "system" fn keyboard(code: i32, w: usize, l: isize) -> isize {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if code >= 0 && l != 0 {
            HOOK.with(|slot| {
                if let Ok(slot) = slot.try_borrow()
                    && let Some(s) = slot.as_ref()
                {
                    let k = unsafe { &*(l as *const KBDLLHOOKSTRUCT) };
                    if s.masks.iter().any(|m| {
                        m.x == 0
                            && m.y == 0
                            && m.width >= s.region.width
                            && m.height >= s.region.height
                    }) {
                        return;
                    }
                    // Scope to a foreground window intersecting the capture region. No clipboard or committed IME text is read.
                    let mut r: RECT = unsafe { std::mem::zeroed() };
                    let foreground = unsafe { GetForegroundWindow() };
                    if foreground.is_null()
                        || unsafe { GetWindowRect(foreground, &mut r) } == 0
                        || r.right <= s.region.x
                        || r.bottom <= s.region.y
                        || r.left as i64 >= s.region.x as i64 + s.region.width as i64
                        || r.top as i64 >= s.region.y as i64 + s.region.height as i64
                    {
                        return;
                    }
                    let mut focus = GUITHREADINFO {
                        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                        ..unsafe { std::mem::zeroed() }
                    };
                    if unsafe { GetGUIThreadInfo(0, &mut focus) } != 0 && !focus.hwndFocus.is_null()
                    {
                        let mut fr: RECT = unsafe { std::mem::zeroed() };
                        if unsafe { GetWindowRect(focus.hwndFocus, &mut fr) } != 0
                            && s.masks.iter().any(|m| {
                                fr.left as i64 >= s.region.x as i64 + m.x as i64
                                    && fr.top as i64 >= s.region.y as i64 + m.y as i64
                                    && fr.right as i64
                                        <= s.region.x as i64 + m.x as i64 + m.width as i64
                                    && fr.bottom as i64
                                        <= s.region.y as i64 + m.y as i64 + m.height as i64
                            })
                        {
                            return;
                        }
                    }
                    let now = s.start.elapsed().as_micros() as u64;
                    if let Ok(mut events) = s.events.try_lock() {
                        if events.len() < MAX_EVENTS {
                            let mut name = [0u16; 64];
                            let effective_scan=if k.scanCode==0{unsafe{windows_sys::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyW(k.vkCode,windows_sys::Win32::UI::Input::KeyboardAndMouse::MAPVK_VK_TO_VSC_EX)}}else{k.scanCode};
                            let scan = ((effective_scan & 0xff) << 16)
                                | if k.flags & LLKHF_EXTENDED != 0 || effective_scan&0xff00!=0 {
                                    1 << 24
                                } else {
                                    0
                                };
                            let len = unsafe {
                                windows_sys::Win32::UI::Input::KeyboardAndMouse::GetKeyNameTextW(
                                    scan as i32,
                                    name.as_mut_ptr(),
                                    64,
                                )
                            };
                            let seq = events.len() as u64;
                            events.push(Event {
                                id: Some(s.next_id.fetch_add(1, Ordering::Relaxed)),
                                device: Device::Keyboard,
                                vk: Some(k.vkCode),
                                scan: Some(k.scanCode),
                                flags: k.flags,
                                key_name: if k.vkCode == 231 {
                                    Some("Unicode packet".into())
                                } else {
                                    (len > 0)
                                        .then(|| String::from_utf16_lossy(&name[..len as usize]))
                                },
                                sample_id: None,
                                offset_us: None,
                                seq,
                                utc_us: utc_us(),
                                mono_us: now,
                                kind: w as u32,
                                point: None,
                                injected: k.flags & LLKHF_INJECTED != 0,
                                wheel: 0,
                                media_us: None,
                                gif_frame: None,
                            });
                        } else {
                            s.dropped.fetch_add(1, Ordering::Relaxed);
                        }
                    } else {
                        s.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    }));
    if let Err(payload) = result {
        std::mem::forget(payload);
        let _ = HOOK.try_with(|slot| {
            if let Ok(slot) = slot.try_borrow()
                && let Some(s) = slot.as_ref()
            {
                s.callback_failed.store(true, Ordering::Relaxed);
                s.stopping.store(true, Ordering::Relaxed);
            }
        });
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, w, l) }
}
pub struct Recorder {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    tid: u32,
    pub data: Data,
    pending: VecDeque<Sample>,
}
impl Recorder {
    pub fn new(region: Region, width: u32, height: u32, masks: &[Region]) -> anyhow::Result<Self> {
        let session_id = crate::identity::new_id()?;
        let shared = Arc::new(Shared {
            start: Instant::now(),
            next_id: AtomicU64::new(0),
            region,
            masks: masks.to_vec(),
            events: Mutex::new(Vec::new()),
            dropped: AtomicU64::new(0),
            stopping: AtomicBool::new(false),
            callback_failed: AtomicBool::new(false),
        });
        let mut data = Data {
            schema: 2,
            asset_id: Some(session_id.clone()),
            gif_sha256: None,
            session_id,
            region,
            width,
            height,
            gif_bytes: 0,
            duration_ms: 0,
            frames: 0,
            samples: Vec::new(),
            events: Vec::new(),
            dropped_events: 0,
            warnings: Vec::new(),
        };
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let s = shared.clone();
        let worker = std::thread::Builder::new()
            .name("framelet-mouse".into())
            .spawn(move || unsafe {
                let mut msg = std::mem::zeroed();
                PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), std::ptr::null_mut(), 0);
                let keyboard_hook =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), std::ptr::null_mut(), 0);
                let tid = GetCurrentThreadId();
                HOOK.with(|slot| *slot.borrow_mut() = Some(s.clone()));
                let _ = tx.send((tid, !hook.is_null(), !keyboard_hook.is_null()));
                if !hook.is_null() || !keyboard_hook.is_null() {
                    while !s.stopping.load(Ordering::Relaxed) {
                        if PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                            if msg.message == WM_QUIT {
                                break;
                            }
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        } else {
                            std::thread::sleep(std::time::Duration::from_millis(5));
                        }
                    }
                    if !hook.is_null() {
                        UnhookWindowsHookEx(hook);
                    }
                    if !keyboard_hook.is_null() {
                        UnhookWindowsHookEx(keyboard_hook);
                    }
                }
                HOOK.with(|slot| *slot.borrow_mut() = None);
            });
        let (thread, tid) = match worker {
            Ok(t) => {
                let (id, ok, keyboard_ok) = rx
                    .recv_timeout(std::time::Duration::from_secs(2))
                    .unwrap_or((0, false, false));
                if !ok && !keyboard_ok {
                    shared.stopping.store(true, Ordering::Relaxed);
                }
                if !ok {
                    data.warnings
                        .push("mouse hook unavailable; frame timestamps remain available".into());
                }
                if !keyboard_ok {
                    data.warnings
                        .push("keyboard hook unavailable; keyboard trace incomplete".into());
                }
                (Some(t), id)
            }
            Err(e) => {
                data.warnings.push(format!("mouse thread unavailable: {e}"));
                (None, 0)
            }
        };
        Ok(Self {
            shared,
            thread,
            tid,
            data,
            pending: VecDeque::new(),
        })
    }
    pub fn begin(&self) -> (u64, u64, u64) {
        (
            self.shared.start.elapsed().as_micros() as u64,
            utc_us(),
            self.shared.next_id.fetch_add(1, Ordering::Relaxed),
        )
    }
    pub fn captured(&mut self, at: (u64, u64, u64)) {
        let mut p = POINT { x: 0, y: 0 };
        let cursor = unsafe {
            if GetCursorPos(&mut p) != 0 {
                self.shared.point(p.x, p.y)
            } else {
                None
            }
        };
        self.pending.push_back(Sample {
            id: Some(at.2),
            seq: (self.data.samples.len() + self.pending.len()) as u64,
            utc_us: at.1,
            capture_us: at.0,
            capture_end_us: self.shared.start.elapsed().as_micros() as u64,
            coverage_start_us: at.0,
            hold_end_us: 0,
            media_start_us: 0,
            media_end_us: 0,
            gif_frame: 0,
            cursor,
        });
    }
    pub fn pushed(&mut self, start: u64, end: u64, frame: u64) {
        if let Some(mut s) = self.pending.pop_front() {
            s.media_start_us = start;
            s.media_end_us = end;
            s.gif_frame = frame;
            s.hold_end_us = self.shared.start.elapsed().as_micros() as u64;
            if self.data.samples.len() < MAX_SAMPLES {
                self.data.samples.push(s);
            } else if !self
                .data
                .warnings
                .iter()
                .any(|w| w == "sample limit exceeded")
            {
                self.data.warnings.push("sample limit exceeded".into());
            }
        }
    }
    pub fn set_identity(&mut self, session: &str, asset: &str) {
        self.data.session_id = session.into();
        self.data.asset_id = Some(asset.into());
    }
    pub fn pressure(&self) -> bool {
        self.data.samples.len() >= MAX_SAMPLES * 4 / 5
            || self.shared.events.lock().map_or(true, |e| {
                e.len() + self.data.events.len() >= MAX_EVENTS * 4 / 5
            })
    }
    pub fn checkpoint_data(&mut self, stats: &Stats) -> Data {
        self.collect_events();
        let mut data = self.data.clone();
        Self::map_events(&mut data, stats);
        data
    }
    pub fn metadata_reserve(&self) -> u64 {
        let queued = self.shared.events.lock().map_or(MAX_EVENTS, |e| e.len());
        (self.data.samples.len() + self.pending.len()) as u64 * 512
            + (self.data.events.len() + queued) as u64 * 1024
            + 65536
    }
    fn collect_events(&mut self) {
        match self.shared.events.try_lock() {
            Ok(mut events) => {
                let remaining = MAX_EVENTS.saturating_sub(self.data.events.len());
                if events.len() > remaining {
                    self.shared
                        .dropped
                        .fetch_add((events.len() - remaining) as u64, Ordering::Relaxed);
                    events.truncate(remaining);
                }
                self.data.events.append(&mut *events)
            }
            Err(_) => {
                let warning = "input event queue unavailable; trace incomplete";
                if !self.data.warnings.iter().any(|s| s == warning) && self.data.warnings.len() < 64
                {
                    self.data.warnings.push(warning.into());
                }
            }
        }
        self.data.dropped_events = self.shared.dropped.load(Ordering::Relaxed);
        if self.shared.callback_failed.load(Ordering::Relaxed)
            && !self
                .data
                .warnings
                .iter()
                .any(|s| s == "input callback failed; trace incomplete")
            && self.data.warnings.len() < 64
        {
            self.data
                .warnings
                .push("input callback failed; trace incomplete".into());
        }
    }
    pub fn rotate(&mut self, stats: &Stats, asset: &str) -> Data {
        let mut data = self.checkpoint_data(stats);
        data.dropped_events = self.shared.dropped.swap(0, Ordering::Relaxed);
        self.pending.clear();
        self.data.samples.clear();
        self.data.events.clear();
        self.data.warnings.clear();
        self.data.asset_id = Some(asset.into());
        self.data.dropped_events = 0;
        data
    }
    pub fn finish(mut self, stats: &Stats) -> Data {
        self.stop();
        if self.shared.callback_failed.load(Ordering::Relaxed) {
            self.data
                .warnings
                .push("mouse callback failed; monitoring stopped".into());
            crate::runtime::report(
                "timeline.callback",
                &anyhow::anyhow!("mouse callback failed"),
            );
        }
        self.collect_events();
        Self::map_events(&mut self.data, stats);
        self.data.clone()
    }
    fn map_events(data: &mut Data, stats: &Stats) {
        for i in 0..data.samples.len().saturating_sub(1) {
            let next = data.samples[i + 1].capture_us;
            data.samples[i].hold_end_us = data.samples[i].hold_end_us.min(next);
        }
        for e in &mut data.events {
            e.media_us = None;
            e.gif_frame = None;
            e.sample_id = None;
            e.offset_us = None;
            let index = data
                .samples
                .partition_point(|s| s.coverage_start_us <= e.mono_us);
            if let Some(s) = index.checked_sub(1).and_then(|i| data.samples.get(i))
                && e.mono_us < s.hold_end_us
            {
                let span = s.hold_end_us.saturating_sub(s.coverage_start_us).max(1);
                e.media_us = Some(
                    s.media_start_us
                        + ((e.mono_us - s.coverage_start_us) as u128
                            * (s.media_end_us - s.media_start_us) as u128
                            / span as u128) as u64,
                );
                e.gif_frame = Some(s.gif_frame);
                e.sample_id = s.id;
                e.offset_us =
                    Some(e.mono_us.saturating_sub(s.capture_us).min(i64::MAX as u64) as i64);
            }
        }
        data.gif_bytes = stats.bytes;
        data.duration_ms = stats.duration_ms;
        data.frames = stats.frames;
    }
    fn stop(&mut self) {
        if let Some(t) = self.thread.take() {
            unsafe {
                PostThreadMessageW(self.tid, WM_QUIT, 0, 0);
            }
            self.shared.stopping.store(true, Ordering::Relaxed);
            let began = Instant::now();
            while !t.is_finished() && began.elapsed() < std::time::Duration::from_millis(500) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            if t.is_finished() {
                if t.join().is_err() {
                    self.data.warnings.push("mouse worker panicked".into());
                }
            } else {
                self.data
                    .warnings
                    .push("mouse worker shutdown timed out; detached".into());
                crate::runtime::report(
                    "timeline.shutdown",
                    &anyhow::anyhow!("mouse worker shutdown timed out"),
                );
            }
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod rotation_tests {
    use super::*;
    #[test]
    fn input_identity_and_mapping_survive_writer_rotation() {
        // Deterministic hook-queue simulation; no injected global keystrokes or user application changes.
        let region = Region {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let session = crate::identity::new_id().unwrap();
        let shared = Arc::new(Shared {
            start: Instant::now(),
            next_id: AtomicU64::new(0),
            region,
            masks: vec![],
            events: Mutex::new(vec![]),
            dropped: AtomicU64::new(0),
            stopping: AtomicBool::new(false),
            callback_failed: AtomicBool::new(false),
        });
        let mut recorder = Recorder {
            shared: shared.clone(),
            thread: None,
            tid: 0,
            pending: VecDeque::new(),
            data: Data {
                schema: 2,
                session_id: session.clone(),
                asset_id: Some(crate::identity::new_id().unwrap()),
                gif_sha256: None,
                region,
                width: 1,
                height: 1,
                gif_bytes: 0,
                duration_ms: 0,
                frames: 0,
                samples: vec![],
                events: vec![],
                dropped_events: 0,
                warnings: vec![],
            },
        };
        let stats = Stats {
            output: "fixture.gif".into(),
            width: 1,
            height: 1,
            frames: 1,
            samples: 1,
            duration_ms: 10,
            bytes: 100,
            timeline: None,
            manifest: None,
            warnings: vec![],
            playback: None,
            capture_metrics: None,
        };
        let mut ids = vec![];
        for kind in [WM_KEYDOWN, WM_KEYUP] {
            let at = recorder.begin();
            recorder.captured(at);
            std::thread::sleep(std::time::Duration::from_millis(1));
            let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
            shared.events.lock().unwrap().push(Event {
                id: Some(id),
                device: Device::Keyboard,
                vk: Some(65),
                scan: Some(30),
                flags: 0,
                key_name: Some("A".into()),
                sample_id: None,
                offset_us: None,
                seq: 0,
                utc_us: utc_us(),
                mono_us: shared.start.elapsed().as_micros() as u64,
                kind,
                point: None,
                injected: true,
                wheel: 0,
                media_us: None,
                gif_frame: None,
            });
            std::thread::sleep(std::time::Duration::from_millis(1));
            recorder.pushed(0, 10000, 0);
            let d = recorder.rotate(&stats, &crate::identity::new_id().unwrap());
            assert_eq!(d.session_id, session);
            assert_eq!(d.events.len(), 1);
            assert_eq!(d.samples.len(), 1);
            assert_eq!(d.events[0].sample_id, d.samples[0].id);
            assert!(d.events[0].media_us.is_some_and(|v| v < 10000));
            assert_eq!(d.events[0].gif_frame, Some(0));
            assert_eq!(d.events[0].id, Some(id));
            ids.extend([d.samples[0].id.unwrap(), id]);
        }
        assert_eq!(ids, [0, 1, 2, 3]);
    }
}
