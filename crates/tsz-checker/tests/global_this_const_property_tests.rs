//! TS2339 ("Property 'X' does not exist on type 'typeof globalThis'") for
//! `globalThis.X` writes/reads where `X` is a user-declared `let`/`const`.
//!
//! In TypeScript, only `var`/`function`/`class`/etc. declarations become
//! properties of `typeof globalThis`. Block-scoped (`let`/`const`)
//! declarations do NOT — even though they're in the script's top-level scope.
//!
//! Regression: `conformance/es2019/globalThisReadonlyProperties.ts` failed
//! because `resolve_lib_global_var_symbol` walked `lib_symbol_ids` looking for
//! a "shadowed lib `var`", and was matching parameter symbols like the `y` in
//! `Math.atan2(y, x)` from `lib.es5.d.ts`. The parameter has
//! `FUNCTION_SCOPED_VARIABLE` flag (same as a `var`), so the existing flag
//! filter let it through, spoofing a "lib var `y` exists" answer and
//! suppressing the legitimate TS2339 for `globalThis.y`.
//!
//! The fix narrows the lookup to declarations whose syntactic kind is
//! plausibly a global value (not `Parameter`).

use std::path::Path;
use std::sync::Arc;
use tsz_binder::state::LibContext as BinderLibContext;
use tsz_binder::{BinderState, lib_loader::LibFile};
use tsz_checker::context::CheckerOptions;
use tsz_checker::context::LibContext as CheckerLibContext;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn load_es5_lib_files() -> Vec<Arc<LibFile>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let candidates = [
        manifest_dir.join("../../TypeScript/lib/lib.es5.d.ts"),
        manifest_dir.join("../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
        manifest_dir.join("../../scripts/conformance/node_modules/typescript/lib/lib.es5.d.ts"),
    ];
    let mut out = Vec::new();
    for path in &candidates {
        if path.exists()
            && let Ok(content) = std::fs::read_to_string(path)
        {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            out.push(Arc::new(LibFile::from_source(name, content)));
        }
    }
    out
}

fn diagnostic_codes_with_lib(source: &str) -> Vec<u32> {
    let lib_files = load_es5_lib_files();
    assert!(
        !lib_files.is_empty(),
        "lib.es5.d.ts not found — required for this regression test"
    );

    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    let binder_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| BinderLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    binder.merge_lib_contexts_into_binder(&binder_lib_contexts);
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        CheckerOptions::default(),
    );
    let checker_lib_contexts: Vec<_> = lib_files
        .iter()
        .map(|lib| CheckerLibContext {
            arena: Arc::clone(&lib.arena),
            binder: Arc::clone(&lib.binder),
        })
        .collect();
    checker.ctx.set_lib_contexts(checker_lib_contexts);
    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

/// `const y` is block-scoped and not a property of `typeof globalThis`.
/// `globalThis.y = 4` must report TS2339, even though `lib.es5.d.ts` happens
/// to mention parameters named `y` (e.g. `Math.atan2(y, x)`).
#[test]
fn const_not_property_of_globalthis_writes_emit_ts2339() {
    let source = "const y = 2;\nglobalThis.y = 4;\n";
    let codes = diagnostic_codes_with_lib(source);
    assert!(
        codes.contains(&2339),
        "expected TS2339 for globalThis.y assignment with `const y`, got {codes:?}"
    );
}

/// `var x` IS a property of `typeof globalThis`, so `globalThis.x = 3` must
/// not report TS2339. Pairs with the test above to lock the flag-filter
/// correctness from both directions.
#[test]
fn var_is_property_of_globalthis_writes_no_ts2339() {
    let source = "var x = 1;\nglobalThis.x = 3;\n";
    let codes = diagnostic_codes_with_lib(source);
    assert!(
        !codes.contains(&2339),
        "did not expect TS2339 for globalThis.x assignment with `var x`, got {codes:?}"
    );
}

/// Read access mirrors the write case — `globalThis.y` reads on a `const y`
/// must report TS2339, otherwise the lib-parameter lookup is silently
/// substituting the parameter's type.
#[test]
fn const_not_property_of_globalthis_reads_emit_ts2339() {
    let source = "const y = 2;\nconst zz: number = globalThis.y;\n";
    let codes = diagnostic_codes_with_lib(source);
    assert!(
        codes.contains(&2339),
        "expected TS2339 for globalThis.y read with `const y`, got {codes:?}"
    );
}
