//! An array literal whose contextual type is a non-array *iterable* (e.g.
//! `Iterable<M>`) must contextually type its elements with the iteration
//! (yield) type, keeping element literals instead of widening them to their
//! base (`string`). The immer `enableArrayMethods` witness
//! (`new Set<MutatingArrayMethod>(["shift", "unshift"])`) reaches this through
//! the merged `SetConstructor` whose *first* overload is
//! `new <T>(iterable?: Iterable<T> | null)`: overload resolution offers the
//! array literal the `Iterable<M>` contextual type, which is expanded to its
//! object form (with `[Symbol.iterator]`) before array-literal typing runs. The
//! Application `args[0]` heuristic no longer applies to that object form, so the
//! element context must be recovered from the iterator protocol. tsc 7.0.2 is
//! clean; the real `tsz` binary must be too.
//!
//! Driven through the real binary so the check runs against the embedded lib's
//! actual merged `SetConstructor` overloads (iterable-first), which is exactly
//! what makes the object-form expansion — and thus the regression — observable.

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
        path.push(format!("tsz_arr_iter_ctx_{name}_{nanos}"));
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

/// Run `tsz --strict --noEmit` on a single source file and return
/// combined stdout+stderr. `TSZ_USE_EMBEDDED_LIBS=1` pins the lib so the merged
/// `SetConstructor` overload order matches the fixture the regression came from.
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

/// Positive: `new Set<M>([...])` with string-literal-union `M`, both as plain
/// literals and via spread of another `Set<M>`, must type-check. This is the
/// immer `enableArrayMethods` shape. tsc 7.0.2 is clean.
#[test]
fn set_of_literal_union_from_array_literal_type_checks() {
    let src = r#"
type M = "push" | "pop" | "shift" | "unshift";
const shifting = new Set<M>(["shift", "unshift"]);
const queue = new Set<M>(["push", "pop"]);
const combined = new Set<M>([...queue, ...shifting]);
const mutating = new Set<M>([...combined, "splice" as M]);
"#;
    let Some(out) = run_tsz_single("set_literal_union", src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS"),
        "`new Set<M>([...])` over a string-literal union must keep element literals \
         (tsc 7.0.2 is clean); got:\n{out}"
    );
}

/// Positive: a bare `Iterable<M>` contextual type on a variable initializer must
/// also contextually type the elements (the root of the overload case).
#[test]
fn bare_iterable_contextual_type_keeps_element_literals() {
    let src = r#"
type M = "a" | "b";
const a: Iterable<M> = ["a", "b"];
const b: Iterable<M> | null = ["a", "b"];
"#;
    let Some(out) = run_tsz_single("bare_iterable", src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        !out.contains("error TS"),
        "array literal against `Iterable<M>` must keep element literals; got:\n{out}"
    );
}

/// Negative control: a genuinely wrong element (not a member of `M`) must still
/// fail — the fix must not erase the element type to `string`/`any`.
#[test]
fn set_of_literal_union_rejects_non_member_element() {
    let src = r#"
type M = "push" | "pop";
const bad = new Set<M>(["push", "nope"]);
"#;
    let Some(out) = run_tsz_single("set_non_member", src) else {
        println!("tsz binary not found; skipping");
        return;
    };
    assert!(
        out.contains("error TS"),
        "`\"nope\"` is not assignable to `M`, so this must still error; got:\n{out}"
    );
}
