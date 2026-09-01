//! `declare export default <non-identifier>;` (#16403 residual, the last two
//! rows of its 110-row modifier x export-form cross-product).
//!
//! Structural rule: same as the sibling `declare export = <expr>;` fix in
//! `declare_export_equals_ambient_ts2714_tests.rs`, one node kind over. The
//! bare-expression form of `export default` always builds an
//! `EXPORT_DECLARATION` node via `parse_export_default`, which hardcoded
//! `ExportDeclData { modifiers: None, .. }` regardless of whether it was
//! reached through the ambient `declare` dispatch. `is_in_ambient_context`
//! reads a node's own `declare` modifier before walking to its parent, and
//! this statement sits at the source file's own top level with no ambient
//! ancestor either, so the ambient gate in `check_export_assignment`
//! (`crates/tsz-checker/src/declarations/import/core/module_exports.rs`)
//! read `false` and TS2714 never fired — even though the parser's own TS1120
//! ("An export assignment cannot have modifiers.") already did, since that
//! diagnostic is emitted directly at parse time and does not depend on the
//! node's `modifiers` field.
//!
//! `parse_export_default` now accepts the caller's modifiers and threads them
//! onto the wrapper only for the bare-expression case (a class/function/
//! interface/enum/type-alias default export already carries `declare` on its
//! own inner node and is excluded from TS2714 by node kind either way).
//!
//! A default export nested inside a `namespace` body is a second, narrower
//! case: TS1319 ("A default export can only be used in an ECMAScript-style
//! module.") already reports there and tsc's grammar check returns before
//! reaching the ambient-expression check for that same node, so TS2714 must
//! not additionally fire — `check_export_assignment` mirrors that early
//! return.
//!
//! Every expectation oracle-pinned against `typescript@7.0.2`
//! (`--strict --target es2022 --module es2022`).

use crate::test_utils::check_source_codes;

const A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE: u32 = 1319;
const THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT: u32 =
    2714;

#[test]
fn declare_export_default_numeric_literal_reports_ts2714() {
    let codes = check_source_codes("declare export default 1;");
    assert!(
        codes.contains(
            &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
        ),
        "expected TS2714 — the `declare` modifier must reach ambient-context \
         detection on the default-export wrapper node; got {codes:?}"
    );
}

#[test]
fn declare_export_default_string_literal_reports_ts2714() {
    let codes = check_source_codes(r#"declare export default "hello";"#);
    assert!(codes.contains(
        &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
    ));
}

// Negative control: an identifier expression is a valid ambient default
// export target, so TS2714 must not fire.
#[test]
fn declare_export_default_identifier_reports_no_ts2714() {
    let codes = check_source_codes(
        "declare const renamedTarget: number;\ndeclare export default renamedTarget;",
    );
    assert!(!codes.contains(
        &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
    ));
}

// Negative control: a plain (non-ambient) `export default <expr>;` must not
// gain TS2714 — the file is not ambient at all.
#[test]
fn plain_export_default_numeric_literal_reports_no_ts2714() {
    let codes = check_source_codes("export default 1;");
    assert!(!codes.contains(
        &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
    ));
}

// Negative control: a declaration default export (class) stays excluded from
// TS2714 by node kind, ambient or not.
#[test]
fn declare_export_default_class_reports_no_ts2714() {
    let codes = check_source_codes("declare export default class {}");
    assert!(!codes.contains(
        &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
    ));
}

// A default export nested in a namespace body is invalid there (TS1319);
// tsc's grammar check returns before the ambient-expression check runs for
// that same node, so a `declare`d bare-expression default export must report
// TS1319 alone, not TS1319 *and* TS2714.
#[test]
fn declare_export_default_expression_inside_namespace_reports_only_ts1319() {
    let codes = check_source_codes("namespace N { declare export default 1; }");
    assert!(
        codes.contains(&A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE),
        "expected TS1319; got {codes:?}"
    );
    assert!(
        !codes.contains(
            &THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER_OR_QUALIFIED_NAME_IN_AN_AMBIENT_CONTEXT
        ),
        "TS1319 already won the placement check for this node — TS2714 must \
         not additionally fire; got {codes:?}"
    );
}
