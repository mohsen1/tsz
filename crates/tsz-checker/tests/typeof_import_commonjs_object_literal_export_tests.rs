//! `typeof import("./mod").X` against a CommonJS module that exports an
//! object literal via `module.exports = { … }` must resolve `X` as a
//! value-side member when one of the literal's properties is named `X`.
//!
//! tsc treats the literal's shorthand/property assignments as value-side
//! exports of the module, so:
//!
//! ```ts
//! // mod.js
//! class Thing { x = 1 }
//! function foo() { return 1 }
//! /** @typedef {() => number} buz */
//! module.exports = { Thing, foo, literal: "" }
//! // index.ts
//! function values(
//!     a: typeof import('./mod').Thing,    // OK — value member
//!     b: typeof import('./mod').foo,      // OK — value member
//!     c: typeof import('./mod').literal,  // OK — value member
//!     d: typeof import('./mod').buz,      // TS2694 — JSDoc typedef is type-only
//! ) {}
//! ```
//!
//! Before this regression test landed, tsz reported TS2694 for `Thing`,
//! `foo`, and `literal` as well, because the binder's exports table for the
//! target file only contains an `export=` symbol (or is empty) and the
//! synthesized typeof-import namespace did not consult the JS export
//! surface for the literal's properties. The fix routes
//! `build_typeof_import_namespace_type` through
//! `merge_js_export_surface_into_typeof_import_namespace_props` so the
//! value-side members of `module.exports = { … }` populate the namespace
//! shape and only the genuinely type-only `@typedef` lookup fails.

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::construction::TypeInterner;

fn diagnostics_for_two_files(js_source: &str, ts_source: &str) -> Vec<(u32, u32, u32, String)> {
    let mut parser_js = ParserState::new("mod.js".to_string(), js_source.to_string());
    let root_js = parser_js.parse_source_file();
    let mut binder_js = BinderState::new();
    binder_js.bind_source_file(parser_js.get_arena(), root_js);

    let mut parser_ts = ParserState::new("index.ts".to_string(), ts_source.to_string());
    let root_ts = parser_ts.parse_source_file();
    let mut binder_ts = BinderState::new();
    binder_ts.bind_source_file(parser_ts.get_arena(), root_ts);

    let arena_js = Arc::new(parser_js.get_arena().clone());
    let arena_ts = Arc::new(parser_ts.get_arena().clone());
    let all_arenas = Arc::new(vec![Arc::clone(&arena_js), Arc::clone(&arena_ts)]);

    let file_js_exports = binder_js.module_exports.get("mod.js").cloned();
    if let Some(exports) = &file_js_exports {
        std::sync::Arc::make_mut(&mut binder_ts.module_exports)
            .insert("./mod".to_string(), exports.clone());
    }

    let mut cross_file_targets = FxHashMap::default();
    if let Some(exports) = &file_js_exports {
        for (_name, &sym_id) in exports.iter() {
            cross_file_targets.insert(sym_id, 0usize);
        }
    }

    let binder_js = Arc::new(binder_js);
    let binder_ts = Arc::new(binder_ts);
    let all_binders = Arc::new(vec![Arc::clone(&binder_js), Arc::clone(&binder_ts)]);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arena_ts.as_ref(),
        binder_ts.as_ref(),
        &types,
        "index.ts".to_string(),
        CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            no_lib: true,
            ..Default::default()
        },
    );

    checker.ctx.set_all_arenas(all_arenas);
    checker.ctx.set_all_binders(all_binders);
    checker.ctx.set_current_file_idx(1);
    for (sym_id, file_idx) in &cross_file_targets {
        checker.ctx.register_symbol_file_target(*sym_id, *file_idx);
    }

    let mut resolved_module_paths: FxHashMap<(usize, String), usize> = FxHashMap::default();
    resolved_module_paths.insert((1, "./mod".to_string()), 0);
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));

    let mut resolved_modules: FxHashSet<String> = FxHashSet::default();
    resolved_modules.insert("./mod".to_string());
    checker.ctx.set_resolved_modules(resolved_modules);

    checker.check_source_file(root_ts);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| {
            // Compute 1-based line + column for the diagnostic start position.
            let (line, column) = line_and_column_for_offset(ts_source, d.start);
            (d.code, line, column, d.message_text.clone())
        })
        .collect()
}

fn line_and_column_for_offset(source: &str, offset: u32) -> (u32, u32) {
    let mut line: u32 = 1;
    let mut col: u32 = 1;
    for (idx, ch) in source.char_indices() {
        if (idx as u32) == offset {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

#[test]
fn typeof_import_resolves_object_literal_value_members_no_ts2694() {
    let diagnostics = diagnostics_for_two_files(
        r#"
class Thing { x = 1 }
function foo() { return 1 }
/** @typedef {() => number} buz */
module.exports = { Thing, foo, literal: "" }
"#,
        r#"
function values(
    a: typeof import('./mod').Thing,
    b: typeof import('./mod').foo,
    c: typeof import('./mod').literal,
) {
    return a;
}
"#,
    );

    let ts2694: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _, _, _)| *c == 2694)
        .collect();
    assert!(
        ts2694.is_empty(),
        "Expected no TS2694 for typeof import('./mod').{{Thing,foo,literal}} when the JS module \
         uses `module.exports = {{ Thing, foo, literal }}` — those names are value-side exports, \
         got: {ts2694:#?}"
    );
}

#[test]
fn typeof_import_for_jsdoc_typedef_still_emits_ts2694() {
    // `buz` is declared via `@typedef`, so it is type-only and
    // `typeof import('./mod').buz` must continue to emit TS2694.
    // The fix only adds value-side members to the typeof-import namespace.
    let diagnostics = diagnostics_for_two_files(
        r#"
/** @typedef {() => number} buz */
module.exports = { other: 1 }
"#,
        r#"
function values(
    a: typeof import('./mod').buz,
) {
    return a;
}
"#,
    );

    let ts2694: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _, _, _)| *c == 2694)
        .collect();
    assert_eq!(
        ts2694.len(),
        1,
        "Expected exactly one TS2694 for `typeof import('./mod').buz` (JSDoc typedefs are not \
         value-side exports), got: {diagnostics:#?}"
    );
    let msg = &ts2694[0].3;
    assert!(
        msg.contains("'buz'"),
        "Expected TS2694 message to mention 'buz', got: {msg}"
    );
}

#[test]
fn typeof_import_member_name_uses_alternate_iteration_variable() {
    // Sanity-check the structural rule by exercising different property
    // names so the fix can't silently be name-scoped to specific identifiers.
    let diagnostics = diagnostics_for_two_files(
        r#"
class A { a = 1 }
class B { b = 2 }
function frobnicate() { return 0 }
module.exports = { A, B, frobnicate }
"#,
        r#"
function values(
    a: typeof import('./mod').A,
    b: typeof import('./mod').B,
    c: typeof import('./mod').frobnicate,
) {
    return a;
}
"#,
    );

    let ts2694: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _, _, _)| *c == 2694)
        .collect();
    assert!(
        ts2694.is_empty(),
        "Expected no TS2694 for typeof import('./mod').{{A,B,frobnicate}} from a CJS object-literal \
         export, got: {ts2694:#?}"
    );
}

#[test]
fn bare_import_type_position_still_emits_ts2694_for_value_only_member() {
    // The fix targets `typeof import("./mod").X` (value-side lookup).
    // `import("./mod").X` (type-side lookup) for a value-only member should
    // continue to emit TS2694, matching tsc.
    let diagnostics = diagnostics_for_two_files(
        r#"
class Thing { x = 1 }
function foo() { return 1 }
module.exports = { Thing, foo }
"#,
        r#"
function types(
    a: import('./mod').Thing,
    b: import('./mod').foo,
) {
    return a;
}
"#,
    );

    let ts2694: Vec<_> = diagnostics
        .iter()
        .filter(|(c, _, _, _)| *c == 2694)
        .collect();
    assert_eq!(
        ts2694.len(),
        2,
        "Expected TS2694 for both `import('./mod').Thing` and `import('./mod').foo` in type \
         position (CJS object-literal exports do not expose value-side members as types), got: {diagnostics:#?}"
    );
}
