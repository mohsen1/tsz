//! DTS emit: transitive retention of non-exported declarations pulled into the
//! public-API surface through a non-exported **class** or **variable**.
//!
//! Structural rule: when a declaration enters the emitted `.d.ts` surface only
//! because an exported declaration references it (an exported function returns
//! a non-exported class, a non-exported class is the base of an exported class,
//! or a non-exported `const` is surfaced through `typeof`), `tsc` also emits —
//! transitively — every local type those member/value signatures name. The
//! declaration emitter's usage analyzer walks the bodies of transitively
//! referenced type aliases, interfaces, functions and modules, but previously
//! skipped class and variable declarations, so a `declare class` (or `declare
//! const`) survived while the local types it referenced were elided, leaving
//! the `.d.ts` with dangling references. The analyzer now walks class bodies
//! and variable types reached transitively, matching `tsc`.
//!
//! Binder names vary per case so the coverage follows the structural shape, not
//! a spelling (anti-hardcoding).

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
        path.push(format!("tsz_non_exported_dep_dts_{name}_{nanos}"));
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

/// Compile `source` with declaration emit and return the generated `.d.ts`
/// text. Returns `None` when the tsz binary is unavailable (lets the test
/// self-skip in environments that do not build the binary).
fn emit_dts(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let _ = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--declaration",
            "--emitDeclarationOnly",
            "--target",
            "es2015",
            "--lib",
            "es6",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz declaration emit");

    Some(std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_default())
}

#[track_caller]
fn assert_dts(name: &str, source: &str, expected: &str) {
    let Some(dts) = emit_dts(name, source) else {
        println!("skipping: tsz binary unavailable");
        return;
    };
    assert_eq!(dts.trim_end(), expected.trim_end(), "fixture: {name}");
}

/// An exported function returns a non-exported class. The class's method
/// parameter and return types are local aliases that must survive elision.
#[test]
fn exported_function_returns_non_exported_class_retains_member_types() {
    assert_dts(
        "exported_fn_returns_local_class",
        r#"type Payload = { id: string };
type Outcome = { ok: boolean };
class Worker {
  run(input: Payload): Outcome { return { ok: true }; }
}
export function spawn(): Worker {
  return new Worker();
}
"#,
        r#"type Payload = {
    id: string;
};
type Outcome = {
    ok: boolean;
};
declare class Worker {
    run(input: Payload): Outcome;
}
export declare function spawn(): Worker;
export {};"#,
    );
}

/// A non-exported class is the base of an exported class. The base's member
/// type (a local interface) must survive — `tsc` emits `interface Spec`.
#[test]
fn non_exported_base_class_retains_member_types() {
    assert_dts(
        "local_base_of_exported_class",
        r#"interface Spec { tag: number }
class Foundation {
  describe(): Spec { return { tag: 1 }; }
}
export class Surface extends Foundation {}
"#,
        r#"interface Spec {
    tag: number;
}
declare class Foundation {
    describe(): Spec;
}
export declare class Surface extends Foundation {
}
export {};"#,
    );
}

/// Transitive chain: an exported function returns a non-exported class whose
/// member returns another non-exported class whose member returns a local
/// alias. Every link must be retained.
#[test]
fn transitive_chain_of_non_exported_classes_retains_every_link() {
    assert_dts(
        "chain_local_classes",
        r#"type Leaf = { v: number };
class Inner {
  leaf(): Leaf { return { v: 0 }; }
}
class Middle {
  inner(): Inner { return new Inner(); }
}
export function entry(): Middle { return new Middle(); }
"#,
        r#"type Leaf = {
    v: number;
};
declare class Inner {
    leaf(): Leaf;
}
declare class Middle {
    inner(): Inner;
}
export declare function entry(): Middle;
export {};"#,
    );
}

/// A non-exported `const` surfaced through `typeof` names a local type in its
/// annotation; the alias must survive (the symmetric variable-declaration gap).
#[test]
fn non_exported_variable_typeof_retains_referenced_types() {
    assert_dts(
        "typeof_local_const",
        r#"type Internal = { z: boolean };
const registry: { read(): Internal } = { read: () => ({ z: true }) };
export type Snapshot = typeof registry;
"#,
        r#"type Internal = {
    z: boolean;
};
declare const registry: {
    read(): Internal;
};
export type Snapshot = typeof registry;
export {};"#,
    );
}
