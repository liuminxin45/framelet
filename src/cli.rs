use crate::{
    capture::{self, Grabber, Region},
    encode::{self, GifWriter},
};
use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    version,
    about = "Tiny native Windows screen recorder. Run without arguments for the capture frame."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    /// Open a resizable, click-through capture frame.
    Gui,
    /// Record a GIF. Ctrl+C finalizes and saves a valid file.
    Record(RecordArgs),
    /// Capture a single PNG.
    Shot {
        #[arg(long, allow_hyphen_values = true)]
        region: Option<Region>,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long)]
        cursor: bool,
    },
    /// List physical-pixel monitor bounds as JSON.
    Monitors,
    /// List visible window handles and bounds as JSON.
    Windows,
    /// Inspect an animated GIF.
    Inspect { input: PathBuf },
    /// Extract reviewable PNG frames, a contact sheet and timestamp/clip suggestions from a GIF.
    Frames(crate::frames::ExtractArgs),
    /// Trim and change playback speed; all times are seconds.
    Edit {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, default_value_t = 0.0)]
        start: f64,
        #[arg(long)]
        end: Option<f64>,
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
    },
    /// Request a running recording to stop. Uses an OS named event.
    Stop {
        #[arg(long)]
        session: String,
    },
    /// Check platform and GDI capture availability.
    Doctor,
    /// Print the versioned agent tool catalog as JSON.
    Schema,
    /// Serve Model Context Protocol using JSON-RPC on stdin/stdout.
    Mcp,
}
#[derive(Parser, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordArgs {
    #[arg(long, allow_hyphen_values = true)]
    #[serde(default)]
    pub region: Option<Region>,
    #[arg(long, default_value_t = 10)]
    #[serde(default = "default_fps")]
    pub fps: u32,
    #[arg(long, default_value_t = 5.0)]
    #[serde(default = "default_duration")]
    pub duration: f64,
    #[arg(short, long)]
    pub output: PathBuf,
    #[arg(long)]
    #[serde(default)]
    pub cursor: bool,
    /// 1 = best color quantization, 30 = fastest.
    #[arg(long, default_value_t = 10)]
    #[serde(default = "default_quality")]
    pub quality: i32,
    /// Named stop handle, e.g. demo. Stop with `framelet stop --session demo`.
    #[arg(long)]
    #[serde(default)]
    pub session: Option<String>,
    /// Emit JSON Lines progress on stderr; final result is always JSON on stdout.
    #[arg(long)]
    #[serde(default)]
    pub events: bool,
}
fn default_fps() -> u32 {
    10
}
fn default_duration() -> f64 {
    5.0
}
fn default_quality() -> i32 {
    10
}
pub fn print(v: impl Serialize) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &v)?;
    writeln!(out)?;
    Ok(())
}
pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Gui) {
        Command::Gui => crate::gui::run()?,
        Command::Record(a) => {
            let cancel = Arc::new(AtomicBool::new(false));
            let c = cancel.clone();
            ctrlc::set_handler(move || c.store(true, Ordering::Relaxed))?;
            print(json!({"ok":true,"result":record(&a,Some(&cancel))?}))?;
        }
        Command::Shot {
            region,
            output,
            cursor,
        } => print(json!({"ok":true,"result":shot(region,&output,cursor)?}))?,
        Command::Monitors => print(json!({"ok":true,"result":capture::monitors()}))?,
        Command::Windows => print(json!({"ok":true,"result":capture::windows()}))?,
        Command::Inspect { input } => print(json!({"ok":true,"result":encode::inspect(&input)?}))?,
        Command::Frames(a) => print(json!({"ok":true,"result":crate::frames::extract(&a)?}))?,
        Command::Edit {
            input,
            output,
            start,
            end,
            speed,
        } => print(json!({"ok":true,"result":encode::edit(&input,&output,start,end,speed)?}))?,
        Command::Stop { session } => {
            stop(&session)?;
            print(json!({"ok":true,"session":session,"stop_requested":true}))?;
        }
        Command::Doctor => print(doctor())?,
        Command::Schema => print(crate::mcp::catalog())?,
        Command::Mcp => crate::mcp::serve()?,
    }
    Ok(())
}
pub fn doctor() -> Value {
    let d = capture::desktop();
    let probe = Grabber::new(
        Region {
            x: d.x,
            y: d.y,
            width: 1,
            height: 1,
        },
        false,
    )
    .and_then(|g| g.rgb());
    json!({"ok":probe.is_ok(),"version":env!("CARGO_PKG_VERSION"),"platform":"windows-x64","backend":"GDI BitBlt + DIB","desktop":d,"capture_probe":probe.is_ok(),"error":probe.err().map(|e|e.to_string()),"formats":["gif","png"],"network_required":false,"audio":false})
}
pub fn shot(region: Option<Region>, path: &std::path::Path, cursor: bool) -> Result<Value> {
    let r = region.unwrap_or_else(capture::desktop);
    let g = Grabber::new(r, cursor)?;
    encode::screenshot(path, r.width, r.height, &g.rgb()?)
}
pub fn record(a: &RecordArgs, cancel: Option<&AtomicBool>) -> Result<encode::Stats> {
    ensure!(
        (1..=50).contains(&a.fps),
        "fps must be 1..50 (GIF timing resolution is 10ms)"
    );
    ensure!(
        a.duration.is_finite() && a.duration > 0.0 && a.duration <= 3600.0,
        "duration must be >0 and <=3600 seconds"
    );
    let event = if let Some(s) = &a.session {
        Some(StopEvent::create(s)?)
    } else {
        None
    };
    let r = a.region.unwrap_or_else(capture::desktop);
    let grab = Grabber::new(r, a.cursor)?;
    let mut writer = GifWriter::new(&a.output, r.width, r.height, a.quality)?;
    let start = Instant::now();
    let duration = Duration::from_secs_f64(a.duration);
    let period = Duration::from_secs_f64(1.0 / f64::from(a.fps));
    let mut next = start;
    let mut last = None;
    let mut previous = None;
    let mut accounted_cs = 0u64;
    let mut progress = Instant::now();
    if a.events {
        eprintln!(
            "{}",
            json!({"event":"started","region":r,"fps":a.fps,"session":a.session})
        );
    }
    loop {
        let now = Instant::now();
        if last.is_some()
            && (now.duration_since(start) >= duration
                || cancel.is_some_and(|c| c.load(Ordering::Relaxed))
                || event.as_ref().is_some_and(|e| e.signaled()))
        {
            break;
        }
        let pixels = grab.rgb()?;
        if let Some(prev) = previous.take() {
            let target = (now.duration_since(start).as_secs_f64() * 100.0).round() as u64;
            let delay = target.saturating_sub(accounted_cs).clamp(1, 65535) as u16;
            writer.push(prev, delay)?;
            accounted_cs += u64::from(delay);
        }
        previous = Some(pixels);
        last = Some(now);
        if a.events && progress.elapsed() >= Duration::from_secs(1) {
            eprintln!(
                "{}",
                json!({"event":"progress","elapsed_ms":start.elapsed().as_millis(),"samples":writer.samples+1})
            );
            progress = Instant::now();
        }
        next += period;
        let now = Instant::now();
        if next > now {
            std::thread::sleep((next - now).min(duration.saturating_sub(start.elapsed())))
        } else {
            next = now;
        }
    }
    if let Some(p) = previous {
        let target = (start.elapsed().as_secs_f64().min(a.duration) * 100.0).round() as u64;
        writer.push(
            p,
            target.saturating_sub(accounted_cs).clamp(1, 65535) as u16,
        )?;
    }
    let stats = writer.finish()?;
    if a.events {
        eprintln!("{}", json!({"event":"saved","result":stats}));
    }
    Ok(stats)
}
struct StopEvent(windows_sys::Win32::Foundation::HANDLE);
fn event_name(s: &str) -> Result<Vec<u16>> {
    ensure!(
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "session must be 1..64 ASCII letters, digits, hyphens or underscores"
    );
    Ok(format!("Local\\Framelet-{s}\0").encode_utf16().collect())
}
impl StopEvent {
    fn create(s: &str) -> Result<Self> {
        use windows_sys::Win32::{Foundation::*, System::Threading::*};
        unsafe {
            let name = event_name(s)?;
            let h = CreateEventW(std::ptr::null(), 1, 0, name.as_ptr());
            ensure!(!h.is_null(), "CreateEvent failed");
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(h);
                anyhow::bail!("session is already active: {s}")
            };
            Ok(Self(h))
        }
    }
    fn signaled(&self) -> bool {
        unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(self.0, 0) == 0 }
    }
}
impl Drop for StopEvent {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}
pub fn stop(s: &str) -> Result<()> {
    use windows_sys::Win32::{Foundation::*, System::Threading::*};
    unsafe {
        let name = event_name(s)?;
        let h = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        ensure!(!h.is_null(), "session does not exist: {s}");
        let ok = SetEvent(h);
        CloseHandle(h);
        ensure!(ok != 0, "SetEvent failed");
    }
    Ok(())
}
