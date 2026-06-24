//! DTS emit: inferred return type of an unannotated arrow / function-expression
//! initializer whose body return depends on the solver.
//!
//! Structural rule: when declaration emit infers the return type of an
//! unannotated function *value* (a `const`-assigned arrow or function
//! expression) and the AST-walking inference cannot spell the body return type
//! from source, `tsc` reports the checker's computed body return type. tsz's
//! source-faithful AST paths cannot resolve a body expression that the checker
//! resolves through the solver — a member access through a primitive's apparent
//! type (`(x: string) => x.length`), an un-called method reference
//! (`(x: string) => x.toUpperCase`), or a generic instantiation
//! (`(x: number[]) => x.length`). Before the fix the fallback was a bare `any`,
//! and because the source-derived `(params) => any` text is *preferred* over the
//! canonical solver type, the precise return was lost
//! (`(x: string) => any` instead of `(x: string) => number`).
//!
//! The fix consults the checker-computed body return type as the last resort
//! before `any`. These run the full checker pipeline (the unit-level
//! declaration-emit harness uses an empty type cache and cannot infer the body
//! return type). Each behavioural case is exercised with at least two distinct
//! binder / member spellings so a regression keyed on a particular identifier
//! rather than the structural shape would fail.

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
        path.push(format!("tsz_fn_value_member_return_dts_{name}_{nanos}"));
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
            "esnext",
            "--lib",
            "esnext",
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

// =============================================================================
// Member access through a primitive's apparent type (the primary repro)
// =============================================================================

/// `(x: string) => x.length` must infer `=> number`, not `=> any`. Exercised in
/// both the concise-arrow and block-body forms, and with renamed binders.
#[test]
fn string_length_member_return_infers_number() {
    let Some(dts) = emit_dts(
        "string_length",
        concat!(
            "export const a = (x: string) => x.length;\n",
            "export const b = (value: string) => { return value.length; };\n",
            "export const c = function (s: string) { return s.length; };\n",
        ),
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const a: (x: string) => number;"),
        "concise arrow member access through apparent type must infer number:\n{dts}"
    );
    assert!(
        dts.contains("export declare const b: (value: string) => number;"),
        "block-body member access through apparent type must infer number:\n{dts}"
    );
    assert!(
        dts.contains("export declare const c: (s: string) => number;"),
        "function-expression member access through apparent type must infer number:\n{dts}"
    );
    assert!(
        !dts.contains("=> any"),
        "no inferred return should fall back to any:\n{dts}"
    );
}

/// An un-called method reference (`x.toUpperCase`) keeps its method type, and a
/// numeric apparent-type method (`x.toFixed`) keeps its signature — both were
/// `any` before the fix.
#[test]
fn uncalled_method_reference_keeps_method_type() {
    let Some(dts) = emit_dts(
        "method_ref",
        concat!(
            "export const upper = (x: string) => x.toUpperCase;\n",
            "export const fixed = (n: number) => n.toFixed;\n",
        ),
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const upper: (x: string) => () => string;"),
        "un-called string method reference must keep its method type:\n{dts}"
    );
    assert!(
        dts.contains("fractionDigits") && dts.contains("=> string"),
        "un-called number method reference must keep its signature:\n{dts}"
    );
    assert!(
        !dts.contains("=> any"),
        "no inferred return should fall back to any:\n{dts}"
    );
}

/// `Array<T>.length` and other generic-instantiation members resolve too: an
/// array `.length` and a `Map.size` both infer `number`.
#[test]
fn generic_instantiation_member_return_infers_number() {
    let Some(dts) = emit_dts(
        "generic_member",
        concat!(
            "export const len = (xs: number[]) => xs.length;\n",
            "export const sz = (m: Map<string, number>) => m.size;\n",
        ),
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const len: (xs: number[]) => number;"),
        "array length must infer number:\n{dts}"
    );
    assert!(
        dts.contains("export declare const sz: (m: Map<string, number>) => number;"),
        "Map size must infer number:\n{dts}"
    );
}

// =============================================================================
// async / nested forms reuse the same body return type
// =============================================================================

/// An `async` arrow whose body reads an apparent-type member must wrap exactly
/// once: `async (s: string) => s.length` -> `=> Promise<number>` (the body type
/// is unwrapped before the async wrapper re-wraps it, so it must not
/// double-wrap to `Promise<Promise<number>>`).
#[test]
fn async_member_return_wraps_promise_once() {
    let Some(dts) = emit_dts(
        "async_member",
        concat!(
            "export const a = async (s: string) => s.length;\n",
            "export const b = async (text: string) => { return text.trim(); };\n",
        ),
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const a: (s: string) => Promise<number>;"),
        "async member access must wrap the body return in a single Promise:\n{dts}"
    );
    assert!(
        dts.contains("export declare const b: (text: string) => Promise<string>;"),
        "async block-body member access must wrap once:\n{dts}"
    );
    assert!(
        !dts.contains("Promise<Promise<"),
        "async wrapper must not double-wrap the body return:\n{dts}"
    );
}

/// A nested arrow whose inner body reads an apparent-type member resolves at the
/// inner level too: `(s: string) => () => s.length` -> `=> () => number`.
#[test]
fn nested_arrow_inner_member_return_infers_number() {
    let Some(dts) = emit_dts(
        "nested_arrow",
        "export const make = (s: string) => () => s.length;\n",
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const make: (s: string) => () => number;"),
        "nested arrow inner member access must infer number:\n{dts}"
    );
}

// =============================================================================
// Unchanged behaviour: a genuinely untyped body still emits `any`
// =============================================================================

/// A body that reads a member of `any` is genuinely `any`; the solver fallback
/// must not invent a type, and an `unknown`-typed identity body stays `unknown`.
#[test]
fn untyped_body_still_emits_any() {
    let Some(dts) = emit_dts(
        "untyped",
        concat!(
            "export const a = (x: any) => x.whatever;\n",
            "export const b = (x: unknown) => x;\n",
        ),
    ) else {
        println!("skipping: tsz binary not found");
        return;
    };
    assert!(
        dts.contains("export declare const a: (x: any) => any;"),
        "a member of any is genuinely any:\n{dts}"
    );
    assert!(
        dts.contains("export declare const b: (x: unknown) => unknown;"),
        "an unknown identity body stays unknown:\n{dts}"
    );
}
