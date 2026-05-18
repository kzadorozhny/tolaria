//! `periscope` CLI — surface for the harness library.
//!
//! Subcommands:
//!
//! - `screenshot` — one-shot capture, optionally `--raise`-ing first.
//! - `watch`      — periodic capture loop with `latest.png` symlink.
//! - `click`      — synthesize a left-click at window-local `(x, y)`.
//! - `list`       — diagnostic dump of every visible window.

use anyhow::{anyhow, Context as _, Result};
use clap::{Args, Parser, Subcommand};
use periscope::WindowTarget;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Time AppKit needs to actually bring a raised window forward before
/// capture sees it.  Empirically ~150 ms is enough on a quiet machine;
/// 250 ms is the pragmatic floor on a loaded one.
const RAISE_SETTLE: Duration = Duration::from_millis(250);

#[derive(Parser)]
#[command(name = "periscope", version, about = "Tolaria e2e screenshot harness")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Capture one PNG of the matching window.
    Screenshot(ScreenshotArgs),
    /// Capture every N seconds; maintain a `latest.png` symlink.
    Watch(WatchArgs),
    /// Synthesize a left-click at the given window-local point.
    Click(ClickArgs),
    /// Dump every visible window with title / pid / app name.
    List,
}

#[derive(Args)]
struct TargetArgs {
    /// Match by window title (e.g. `Tolaria`).
    #[arg(long, conflicts_with = "pid")]
    title: Option<String>,
    /// Match by owning process id.
    #[arg(long, conflicts_with = "title")]
    pid: Option<u32>,
}

impl TargetArgs {
    fn to_target(&self) -> Result<WindowTarget> {
        match (&self.title, self.pid) {
            (Some(t), None) => Ok(WindowTarget::ByTitle(t.clone())),
            (None, Some(p)) => Ok(WindowTarget::ByPid(p)),
            _ => Err(anyhow!("exactly one of --title or --pid is required")),
        }
    }
}

#[derive(Args)]
struct ScreenshotArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Output PNG path.
    #[arg(long)]
    out: PathBuf,
    /// Raise the window via the Accessibility API before capture.
    #[arg(long, default_value_t = false)]
    raise: bool,
}

#[derive(Args)]
struct ClickArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// X coordinate, window-local (origin at top-left, in window points).
    #[arg(long)]
    x: f64,
    /// Y coordinate, window-local (origin at top-left, in window points).
    #[arg(long)]
    y: f64,
    /// Raise the window via the Accessibility API before clicking.
    #[arg(long, default_value_t = false)]
    raise: bool,
}

#[derive(Args)]
struct WatchArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Directory for `frame-NNNN.png` files (and the `latest.png` symlink).
    #[arg(long)]
    dir: PathBuf,
    /// Seconds between captures.
    #[arg(long, default_value_t = 3)]
    interval_secs: u64,
    /// Stop after this many frames; `0` means loop until Ctrl-C.
    #[arg(long, default_value_t = 0)]
    max_frames: u32,
}

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_module("periscope", log::LevelFilter::Info)
        .parse_default_env()
        .init();

    match Cli::parse().cmd {
        Cmd::Screenshot(a) => screenshot(a),
        Cmd::Watch(a) => watch(a),
        Cmd::Click(a) => click(a),
        Cmd::List => list(),
    }
}

fn screenshot(args: ScreenshotArgs) -> Result<()> {
    let target = args.target.to_target()?;
    if args.raise {
        periscope::raise(&target).context("raise before screenshot")?;
        std::thread::sleep(RAISE_SETTLE);
    }
    let path = periscope::screenshot(&target, &args.out)?;
    log::info!("wrote {}", path.display());
    Ok(())
}

fn click(args: ClickArgs) -> Result<()> {
    let target = args.target.to_target()?;
    if args.raise {
        periscope::raise(&target).context("raise before click")?;
        std::thread::sleep(RAISE_SETTLE);
    }
    periscope::click(&target, args.x, args.y)
}

fn watch(args: WatchArgs) -> Result<()> {
    let target = args.target.to_target()?;
    std::fs::create_dir_all(&args.dir)
        .with_context(|| format!("creating watch dir {:?}", args.dir))?;
    let interval = Duration::from_secs(args.interval_secs);
    log::info!(
        "watch: dir={} interval={}s max_frames={}",
        args.dir.display(),
        args.interval_secs,
        if args.max_frames == 0 {
            "unlimited".into()
        } else {
            args.max_frames.to_string()
        },
    );

    let mut frame = 0u32;
    loop {
        frame += 1;
        let started = Instant::now();
        let path = args.dir.join(format!("frame-{frame:04}.png"));
        match periscope::screenshot(&target, &path) {
            Ok(_) => {
                update_latest_symlink(&args.dir, &path)?;
                log::info!("frame {frame} → {}", path.display());
            }
            Err(err) => log::warn!("frame {frame} failed: {err:#}"),
        }
        if args.max_frames != 0 && frame >= args.max_frames {
            return Ok(());
        }
        // Subtract the time the capture already consumed so the
        // interval is a ceiling, not a floor.
        let elapsed = started.elapsed();
        if interval > elapsed {
            std::thread::sleep(interval - elapsed);
        }
    }
}

fn list() -> Result<()> {
    for w in periscope::list_windows()? {
        println!("pid={:<8} app={:<32} title={}", w.pid, w.app_name, w.title);
    }
    Ok(())
}

/// Maintain `<dir>/latest.png` as a symlink to the most recent frame.
///
/// Atomic via tmp + rename: a reader that calls
/// `read_link("latest.png")` between frames either gets the old target
/// or the new one — never an `ENOENT` window.
fn update_latest_symlink(dir: &std::path::Path, target: &std::path::Path) -> Result<()> {
    let link = dir.join("latest.png");
    let tmp = dir.join("latest.png.tmp");
    // Use just the filename so the symlink is portable within `dir`.
    let target_name = target
        .file_name()
        .ok_or_else(|| anyhow!("target {target:?} has no filename"))?;
    // Clean up any leftover tmp from a previously crashed run.
    if tmp.is_symlink() || tmp.exists() {
        std::fs::remove_file(&tmp).with_context(|| format!("removing stale {tmp:?}"))?;
    }
    std::os::unix::fs::symlink(target_name, &tmp)
        .with_context(|| format!("symlinking {tmp:?} → {target_name:?}"))?;
    std::fs::rename(&tmp, &link).with_context(|| format!("atomic rename {tmp:?} → {link:?}"))?;
    Ok(())
}
