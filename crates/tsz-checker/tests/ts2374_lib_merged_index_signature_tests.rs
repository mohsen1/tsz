//! TS2374 anchor for cross-arena merged interface duplicate index signatures.
//!
//! When a user file declares an `interface String { [x: number]: string;
//! [x: number]: string; }` (or any other lib-merging interface), tsc treats
//! the two user-side index signatures **and** the lib's same-kind index
//! signature in `lib.es5.d.ts` as duplicates and emits TS2374 on each of
//! them. The local-body emissions are produced by the existing checker
//! paths; this test locks in the cross-arena emission on the lib side.
//!
//! Conformance: `conformance/types/members/duplicateNumericIndexers.ts`.

use std::path::Path;
use std::sync::Arc;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_es5_lib_files_for_test() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Try paths in priority order; load only the first one that exists so
    // the test sees a single canonical es5 lib (not multiple conflicting copies).
    let lib_paths = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../TypeScript/src/lib/es5.d.ts"),
    ];
    for lib_path in &lib_paths {
        if lib_path.exists()
            && let Ok(content) = std::fs::read_to_string(lib_path)
        {
            let file_name = lib_path.file_name().unwrap().to_string_lossy().to_string();
            let lib_file = LibFile::from_source(file_name, content);
            return vec![Arc::new(lib_file)];
        }
    }
    Vec::new()
}

fn check_with_es5_lib(source: &str, file_name: &str) -> Vec<Diagnostic> {
    let lib_files = load_es5_lib_files_for_test();
    if lib_files.is_empty() {
        return Vec::new();
    }

    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&lib_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let options = CheckerOptions::default();

    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        options,
    );
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.ctx.set_actual_lib_file_count(lib_files.len());
    checker.check_source_file(root);
    checker.ctx.diagnostics.clone()
}

fn ts2374_for_kind<'a>(diags: &'a [Diagnostic], kind: &str) -> Vec<&'a Diagnostic> {
    diags
        .iter()
        .filter(|d| {
            d.code == 2374
                && d.message_text
                    .contains(&format!("Duplicate index signature for type '{kind}'"))
        })
        .collect()
}

/// User code declares `interface String { [x: number]: string; [x: number]: string; }`
/// which merges with `lib.es5.d.ts`'s `interface String { ... readonly [index: number]: string; ... }`.
/// tsc reports TS2374 at each of the three number-index signatures (two user, one lib).
/// This test locks in the *lib-side* emission, which the local-body checker paths
/// do not produce on their own.
#[test]
fn lib_merged_string_interface_duplicate_number_index_emits_at_lib() {
    let user_ts = "interface String {\n    [x: number]: string;\n    [x: number]: string;\n}\n";
    let diags = check_with_es5_lib(user_ts, "merge_string.ts");
    if diags.is_empty() {
        // Lib not available in this environment — skip rather than fail.
        return;
    }

    let number_dups = ts2374_for_kind(&diags, "number");
    assert!(
        !number_dups.is_empty(),
        "expected TS2374 'number' diagnostics, got: {diags:?}"
    );

    let on_lib: Vec<&Diagnostic> = number_dups
        .iter()
        .copied()
        .filter(|d| d.file.ends_with("lib.es5.d.ts") || d.file.ends_with("es5.d.ts"))
        .collect();
    assert!(
        !on_lib.is_empty(),
        "expected at least one TS2374 'number' diagnostic anchored at the lib's \
         String interface index signature; got: {number_dups:?}"
    );

    let on_user: Vec<&Diagnostic> = number_dups
        .iter()
        .copied()
        .filter(|d| d.file.ends_with("merge_string.ts"))
        .collect();
    assert_eq!(
        on_user.len(),
        2,
        "expected two TS2374 'number' diagnostics anchored at the user's two \
         duplicate index signatures; got: {on_user:?}"
    );
}

/// Same structural rule for `interface Array<T>` merging: the lib provides
/// `[n: number]: T;`, and a user adding two more number-index signatures
/// must see the lib signature flagged as well.
#[test]
fn lib_merged_array_interface_duplicate_number_index_emits_at_lib() {
    let user_ts = "interface Array<T> {\n    [x: number]: T;\n    [x: number]: T;\n}\n";
    let diags = check_with_es5_lib(user_ts, "merge_array.ts");
    if diags.is_empty() {
        return;
    }

    let number_dups = ts2374_for_kind(&diags, "number");
    assert!(
        !number_dups.is_empty(),
        "expected TS2374 'number' diagnostics, got: {diags:?}"
    );

    let on_lib: Vec<&Diagnostic> = number_dups
        .iter()
        .copied()
        .filter(|d| d.file.ends_with("lib.es5.d.ts") || d.file.ends_with("es5.d.ts"))
        .collect();
    assert!(
        !on_lib.is_empty(),
        "expected the lib's Array<T> number-index signature to be flagged as a \
         TS2374 duplicate; got: {number_dups:?}"
    );
}

/// Coverage gap surfaced by review of the original scope-only branch:
/// when the file contains both a scoped container (e.g. a `namespace`) and
/// a top-level interface augmenting a lib type, the top-level interface
/// lives in `file_locals` rather than any nested scope's table. The
/// lib-merge helper must walk both `scopes` and `file_locals` to flag the
/// lib-side TS2374 in this configuration.
#[test]
fn lib_merged_string_interface_with_sibling_namespace_still_emits_at_lib() {
    let user_ts = "namespace Helpers { export const x = 1; }\n\
                   interface String {\n    [x: number]: string;\n    [x: number]: string;\n}\n";
    let diags = check_with_es5_lib(user_ts, "merge_string_with_ns.ts");
    if diags.is_empty() {
        return;
    }

    let number_dups = ts2374_for_kind(&diags, "number");
    let on_lib: Vec<&Diagnostic> = number_dups
        .iter()
        .copied()
        .filter(|d| d.file.ends_with("lib.es5.d.ts") || d.file.ends_with("es5.d.ts"))
        .collect();
    assert!(
        !on_lib.is_empty(),
        "expected the lib's String index signature to be flagged even with a \
         sibling namespace in the same file; got: {number_dups:?}"
    );
}

/// Negative case: `interface Number` has no number-index signature in the lib,
/// so a single user-body duplicate yields the two user-side errors but **no**
/// lib-side error — the lib-merge helper must not over-fire when the lib has
/// no same-kind signature to flag.
#[test]
fn lib_merged_number_interface_no_lib_index_does_not_emit_on_lib() {
    let user_ts = "interface Number {\n    [x: number]: string;\n    [x: number]: string;\n}\n";
    let diags = check_with_es5_lib(user_ts, "merge_number.ts");
    if diags.is_empty() {
        return;
    }

    let number_dups = ts2374_for_kind(&diags, "number");
    let on_lib: Vec<&Diagnostic> = number_dups
        .iter()
        .copied()
        .filter(|d| d.file.ends_with("lib.es5.d.ts") || d.file.ends_with("es5.d.ts"))
        .collect();
    assert!(
        on_lib.is_empty(),
        "the lib's `interface Number` does not declare a number index, so the \
         lib-merge helper must not flag any lib position. Got: {on_lib:?}"
    );
}
