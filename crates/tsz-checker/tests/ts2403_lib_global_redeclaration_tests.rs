//! Regression coverage for #15913: redeclaring a lib global whose symbol
//! merges a VALUE declaration (`declare var X: XConstructor`) with a
//! same-named TYPE declaration (`interface X { ... }`) must compare the
//! new declaration's type against the *value* declaration's type — not
//! materialize a garbage type by resolving the wrong declaration in the
//! merged symbol's declaration list through a raw, colliding identity.
//!
//! `crates/tsz-checker/src/state/variable_checking/core.rs`'s TS2403 check
//! looks up prior lib declarations by name across each lib file's own
//! binder. Every lib binder's symbols share the `u32::MAX`
//! declaration-file sentinel, so resolving such a symbol's type through an
//! ad-hoc child checker scoped to one lib binder can collide with an
//! unrelated def a *different* lib binder registered under the same raw
//! `SymbolId` (same family as #13862/#15778). It also has to skip the
//! TYPE-side (`interface`/`type alias`) and namespace declarations that
//! share the merged symbol's declaration list, since only the VALUE
//! declaration is a "prior declaration" for TS2403.
//!
//! `load_default_lib_files` + `check_source_with_libs` load each lib file
//! into its own `LibFile` (own arena/binder), matching the multi-lib-context
//! shape the CLI driver produces — `check_source`'s single merged-lib-file
//! harness does not exercise this code path at all, so it cannot regression
//! test this fix.

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{check_source_with_libs, load_default_lib_files};

fn ts2403_messages(source: &str) -> Vec<String> {
    let libs = load_default_lib_files();
    check_source_with_libs(source, "test.ts", CheckerOptions::default(), &libs)
        .iter()
        .filter(|d| d.code == 2403)
        .map(|d| d.message_text.clone())
        .collect()
}

/// `declare var Symbol: SymbolConstructor;` in lib. Redeclaring it with an
/// incompatible type must report the real lib type, not an unrelated lib
/// entity's name (the original bug: `must be of type 'blur'`).
#[test]
fn incompatible_redeclaration_of_symbol_reports_symbol_constructor() {
    let messages = ts2403_messages("var Symbol: any;");
    assert_eq!(messages.len(), 1, "Expected one TS2403: {messages:#?}");
    assert!(
        messages[0].contains("SymbolConstructor"),
        "Expected the prior type to render as 'SymbolConstructor', got: {}",
        messages[0]
    );
}

/// `Array`, `Math`, and `JSON` are each a merged TYPE+VALUE lib symbol
/// (`declare var Array: ArrayConstructor;` alongside `interface Array<T>
/// {...}`, `Math`/`JSON` alongside their own same-named interfaces).
/// Redeclaring each with its own correct value type must be a no-op — tsc
/// emits nothing for the ordinary `declare var X: X` ambient-shim idiom.
#[test]
fn redeclaring_merged_lib_globals_with_correct_type_emits_no_ts2403() {
    let messages = ts2403_messages(
        r#"
var Array: ArrayConstructor;
var Math: Math;
var JSON: JSON;
var Promise: PromiseConstructor;
"#,
    );
    assert!(
        messages.is_empty(),
        "declare var X: X for a merged lib global must not report TS2403: {messages:#?}"
    );
}

/// The same merged lib globals, redeclared with an INCOMPATIBLE type, must
/// still report TS2403 against their real value type (not a garbage
/// unrelated lib entity — the collision family this fix removes).
#[test]
fn redeclaring_merged_lib_globals_with_wrong_type_reports_real_type() {
    let cases: &[(&str, &str)] = &[
        ("var Array: any;", "ArrayConstructor"),
        ("var Math: any;", "Math"),
        ("var JSON: any;", "JSON"),
        ("var Promise: any;", "PromiseConstructor"),
    ];
    for (source, expected_type_name) in cases {
        let messages = ts2403_messages(source);
        assert_eq!(
            messages.len(),
            1,
            "Expected one TS2403 for `{source}`: {messages:#?}"
        );
        assert!(
            messages[0].contains(expected_type_name),
            "Expected `{source}`'s prior type to render as '{expected_type_name}', got: {}",
            messages[0]
        );
    }
}

/// `Reflect` is a `declare namespace Reflect { ... }` global (no
/// `VariableDeclaration` at all), a different declaration-merging shape
/// than the `interface`+`var` family above. This pass does not attempt to
/// synthesize the namespace's "typeof Reflect" value type, so it must skip
/// the declaration rather than materialize a garbage type for it.
#[test]
fn redeclaring_namespace_shaped_lib_global_emits_no_garbage_ts2403() {
    let messages = ts2403_messages("var Reflect: any;");
    assert!(
        messages
            .iter()
            .all(|m| !m.contains("must be of type 'void'")),
        "Must not report a collided placeholder type for a namespace-shaped \
         lib global: {messages:#?}"
    );
}
