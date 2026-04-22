//! Unit tests for property access resolution diagnostics (TS2708, TS2693, …).
//!
//! These tests exercise the TS2708 "Cannot use namespace as a value"
//! emission/suppression logic in `resolve_property_access`. In particular
//! they lock in that JS `checkJs` expando-assignment LHS chains targeting
//! type-only namespace members do *not* emit TS2708 — matching tsc's
//! `prototype-property assignment merge` behaviour.

use crate::context::{CheckerOptions, ScriptTarget};
use crate::query_boundaries::common::TypeInterner;
use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_parser::parser::ParserState;

fn diagnostics_for_files(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    let mut roots = Vec::with_capacity(files.len());
    let file_names: Vec<String> = files.iter().map(|(name, _)| (*name).to_string()).collect();

    for (name, source) in files {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
        roots.push(root);
    }

    let entry_idx = file_names
        .iter()
        .position(|name| name == entry)
        .expect("entry file should be in the files list");
    let all_arenas = Arc::new(arenas);
    let all_binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        all_arenas[entry_idx].as_ref(),
        all_binders[entry_idx].as_ref(),
        &types,
        file_names[entry_idx].clone(),
        CheckerOptions {
            target: ScriptTarget::ES2015,
            allow_js: true,
            check_js: true,
            ..Default::default()
        },
    );

    checker.ctx.set_lib_contexts(Vec::new());
    checker.ctx.set_all_arenas(Arc::clone(&all_arenas));
    checker.ctx.set_all_binders(Arc::clone(&all_binders));
    checker.ctx.set_current_file_idx(entry_idx);
    checker.check_source_file(roots[entry_idx]);

    checker
        .ctx
        .diagnostics
        .iter()
        .map(|d| d.code)
        .collect::<Vec<_>>()
}

/// `ns.Interface = function() {}` in a JS `checkJs` file must NOT emit
/// TS2708 — tsc treats this as a prototype-property assignment merge onto
/// a type-only namespace, which is valid JS salsa.
///
/// Regression test for the `prototypePropertyAssignmentMergeWithInterfaceMethod`
/// conformance case (github.com/google/lovefield crash).
#[test]
fn js_expando_assignment_to_type_only_namespace_member_does_not_emit_ts2708() {
    let codes = diagnostics_for_files(
        &[
            (
                "/lovefield-ts.d.ts",
                r#"
declare namespace lf {
  export interface Transaction {
    commit(): Promise<void>
  }
}
"#,
            ),
            (
                "/lovefield.js",
                r#"
lf.Transaction = function() {};
lf.Transaction.prototype.begin = function(scope) {};
"#,
            ),
        ],
        "/lovefield.js",
    );

    assert!(
        !codes.contains(&2708),
        "TS2708 should be suppressed on JS expando-assignment LHS chains \
         targeting a type-only namespace member, got: {codes:?}"
    );
}

/// The nested variant — `ns.Interface.prototype.method = ...` — exercises the
/// write-target *chain* walk, not just the direct-write case. The base
/// access `ns.Interface` is buried under two further property accesses; the
/// suppression must still apply.
#[test]
fn js_prototype_property_assignment_chain_does_not_emit_ts2708() {
    let codes = diagnostics_for_files(
        &[
            (
                "/ns.d.ts",
                r#"
declare namespace NS {
  export interface Ctor { kind: string }
}
"#,
            ),
            (
                "/impl.js",
                r#"
NS.Ctor.prototype.extra = function() { return 1; };
"#,
            ),
        ],
        "/impl.js",
    );

    assert!(
        !codes.contains(&2708),
        "TS2708 should be suppressed on nested JS prototype-assignment chains, got: {codes:?}"
    );
}

/// Regression guard: the TS2708 suppression is scoped to JS `checkJs`
/// expando-assignment LHS chains. In pure TypeScript, writing to a
/// type-only namespace member must STILL emit TS2708.
#[test]
fn ts_assignment_to_type_only_namespace_member_still_emits_ts2708() {
    let codes = diagnostics_for_files(
        &[(
            "/ts-assign.ts",
            r#"
declare namespace NS {
  export interface I { x: number }
}
NS.I = function() {};
"#,
        )],
        "/ts-assign.ts",
    );

    assert!(
        codes.contains(&2708),
        "TS2708 must still fire in pure TS (no checkJs expando) for \
         assignments to type-only namespace members, got: {codes:?}"
    );
}
