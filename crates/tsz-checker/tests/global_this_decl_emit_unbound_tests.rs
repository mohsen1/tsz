//! Locks in TS4025 ("Exported variable has or is using private name") for the
//! `export const globalThis = ...` self-export pattern, even when the
//! `globalThis` reference inside the imported source has no local symbol on
//! the binder.
//!
//! `value_symbol_in_arena` returns `SymbolId::NONE` for the bare identifier
//! `globalThis` in a module that doesn't shadow it, because `globalThis` is
//! the built-in global rather than a binder-tracked local. The earlier code
//! treated the `NONE` result as "no cross-file reference" and returned
//! `false`, silently dropping the diagnostic. The fix: treat the
//! still-unbound `globalThis` identifier as the global itself, since that is
//! the only thing it can resolve to at runtime.
//!
//! Regression: conformance test
//! `compiler/globalThisDeclarationEmit.ts`.

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_checker::context::CheckerOptions;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn diagnostics(source: &str, file_name: &str) -> Vec<u32> {
    let mut parser = ParserState::new(file_name.to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let opts = CheckerOptions {
        emit_declarations: true,
        ..CheckerOptions::default()
    };
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        file_name.to_string(),
        opts,
    );

    checker.check_source_file(root);
    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// `export const globalThis = globalThis;` (single-file form): the right-hand
/// `globalThis` is unbound (no local of that name) and must be treated as the
/// built-in global. tsc emits TS4025 here; tsz used to silently emit nothing
/// because the binder lookup of `globalThis` returned `SymbolId::NONE`.
#[test]
fn export_const_globalThis_assignment_emits_ts4025_for_self_named_export() {
    let codes = diagnostics("export const globalThis = globalThis;\n", "index.ts");
    assert!(
        codes.contains(&4025),
        "expected TS4025 for `export const globalThis = globalThis`, got: {codes:?}"
    );
}
