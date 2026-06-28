//! Regression (#14746): a callback whose rest parameter is contextually typed
//! across a module boundary by a generic rest-parameter signature, then spread
//! back into a rest-parameter callee, must not emit a false `TS2556`.
//!
//! Mined from jotai `freezeAtom`. A callback `(...rest) => apply(...rest)` is
//! contextually typed by a generic `Setter`-style signature
//! `<V, A extends unknown[], R>(first: Cell<V, A, R>, ...args: A) => R` reached
//! only through an *imported* alias body. The lowering pass leaves the inner
//! signature reference a bare `UnresolvedTypeName` (its name is in scope only in
//! the declaring file), so the contextual-signature extraction for the callback
//! parameter found nothing and fell the rest parameter back to `any`. Spreading
//! that `any` over the leading fixed parameter of the rest-parameter callee then
//! tripped `TS2556` — a spread of `any` over a non-rest position is a genuine
//! `TS2556` in `tsc`, so the real defect was the contextual type degrading to
//! `any` instead of the tuple `[first, ...A]` that `tsc` (and the single-file
//! form) infer.
//!
//! This bug only manifests through the real multi-file driver (the merged binder
//! graph plus the cross-arena lowering order), so it is pinned at the CLI level;
//! the in-process checker test harness resolves the imported reference eagerly
//! and cannot reproduce it. Binder names are varied from the jotai originals so
//! the rule is keyed on structure, not identifiers.

use std::path::PathBuf;
use std::process::Command;

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

const FLAGS: &[&str] = &[
    "--strict",
    "--target",
    "es2022",
    "--lib",
    "es2022",
    "--types",
    "",
    "--moduleResolution",
    "bundler",
    "--module",
    "esnext",
    "--noEmit",
    "--pretty",
    "false",
];

/// The reported false positive (renamed binders): the contextually-typed
/// callback rest parameter recovers its tuple type across the module boundary,
/// so `apply(...inner)` lands on the rest position and no `TS2556` is emitted.
#[test]
fn cross_module_contextual_rest_tuple_spread_no_ts2556() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");

    std::fs::write(
        dir.path().join("lib.ts"),
        r#"
export type Apply = <V, A extends unknown[], R>(
  cell: Cell<V, A, R>, ...args: A) => R
type WriteFn<A extends unknown[], R> = (apply: Apply, ...args: A) => R
export interface Cell<V, A extends unknown[], R> {
  write: WriteFn<A, R>
}
"#,
    )
    .expect("write lib.ts");
    std::fs::write(
        dir.path().join("consumer.ts"),
        r#"
import type { Cell } from './lib'
export function wrap(cell: Cell<unknown, unknown[], unknown>) {
  const original = cell.write
  cell.write = function (apply, ...rest) {
    return original((...inner) => apply(...inner), ...rest)
  }
}
"#,
    )
    .expect("write consumer.ts");

    let output = Command::new(tsz_bin)
        .args(FLAGS)
        .arg("lib.ts")
        .arg("consumer.ts")
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stdout.contains("TS2556"),
        "cross-module contextual rest-tuple spread must not emit TS2556.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// The literal jotai `.call`-contextual form (under `strictBindCallApply`), with
/// a heritage chain and tuple indexing in the callback body. The callback's
/// contextual type is derived from the inferred `CallableFunction.call` rest
/// tuple, which still resolves the imported signature reference after the fix.
#[test]
fn cross_module_contextual_rest_tuple_spread_via_call_no_ts2556() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");

    std::fs::write(
        dir.path().join("lib.ts"),
        r#"
type Read = <V>(cell: Cell<V>) => V
export type Apply = <V, A extends unknown[], R>(
  cell: CellW<V, A, R>, ...args: A) => R
type WriteFn<A extends unknown[], R> = (read: Read, apply: Apply, ...args: A) => R
export interface Cell<V> { read: (read: Read) => V }
export interface CellW<V, A extends unknown[], R> extends Cell<V> {
  write: WriteFn<A, R>
}
"#,
    )
    .expect("write lib.ts");
    std::fs::write(
        dir.path().join("consumer.ts"),
        r#"
import type { CellW } from './lib'
declare const deepFreeze: <T>(v: T) => T
export function wrap(target: CellW<unknown, unknown[], unknown>) {
  const original = target.write
  target.write = function (read, apply, ...rest) {
    return original.call(this, read,
      (...inner) => {
        if (inner[0] === target) { inner[1] = deepFreeze(inner[1]) }
        return apply(...inner)
      }, ...rest)
  }
}
"#,
    )
    .expect("write consumer.ts");

    let output = Command::new(tsz_bin)
        .args(FLAGS)
        .arg("lib.ts")
        .arg("consumer.ts")
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stdout.contains("TS2556"),
        "cross-module `.call` contextual rest-tuple spread must not emit TS2556.\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Negative control: a genuine spread of an `any`-typed value over a leading
/// fixed (non-rest) parameter is a real `TS2556` in `tsc`. The fix only restores
/// the contextual tuple type; it must not exempt `any` spreads. Single-file.
#[test]
fn explicit_any_spread_over_fixed_param_still_ts2556() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");

    std::fs::write(
        dir.path().join("main.ts"),
        r#"
declare const loose: any;
declare function takesFixedThenRest(head: number, ...tail: string[]): void;
takesFixedThenRest(...loose);
"#,
    )
    .expect("write main.ts");

    let output = Command::new(tsz_bin)
        .args(FLAGS)
        .arg("main.ts")
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("TS2556"),
        "explicit any spread over a fixed parameter must still emit TS2556.\nstdout:\n{stdout}"
    );
}
