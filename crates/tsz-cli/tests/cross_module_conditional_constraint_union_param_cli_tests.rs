//! Regression: a cross-module generic function whose type parameter has a
//! *conditional-type* constraint must not collapse a union parameter's arms.
//!
//! Issue #14753. When `resolve<K extends Key>(opt: undefined | Opt<K>, …)` is
//! imported across a module boundary and `Key` is a conditional alias carrying
//! a definitional `infer T` (`R extends { key: infer T } ? T : …`), the
//! placeholder-union pruning in generic-call finalization walked into `K`'s
//! conditional constraint, classified the bound `infer T` as a live inference
//! placeholder, and dropped the `Opt<K>` arm — leaving the parameter as bare
//! `undefined`. The relation then checked `number` (a valid `Opt<K>` arm)
//! against `undefined`, yielding a spurious `TS2345`. `tsc` accepts.
//!
//! Binder names below are deliberately varied from the issue's repro (no
//! `Register`/`Key`/`Opt`/`resolve`) so the fix cannot be a name-scoped path.

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

/// The reported false positive: a `number | fn` union arm survives the
/// cross-module conditional-constraint instantiation, so the call type-checks
/// exactly as `tsc` accepts it.
#[test]
fn cross_module_conditional_constraint_keeps_union_param_arms() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");

    std::fs::write(
        dir.path().join("registry.ts"),
        r#"
interface Slots {}
export type Tag = Slots extends { entry: infer Picked }
  ? Picked extends ReadonlyArray<unknown> ? Picked : ReadonlyArray<unknown>
  : ReadonlyArray<unknown>
export type Choice<Slot extends Tag = Tag> = number | ((cell: Holder<Slot>) => number)
export declare class Holder<Slot extends Tag = Tag> { stamp: Slot }
export declare function settle<Slot extends Tag = Tag>(
  choice: undefined | Choice<Slot>,
  holder: Holder<Slot>,
): number | undefined
"#,
    )
    .expect("write registry.ts");
    std::fs::write(
        dir.path().join("consumer.ts"),
        r#"
import { settle } from './registry'
import type { Holder, Choice, Tag } from './registry'
export class Gauge<Slot extends Tag> {
  choice!: Choice<any> | undefined
  holder!: Holder<Slot>
  run(): number | undefined { return settle(this.choice, this.holder) }
}
"#,
    )
    .expect("write consumer.ts");

    let output = Command::new(tsz_bin)
        .args(FLAGS)
        .arg("registry.ts")
        .arg("consumer.ts")
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && !stdout.contains("TS2345"),
        "cross-module conditional-constraint union param must keep its `number | fn` arm \
         (no spurious TS2345).\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

/// Adjacent guard: the same conditional-constraint shape *inlined* in a single
/// file must reject a genuinely-bad `string` argument (the alias resolves, so
/// the parameter is `number | fn`). Pins that the pruning fix does not blanket
/// the union into `any`.
#[test]
fn single_file_conditional_constraint_still_rejects_bad_arg() {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping: tsz binary not found");
        return;
    };
    let dir = tempfile::tempdir().expect("temp dir");

    std::fs::write(
        dir.path().join("inline.ts"),
        r#"
interface Slots {}
type Tag = Slots extends { entry: infer Picked }
  ? Picked extends ReadonlyArray<unknown> ? Picked : ReadonlyArray<unknown>
  : ReadonlyArray<unknown>
type Choice<Slot extends Tag = Tag> = number | ((cell: Holder<Slot>) => number)
declare class Holder<Slot extends Tag = Tag> { stamp: Slot }
declare function settle<Slot extends Tag = Tag>(
  choice: undefined | Choice<Slot>,
  holder: Holder<Slot>,
): number | undefined
class Gauge<Slot extends Tag> {
  bad!: string
  holder!: Holder<Slot>
  run(): number | undefined { return settle(this.bad, this.holder) }
}
"#,
    )
    .expect("write inline.ts");

    let output = Command::new(tsz_bin)
        .args(FLAGS)
        .arg("inline.ts")
        .current_dir(dir.path())
        .output()
        .expect("run tsz");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("TS2345"),
        "single-file conditional-constraint param `number | fn` must reject a `string` arg.\nstdout:\n{stdout}"
    );
}
