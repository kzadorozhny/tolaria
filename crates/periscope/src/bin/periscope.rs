//! `periscope` CLI — surface for the harness library.
//!
//! Subcommands:
//!
//! - `screenshot` — one-shot capture, optionally `--raise`-ing first.
//! - `watch`      — periodic capture loop with `latest.png` symlink.
//! - `click`      — synthesize a left-click at window-local `(x, y)`.
//! - `click-id`   — click an element by its `.dump_as("name")` ID:
//!   sends SIGUSR1 to the target, waits for a fresh `tree_dump` JSON
//!   snapshot, then clicks the centre of the recorded bounds.
//! - `dump-tree`  — refresh + print the target's `tree_dump` JSON.
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
    /// Click an element by its `.dump_as("name")` ID.  Triggers a
    /// SIGUSR1 refresh of the target's `tree_dump` JSON, looks up
    /// the name, and clicks the geometric centre of the recorded
    /// bounds.
    ClickId(ClickIdArgs),
    /// Read the most recent `tree_dump` JSON for the target window
    /// (optionally triggering a fresh dump first) and pretty-print
    /// every registered element name + bounds.  Diagnostic aid for
    /// `click-id name not found`.
    DumpTree(DumpTreeArgs),
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
struct ClickIdArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// `.dump_as("name")` identifier from the target process's
    /// `tree_dump` JSON registry.  Use `dump-tree` to discover what
    /// names are available.
    #[arg(long = "id")]
    id: String,
    /// Raise the window via the Accessibility API before clicking.
    #[arg(long, default_value_t = false)]
    raise: bool,
    /// Skip the SIGUSR1 refresh and click against whatever dump
    /// already exists on disk.  Faster, but stale if the layout
    /// changed since the last dump.
    #[arg(long, default_value_t = false)]
    no_refresh: bool,
    /// Max time to wait for a fresh dump file (milliseconds).
    #[arg(long, default_value_t = 2000)]
    timeout_ms: u64,
}

#[derive(Args)]
struct DumpTreeArgs {
    #[command(flatten)]
    target: TargetArgs,
    /// Skip the SIGUSR1 refresh; print whatever the dump file
    /// currently contains.
    #[arg(long, default_value_t = false)]
    no_refresh: bool,
    /// Max time to wait for a fresh dump file (milliseconds).
    #[arg(long, default_value_t = 2000)]
    timeout_ms: u64,
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
        Cmd::ClickId(a) => click_id(a),
        Cmd::DumpTree(a) => dump_tree(a),
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

/// Send SIGUSR1 to `pid` and block until the dump file's sequence
/// counter has strictly increased past the previous value.  Returns
/// the freshly loaded `DumpFile`, or whatever exists on disk when
/// `refresh` is `false`.  Shared between `click-id` and `dump-tree`
/// to keep the IPC handshake in one place.
fn refresh_dump(
    pid: u32,
    dump_path: &std::path::Path,
    refresh: bool,
    timeout: Duration,
) -> Result<periscope::tree_dump::DumpFile> {
    use periscope::tree_dump;

    if !refresh {
        return tree_dump::load(dump_path)
            .with_context(|| format!("load tree_dump from {dump_path:?}"));
    }
    let prev_seq = tree_dump::read_sequence(dump_path);
    tree_dump::request_dump_via_signal(pid).context("send SIGUSR1 to target")?;
    let deadline = Instant::now() + timeout;
    tree_dump::wait_for_fresh_dump(dump_path, prev_seq, deadline)
        .context("wait for fresh tree_dump")
}

/// Click an element by its `.dump_as("name")` ID.  Sends SIGUSR1 to
/// the target so its `tree_dump` writes a fresh JSON snapshot, polls
/// the embedded sequence counter, then clicks the geometric centre of
/// the bounds recorded under `args.id`.
fn click_id(args: ClickIdArgs) -> Result<()> {
    let target = args.target.to_target()?;
    let pid = periscope::resolve_pid(&target)?;
    let dump_path = periscope::tree_dump::default_dump_path_for_pid(pid);
    let dump = refresh_dump(
        pid,
        &dump_path,
        !args.no_refresh,
        Duration::from_millis(args.timeout_ms),
    )?;
    let bounds = dump.entries.get(&args.id).ok_or_else(|| {
        anyhow!(
            "no element registered as {:?} in {dump_path:?} \
             (run `periscope dump-tree --pid {pid}` to list known names)",
            args.id
        )
    })?;
    let (x, y) = bounds.center();
    log::info!(
        "click-id id={:?} pid={pid} bounds=({:.1},{:.1} {:.1}x{:.1}) → click ({x:.1},{y:.1})",
        args.id,
        bounds.x,
        bounds.y,
        bounds.width,
        bounds.height,
    );

    if args.raise {
        periscope::raise(&target).context("raise before click-id")?;
        std::thread::sleep(RAISE_SETTLE);
    }
    periscope::click(&target, x, y)
}

/// Read the most recent dump for the target and print every
/// registered element.  Optionally triggers a SIGUSR1 refresh first.
fn dump_tree(args: DumpTreeArgs) -> Result<()> {
    let target = args.target.to_target()?;
    let pid = periscope::resolve_pid(&target)?;
    let dump_path = periscope::tree_dump::default_dump_path_for_pid(pid);
    let dump = refresh_dump(
        pid,
        &dump_path,
        !args.no_refresh,
        Duration::from_millis(args.timeout_ms),
    )?;
    println!(
        "# tree_dump  pid={pid}  path={dump_path:?}  sequence={}  entries={}",
        dump.sequence,
        dump.entries.len(),
    );
    for (name, b) in &dump.entries {
        println!(
            "{name:<40} x={:>7.1} y={:>7.1} w={:>6.1} h={:>6.1}",
            b.x, b.y, b.width, b.height,
        );
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
