//! Driver-level regression tests for the "user `interface Error` merges into the
//! lib `Error`" identity-collision bug.
//!
//! When a user script-scope `interface Error { extra?: number }` (or a module
//! `declare global { interface Error { ... } }`) merges into the lib `Error`,
//! UNRELATED lib type references must keep their real identity:
//! - `Array.prototype.flat`'s result stays `FlatArray<...>[]` / `number[]`
//!   (it must not collapse to an unevaluated `alert<...>[]` / `eval<...>[]`
//!   application, which produced a spurious `TS2322` against `number[]`).
//! - `RegExp.exec` / `String.match` results keep their `Array<string>` surface
//!   (`.map`, `.length`, and the numeric index signature), so `m.map(...)` /
//!   `m.length` must not report `TS2339` and `m?.[1]` must not report `TS7053`.
//!
//! The bug is a merged-global vs per-lib-context `SymbolId` id-space collision
//! triggered only by the full parse -> bind -> merge -> check driver (the
//! lightweight `tsz-checker` harness never merges global script symbols across
//! files and always has `FlatArray` resident), so this guard lives at the
//! driver layer, next to `cross_file_interface_merge_ts2717_driver_tests.rs`.

use super::{check_files_parallel, compile_files_with_libs};
use crate::checker::context::CheckerOptions;

/// Compile and check a multi-file program WITH the default ES2019 lib set
/// (`es2019.full`, which includes DOM — the lib that carries the colliding
/// symbol), returning every diagnostic code across all files.
fn diagnostics_with_libs(files: &[(&str, &str)]) -> Vec<u32> {
    let lib_paths = crate::config::resolve_default_lib_files(
        tsz_common::common::ScriptTarget::ES2019,
    )
    .expect("default ES2019 libs");
    let owned: Vec<(String, String)> = files
        .iter()
        .map(|(name, src)| ((*name).to_string(), (*src).to_string()))
        .collect();
    let program = compile_files_with_libs(owned, &lib_paths);
    let options = CheckerOptions {
        strict: true,
        ..CheckerOptions::default()
    };
    let result = check_files_parallel(&program, &options, &[]);
    result
        .file_results
        .iter()
        .flat_map(|file| file.diagnostics.iter().map(|d| d.code))
        .collect()
}

fn program_has_code(files: &[(&str, &str)], code: u32) -> bool {
    diagnostics_with_libs(files).iter().any(|&c| c == code)
}

/// Core witness: `[[1]].flat()` stays assignable to `number[]` when a user
/// `interface Error` merges into the lib `Error`. The collision rendered the
/// result as an unevaluated `alert<...>[]`, which is not assignable to
/// `number[]` and produced a spurious `TS2322`.
#[test]
fn top_level_interface_error_merge_keeps_flat_result_number_array() {
    let files = [(
        "main.ts",
        "interface Error { extra?: number }\n\
         const fl = [[1]].flat();\n\
         const ok: number[] = fl;\n",
    )];
    assert!(
        !program_has_code(&files, 2322),
        "flat() must stay number[] under a lib-interface merge (no spurious TS2322)"
    );
}

/// The `declare global { interface Error }` module form must be equivalent to
/// the script-scope form and must not corrupt `flat()`'s result either.
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
        !program_has_code(&files, 2322),
        "declare global interface Error merge must not corrupt flat()'s number[] result"
    );
}

/// The merge must not strip `RegExpExecArray`'s `Array<string>` surface:
/// `m.map(...)` and `m.length` must resolve (no spurious `TS2339`).
#[test]
fn interface_error_merge_keeps_regexp_exec_array_surface() {
    let files = [(
        "main.ts",
        "interface Error { extra?: number }\n\
         const m = /x/.exec(\"x\");\n\
         if (m) { const u = m.map(x => x.length); const v: number[] = u; const w = m.length; }\n",
    )];
    assert!(
        !program_has_code(&files, 2339),
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
        !program_has_code(&files, 7053),
        "match() result must keep its numeric index signature (no spurious TS7053)"
    );
}

/// Control: with NO augmentation, the same surfaces are already clean. This
/// pins that the guard above measures the merge effect, not a pre-existing
/// failure of `flat()`/`exec()`/`match()`.
#[test]
fn no_augmentation_control_keeps_lib_array_surfaces_clean() {
    let files = [(
        "main.ts",
        "const fl = [[1]].flat();\n\
         const ok: number[] = fl;\n\
         const m = /x/.exec(\"x\");\n\
         if (m) { const u = m.map(x => x.length); const w = m.length; }\n",
    )];
    assert!(
        !program_has_code(&files, 2322),
        "control: flat() is number[] with no augmentation"
    );
    assert!(
        !program_has_code(&files, 2339),
        "control: exec() keeps its Array surface with no augmentation"
    );
}
