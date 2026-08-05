//! #16403's three remaining mismatches left open by #16452 (the `async`
//! slice): `declare export namespace N {}`, `declare export = <expr>`, and
//! `abstract export = <expr>`.
//!
//! **`declare export namespace N {}` missed TS1029 entirely.** The parser's
//! TS1029 gate (`state_declarations.rs`'s `parse_ambient_declaration_with_modifiers`,
//! `SyntaxKind::ExportKeyword` arm) carried a stale exclusion for
//! `ModuleKeyword`/`NamespaceKeyword`, commented as "tsc 6.0 accepts this form
//! without TS1029" — measured false against the pinned `typescript@7.0.2`
//! oracle: a `declare export namespace` at the source file's own top level or
//! inside a namespace body reports TS1029 like every other `declare export
//! <declaration>` form; only a Block silences it (there, the nested module
//! declaration's own TS1235 wins, oracle-confirmed, itself unaffected by this
//! fix since it is checker-side).
//!
//! **`declare export = <expr>` and `abstract export = <expr>` both picked a
//! wrong code or position.** The root cause is one structural bug shared by
//! every modifier family: `parse_export_assignment` hard-coded
//! `ExportAssignmentData.modifiers: None`, so no modifier-prefixed `export =`
//! node ever carried its `declare` (or any other) modifier. Two independent
//! checker checks read that field through `is_in_ambient_context`'s walk over
//! `get_declaration_modifiers`:
//!
//! - TS1203 ("Export assignment cannot be used when targeting ECMAScript
//!   modules") is suppressed for an *ambient* `export =`; without the
//!   `declare` modifier attached, `declare export = 1;` incorrectly kept
//!   TS1203 instead of suppressing it, and the ambient-only TS2714 ("The
//!   expression of an export assignment must be an identifier or qualified
//!   name in an ambient context") never fired at all.
//! - Separately, `parse_accessor_modified_statement`'s `ExportKeyword` arm
//!   (shared by every modifier family reaching it — `static`/`readonly`/
//!   `public`/`protected`/`private`/`abstract`) delegated `export = <expr>`
//!   straight to `parse_export_declaration()`, which recomputes its own
//!   `start_pos` from the *current* `export` token and drops every modifier
//!   collected so far — so TS1203's node-anchored position landed on
//!   `export`, not on the modifier that actually starts the statement
//!   (oracle-confirmed: tsc anchors both TS1044/TS1242-family and TS1203 on
//!   the modifier, not on `export`).
//!
//! `abstract` additionally never recognized `export =` as a target at all
//! (`look_ahead_abstract_before_export_target`'s comment called it "routed
//! through TS1120" — that claim does not hold against the oracle either:
//! `abstract` gets its own per-modifier TS1242, not the generic TS1120
//! `declare`/`export export =` share), so `abstract export = 1;` degraded
//! `abstract` to a bare identifier expression (dropping TS1242 outright) and
//! then re-parsed `export = 1;` as an unrelated, unmodified top-level
//! statement.
//!
//! All expectations measured directly against the pinned `typescript@7.0.2`
//! oracle (`scripts/conformance/typescript-versions.json`),
//! `--noEmit --strict --pretty false --target es2022 --module es2022`.

use crate::context::CheckerOptions;
use crate::test_utils::{
    DiagnosticCodePositions, check_source_with_grammar_only_parse_health_positions,
};
use tsz_common::common::ModuleKind;

const MODIFIER_MUST_PRECEDE_MODIFIER: u32 = 1029;
const A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL: u32 = 1235;
const AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS: u32 = 1120;
const AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL: u32 = 1231;
const AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE: u32 = 1063;
const ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION: u32 = 1242;
const EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES: u32 = 1203;
const THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER: u32 = 2714;
const MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT: u32 = 1044;

fn diagnostics(source: &str) -> (DiagnosticCodePositions, DiagnosticCodePositions) {
    let options = CheckerOptions {
        module: ModuleKind::ES2022,
        ..CheckerOptions::default()
    };
    check_source_with_grammar_only_parse_health_positions(source, options)
}

fn all_codes(source: &str) -> Vec<u32> {
    let (parse, chk) = diagnostics(source);
    parse.into_iter().chain(chk).map(|(code, _)| code).collect()
}

// -- `declare export namespace N {}`: oracle TS1029 alone at top level and
//    inside a namespace body; a Block silences it (the nested module
//    declaration's own TS1235 wins instead, checker-side and unaffected). --

#[test]
fn declare_export_namespace_reports_ts1029_at_top_level() {
    let source = "declare export namespace N {}";
    let (parse, chk) = diagnostics(source);
    let export_start = source.find("export").unwrap() as u32;
    assert_eq!(
        parse,
        vec![(MODIFIER_MUST_PRECEDE_MODIFIER, export_start)],
        "expected TS1029 alone anchored on `export`, got parse={parse:?} checker={chk:?}"
    );
    assert!(chk.is_empty(), "unexpected checker diagnostics: {chk:?}");
}

#[test]
fn declare_export_namespace_reports_ts1029_in_a_namespace_body() {
    let source = "namespace M { declare export namespace N {} }";
    let (parse, chk) = diagnostics(source);
    let export_start = source.find("export").unwrap() as u32;
    assert_eq!(
        parse,
        vec![(MODIFIER_MUST_PRECEDE_MODIFIER, export_start)],
        "expected TS1029 alone anchored on `export`, got parse={parse:?} checker={chk:?}"
    );
    assert!(chk.is_empty(), "unexpected checker diagnostics: {chk:?}");
}

#[test]
fn declare_export_namespace_in_a_block_yields_to_ts1235_alone() {
    let source = "function f() { declare export namespace N {} }";
    let (parse, chk) = diagnostics(source);
    assert!(
        parse.is_empty(),
        "a Block must not additionally gain TS1029: {parse:?}"
    );
    let declare_start = source.find("declare").unwrap() as u32;
    assert_eq!(
        chk,
        vec![(
            A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL,
            declare_start
        )],
        "expected TS1235 alone anchored on the modifier run's start, got {chk:?}"
    );
}

// -- `declare export = <expr>`: oracle TS1120 + TS2714 at top level (the
//    ambient `declare` suppresses TS1203 and enables the ambient-expression
//    check instead); TS1231/TS1063 alone in a Block / namespace body. --

#[test]
fn declare_export_equals_reports_ts1120_and_ts2714_at_top_level() {
    let source = "declare export = 1;";
    let (parse, chk) = diagnostics(source);
    assert_eq!(
        parse,
        vec![(AN_EXPORT_ASSIGNMENT_CANNOT_HAVE_MODIFIERS, 0)],
        "expected TS1120 anchored at the statement's own start, got {parse:?}"
    );
    let expr_start = source.find('1').unwrap() as u32;
    assert_eq!(
        chk,
        vec![(
            THE_EXPRESSION_OF_AN_EXPORT_ASSIGNMENT_MUST_BE_AN_IDENTIFIER,
            expr_start
        )],
        "expected TS2714 alone (ambient `declare` must suppress TS1203), got {chk:?}"
    );
}

#[test]
fn declare_export_equals_in_a_block_yields_to_ts1231_alone() {
    let source = "function f() { declare export = 1; }";
    let (parse, chk) = diagnostics(source);
    assert!(parse.is_empty(), "unexpected parse diagnostics: {parse:?}");
    let declare_start = source.find("declare").unwrap() as u32;
    assert_eq!(
        chk,
        vec![(AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL, declare_start)],
        "expected TS1231 alone anchored on the modifier run's start, got {chk:?}"
    );
}

#[test]
fn declare_export_equals_in_a_namespace_body_yields_to_ts1063_alone() {
    let source = "namespace M { declare export = 1; }";
    let (parse, chk) = diagnostics(source);
    assert!(parse.is_empty(), "unexpected parse diagnostics: {parse:?}");
    let declare_start = source.find("declare").unwrap() as u32;
    assert_eq!(
        chk,
        vec![(
            AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE,
            declare_start
        )],
        "expected TS1063 alone anchored on the modifier run's start, got {chk:?}"
    );
}

// -- `abstract export = <expr>`: oracle TS1242 + TS1203 at top level (both
//    anchored on `abstract`, not `export`); TS1231/TS1063 alone in a Block /
//    namespace body, same silencing shape as `abstract export default`. --

#[test]
fn abstract_export_equals_reports_ts1242_and_ts1203_at_top_level() {
    let source = "abstract export = 1;";
    let (parse, chk) = diagnostics(source);
    assert_eq!(
        parse,
        vec![(
            ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
            0
        )],
        "expected TS1242 anchored on `abstract`, got {parse:?}"
    );
    assert_eq!(
        chk,
        vec![(
            EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES,
            0
        )],
        "expected TS1203 anchored on the statement's own start (not `export`), got {chk:?}"
    );
}

#[test]
fn abstract_export_equals_in_a_block_yields_to_ts1231_alone() {
    let source = "function f() { abstract export = 1; }";
    let (parse, chk) = diagnostics(source);
    assert!(parse.is_empty(), "unexpected parse diagnostics: {parse:?}");
    let abstract_start = source.find("abstract").unwrap() as u32;
    assert_eq!(
        chk,
        vec![(
            AN_EXPORT_ASSIGNMENT_MUST_BE_AT_THE_TOP_LEVEL,
            abstract_start
        )],
        "expected TS1231 alone anchored on the modifier run's start, got {chk:?}"
    );
}

#[test]
fn abstract_export_equals_in_a_namespace_body_yields_to_ts1063_alone() {
    let source = "namespace M { abstract export = 1; }";
    let (parse, chk) = diagnostics(source);
    assert!(parse.is_empty(), "unexpected parse diagnostics: {parse:?}");
    let abstract_start = source.find("abstract").unwrap() as u32;
    assert_eq!(
        chk,
        vec![(
            AN_EXPORT_ASSIGNMENT_CANNOT_BE_USED_IN_A_NAMESPACE,
            abstract_start
        )],
        "expected TS1063 alone anchored on the modifier run's start, got {chk:?}"
    );
}

// -- The shared `parse_accessor_modified_statement` fix benefits every
//    modifier family reaching it, not just `abstract` — `static`'s own
//    TS1044 + TS1203 pair must anchor on `static`, not `export`, exactly the
//    same way. --

#[test]
fn static_export_equals_anchors_both_diagnostics_on_static_not_export() {
    let source = "static export = 1;";
    let (parse, chk) = diagnostics(source);
    assert_eq!(
        parse,
        vec![(MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT, 0)],
        "expected TS1044 anchored on `static`, got {parse:?}"
    );
    assert_eq!(
        chk,
        vec![(
            EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES,
            0
        )],
        "expected TS1203 anchored on the statement's own start (not `export`), got {chk:?}"
    );
}

// -- Negative controls: shapes this fix must leave exactly as they were. --

#[test]
fn plain_export_equals_is_unaffected() {
    let source = "const a = {}; export = a;";
    let (parse, chk) = diagnostics(source);
    assert!(parse.is_empty());
    let export_start = source.find("export").unwrap() as u32;
    assert_eq!(
        chk,
        vec![(
            EXPORT_ASSIGNMENT_CANNOT_BE_USED_WHEN_TARGETING_ECMASCRIPT_MODULES,
            export_start
        )]
    );
}

#[test]
fn abstract_export_default_expression_is_unaffected() {
    // A pre-existing, already-correct sibling shape (no `=`, so it never
    // touches the new `EqualsToken` lookahead arm or the export-assignment
    // routing fix): still TS1242 alone, anchored on `abstract`.
    assert_eq!(
        all_codes("abstract export default 1;"),
        vec![ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION]
    );
}

#[test]
fn abstract_export_class_is_unaffected() {
    // `abstract` is legal on a class — still TS1029 alone, anchored on
    // `export`, the pre-existing #16389 answer.
    let source = "abstract export class C {}";
    let (parse, chk) = diagnostics(source);
    let export_start = source.find("export").unwrap() as u32;
    assert_eq!(parse, vec![(MODIFIER_MUST_PRECEDE_MODIFIER, export_start)]);
    assert!(chk.is_empty());
}
