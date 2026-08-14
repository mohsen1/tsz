//! Regression coverage for a JSDoc `@type` qualifier rooted at a default
//! import of a plain runtime value.
//!
//! Structural rule: `jsdoc_qualified_root_is_plain_value` (owner:
//! `crates/tsz-checker/src/jsdoc/resolution/name_resolution.rs`) decides
//! whether a bare dotted JSDoc type name `A.B` can use `A` as a namespace
//! qualifier. It always exempted an `ALIAS`-flagged root outright, so a
//! default import (`import MC from './a'`) was never checked against what it
//! actually points at. When the imported value is a plain function/class
//! expression or variable that only grew members via JS expando writes
//! (`MyClass.bar = class C {}`), `tsc` still reports `TS2503` ("Cannot find
//! namespace 'MC'.") for `/** @type {MC.bar} */` — oracle-verified
//! (typescript@7.0.2) against `TypeScript/tests/cases/conformance/salsa/
//! typeFromPropertyAssignment5.ts`. tsz silently accepted it.
//!
//! Fix: follow the import-alias chain (via `resolve_import_alias`) to the
//! value the default import ultimately names before judging plainness. A
//! default export additionally goes through a synthesized `ALIAS |
//! EXPORT_VALUE` placeholder symbol with no `import_module` payload of its
//! own (`crates/tsz-binder/src/modules/import_export.rs`) — `resolve_import_alias`
//! can't follow that hop, so its own declaration list (which reuses the
//! `export default <decl>` clause's node index) is classified directly
//! instead, read from the *declaring* file's arena rather than the checking
//! file's arena (a default export's declaration lives in the exporting
//! file, not the importing one).

use tsz_checker::test_utils::check_multi_file_with_global_index;
use tsz_common::CheckerOptions;

fn opts() -> CheckerOptions {
    CheckerOptions {
        allow_js: true,
        check_js: true,
        strict: true,
        ..CheckerOptions::default()
    }
}

fn codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    check_multi_file_with_global_index(files, entry, opts())
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// The exact `typeFromPropertyAssignment5.ts` shape: a default-exported
/// plain function that grows an expando member, referenced through a
/// default import as a JSDoc namespace qualifier.
#[test]
fn default_imported_plain_function_expando_qualifier_reports_ts2503() {
    let diags = check_multi_file_with_global_index(
        &[
            (
                "a.js",
                "export default function MyClass() {\n}\nMyClass.bar = class C {\n}\nMyClass.bar\n",
            ),
            (
                "b.js",
                "import MC from './a'\nMC.bar\n/** @type {MC.bar} */\nvar x\n",
            ),
        ],
        "b.js",
        opts(),
    );
    let codes: Vec<_> = diags.iter().map(|d| d.code).collect();
    assert_eq!(codes, vec![2503], "expected TS2503, got {diags:?}");
    assert!(
        diags[0].message_text.contains("MC"),
        "TS2503 should name the unresolved root, got {diags:?}"
    );
}

/// Renamed binder: the rule must key on symbol shape, not identifier
/// spelling.
#[test]
fn default_imported_plain_function_expando_qualifier_renamed_binder_reports_ts2503() {
    let got = codes(
        &[
            (
                "lib.js",
                "export default function Widget() {\n}\nWidget.part = class P {\n}\n",
            ),
            (
                "use.js",
                "import Local from './lib'\n/** @type {Local.part} */\nvar y\n",
            ),
        ],
        "use.js",
    );
    assert_eq!(got, vec![2503], "got {got:?}");
}

/// Negative control: a default-imported *class* keeps its namespace
/// meaning — classes are in `member_holder_flags`, so the declaration-shape
/// fallback must never fire for one.
#[test]
fn default_imported_class_qualifier_stays_clean() {
    let got = codes(
        &[
            ("a.js", "export default class Foo {\n  static bar = 1\n}\n"),
            (
                "b.js",
                "import MC from './a'\n/** @type {MC.bar} */\nvar x\n",
            ),
        ],
        "b.js",
    );
    assert!(
        !got.contains(&2503),
        "class-exported default import must not be treated as a plain value, got {got:?}"
    );
}

/// Negative control: `import * as NS` stays exempt — its own declaration is
/// the namespace-import specifier, not a plain-value declaration shape, so
/// the fallback classification conservatively declines rather than reading
/// through into the target module.
#[test]
fn namespace_style_import_qualifier_stays_clean() {
    let got = codes(
        &[
            ("a.js", "export function helper() {}\nhelper.extra = 1\n"),
            (
                "b.js",
                "import * as NS from './a'\n/** @type {NS.extra} */\nvar x\n",
            ),
        ],
        "b.js",
    );
    assert!(
        !got.contains(&2503),
        "namespace-style import qualifier must stay exempt, got {got:?}"
    );
}

/// Named (non-default) import of a plain expando-only value: a single alias
/// hop, no synthetic placeholder — must also report TS2503.
#[test]
fn named_imported_plain_function_expando_qualifier_reports_ts2503() {
    let got = codes(
        &[
            (
                "a.js",
                "export function Helper() {\n}\nHelper.extra = class E {\n}\n",
            ),
            (
                "b.js",
                "import { Helper } from './a'\n/** @type {Helper.extra} */\nvar x\n",
            ),
        ],
        "b.js",
    );
    assert_eq!(got, vec![2503], "got {got:?}");
}
