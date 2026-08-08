//! The removed `assert` import-attribute keyword abandons the whole declaration.
//!
//! In TypeScript 7 the `assert` import-attribute keyword has been removed. `tsc`
//! reports TS2880 at the `assert` keyword and then *abandons the entire
//! declaration* — it never runs the module-support gate (TS2823/TS2821), the
//! type-only attribute check (TS2857/TS2822), the value-literal / assignability
//! checks (TS2858/TS2322), or module resolution (TS2307). This holds at every
//! module mode, so TS2880 takes precedence over all of them. `with` clauses are
//! unaffected and keep their full grammar checks.
//!
//! These are lib-free checks (no `ImportAttributes` interface, no module
//! resolution), so they exercise the grammar-level codes that distinguish the
//! abandon: a type-only or module-unsupported `assert` used to leak
//! TS2857/TS2822/TS2821 alongside (or instead of) TS2880. Oracle-pinned against
//! `typescript@7.0.2`. Binder names are irrelevant here (the rule keys on the
//! `assert` keyword, not any identifier), so nothing can key on a fixed name.

use tsz_checker::context::CheckerOptions;
use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source;
use tsz_common::common::ModuleKind;

fn check_module(source: &str, module: ModuleKind) -> Vec<Diagnostic> {
    check_source(
        source,
        "test.ts",
        CheckerOptions {
            module,
            ..CheckerOptions::default()
        },
    )
}

fn codes(diags: &[Diagnostic]) -> Vec<u32> {
    let mut c: Vec<u32> = diags.iter().map(|d| d.code).collect();
    c.sort_unstable();
    c
}

/// The attribute-grammar diagnostic codes that an `assert` clause must abandon.
const IMPORT_ATTRIBUTE_GRAMMAR_CODES: [u32; 6] = [2821, 2822, 2823, 2836, 2857, 2858];

#[test]
fn import_assert_type_only_abandons_no_ts2822() {
    // `import type X ... assert { ... }`: main leaked TS2822 (type-only assert)
    // from the grammar check that ran after TS2880. The abandon suppresses it.
    let c = codes(&check_module(
        r#"import type X from "./x.json" assert { type: "json" };"#,
        ModuleKind::ESNext,
    ));
    assert!(c.contains(&2880), "expected TS2880, got: {c:?}");
    assert!(
        !c.contains(&2822),
        "TS2822 must be abandoned under the removed `assert` keyword, got: {c:?}"
    );
}

#[test]
fn import_assert_module_unsupported_reports_ts2880_not_ts2821() {
    // Under `commonjs` (attributes unsupported) the module-support gate would
    // fire TS2821 for `assert` first. TS2880 takes precedence and abandons, so
    // TS2821 must not appear.
    let c = codes(&check_module(
        r#"import a from "./x.json" assert { type: "json" };"#,
        ModuleKind::CommonJS,
    ));
    assert!(c.contains(&2880), "expected TS2880, got: {c:?}");
    assert!(
        !c.contains(&2821),
        "TS2821 must be superseded by TS2880 under `assert`, got: {c:?}"
    );
}

#[test]
fn import_assert_esnext_leaves_only_ts2880_grammar() {
    // The full abandon: no import-attribute grammar code other than TS2880
    // survives on an `assert` clause.
    let c = codes(&check_module(
        r#"import a from "./x.json" assert { type: "json" };"#,
        ModuleKind::ESNext,
    ));
    assert!(c.contains(&2880), "expected TS2880, got: {c:?}");
    for code in IMPORT_ATTRIBUTE_GRAMMAR_CODES {
        assert!(
            !c.contains(&code),
            "TS{code} must be abandoned under the removed `assert` keyword, got: {c:?}"
        );
    }
}

#[test]
fn import_assert_hard_error_mode_leaves_only_ts2880() {
    // `node20`/`nodenext` were tsz's only pre-fix "hard error" modes, yet they
    // still over-reported because the assignability check ran before the grammar
    // check. The abandon covers them too.
    for module in [ModuleKind::Node20, ModuleKind::NodeNext] {
        let c = codes(&check_module(
            r#"import a from "./x.json" assert { type: "json" };"#,
            module,
        ));
        assert!(
            c.contains(&2880),
            "expected TS2880 for {module:?}, got: {c:?}"
        );
        for code in IMPORT_ATTRIBUTE_GRAMMAR_CODES {
            assert!(
                !c.contains(&code),
                "TS{code} must be abandoned for {module:?}, got: {c:?}"
            );
        }
    }
}

#[test]
fn export_star_assert_module_unsupported_reports_ts2880_not_ts2821() {
    // The export path abandons identically to the import path.
    let c = codes(&check_module(
        r#"export * from "./x.json" assert { type: "json" };"#,
        ModuleKind::CommonJS,
    ));
    assert!(c.contains(&2880), "expected TS2880, got: {c:?}");
    assert!(
        !c.contains(&2821),
        "TS2821 must be superseded by TS2880 under `assert` on exports, got: {c:?}"
    );
}

#[test]
fn export_named_assert_type_only_abandons_no_ts2822() {
    let c = codes(&check_module(
        r#"export type { X } from "./x.json" assert { type: "json" };"#,
        ModuleKind::ESNext,
    ));
    assert!(c.contains(&2880), "expected TS2880, got: {c:?}");
    assert!(
        !c.contains(&2822),
        "TS2822 must be abandoned under `assert` on exports, got: {c:?}"
    );
}

/// Negative control: `with` clauses are untouched — a type-only `with` still
/// reports TS2857, and none of it turns into TS2880.
#[test]
fn import_with_type_only_still_reports_ts2857_not_ts2880() {
    let c = codes(&check_module(
        r#"import type X from "./x.json" with { type: "json" };"#,
        ModuleKind::ESNext,
    ));
    assert!(
        c.contains(&2857),
        "`with` on a type-only import must still report TS2857, got: {c:?}"
    );
    assert!(
        !c.contains(&2880),
        "`with` must never report TS2880, got: {c:?}"
    );
}

/// Negative control: `with` under an unsupported module still reports TS2823.
#[test]
fn import_with_module_unsupported_still_reports_ts2823_not_ts2880() {
    let c = codes(&check_module(
        r#"import a from "./x.json" with { type: "json" };"#,
        ModuleKind::CommonJS,
    ));
    assert!(
        c.contains(&2823),
        "`with` under commonjs must still report TS2823, got: {c:?}"
    );
    assert!(
        !c.contains(&2880),
        "`with` must never report TS2880, got: {c:?}"
    );
}
