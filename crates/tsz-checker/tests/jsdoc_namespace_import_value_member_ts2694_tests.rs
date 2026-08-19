//! TS2694 for a JSDoc qualified type name whose intermediate qualifier is a
//! value-only export of a namespace import.
//!
//! A qualified type name `s.n.K` (root `s` a namespace import
//! `import * as s from './mod'`) must have namespace/type meaning at every
//! *non-terminal* qualifier. When `exports.n = {}` makes `n` a plain value
//! (tsc 7.0.2 no longer grows a nested namespace under a CommonJS export
//! member — the value-space side of this is covered by
//! `commonjs_module_export_alias_tests.rs`), the type reference `s.n.K` cannot
//! qualify `.K` off the value `n`, so tsc reports
//! `TS2694 "Namespace '"mod"' has no exported member 'n'."` anchored at `n`.
//!
//! A terminal segment used with type meaning (`s.Classic`, a class) stays
//! clean — only intermediate qualifiers are rejected. Oracle-verified against
//! pinned `typescript@7.0.2` (conformance fixtures
//! `salsa/exportNestedNamespaces.ts` / `salsa/moduleExportNestedNamespaces.ts`).

use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::check_multi_file;
use tsz_common::common::ModuleKind;

const TS2694: u32 = 2694;

fn check_js(files: &[(&str, &str)], entry: &str) -> Vec<(u32, String)> {
    check_multi_file(
        files,
        entry,
        CheckerOptions {
            allow_js: true,
            check_js: true,
            module: ModuleKind::ESNext,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect()
}

/// The `module.exports.n = {}` fixture shape: `s.n.K` in a `@param` type has a
/// value-only intermediate qualifier `n`, so it is TS2694, named by the
/// resolved module `"mod"`, anchored at `n`. The terminal class member
/// `s.Classic` stays clean.
#[test]
fn namespace_import_value_only_qualifier_reports_ts2694() {
    let diags = check_js(
        &[
            (
                "mod.js",
                "module.exports.n = {};\n\
                 module.exports.n.K = function C() {\n    this.x = 10;\n}\n\
                 module.exports.Classic = class {\n    constructor() {\n        this.p = 1\n    }\n}\n",
            ),
            (
                "use.js",
                "import * as s from './mod'\n\n\
                 /** @param {s.n.K} c\n    @param {s.Classic} classic */\n\
                 function f(c, classic) {\n    c.x\n    classic.p\n}\n",
            ),
        ],
        "use.js",
    );
    let ts2694: Vec<_> = diags.iter().filter(|(code, _)| *code == TS2694).collect();
    assert_eq!(
        ts2694.len(),
        1,
        "exactly one TS2694 for the value-only qualifier `n`; got: {diags:#?}"
    );
    assert_eq!(
        ts2694[0].1, "Namespace '\"mod\"' has no exported member 'n'.",
        "TS2694 must name the resolved module `\"mod\"` and the failing member `n`; got: {diags:#?}"
    );
}

/// Binder names must not matter — the same shape with a renamed import alias
/// and renamed value member is the same TS2694.
#[test]
fn namespace_import_value_only_qualifier_renamed_binders() {
    let diags = check_js(
        &[
            (
                "shape.js",
                "exports.inner = {};\nexports.inner.Make = function () {};\n",
            ),
            (
                "consumer.js",
                "import * as ns from './shape'\n\n\
                 /** @param {ns.inner.Make} v */\nfunction g(v) { return v; }\n",
            ),
        ],
        "consumer.js",
    );
    let ts2694: Vec<_> = diags.iter().filter(|(code, _)| *code == TS2694).collect();
    assert_eq!(
        ts2694.len(),
        1,
        "renamed shape still TS2694; got: {diags:#?}"
    );
    assert_eq!(
        ts2694[0].1, "Namespace '\"shape\"' has no exported member 'inner'.",
        "{diags:#?}"
    );
}

/// A terminal class member of a namespace import (`s.Classic`) is a valid type
/// and must stay clean — the fix rejects only *intermediate* value qualifiers.
#[test]
fn namespace_import_terminal_class_member_is_clean() {
    let diags = check_js(
        &[
            (
                "mod.js",
                "module.exports.Classic = class {\n    constructor() {\n        this.p = 1\n    }\n}\n",
            ),
            (
                "use.js",
                "import * as s from './mod'\n\n\
                 /** @param {s.Classic} classic */\nfunction f(classic) { return classic.p; }\n",
            ),
        ],
        "use.js",
    );
    assert!(
        !diags.iter().any(|(code, _)| *code == TS2694),
        "a terminal class member of a namespace import must not be TS2694; got: {diags:#?}"
    );
}

/// A genuinely valid nested type qualifier (a real namespace member holding a
/// type) must not be falsely rejected: `s.NS.T` where `NS` is a namespace and
/// `T` a type stays clean.
#[test]
fn namespace_import_valid_nested_namespace_qualifier_is_clean() {
    let diags = check_multi_file(
        &[
            (
                "lib.ts",
                "export namespace NS {\n  export interface T { a: number }\n}\n",
            ),
            (
                "use.js",
                "import * as s from './lib'\n\n\
                 /** @param {s.NS.T} t */\nfunction f(t) { return t.a; }\n",
            ),
        ],
        "use.js",
        CheckerOptions {
            allow_js: true,
            check_js: true,
            module: ModuleKind::ESNext,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .map(|d| (d.code, d.message_text))
    .collect::<Vec<_>>();
    assert!(
        !diags.iter().any(|(code, _)| *code == TS2694),
        "a valid nested namespace type qualifier must not be TS2694; got: {diags:#?}"
    );
}
