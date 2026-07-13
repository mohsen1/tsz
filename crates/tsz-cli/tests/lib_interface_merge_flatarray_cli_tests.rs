//! Full-driver regression guards for the "user `interface Error` merges into the
//! lib `Error`" identity-collision bug.
//!
//! When a user script-scope `interface Error { extra?: number }` (or a module
//! `declare global { interface Error { ... } }`) merges into the lib `Error`,
//! UNRELATED lib type references must keep their real identity:
//! - `Array.prototype.flat`'s result stays `FlatArray<...>[]` / `number[]` (the
//!   collision rendered it as an unevaluated `alert<...>[]` / `eval<...>[]`
//!   application, not assignable to `number[]` => spurious `TS2322`).
//! - `RegExp.exec` / `String.match` results keep their `Array<string>` surface,
//!   so `m.map(...)` / `m.length` must not report `TS2339` and `m?.[1]` must not
//!   report `TS7053`.
//!
//! Root cause: during shared-lib priming, `Array`'s members were lowered with
//! name-first def resolution DISABLED, so `FlatArray`'s reference resolved
//! through a merged-global `SymbolId` that (after the interface merge shifts the
//! id layout) aliases an unrelated lib symbol (`alert`/`eval`).
//!
//! This only reproduces through the REAL multi-file driver (`crate::driver::compile`),
//! not the in-crate `check_files_parallel` / `check_multi_file_with_libs`
//! harnesses (their priming installs the global indices before lib member
//! lowering, so name-first resolution is already on and the collision never
//! forms) — so the guard lives here, mirroring
//! `cross_file_local_callee_symbol_identity_tests`.
//!
//! The collision target varies with the lib set (`alert` with DOM, `eval` with a
//! core-only lib), so these assert on diagnostic CODES, not the rendered name.

use crate::args::CliArgs;
use clap::Parser;
use tsz_checker::diagnostics::Diagnostic;

/// Compile `files` (written into one temp dir) through the full driver with a
/// fixed strict, no-emit, core-lib config, returning all diagnostics.
fn compile_files(files: &[(&str, &str)]) -> Vec<Diagnostic> {
    let dir = tempfile::tempdir().expect("temp dir");
    for (name, contents) in files {
        std::fs::write(dir.path().join(name), contents).expect("write repro file");
    }

    let mut argv: Vec<&str> = vec![
        "tsz",
        "--ignoreConfig",
        "--noEmit",
        "--strict",
        "--target",
        "es2022",
        "--lib",
        "es2022",
    ];
    for (name, _) in files {
        argv.push(name);
    }

    let args = CliArgs::try_parse_from(argv).expect("parse args");
    crate::driver::compile(&args, dir.path())
        .expect("compile should succeed")
        .diagnostics
}

fn has_code(files: &[(&str, &str)], code: u32) -> bool {
    compile_files(files).iter().any(|d| d.code == code)
}

/// Core witness: `[[1]].flat()` stays assignable to `number[]` when a user
/// `interface Error` merges into the lib `Error` (no spurious `TS2322`).
#[test]
fn top_level_interface_error_merge_keeps_flat_result_number_array() {
    let files = [(
        "main.ts",
        "interface Error { extra?: number }\n\
         const fl = [[1]].flat();\n\
         const ok: number[] = fl;\n",
    )];
    assert!(
        !has_code(&files, 2322),
        "flat() must stay number[] under a lib-interface merge (no spurious TS2322)"
    );
}

/// The `declare global { interface Error }` module form must be equivalent and
/// must not corrupt `flat()`'s result either.
#[test]
fn declare_global_interface_error_merge_keeps_flat_result_number_array() {
    let files = [
        (
            "aug.ts",
            "export {};\n\
             declare global { interface Error { extra?: number } }\n",
        ),
        (
            "main.ts",
            "const fl = [[1]].flat();\n\
             const ok: number[] = fl;\n",
        ),
    ];
    assert!(
        !has_code(&files, 2322),
        "declare global interface Error merge must not corrupt flat()'s number[] result"
    );
}

/// The merge must not strip `RegExpExecArray`'s `Array<string>` surface:
/// `m.map(...)` / `m.length` must resolve (no spurious `TS2339`).
#[test]
fn interface_error_merge_keeps_regexp_exec_array_surface() {
    let files = [(
        "main.ts",
        "interface Error { extra?: number }\n\
         const m = /x/.exec(\"x\");\n\
         if (m) { const u = m.map(x => x.length); const v: number[] = u; const w = m.length; }\n",
    )];
    assert!(
        !has_code(&files, 2339),
        "RegExpExecArray must keep its Array surface (.map/.length) under a lib-interface merge"
    );
}

/// The merge must not strip `RegExpMatchArray`'s numeric index signature:
/// `m?.[1]` must not report `TS7053`.
#[test]
fn interface_error_merge_keeps_match_array_numeric_index() {
    let files = [(
        "main.ts",
        "interface Error { extra?: number }\n\
         const mm = \"x\".match(/x/);\n\
         const q = mm?.[1];\n",
    )];
    assert!(
        !has_code(&files, 7053),
        "match() result must keep its numeric index signature (no spurious TS7053)"
    );
}

/// Control: with NO augmentation the same surfaces are already clean, pinning
/// that the guards above measure the merge effect, not a pre-existing failure.
#[test]
fn no_augmentation_control_keeps_lib_array_surfaces_clean() {
    let files = [(
        "main.ts",
        "const fl = [[1]].flat();\n\
         const ok: number[] = fl;\n\
         const m = /x/.exec(\"x\");\n\
         if (m) { const u = m.map(x => x.length); const w = m.length; }\n",
    )];
    let diags = compile_files(&files);
    assert!(
        !diags.iter().any(|d| d.code == 2322),
        "control: flat() is number[] with no augmentation"
    );
    assert!(
        !diags.iter().any(|d| d.code == 2339),
        "control: exec() keeps its Array surface with no augmentation"
    );
}

/// The multi-file facet: a user global augmentation in a SEPARATE .d.ts
/// (here `ErrorConstructor`) triggers the lib-baseline passes, and the first
/// of those must not freeze the shared `Array<T>` base — a baseline-baked
/// base broke `flatMap`'s `ReadonlyArray<U>` inference position so the
/// callback return stopped flattening (`number[][][]` instead of
/// `number[][]`). Assert the flatten via an exact reveal-type mismatch.
#[test]
fn declaration_file_augmentation_keeps_flatmap_flattening() {
    let files = [
        (
            "globals.d.ts",
            "interface ErrorConstructor {\n\
             \x20   captureStackTrace?(targetObject: object, constructorOpt?: Function): void;\n\
             }\n",
        ),
        (
            "main.ts",
            "const r = [1, 2, 3].flatMap(x => [[x]]);\n\
             const reveal: null = r;\n",
        ),
    ];
    let diags = compile_files(&files);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly the reveal-type mismatch, got: {diags:?}"
    );
    assert!(
        ts2322[0].message_text.contains("'number[][]'"),
        "flatMap must flatten one level under a lib-global augmentation \
         (tsc infers number[][]); got: {}",
        ts2322[0].message_text
    );
}

/// Control for the flatMap facet: without the augmentation the same program
/// already flattens, pinning that the guard above measures the augmentation
/// effect.
#[test]
fn no_augmentation_control_keeps_flatmap_flattening() {
    let files = [(
        "main.ts",
        "const r = [1, 2, 3].flatMap(x => [[x]]);\n\
         const reveal: null = r;\n",
    )];
    let diags = compile_files(&files);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "control: one reveal mismatch, got: {diags:?}"
    );
    assert!(
        ts2322[0].message_text.contains("'number[][]'"),
        "control: flatMap flattens with no augmentation; got: {}",
        ts2322[0].message_text
    );
}
