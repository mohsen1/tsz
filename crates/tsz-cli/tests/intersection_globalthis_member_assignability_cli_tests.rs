//! Assigning the global `window` (type `Window & typeof globalThis`) into a
//! `Window`-typed position must not report a spurious TS2322/TS2859 (issue
//! #17390).
//!
//! Structural rule: a source intersection is assignable to a target the moment
//! ANY constituent is (`tsc`'s `someTypeRelatedToType`), so `Window & typeof
//! globalThis <: Window` holds because the `Window` constituent is the target.
//! `tsz` materializes the global `window` value's type as a single merged
//! `Window & typeof globalThis` object surface whose own `window`/`self`/`frames`
//! members are again `Window & typeof globalThis`, so a structural walk against
//! `Window` re-mints `this`-bound `Window` instantiations without converging —
//! exhausting the relation budget. That surfaced as TS2859 for a direct
//! assignment (`const w: Window = window`) and as a spurious TS2322 when an
//! object-literal / argument / array-element context turned the depth-exceeded
//! verdict into `False`.
//!
//! Owner: `tsz_solver::relations::subtype` — the dispatch runs an early
//! `someTypeRelatedToType` fast path
//! (`intersection_or_merged_source_satisfies_target`) that recovers the merged
//! surface's origin intersection via `merged_intersection_origin` and
//! short-circuits on the `Window` constituent before any property walk.
//!
//! These are end-to-end binary tests because the global `window`/`self` value
//! reads resolve to their real self-referential lib surface only in the full CLI
//! pipeline (the in-process checker harness returns `any` for lib value vars).
//! They skip gracefully when the DOM lib is unavailable in the checkout.

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
        path.push(format!("tsz_intersection_globalthis_{name}_{nanos}"));
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

/// Run `tsz --target es2015 --noEmit` on `source`; returns combined output.
/// `None` when the binary is unavailable.
fn run_tsz(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let file = temp.path.join("repro.ts");
    std::fs::write(&file, source).expect("write repro file");
    let output = Command::new(tsz_bin)
        .args([
            "repro.ts", "--strict", "false", "--noEmit", "--pretty", "false", "--target", "es2015",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// The DOM lib is unavailable when `window`/`Window` cannot be resolved.
fn dom_unavailable(out: &str) -> bool {
    out.contains("Cannot find name 'window'")
        || out.contains("Cannot find name 'self'")
        || out.contains("Cannot find name 'Window'")
        || out.contains("TS2304")
}

#[test]
fn global_window_assignable_to_window_positions() {
    // Every one of these flows the global `window`/`self`
    // (`Window & typeof globalThis`) into a `Window`-typed position: the
    // reported object-literal property, a required property, a direct
    // assignment (was TS2859), an array element, a call argument, a nested
    // object literal, and the `self` alias. All are clean in `tsc`.
    let source = r#"
interface P { z?: Window; }
const v1: P = { z: window };

interface Q { z: Window; }
const v2: Q = { z: window };

const w1: Window = window;

const arr: Window[] = [window];

declare function takesP(p: { z?: Window }): void;
takesP({ z: window });

const w2: Window = self;

interface Nested { inner: { w: Window }; }
const v3: Nested = { inner: { w: window } };
"#;

    let Some(out) = run_tsz("positive", source) else {
        println!("tsz binary not found; skipping");
        return;
    };
    if dom_unavailable(&out) {
        println!("DOM lib unavailable; skipping");
        return;
    }
    assert!(
        !out.contains("TS2322"),
        "spurious TS2322 assigning `Window & typeof globalThis` to `Window`:\n{out}"
    );
    assert!(
        !out.contains("TS2859"),
        "excessive-complexity TS2859 comparing `Window & typeof globalThis` to `Window`:\n{out}"
    );
}

#[test]
fn bare_global_this_surface_still_rejected_from_window() {
    // Negative control: `typeof globalThis` on its own is NOT a `Window`
    // (it is missing `name`, among others). The constituent short-circuit must
    // not fire here — there is no `Window` constituent — so the assignment must
    // still be rejected, and it must be rejected cleanly (no TS2859 blowup).
    let source = r#"
declare const bareGlobal: typeof globalThis;
const bad: Window = bareGlobal;
"#;

    let Some(out) = run_tsz("negative", source) else {
        println!("tsz binary not found; skipping");
        return;
    };
    if dom_unavailable(&out) {
        println!("DOM lib unavailable; skipping");
        return;
    }
    assert!(
        out.contains("TS2322") || out.contains("TS2740") || out.contains("TS2741"),
        "expected a missing-property rejection assigning `typeof globalThis` to `Window`:\n{out}"
    );
    assert!(
        !out.contains("TS2859"),
        "assigning `typeof globalThis` to `Window` must not blow the relation budget:\n{out}"
    );
}

#[test]
fn global_window_assignable_to_window_and_globalthis_positions() {
    // The global `window` value's type is `Window & typeof globalThis`. Assigning
    // it into a `Window & typeof globalThis` *annotation* (or argument/union
    // target) must be clean: it is the same type on both sides. The bug (#17436)
    // was that the `window` value's own re-minted `typeof globalThis` surface and
    // a directly written `typeof globalThis` annotation are distinct `TypeId`s, so
    // the target intersection was property-merged and the two surfaces compared
    // structurally rather than by identity — and `typeof globalThis` is not even a
    // structural subtype of itself (a merged constructor global like `ArrayBuffer`
    // materializes as its instance type on one side and its `typeof`/constructor
    // type on the other; `NaN`/`Infinity` are numeric-literal names checked
    // against the numeric index signature). `tsc` never hits this because it
    // short-circuits identical types. The fix relates two `typeof globalThis`
    // surface mints by their `GLOBAL_THIS_SURFACE` identity.
    let source = r#"
const w: Window & typeof globalThis = window;

declare function take(x: Window & typeof globalThis): void;
take(window);

declare function pick<T>(a: T, b: T, c: T): T;
var r = pick(undefined, { x: 6, z: window }, { x: 6, y: '' });
"#;

    let Some(out) = run_tsz("window_and_globalthis", source) else {
        println!("tsz binary not found; skipping");
        return;
    };
    if dom_unavailable(&out) {
        println!("DOM lib unavailable; skipping");
        return;
    }
    assert!(
        !out.contains("TS2322"),
        "spurious TS2322 relating `Window & typeof globalThis` to itself:\n{out}"
    );
    assert!(
        !out.contains("TS2345"),
        "spurious TS2345 relating `Window & typeof globalThis` to itself:\n{out}"
    );
    assert!(
        !out.contains("TS2859"),
        "excessive-complexity TS2859 relating `Window & typeof globalThis` to itself:\n{out}"
    );
}
