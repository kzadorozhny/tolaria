//! End-to-end smoke test for the `periscope` harness.
//!
//! Builds the `tolaria` binary via `cargo build -p tolaria`, then
//! execs `target/debug/tolaria --vault demo-vault-v2` directly so
//! `child.id()` is the binary's pid (not a `cargo run` wrapper).
//! Polls for the window to appear, captures a PNG via the public
//! `periscope::screenshot` API, asserts the file is plausibly
//! non-empty, then tears the child process down.
//!
//! Skipped via `TOLARIA_E2E_SKIP_SMOKE=1` on hosts that lack the
//! Screen Recording entitlement (typically headless CI).

#![cfg(target_os = "macos")]

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const SMOKE_TIMEOUT_SECS: u64 = 15;
const SMOKE_POLL_INTERVAL_MS: u64 = 500;
const MIN_USEFUL_PNG_BYTES: u64 = 10_000;

/// RAII wrapper around the spawned `tolaria` child so a panicking
/// assertion still tears the GPUI window down on stack unwind.
struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl ChildGuard {
    fn id(&self) -> u32 {
        self.0.id()
    }
}

#[test]
fn screenshot_smoke() {
    if std::env::var("TOLARIA_E2E_SKIP_SMOKE").is_ok() {
        eprintln!("periscope::screenshot_smoke skipped (TOLARIA_E2E_SKIP_SMOKE set)");
        return;
    }

    let vault_path = repo_root().join("demo-vault-v2");
    assert!(
        vault_path.is_dir(),
        "demo-vault-v2 missing at {vault_path:?} — smoke test fixture is broken"
    );

    // First, ensure the binary is built — without this `cargo run` would
    // print compilation noise that the test framework would log against
    // this test.  We run a separate `cargo build` step, then exec the
    // binary directly so `child.id()` returns the binary's pid (and not
    // the surrounding `cargo` wrapper's).
    let build_status = Command::new("cargo")
        .args(["build", "-p", "tolaria"])
        .current_dir(repo_root())
        .status()
        .expect("cargo build -p tolaria");
    assert!(build_status.success(), "cargo build -p tolaria failed");

    let bin = repo_root().join("target").join("debug").join("tolaria");
    assert!(
        bin.is_file(),
        "tolaria binary missing at {bin:?} after cargo build"
    );

    let child = Command::new(&bin)
        .arg("--vault")
        .arg(&vault_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn tolaria");
    let guard = ChildGuard(child);

    let pid = guard.id();
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("periscope-smoke.png");
    let deadline = Instant::now() + Duration::from_secs(SMOKE_TIMEOUT_SECS);

    let capture_result = loop {
        match periscope::screenshot(&periscope::WindowTarget::ByPid(pid), &out) {
            Ok(path) => break Ok(path),
            Err(err) if Instant::now() < deadline => {
                eprintln!("periscope::screenshot_smoke: waiting for window ({err:#})");
                thread::sleep(Duration::from_millis(SMOKE_POLL_INTERVAL_MS));
            }
            Err(err) => break Err(err),
        }
    };

    // `guard` drops here (or on assertion-unwind below), killing the
    // GPUI window regardless of test outcome.
    let path = capture_result.expect("screenshot eventually succeeds within the deadline");
    let bytes = std::fs::metadata(&path).expect("stat smoke PNG").len();
    assert!(
        bytes >= MIN_USEFUL_PNG_BYTES,
        "PNG too small ({bytes} bytes) — Screen Recording permission missing? \
         Set TOLARIA_E2E_SKIP_SMOKE=1 to opt out on permission-less hosts."
    );
}

/// Repo root deduced from `CARGO_MANIFEST_DIR`, which Cargo sets to
/// `crates/periscope/`.  Going up two levels lands on the workspace
/// root that owns `demo-vault-v2/`.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root above crates/periscope")
        .to_path_buf()
}
