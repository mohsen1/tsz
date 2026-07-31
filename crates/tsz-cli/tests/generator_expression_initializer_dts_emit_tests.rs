//! Pin: an unannotated generator function *expression* assigned to a
//! variable declaration keeps its `Generator<Y, R, N>` / `AsyncGenerator<...>`
//! shape in declaration emit (issue #15632).
//!
//! `function_expression_type_text_from_ast_at` (the emitter's AST-only
//! "preferred type text" fallback for unannotated function-expression
//! initializers) treated a body with no explicit `return` statement as
//! `void`, with no `func.asterisk_token` check. A generator's return type
//! comes from `yield` operands the solver aggregates, never from `return`,
//! so every unannotated generator function expression collapsed to a bare
//! `() => void` in `.d.ts` output, discarding the generator shape entirely
//! (not just the yield type) — regardless of whether the body used `yield`,
//! `yield*`, or nothing at all. Function *declarations* were unaffected:
//! they emit through a different, generator-aware signature path.
//!
//! The fix bails out of the AST-only reconstruction for unannotated
//! generators and lets the caller fall back to the solver-resolved
//! signature type, which already builds the correct `Generator<...>`
//! application (`unannotated_generator_return_type` in the checker).

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
        path.push(format!("tsz_generator_expr_initializer_dts_{name}_{nanos}"));
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
/// self-skip). `lib` selects the `--lib` flag so async-generator fixtures
/// can request `es2018` while sync fixtures stay on the smaller `es6`.
fn emit_dts_with_lib(name: &str, source: &str, lib: &str) -> Option<String> {
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
            lib,
            "--pretty",
            "false",
        ])
        .current_dir(&temp.path)
        .output()
        .expect("run tsz declaration emit");

    Some(std::fs::read_to_string(temp.path.join("repro.d.ts")).unwrap_or_default())
}

fn emit_dts(name: &str, source: &str) -> Option<String> {
    emit_dts_with_lib(name, source, "es6")
}

#[track_caller]
fn assert_dts(name: &str, source: &str, expected: &str) {
    let Some(dts) = emit_dts(name, source) else {
        println!("skipping: tsz binary unavailable");
        return;
    };
    assert_eq!(dts.trim_end(), expected.trim_end(), "fixture: {name}");
}

#[track_caller]
fn assert_dts_with_lib(name: &str, source: &str, lib: &str, expected: &str) {
    let Some(dts) = emit_dts_with_lib(name, source, lib) else {
        println!("skipping: tsz binary unavailable");
        return;
    };
    assert_eq!(dts.trim_end(), expected.trim_end(), "fixture: {name}");
}

/// Baseline: a plain (non-generator) unannotated function expression with no
/// `return` statement still correctly infers `void`. The fix must not touch
/// this case — it only bails out of the AST reconstruction when
/// `asterisk_token` is set.
#[test]
fn plain_function_expression_still_infers_void() {
    assert_dts(
        "plain_void",
        r#"export const plainFn = function () {};
export const plainArrow = () => {};
"#,
        r#"export declare const plainFn: () => void;
export declare const plainArrow: () => void;"#,
    );
}

/// A generator function expression with plain `yield` operands and no
/// `return` statement must keep the `Generator<...>` shape, not collapse to
/// `void`.
#[test]
fn generator_function_expression_with_yield_keeps_generator_shape() {
    assert_dts(
        "yield_shape",
        r#"export const counter = function* () { yield 1; yield 2; };
"#,
        r#"export declare const counter: () => Generator<1 | 2, void, unknown>;"#,
    );
}

/// An empty generator body (no `yield`, no `return`) must still produce
/// `Generator<never, void, unknown>`, not `void` — this is the shape most
/// directly falsified by the old `body_returns_void` shortcut, since an
/// empty body trivially "returns void" syntactically.
#[test]
fn empty_generator_function_expression_keeps_generator_shape() {
    assert_dts(
        "empty_body",
        r#"export const empty = function* () {};
"#,
        r#"export declare const empty: () => Generator<never, void, unknown>;"#,
    );
}

/// `yield*` delegating to an array must still infer the delegated element
/// type through the generator wrapper.
#[test]
fn generator_function_expression_with_yield_star_keeps_generator_shape() {
    assert_dts(
        "yield_star",
        r#"export const delegated = function* () { yield* [1, 2, 3]; };
"#,
        r#"export declare const delegated: () => Generator<number, void, unknown>;"#,
    );
}

/// Async generator function expressions must keep the `AsyncGenerator<...>`
/// shape, mirroring the sync case (`func.is_async && func.asterisk_token`).
#[test]
fn async_generator_function_expression_keeps_generator_shape() {
    assert_dts_with_lib(
        "async_generator",
        r#"export const asyncGen = async function* () { yield 1; };
"#,
        "es2018",
        r#"export declare const asyncGen: () => AsyncGenerator<number, void, unknown>;"#,
    );
}

/// Adjacent case: renaming the binder must not change the shape (rules out
/// any accidental name-keyed behavior per the anti-hardcoding gate).
#[test]
fn renamed_binder_generator_function_expression_keeps_generator_shape() {
    assert_dts(
        "renamed_binder",
        r#"export const totallyDifferentName = function* () { yield "a"; yield "b"; };
"#,
        r#"export declare const totallyDifferentName: () => Generator<"a" | "b", void, unknown>;"#,
    );
}

/// A function *declaration* (not an expression assigned to a variable) was
/// already correct before this fix — it emits through a different,
/// generator-aware signature path. Pinned here as a control so a future
/// regression in the declaration path is caught by the same file.
#[test]
fn generator_function_declaration_keeps_generator_shape() {
    assert_dts(
        "declaration_control",
        r#"export function* counterDecl() { yield 1; yield 2; }
"#,
        r#"export declare function counterDecl(): Generator<1 | 2, void, unknown>;"#,
    );
}
