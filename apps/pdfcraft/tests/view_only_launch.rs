//! SwiftPDF v0.1.0 is view-only: `SwiftPDF --control <file>` doesn't start. It exits with an
//! error before it opens a window, listens on a port, writes the control file or hands anything
//! to a running instance.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn the_control_channel_is_refused_before_anything_starts() {
    let dir = std::env::temp_dir().join(format!("swiftpdf-view-only-launch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let control = dir.join("ctl.json");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).unwrap();
    // The app's own folders (logs, the single-instance socket) go to a scratch folder, not the
    // user's.
    let mut child = Command::new(env!("CARGO_BIN_EXE_SwiftPDF"))
        .arg("--control")
        .arg(&control)
        .env("LOCALAPPDATA", &data)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CONFIG_HOME", &data)
        .env("HOME", &data)
        .env("PDFCRAFT_SYSTEM_FONTS", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("SwiftPDF --control kept running: the launch wasn't refused");
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut stderr = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut e, &mut stderr);
    }
    assert!(!status.success(), "the launch fails");
    assert!(stderr.contains("--control") && stderr.contains("view-only"), "it says why: {stderr}");
    assert!(!control.exists(), "no control file (no port, no token)");
    let made: Vec<String> = walk(&data);
    assert!(made.iter().all(|p| p.contains("logs")), "only the log was written: {made:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every file under `dir`, as paths relative to it.
fn walk(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.strip_prefix(dir).map(|r| r.to_string_lossy().into_owned()).unwrap_or_default());
            }
        }
    }
    out
}
