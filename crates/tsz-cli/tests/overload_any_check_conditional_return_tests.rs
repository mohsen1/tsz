//! An overload whose return type is a conditional over a naked type parameter
//! (`T extends U ? A : B`) must stay compatible with an implementation whose
//! return matches EITHER branch. `getErasedSignature` erases the overload's type
//! parameters to `any`, so tsc relates the implementation return against
//! `any extends U ? A : B`, which `getConditionalType` resolves to `A | B` (both
//! branches). tsz previously resolved the `any` check as a single true-branch
//! pick, dropping the false branch; when the implementation return only matched
//! the dropped branch the comparison degraded to a false-positive TS2394. This
//! is the ts-pattern `select()` witness (patterns.ts:673).
//!
//! Driven through the real `tsz` binary: the in-process checker harness resolves
//! the erased conditional differently and does not reproduce the false positive,
//! so only the real binary flips here.

use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_ovl_anycond_{name}_{nanos}"));
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

fn run_tsz_single(name: &str, src: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    std::fs::write(temp.path.join("main.ts"), src).expect("write main");
    let output = Command::new(tsz_bin)
        .args(["main.ts", "--strict", "--noEmit", "--pretty", "false"])
        .env("TSZ_USE_EMBEDDED_LIBS", "1")
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// Positive: the implementation return matches only the conditional's FALSE
/// branch. tsc 7.0.2 accepts it (erased overload return is `A | B`). Binder
/// names are deliberately not the ts-pattern ones so no fix can key on them.
#[test]
fn overload_any_check_conditional_return_matching_false_branch_is_compatible() {
    let src = r#"
type Alpha = { tag: "a"; a: number };
type Beta = { tag: "b"; b: string };

export function choose<Key extends string>(k: Key): Key extends "x" ? Alpha : Beta;
export function choose(k: string): Beta {
    return { tag: "b", b: k };
}
"#;
    let Some(out) = run_tsz_single("false_branch", src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS"),
        "impl return matches the conditional's false branch; erased overload \
         return is `Alpha | Beta`, so no TS2394 (tsc 7.0.2 clean); got:\n{out}"
    );
}

/// Positive: the implementation return matches only the conditional's TRUE
/// branch — the pre-existing direction, which must remain compatible.
#[test]
fn overload_any_check_conditional_return_matching_true_branch_is_compatible() {
    let src = r#"
type Alpha = { tag: "a"; a: number };
type Beta = { tag: "b"; b: string };

export function choose<Key extends string>(k: Key): Key extends "x" ? Alpha : Beta;
export function choose(k: string): Alpha {
    return { tag: "a", a: k.length };
}
"#;
    let Some(out) = run_tsz_single("true_branch", src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS"),
        "impl return matches the conditional's true branch; must stay compatible; got:\n{out}"
    );
}

/// Negative control: the implementation return matches NEITHER branch. tsc 7.0.2
/// reports TS2394, and tsz must too — the distribution must not silence a real
/// incompatibility by widening to `any`.
#[test]
fn overload_any_check_conditional_return_matching_no_branch_reports_ts2394() {
    let src = r#"
type Alpha = { tag: "a"; a: number };
type Beta = { tag: "b"; b: string };

export function choose<Key extends string>(k: Key): Key extends "x" ? Alpha : Beta;
export function choose(k: string): { tag: "c"; c: boolean } {
    return { tag: "c", c: true };
}
"#;
    let Some(out) = run_tsz_single("no_branch", src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        out.contains("TS2394"),
        "impl return matches neither branch of `Alpha | Beta`; TS2394 must still fire; got:\n{out}"
    );
}
