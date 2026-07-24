//! Real-DOM regression for a primitive lib alias returned from a lazily
//! materialized interface member.
//!
//! Each run uses a separate CLI process because the on-demand forcing switch
//! is process-cached. Default forcing must agree byte-for-byte with the legacy
//! eager kill switch in both root orders, including a negative assignment that
//! proves the alias remained `number` rather than degrading to `any`.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_lib_primitive_alias_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

struct RunResult {
    status: ExitStatus,
    output: String,
}

fn run_tsz(tsz: &Path, dir: &TempDir, roots: &[&str], eager: bool) -> RunResult {
    let mut command = Command::new(tsz);
    command
        .args([
            "--ignoreConfig",
            "--noEmit",
            "--strict",
            "--pretty",
            "false",
            "--target",
            "es2022",
            "--lib",
            "es2022,dom,dom.iterable",
        ])
        .args(roots)
        .current_dir(&dir.path);
    if eager {
        command.env("TSZ_DISABLE_ON_DEMAND_FORCING", "1");
    } else {
        command.env_remove("TSZ_DISABLE_ON_DEMAND_FORCING");
    }
    let result = command.output().expect("run tsz");
    let mut output = String::from_utf8_lossy(&result.stdout).into_owned();
    output.push_str(&String::from_utf8_lossy(&result.stderr));
    RunResult {
        status: result.status,
        output,
    }
}

fn assert_default_matches_eager(tsz: &Path, dir: &TempDir, roots: &[&str]) {
    let default = run_tsz(tsz, dir, roots, false);
    let eager = run_tsz(tsz, dir, roots, true);

    assert!(!default.status.success());
    assert!(!eager.status.success());
    assert_eq!(
        default.output, eager.output,
        "default and eager forcing must agree for root order {roots:?}",
    );
    let diagnostics: Vec<_> = default
        .output
        .lines()
        .filter(|line| line.contains("error TS"))
        .collect();
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one negative-control diagnostic for root order {roots:?}; got:\n{}",
        default.output,
    );
    assert!(
        diagnostics[0].contains("error TS2322:")
            && diagnostics[0].contains("Type 'number' is not assignable to type 'string'."),
        "DOM primitive alias must materialize as number for root order {roots:?}; got:\n{}",
        default.output,
    );
}

#[test]
fn dom_primitive_alias_return_matches_eager_forcing_in_both_root_orders() {
    let Some(tsz) = find_tsz_binary() else {
        println!("tsz binary not found; skipping");
        return;
    };
    let dir = TempDir::new().expect("temp dir");
    std::fs::write(
        dir.path.join("clock.ts"),
        r#"
function isCallable(value: unknown): value is (...args: never[]) => unknown {
    return typeof value === "function";
}

export function clockValue() {
    if (typeof performance !== "undefined" && isCallable(performance.now)) {
        return performance.now();
    }
    return Date.now();
}
"#,
    )
    .expect("write producer");
    std::fs::write(
        dir.path.join("consumer.ts"),
        r#"
import { clockValue } from "./clock.js";

declare function needsNumber(value: number): void;
needsNumber(clockValue());
export const elapsed = clockValue() - 1;
export const wrong: string = clockValue();
"#,
    )
    .expect("write consumer");

    assert_default_matches_eager(&tsz, &dir, &["clock.ts", "consumer.ts"]);
    assert_default_matches_eager(&tsz, &dir, &["consumer.ts", "clock.ts"]);
}
