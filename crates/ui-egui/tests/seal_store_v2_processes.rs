//! Real processes sharing one SWSEAL2 store. The test binary starts copies of itself, which run
//! `child_process_entry` with a role in the environment; without that environment it does nothing.
//! Every store lives in a folder named `swiftpdf-sealv2-test-*` under the temp dir, or under
//! `SWIFTPDF_SEALV2_ROOT` when set, and never in the user's SwiftPDF folder.
#![cfg(windows)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use pdfcraft_ui_egui::seal_store_v2::{
    ApprovalRef, LOCK_FILE, Limits, NewSeal, SealEffects, SealVault, StoreState, TOMBSTONE, V2_FILE, VaultError, kind,
};

const DIR_ENV: &str = "SWIFTPDF_SEALV2_TEST_DIR";
const ROLE_ENV: &str = "SWIFTPDF_SEALV2_ROLE";
const SIGNAL_ENV: &str = "SWIFTPDF_SEALV2_SIGNAL";
const ROOT_ENV: &str = "SWIFTPDF_SEALV2_ROOT";
const EVENTS_PER_PROCESS: usize = 20;
const TRIALS: usize = 20;

fn isolated(dir: &Path) -> bool {
    let name_ok = dir.file_name().is_some_and(|name| name.to_string_lossy().starts_with("swiftpdf-sealv2-test-"));
    let user_ok = std::env::var_os("LOCALAPPDATA").is_none_or(|local| !dir.starts_with(PathBuf::from(local).join("SwiftPDF")));
    dir.is_absolute() && name_ok && user_ok
}

fn root() -> PathBuf {
    let root = std::env::var_os(ROOT_ENV).map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    assert!(root.is_absolute());
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        assert!(!root.starts_with(PathBuf::from(local).join("SwiftPDF")), "never the user's seal folder");
    }
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn new_seal() -> NewSeal {
    let image = image::RgbaImage::from_fn(4, 4, |x, y| image::Rgba([200, x as u8 * 40, y as u8 * 40, 255]));
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    NewSeal {
        name: "合成理事長印".into(),
        file: "synthetic.png".into(),
        width: 4,
        height: 4,
        png: png.into_inner(),
        scale: 100.0,
        effects: SealEffects::off(),
    }
}

fn wait_for(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(start.elapsed() < Duration::from_secs(60), "timed out waiting for {}", path.display());
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn child_process_entry() {
    let (Some(dir), Ok(role), Some(signal)) = (std::env::var_os(DIR_ENV), std::env::var(ROLE_ENV), std::env::var_os(SIGNAL_ENV)) else {
        return;
    };
    let dir = PathBuf::from(dir);
    let signal = PathBuf::from(signal);
    assert!(isolated(&dir), "refusing a folder outside the test area");
    println!("CHILD_PID={}", std::process::id());
    match role.as_str() {
        "append" => {
            let vault = SealVault::new(&dir);
            let seal = match vault.state().unwrap() {
                StoreState::Ready { snapshot, .. } => snapshot.seals[0].clone(),
                other => panic!("{other:?}"),
            };
            wait_for(&signal);
            let mut busy = 0;
            for _ in 0..EVENTS_PER_PROCESS {
                let approval = ApprovalRef::new(&seal).unwrap();
                loop {
                    match vault.record_approval(&approval) {
                        Ok(applied) => {
                            assert!(applied.changed);
                            break;
                        }
                        Err(VaultError::Busy) => busy += 1,
                        Err(error) => panic!("{error:?}"),
                    }
                }
            }
            println!("CHILD_DONE busy={busy}");
        }
        "hold-abort" | "hold-exit" => {
            let lock = pdfcraft_platform::lock::lock_with_timeout(&dir.join(LOCK_FILE), Duration::from_secs(3), Duration::from_millis(5)).unwrap();
            std::fs::write(&signal, b"held").unwrap();
            std::thread::sleep(Duration::from_millis(500));
            if role == "hold-abort" {
                std::process::abort();
            }
            // `exit` runs no destructors, so the lock is never dropped by this process.
            std::mem::forget(lock);
            std::process::exit(0);
        }
        other => panic!("unknown role {other}"),
    }
}

fn spawn(role: &str, dir: &Path, signal: &Path) -> Child {
    assert!(isolated(dir));
    Command::new(std::env::current_exe().unwrap())
        .args(["child_process_entry", "--exact", "--nocapture", "--test-threads=1"])
        .env(DIR_ENV, dir)
        .env(ROLE_ENV, role)
        .env(SIGNAL_ENV, signal)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn child_pid(output: &Output) -> u32 {
    let stdout = String::from_utf8_lossy(&output.stdout);
    after(&stdout, "CHILD_PID=").unwrap_or_else(|| panic!("no pid in {stdout}"))
}

fn busy_count(output: &Output) -> usize {
    let stdout = String::from_utf8_lossy(&output.stdout);
    after(&stdout, "CHILD_DONE busy=").unwrap_or_else(|| panic!("child did not finish: {stdout}"))
}

/// The number after `marker`. libtest may print it on the same line as the test name.
fn after<T: std::str::FromStr>(stdout: &str, marker: &str) -> Option<T> {
    let rest = stdout.split(marker).nth(1)?;
    rest.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    names
}

#[test]
fn two_processes_add_twenty_events_each_and_all_forty_are_kept_in_every_trial() {
    let root = root();
    let parent = std::process::id();
    let mut lines = Vec::new();
    for trial in 1..=TRIALS {
        let dir = root.join(format!("swiftpdf-sealv2-test-{parent}-t{trial:02}"));
        let signal = root.join(format!("swiftpdf-sealv2-test-{parent}-t{trial:02}.go"));
        assert!(isolated(&dir), "checked before any process starts");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&signal);
        std::fs::create_dir_all(&dir).unwrap();
        SealVault::new(&dir).register(new_seal()).unwrap();
        let first = spawn("append", &dir, &signal);
        let second = spawn("append", &dir, &signal);
        std::thread::sleep(Duration::from_millis(300));
        let start = Instant::now();
        std::fs::write(&signal, b"go").unwrap();
        let first = first.wait_with_output().unwrap();
        let second = second.wait_with_output().unwrap();
        let elapsed = start.elapsed();
        assert!(first.status.success(), "{}", String::from_utf8_lossy(&first.stderr));
        assert!(second.status.success(), "{}", String::from_utf8_lossy(&second.stderr));
        let pids = [child_pid(&first), child_pid(&second)];
        assert!(pids[0] != pids[1] && !pids.contains(&parent), "two separate processes: {pids:?} parent {parent}");
        let busy = [busy_count(&first), busy_count(&second)];

        let snapshot = match SealVault::new(&dir).state().unwrap() {
            StoreState::Ready { snapshot, .. } => *snapshot,
            other => panic!("{other:?}"),
        };
        let approvals = snapshot.events.iter().filter(|event| event.kind == kind::USE_APPROVED).count();
        assert_eq!(approvals, 2 * EVENTS_PER_PROCESS, "trial {trial}: every event is kept");
        assert_eq!(snapshot.events.len(), 2 * EVENTS_PER_PROCESS + 1);
        let ids: HashSet<&str> = snapshot.events.iter().map(|event| event.id.as_str()).collect();
        let ops: HashSet<&str> = snapshot.events.iter().filter(|event| !event.op_id.is_empty()).map(|event| event.op_id.as_str()).collect();
        assert_eq!(ids.len(), snapshot.events.len(), "event ids are unique");
        assert_eq!(ops.len(), 2 * EVENTS_PER_PROCESS, "op ids are unique");
        let seqs: Vec<u64> = snapshot.events.iter().map(|event| event.seq).collect();
        assert_eq!(seqs, (1..=snapshot.events.len() as u64).collect::<Vec<_>>(), "seq is consecutive");
        assert_eq!(snapshot.generation, snapshot.events.len() as u64, "one write per event");
        assert_eq!(names(&dir), ["seals.bin", LOCK_FILE, V2_FILE], "no staging file is left");
        assert_eq!(std::fs::read(dir.join("seals.bin")).unwrap(), TOMBSTONE);
        lines.push(format!(
            "trial {trial:02}: events={} approvals={approvals} generation={} unique_ids={} pids={pids:?} parent={parent} busy_retries={busy:?} elapsed_ms={}",
            snapshot.events.len(),
            snapshot.generation,
            ids.len(),
            elapsed.as_millis()
        ));
        let _ = std::fs::remove_file(&signal);
        if std::env::var_os(ROOT_ENV).is_none() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    if std::env::var_os(ROOT_ENV).is_some() {
        std::fs::write(root.join("process-trials.txt"), lines.join("\n") + "\n").unwrap();
    }
    for line in &lines {
        println!("{line}");
    }
}

#[test]
fn a_lock_held_by_a_process_that_aborts_or_exits_is_released() {
    let root = root();
    let parent = std::process::id();
    for role in ["hold-abort", "hold-exit"] {
        let dir = root.join(format!("swiftpdf-sealv2-test-{parent}-{role}"));
        let signal = root.join(format!("swiftpdf-sealv2-test-{parent}-{role}.held"));
        assert!(isolated(&dir));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&signal);
        std::fs::create_dir_all(&dir).unwrap();
        let vault = SealVault::with_limits(&dir, Limits { lock_timeout: Duration::from_millis(200), ..Limits::default() });
        let child = spawn(role, &dir, &signal);
        wait_for(&signal);
        assert_eq!(vault.register(new_seal()), Err(VaultError::Busy), "{role}: the child holds the lock");
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.success(), role == "hold-exit", "{role}: {:?}", output.status);
        assert_ne!(child_pid(&output), parent);
        let start = Instant::now();
        vault.register(new_seal()).unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "{role}: released by the system when the process ended");
        assert_eq!(std::fs::metadata(dir.join(LOCK_FILE)).unwrap().len(), 0);
        let _ = std::fs::remove_file(&signal);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
