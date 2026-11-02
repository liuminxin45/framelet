pub use crate::recording::RecordArgs;
use crate::{
    capture::{self, Region},
    encode,
};
use anyhow::{Result, ensure};
use clap::{Parser, Subcommand};
use serde::Serialize;
use serde_json::json;
use std::{
    io::Write,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
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
    /// Install, check or remove a user-scoped MCP/Skill integration.
    Integrate(crate::integration::Args),
    /// Open a resizable, click-through capture frame.
    Gui,
    #[command(hide = true)]
    WorkerRecord,
    #[command(hide = true)]
    WorkerAnalyze,
    /// Record a GIF. Ctrl+C finalizes and saves a valid file.
    Record(RecordArgs),
    /// Record a uniquely named group of GIFs; each part is at most 300 seconds.
    RecordSeries(crate::series::SeriesArgs),
    /// List recording groups in an output directory (bounded, paginated).
    Recordings {
        root: PathBuf,
        #[arg(long, default_value_t = 0)]
        offset: usize,
        #[arg(long, default_value_t = 16)]
        limit: usize,
    },
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
    /// Recover a finalized pending GIF/timeline pair without inventing missing data.
    Recover { input: PathBuf },
    /// Query capture timestamps and mouse interaction (bounded JSON).
    Timeline {
        input: PathBuf,
        #[arg(long)]
        at: Option<f64>,
        #[arg(long)]
        utc_us: Option<u64>,
        #[arg(long)]
        event_id: Option<u64>,
        #[command(flatten)]
        query: crate::timeline::QueryOptions,
    },
    /// Extract reviewable PNG frames, a contact sheet and timestamp/clip suggestions from a GIF.
    Frames(crate::frames::ExtractArgs),
    /// Probe multiple timestamps with one compositing pass.
    Probe(crate::review::probe::Args),
    /// Find bounded visual/input action intervals without semantic claims.
    Analyze(crate::review::analyze::Args),
    /// Return the composited frame covering a timestamp (seconds); useful for binary-search review.
    FrameAt {
        input: PathBuf,
        #[arg(long)]
        at: f64,
        #[arg(short, long)]
        output: PathBuf,
        /// Compare against a full-size PNG anchor.
        #[arg(long)]
        reference: Option<PathBuf>,
        /// Relative image rectangle x,y,width,height; defaults to the full image.
        #[arg(long, allow_hyphen_values = true)]
        compare_region: Option<Region>,
        #[arg(long, default_value_t = 24)]
        difference_threshold: u8,
    },
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
pub fn print(v: impl Serialize) -> Result<()> {
    let mut out = std::io::stdout().lock();
    serde_json::to_writer(&mut out, &v)?;
    writeln!(out)?;
    Ok(())
}
pub fn run() -> Result<()> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            if matches!(
                e.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                e.print()?;
                return Ok(());
            }
            return Err(anyhow::anyhow!("invalid CLI arguments: {e}"));
        }
    };
    match cli.command.unwrap_or(Command::Gui) {
        Command::Integrate(a) => print(json!({"ok":true,"result":crate::integration::run(&a)?}))?,
        Command::WorkerRecord => crate::worker::child_main()?,
        Command::WorkerAnalyze => crate::mcp::analysis::child_main()?,
        Command::Gui => crate::gui::run()?,
        Command::Record(a) => {
            let cancel = Arc::new(AtomicBool::new(false));
            let c = cancel.clone();
            ctrlc::set_handler(move || c.store(true, Ordering::Relaxed))?;
            print(json!({"ok":true,"result":record(&a,Some(&cancel))?}))?;
        }
        Command::RecordSeries(a) => {
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = cancel.clone();
            ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
            let event = a.session.as_deref().map(StopEvent::create).transpose()?;
            let plan = crate::series::create(&a)?;
            ensure!(
                plan.index.state == "starting" && plan.index.owner_pid == std::process::id(),
                "request_id already exists; inspect/recover its recording group"
            );
            print(
                json!({"ok":true,"result":crate::series::run(&plan,&crate::recording::Control::default(),||cancel.load(Ordering::Relaxed)||event.as_ref().is_some_and(|e|e.signaled()))?}),
            )?;
        }
        Command::Recordings {
            root,
            offset,
            limit,
        } => print(json!({"ok":true,"result":crate::series::discover(&root,offset,limit)?}))?,
        Command::Shot {
            region,
            output,
            cursor,
        } => print(json!({"ok":true,"result":crate::recording::shot(region,&output,cursor)?}))?,
        Command::Monitors => print(json!({"ok":true,"result":capture::monitors()}))?,
        Command::Windows => print(json!({"ok":true,"result":capture::windows()}))?,
        Command::Recover { input } => print(
            json!({"ok":true,"result":if crate::series::is_group(&input){crate::series::recover(&input)?}else{crate::transaction::recover(&input)?}}),
        )?,
        Command::Inspect { input } => print(
            json!({"ok":true,"result":if crate::series::is_group(&input){crate::series::inspect(&input,0,16)?}else{encode::inspect(&input)?}}),
        )?,
        Command::Timeline {
            input,
            at,
            utc_us,
            event_id,
            query,
        } => print(
            json!({"ok":true,"result":crate::timeline::query(&input,at,utc_us,event_id,&query)?}),
        )?,
        Command::Analyze(a) => print(json!({"ok":true,"result":crate::review::analyze::run(&a)?}))?,
        Command::Probe(a) => print(
            json!({"ok":true,"result":crate::review::probe::batch(&a.input,&a.times,crate::frames::ProbeOptions{reference:a.reference,region:a.compare_region,threshold:a.difference_threshold},a.output.as_deref(),a.export)?}),
        )?,
        Command::Frames(a) => print(json!({"ok":true,"result":crate::frames::extract_value(&a)?}))?,
        Command::FrameAt {
            input,
            at,
            output,
            reference,
            compare_region,
            difference_threshold,
        } => print(
            json!({"ok":true,"result":crate::frames::at(&input,at,Some(&output),crate::frames::ProbeOptions{reference,region:compare_region,threshold:difference_threshold})?}),
        )?,
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
        Command::Doctor => print(crate::recording::doctor())?,
        Command::Schema => print(crate::mcp::catalog())?,
        Command::Mcp => crate::mcp::serve()?,
    }
    Ok(())
}
pub fn record(a: &RecordArgs, cancel: Option<&AtomicBool>) -> Result<encode::Stats> {
    let event = a.session.as_deref().map(StopEvent::create).transpose()?;
    crate::recording::run(a, &crate::recording::Control::default(), || {
        cancel.is_some_and(|c| c.load(Ordering::Relaxed))
            || event.as_ref().is_some_and(|e| e.signaled())
    })
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
