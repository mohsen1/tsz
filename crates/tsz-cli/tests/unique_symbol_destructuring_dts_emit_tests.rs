//! DTS emit: a bare `unique symbol` destructuring binding element is declared as
//! `symbol`, matching tsc's `getWidenedUniqueESSymbolType` (the
//! `isBindingElement` branch of `widenTypeForVariableLikeDeclaration`, which
//! widens a binding element regardless of a pattern annotation). So
//! `const [db] = t` with `t: [typeof cs]` emits `db: symbol`, keeping the
//! emitted `.d.ts` consistent with the type the checker enforces for the same
//! binding (#60). A `typeof a | typeof b` union element is preserved (only a
//! *bare* unique symbol widens).
//!
//! These run the full checker pipeline (the unit-level declaration-emit harness
//! uses an empty type cache and cannot infer destructured element types). Each
//! rule is exercised with more than one spelling / binder name so a regression
//! keyed on a particular name rather than the structural shape would fail.

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
        path.push(format!(
            "tsz_unique_symbol_destructuring_dts_{name}_{nanos}"
        ));
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

fn emit_dts(name: &str, source: &str) -> Option<String> {
    let tsz_bin = find_tsz_binary()?;
    let temp = TempDir::new(name).expect("temp dir");
    let src_path = temp.path.join("repro.ts");
    std::fs::write(&src_path, source).expect("write repro file");

    let output = Command::new(tsz_bin)
        .args([
            "repro.ts",
            "--declaration",
            "--emitDeclarationOnly",
            "--strict",
            "--target",
            "es2020",
            "--lib",
            "es2020",
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz declaration emit");

    let dts = std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_else(|_| {
        panic!(
            "expected repro.d.ts to be emitted.\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    Some(dts)
}

macro_rules! dts_or_skip {
    ($name:expr, $src:expr) => {
        match emit_dts($name, $src) {
            Some(dts) => dts,
            None => {
                println!("skipping: tsz binary not found");
                return;
            }
        }
    };
}

fn squash(dts: &str) -> String {
    dts.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn array_destructuring_unique_symbol_element_emits_symbol() {
    let dts = dts_or_skip!(
        "array",
        "declare const cs: unique symbol;\n\
         declare const t: [typeof cs];\n\
         export const [db] = t;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("const db: symbol"),
        "bare unique-symbol array element must emit `db: symbol`:\n{dts}"
    );
    assert!(
        !s.contains("typeof cs"),
        "must not emit the un-widened `typeof cs`:\n{dts}"
    );
}

#[test]
fn object_destructuring_unique_symbol_element_emits_symbol() {
    // Different binder name / pattern kind than the array case.
    let dts = dts_or_skip!(
        "object",
        "declare const alpha: unique symbol;\n\
         declare const o: { k: typeof alpha };\n\
         export const { k: beta } = o;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("const beta: symbol"),
        "bare unique-symbol object element must emit `beta: symbol`:\n{dts}"
    );
    assert!(
        !s.contains("typeof alpha"),
        "must not emit the un-widened `typeof alpha`:\n{dts}"
    );
}

#[test]
fn union_destructuring_element_preserved_in_emit() {
    // Only a *bare* unique symbol widens; a union element keeps its identity,
    // matching tsc.
    let dts = dts_or_skip!(
        "union",
        "declare const sA: unique symbol;\n\
         declare const sB: unique symbol;\n\
         declare const t: [typeof sA | typeof sB];\n\
         export const [u] = t;\n"
    );
    let s = squash(&dts);
    assert!(
        s.contains("const u: typeof sA | typeof sB"),
        "a union of unique symbols must be preserved in the emit, not widened:\n{dts}"
    );
}
