//! A stray modifier keyword immediately before an `export` statement.
//!
//! `parse_statement_top_level_modifier` used to drop the modifier silently for
//! *every* `modifier export ...` shape. tsc instead attaches the modifier to
//! whichever node the `export` begins and lets `checkGrammarModifiers` answer
//! by that node's own kind, which splits three ways:
//!
//! - `export as namespace Foo;` is a `NamespaceExportDeclaration`, which admits
//!   no modifiers in any container — TS1184 in a Block, in a namespace body,
//!   and at the source file's own top level alike.
//! - `export const` / `export class` / `export function` / ... is an ordinary
//!   modified declaration, so it takes the container split #16368/#16375 built
//!   for the sibling modifiers: TS1184 in a Block, TS1044 elsewhere.
//! - `export {}` / `export * from` / `export =` / `export default` draw their
//!   own placement diagnostic (TS1233 / TS1231 / TS1258) and no modifier
//!   diagnostic at all, so the silent drop was right for those three.
//!
//! Every row below is pinned against `typescript@7.0.2`
//! (`--noEmit --strict --lib es2022 --target es2022`).
//!
//! Note on the `as namespace` rows: tsc reports TS1184 *and* the placement
//! diagnostic (TS1316 / TS1314) that the checker raises for the export itself.
//! The second half is a checker diagnostic and so is out of this file's reach;
//! at the CLI it is currently suppressed for the whole file by
//! `has_syntax_parse_errors`, which #16367 removes for exactly this class of
//! code. Verified locally: with #16367's containment applied, these sources
//! report the full tsc pair.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn diagnostic_codes_at(source: &str, needle: &str) -> Vec<u32> {
    let pos = source.find(needle).unwrap() as u32;
    let (parser, _root) = parse_source(source);
    parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.start == pos)
        .map(|d| d.code)
        .collect()
}

fn assert_only_diagnostic(source: &str, needle: &str, expected_code: u32) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(
        diagnostic_codes_at(source, needle),
        vec![expected_code],
        "expected exactly TS{expected_code} anchored on {needle:?} for {source:?}, \
         got {diagnostics:?}"
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one parser diagnostic for {source:?}, got {diagnostics:?}"
    );
}

fn assert_no_parser_diagnostics(source: &str) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "expected no parser diagnostic for {source:?} — tsc reports only the \
         export form's own placement diagnostic, which is a checker check; \
         got {diagnostics:?}"
    );
}

// --------------------------------------------------------------------------
// `export as namespace` — TS1184 in every container.
// --------------------------------------------------------------------------

#[test]
fn modifier_before_export_as_namespace_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  public export as namespace Telemetry;\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

/// The container that distinguishes this rule from the sibling-modifier one:
/// a modified *declaration* here would be TS1044, not TS1184.
#[test]
fn modifier_before_export_as_namespace_at_top_level_reports_ts1184_not_ts1044() {
    assert_only_diagnostic(
        "public export as namespace Telemetry;",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

/// The other non-Block container, for the same reason.
#[test]
fn modifier_before_export_as_namespace_in_namespace_body_reports_ts1184_not_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export as namespace Telemetry;\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn modifier_before_export_as_namespace_in_nested_block_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  { public export as namespace Telemetry; }\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn modifier_before_export_as_namespace_in_class_static_block_reports_ts1184() {
    assert_only_diagnostic(
        "class Registry { static { public export as namespace Telemetry; } }",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

/// The answer is keyed on the node the `export` begins, never on which modifier
/// was written, so the whole modifier set lands on TS1184 here — including the
/// ones whose own misplacement message (TS1044 for `public`) would differ in the
/// sibling-declaration case.
///
/// `abstract` is deliberately absent: it never reaches this dispatch at all.
/// `look_ahead_is_abstract_before_var_or_function` matches only
/// `var`/`let`/`const`/`function`, so `abstract export as namespace Foo;` falls
/// through to the expression-statement path and reports nothing, where tsc
/// reports the same TS1184 as its siblings. That is the `abstract`-has-its-own
/// dispatcher family (#16380) rather than this one, and it is filed separately.
/// `declare` is absent for the same reason, via `parse_statement_declare_keyword`.
///
/// `public` and `static` are the two rows pinned directly against
/// `typescript@7.0.2`; `private`/`protected` share their dispatch exactly.
#[test]
fn every_modifier_before_export_as_namespace_reports_ts1184() {
    for modifier in ["public", "private", "protected", "static"] {
        let source =
            format!("function collect() {{\n  {modifier} export as namespace Telemetry;\n}}");
        assert_eq!(
            diagnostic_codes_at(&source, modifier),
            vec![diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE],
            "modifier {modifier:?} must not change the answer — it is the \
             NamespaceExportDeclaration node kind that admits no modifiers"
        );
    }
}

/// Renamed binder, to pin that nothing keys on the namespace's name.
#[test]
fn export_as_namespace_diagnostic_does_not_depend_on_the_namespace_name() {
    assert_only_diagnostic(
        "function gather() {\n  static export as namespace QuiteDifferentName;\n}",
        "static",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

// --------------------------------------------------------------------------
// `export <declaration>` — the ordinary container split.
// --------------------------------------------------------------------------

#[test]
fn modifier_before_export_const_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export const seeded = 1;",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_const_in_namespace_body_reports_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export const seeded = 1;\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_class_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export class Widget {}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

/// The message interpolates the modifier that was actually written, so the
/// non-`public` members of the set have to be exercised too.
#[test]
fn modifier_before_export_function_at_top_level_reports_ts1044_naming_that_modifier() {
    let source = "static export function build() {}";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(diagnostics.len(), 1, "got {diagnostics:?}");
    assert_eq!(
        diagnostics[0].code,
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT
    );
    assert!(
        diagnostics[0].message.contains("'static'"),
        "the TS1044 message names the modifier that was written; got {:?}",
        diagnostics[0].message
    );
}

/// In a Block the same shape is TS1184 — this half already worked and is the
/// regression guard on the arm that now falls through to the container gate.
#[test]
fn modifier_before_export_const_in_function_body_still_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  public export const seeded = 1;\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

// --------------------------------------------------------------------------
// `export {}` / `export *` / `export =` / `export default` — no modifier
// diagnostic. These are the rows a container-gate-everything fix would break.
// --------------------------------------------------------------------------

#[test]
fn modifier_before_export_list_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  public export {};\n}");
}

#[test]
fn modifier_before_export_star_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  public export * from \"./source\";\n}");
}

#[test]
fn modifier_before_export_assignment_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  public export = 1;\n}");
}

#[test]
fn modifier_before_export_default_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  public export default 1;\n}");
}

// --------------------------------------------------------------------------
// `export {}` / `export *` / `export =` / `export default` outside a Block —
// the container-gate-everything fix WAS right, just not unconditionally.
// tsc's grammar check still reaches the modifier at the source file's own
// top level (all four forms) and inside a namespace body (`{}`/`*` only —
// `=`/`default` stay silent there too, since their own placement diagnostic,
// TS1063/TS1319, wins the same way TS1231/TS1258 wins in a Block). Every row
// pinned against `typescript@7.0.2` (#16403 slice 1).
// --------------------------------------------------------------------------

#[test]
fn modifier_before_export_list_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export {};",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_star_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export * from \"./source\";",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_assignment_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export = 1;",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_default_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export default 1;",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_list_in_namespace_body_reports_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export {};\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_star_in_namespace_body_reports_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export * from \"./source\";\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

/// Unlike the declaration forms above, `export =` stays silent in a
/// namespace body — its own TS1063 ("An export assignment cannot be used in
/// a namespace.") wins there too, not just in a Block.
#[test]
fn modifier_before_export_assignment_in_namespace_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("namespace Outer {\n  public export = 1;\n}");
}

/// Same reasoning for `export default` (its own TS1319 wins).
#[test]
fn modifier_before_export_default_in_namespace_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("namespace Outer {\n  public export default 1;\n}");
}

/// The message interpolates the modifier written, so the non-`public` members
/// of the set have to be exercised too, same as the plain-declaration form.
#[test]
fn modifier_before_export_list_at_top_level_reports_ts1044_naming_that_modifier() {
    let source = "static export {};";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(diagnostics.len(), 1, "got {diagnostics:?}");
    assert_eq!(
        diagnostics[0].code,
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT
    );
    assert!(
        diagnostics[0].message.contains("'static'"),
        "the TS1044 message names the modifier that was written; got {:?}",
        diagnostics[0].message
    );
}

// --------------------------------------------------------------------------
// `export namespace N {}` / `export module M {}` — a nested module
// declaration is itself illegal in a Block (TS1235), independent of any
// modifier, so that placement diagnostic wins there and the modifier is
// dropped — unlike the sibling declaration forms (`const`/`class`/...),
// which stay legal in a Block and keep the generic TS1184.
// --------------------------------------------------------------------------

/// TS1235 ("A namespace declaration is only allowed at the top level of a
/// namespace or module.") is a checker check, out of this parser-only
/// harness's reach — same as the `export {}`/`export *`/`export =`/
/// `export default` placement diagnostics above. What the parser controls is
/// TS1184 no longer piling on top of it (pinned at the CLI level against
/// `typescript@7.0.2`: TS1235 alone, not TS1184 + TS1235).
#[test]
fn modifier_before_export_namespace_declaration_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  public export namespace N {}\n}");
}

/// Outside a Block a nested namespace/module still nests validly, so this is
/// an ordinary modified declaration and keeps TS1044 — regression guard on
/// the arm that now special-cases the Block container only.
#[test]
fn modifier_before_export_namespace_declaration_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export namespace N {}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_namespace_declaration_in_namespace_body_reports_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export namespace N {}\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

// --------------------------------------------------------------------------
// `readonly` — TS1024, oracle-pinned to the exact same container/form
// silencing shape as the TS1044 family (#16403 slice 2).
// --------------------------------------------------------------------------

#[test]
fn readonly_before_export_list_at_top_level_reports_ts1024() {
    assert_only_diagnostic(
        "readonly export {};",
        "readonly",
        diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
    );
}

#[test]
fn readonly_before_export_list_in_function_body_reports_only_ts1233() {
    // Silenced the same way the TS1044 family is: the export declaration's
    // own placement diagnostic wins inside a Block and the modifier is
    // dropped silently.
    assert_no_parser_diagnostics("function f() {\n  readonly export {};\n}");
}

#[test]
fn readonly_before_export_assignment_in_namespace_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("namespace Outer {\n  readonly export = 1;\n}");
}

#[test]
fn readonly_before_export_class_at_top_level_reports_ts1024() {
    assert_only_diagnostic(
        "readonly export class C {}",
        "readonly",
        diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
    );
}

#[test]
fn readonly_before_export_class_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function f() {\n  readonly export class C {}\n}",
        "readonly",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

// --------------------------------------------------------------------------
// `override` — never a valid statement/declaration modifier outside a class
// member, so tsc's parser reports a single unconditional TS1434 at the
// `override` token regardless of container or of what follows, `export` or
// otherwise (#16403 slice 2). This is a different mechanism from every other
// modifier in this file: it never reaches `modified_export_form`'s
// container/form split at all.
// --------------------------------------------------------------------------

#[test]
fn override_before_export_default_at_top_level_reports_ts1434() {
    assert_only_diagnostic(
        "override export default 1;",
        "override",
        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
    );
}

#[test]
fn override_before_export_namespace_declaration_at_top_level_reports_ts1434() {
    assert_only_diagnostic(
        "override export namespace N {}",
        "override",
        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
    );
}

#[test]
fn override_before_export_class_in_function_body_reports_ts1434_not_ts1184() {
    assert_only_diagnostic(
        "function f() {\n  override export class C {}\n}",
        "override",
        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
    );
}

#[test]
fn override_before_export_list_in_namespace_body_reports_ts1434_not_ts1194() {
    assert_only_diagnostic(
        "namespace Outer {\n  override export {};\n}",
        "override",
        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
    );
}

#[test]
fn override_before_export_as_namespace_reports_ts1434_not_ts1184() {
    // `export as namespace` is TS1184-unconditional for every other
    // modifier in this file; `override` still short-circuits to its own
    // TS1434 before the `NamespaceExport` arm is ever reached.
    assert_only_diagnostic(
        "override export as namespace Foo;",
        "override",
        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
    );
}

#[test]
fn override_before_non_export_declaration_reports_ts1434() {
    // Confirms the rule is about `override` itself, not about `export`:
    // the same unconditional TS1434 fires with no `export` involved at all.
    assert_only_diagnostic(
        "override class C {}",
        "override",
        diagnostic_codes::UNEXPECTED_KEYWORD_OR_IDENTIFIER,
    );
}

#[test]
fn override_on_its_own_line_is_still_an_identifier_expression() {
    // A line break before the next token takes the same ASI path every
    // other modifier in this file takes — `override` is parsed as a
    // standalone identifier expression, not specially. No parser diagnostic:
    // `override` resolving as a name is a checker-level TS2304, out of this
    // file's reach.
    assert_no_parser_diagnostics("override\nexport class C {}\n");
}

// --------------------------------------------------------------------------
// Negative controls.
// --------------------------------------------------------------------------

/// A line break between the modifier and `export` is an expression statement
/// followed by an export statement, not a modified export — ASI, unchanged.
#[test]
fn modifier_on_its_own_line_before_export_is_not_a_modified_export() {
    let source = "public\nexport as namespace Telemetry;";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
        "a line break makes `public` its own expression statement, so no \
         modifier diagnostic belongs here; got {diagnostics:?}"
    );
}

/// A well-formed `export as namespace` with no modifier keeps parsing clean —
/// the placement question is the checker's, not the parser's.
#[test]
fn plain_export_as_namespace_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("export as namespace Telemetry;");
    assert_no_parser_diagnostics("function collect() {\n  export as namespace Telemetry;\n}");
}
