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

use crate::parser::syntax_kind_ext;
use crate::parser::test_fixture::{assert_span, parse_source};
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

// --------------------------------------------------------------------------
// `async` before `export ...` (#16403 slice 3) — distinct from every sibling
// modifier family because `async` is legal on a function declaration in
// *every* container, including a Block, so a Block cannot uniformly silence
// it. Every row pinned against `typescript@7.0.2`
// (`--strict --lib es2022 --target es2022 --module es2022`).
// --------------------------------------------------------------------------

/// `async export function f() {}` — legal in every container, so the only
/// violation is modifier order: TS1029 on `export`, unconditionally,
/// including inside a Block where a nested async function declaration is
/// otherwise completely legal.
#[test]
fn async_before_export_function_at_top_level_reports_ts1029() {
    assert_only_diagnostic(
        "async export function build() {}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

#[test]
fn async_before_export_function_in_function_body_reports_ts1029_not_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  async export function build() {}\n}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

#[test]
fn async_before_export_function_in_namespace_body_reports_ts1029() {
    assert_only_diagnostic(
        "namespace Outer {\n  async export function build() {}\n}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

/// `export default function` reads the same always-legal answer as a bare
/// `export function`, named or anonymous alike.
#[test]
fn async_before_export_default_function_in_function_body_reports_ts1029() {
    assert_only_diagnostic(
        "function collect() {\n  async export default function f() {}\n}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

#[test]
fn async_before_export_default_anonymous_function_at_top_level_reports_ts1029() {
    assert_only_diagnostic(
        "async export default function () {}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

/// The message anchors on `export` (the later of the two out-of-order
/// modifiers), naming both — same convention `abstract`'s sibling ordering
/// check uses.
#[test]
fn async_before_export_function_reports_message_naming_both_modifiers() {
    let source = "async export function build() {}";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert_eq!(diagnostics.len(), 1, "got {diagnostics:?}");
    assert_eq!(
        diagnostics[0].code,
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER
    );
    assert_eq!(
        diagnostics[0].message,
        "'export' modifier must precede 'async' modifier."
    );
}

/// `async export const/class/interface/type/enum ...` — `async` is not legal
/// on any of these, but `export` is a legal modifier at this container, so
/// the answer is TS1029 (order) outside a Block, TS1184 inside one — the
/// block gate does not special-case `async` the way it does for a function.
#[test]
fn async_before_export_const_at_top_level_reports_ts1029() {
    assert_only_diagnostic(
        "async export const seeded = 1;",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

#[test]
fn async_before_export_class_in_namespace_body_reports_ts1029() {
    assert_only_diagnostic(
        "namespace Outer {\n  async export class Widget {}\n}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

#[test]
fn async_before_export_interface_in_function_body_reports_ts1184_not_ts1029() {
    assert_only_diagnostic(
        "function collect() {\n  async export interface Widget {}\n}",
        "async",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn async_before_export_type_alias_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  async export type T = 1;\n}",
        "async",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn async_before_export_enum_at_top_level_reports_ts1029() {
    assert_only_diagnostic(
        "async export enum E {}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

/// `export default class` is the same "not legal on async" declaration
/// answer as a bare `export class` — not the `ExportAssignment` node kind
/// the plain-expression `export default 1` form uses.
#[test]
fn async_before_export_default_class_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  async export default class {}\n}",
        "async",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn async_before_export_default_class_at_top_level_reports_ts1029() {
    assert_only_diagnostic(
        "async export default class {}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

// --------------------------------------------------------------------------
// `export {}` / `export *` / `export { a } from` — an `ExportDeclaration`
// node, which admits no modifiers at all (a structural mismatch, not an
// ordering one): TS1042, not TS1029/TS1044, wherever the form's own
// placement diagnostic does not already win outright.
// --------------------------------------------------------------------------

#[test]
fn async_before_export_list_at_top_level_reports_ts1042() {
    assert_only_diagnostic(
        "async export {};",
        "async",
        diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
    );
}

#[test]
fn async_before_export_star_at_top_level_reports_ts1042() {
    assert_only_diagnostic(
        "async export * from \"./source\";",
        "async",
        diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
    );
}

#[test]
fn async_before_export_list_in_namespace_body_reports_ts1042() {
    assert_only_diagnostic(
        "namespace Outer {\n  async export {};\n}",
        "async",
        diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
    );
}

/// Inside a Block the `ExportDeclaration` node's own placement diagnostic
/// (TS1233, a checker check out of this parser-only harness's reach) wins
/// alone — same silencing shape the sibling modifier families use.
#[test]
fn async_before_export_list_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  async export {};\n}");
}

// --------------------------------------------------------------------------
// `export =` / `export default <expr>` — an `ExportAssignment` node, the
// same structural mismatch as `ExportDeclaration` but wider silencing: its
// own placement diagnostic wins in a Block *and* a namespace body, so TS1042
// survives only at the source file's own top level.
// --------------------------------------------------------------------------

#[test]
fn async_before_export_assignment_at_top_level_reports_ts1042() {
    assert_only_diagnostic(
        "async export = 1;",
        "async",
        diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
    );
}

#[test]
fn async_before_export_default_expression_at_top_level_reports_ts1042() {
    assert_only_diagnostic(
        "async export default 1;",
        "async",
        diagnostic_codes::MODIFIER_CANNOT_BE_USED_HERE,
    );
}

#[test]
fn async_before_export_assignment_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  async export = 1;\n}");
}

#[test]
fn async_before_export_assignment_in_namespace_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("namespace Outer {\n  async export = 1;\n}");
}

#[test]
fn async_before_export_default_expression_in_namespace_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("namespace Outer {\n  async export default 1;\n}");
}

// --------------------------------------------------------------------------
// `export namespace N {}` / `export module M {}` — a nested module
// declaration is itself illegal in a Block (TS1235), independent of any
// modifier, so that placement diagnostic wins there; outside a Block it
// nests validly and takes the ordinary TS1029 order answer.
// --------------------------------------------------------------------------

#[test]
fn async_before_export_namespace_declaration_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  async export namespace N {}\n}");
}

#[test]
fn async_before_export_namespace_declaration_at_top_level_reports_ts1029() {
    assert_only_diagnostic(
        "async export namespace N {}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

#[test]
fn async_before_export_namespace_declaration_in_namespace_body_reports_ts1029() {
    assert_only_diagnostic(
        "namespace Outer {\n  async export namespace N {}\n}",
        "export",
        diagnostic_codes::MODIFIER_MUST_PRECEDE_MODIFIER,
    );
}

// --------------------------------------------------------------------------
// `async export as namespace Foo;` — a `NamespaceExportDeclaration`, which
// like every other modifier family admits no modifiers in any container.
// --------------------------------------------------------------------------

#[test]
fn async_before_export_as_namespace_at_top_level_reports_ts1184() {
    assert_only_diagnostic(
        "async export as namespace Telemetry;",
        "async",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn async_before_export_as_namespace_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  async export as namespace Telemetry;\n}",
        "async",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn async_before_export_as_namespace_in_namespace_body_reports_ts1184() {
    assert_only_diagnostic(
        "namespace Outer {\n  async export as namespace Telemetry;\n}",
        "async",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

// --------------------------------------------------------------------------
// Negative control — `async function` (no `export`) is unaffected.
// --------------------------------------------------------------------------

#[test]
fn plain_async_function_declaration_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("async function build() {}");
    assert_no_parser_diagnostics("function collect() {\n  async function build() {}\n}");
}

// --------------------------------------------------------------------------
// `accessor` before `export ...` (#16403 slice 5). Unlike every sibling
// family, `accessor` is parsed on its own dedicated statement-entry path
// (`parse_statement_accessor_keyword`), not through
// `parse_statement_top_level_modifier`'s dispatch — but it takes the exact
// same `ModifiedExportForm` container split: `export {}` / `export *` and
// `export namespace` / `export module` are silenced by their own placement
// diagnostic inside a Block; `export =` / `export default <expr>` are
// silenced there AND in a namespace body (like `readonly`, not like
// `async`'s function exception); `export as namespace` gets the uniform
// TS1184 every sibling family reports, not TS1275; every other export form
// keeps TS1275 outside a Block and swaps to the generic TS1184 inside one.
// --------------------------------------------------------------------------

#[test]
fn accessor_before_export_const_at_top_level_reports_ts1275() {
    assert_only_diagnostic(
        "accessor export const seeded = 1;",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_class_in_namespace_body_reports_ts1275() {
    assert_only_diagnostic(
        "namespace Outer {\n  accessor export class Widget {}\n}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_function_in_function_body_reports_ts1184_not_ts1275() {
    assert_only_diagnostic(
        "function collect() {\n  accessor export function build() {}\n}",
        "accessor",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn accessor_before_export_type_alias_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  accessor export type T = 1;\n}",
        "accessor",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn accessor_before_export_interface_at_top_level_reports_ts1275() {
    assert_only_diagnostic(
        "accessor export interface Widget {}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

/// `export {}` / `export *` — an `ExportDeclaration` node: TS1275 survives at
/// the source file's own top level and in a namespace body, but is silenced
/// entirely inside a Block by the form's own placement diagnostic (TS1233, a
/// checker check out of this parser-only harness's reach).
#[test]
fn accessor_before_export_list_at_top_level_reports_ts1275() {
    assert_only_diagnostic(
        "accessor export {};",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_star_in_namespace_body_reports_ts1275() {
    assert_only_diagnostic(
        "namespace Outer {\n  accessor export * from \"./source\";\n}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_list_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  accessor export {};\n}");
}

/// `export =` / `export default <expr>` — an `ExportAssignment` node: unlike
/// the `ExportDeclaration` form above, silencing extends to a namespace body
/// too, not just a Block (the same wider shape `readonly`/`override` use).
#[test]
fn accessor_before_export_assignment_at_top_level_reports_ts1275() {
    assert_only_diagnostic(
        "accessor export = 1;",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_assignment_in_namespace_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("namespace Outer {\n  accessor export = 1;\n}");
}

#[test]
fn accessor_before_export_assignment_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  accessor export = 1;\n}");
}

#[test]
fn accessor_before_export_default_expr_in_namespace_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("namespace Outer {\n  accessor export default 1;\n}");
}

/// `export namespace` / `export module` — a nested module declaration is
/// itself illegal inside a Block (TS1235) independent of any modifier, and
/// that placement diagnostic wins there the same way `ExportDeclaration`
/// does; outside a Block this takes the ordinary container split.
#[test]
fn accessor_before_export_namespace_declaration_at_top_level_reports_ts1275() {
    assert_only_diagnostic(
        "accessor export namespace N {}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_namespace_declaration_in_namespace_body_reports_ts1275() {
    assert_only_diagnostic(
        "namespace Outer {\n  accessor export namespace N {}\n}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_namespace_declaration_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics("function collect() {\n  accessor export namespace N {}\n}");
}

// --------------------------------------------------------------------------
// `export as namespace Foo;` — a `NamespaceExportDeclaration`, which like
// every other modifier family admits no modifiers in any container: TS1184,
// not TS1275, unconditionally.
// --------------------------------------------------------------------------

#[test]
fn accessor_before_export_as_namespace_at_top_level_reports_ts1184_not_ts1275() {
    assert_only_diagnostic(
        "accessor export as namespace Telemetry;",
        "accessor",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn accessor_before_export_as_namespace_in_namespace_body_reports_ts1184() {
    assert_only_diagnostic(
        "namespace Outer {\n  accessor export as namespace Telemetry;\n}",
        "accessor",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn accessor_before_export_as_namespace_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  accessor export as namespace Telemetry;\n}",
        "accessor",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

// --------------------------------------------------------------------------
// Negative control — `accessor` outside any export/class-member position
// (e.g. a bare top-level declaration) is unaffected by this change; it still
// reports the same unconditional TS1275 it did before this class ever
// touched `export`.
// --------------------------------------------------------------------------

#[test]
fn accessor_before_non_export_class_reports_ts1275() {
    assert_only_diagnostic(
        "accessor class Widget {}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

// --------------------------------------------------------------------------
// `export default class` / `export default function` — a `ClassDeclaration` /
// `FunctionDeclaration` carrying a `default` modifier, NOT an
// `ExportAssignment` node (#16403 slice 4).
//
// The whole `default` arm used to be one `ExportAssignment` bucket, which
// silences the modifier in a namespace body (the assignment's own TS1319 wins)
// and in a Block (TS1258). Only a bare `export default <expr>` is that node;
// with a declaration keyword after `default` tsc keeps the ordinary container
// split instead — TS1044/TS1024 outside a Block, TS1184 inside one.
//
// Oracle-pinned against `typescript@7.0.2`
// (`--noEmit --strict --pretty false --target es2022 --module es2022`) across
// `static`/`public`/`protected`/`private`/`readonly` x 3 containers; the rows
// below are the representatives of each distinct answer.
// --------------------------------------------------------------------------

#[test]
fn modifier_before_export_default_class_in_namespace_body_reports_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export default class Widget {}\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_default_function_in_namespace_body_reports_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export default function build() {}\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_export_default_class_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  public export default class Widget {}\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn modifier_before_export_default_function_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  public export default function build() {}\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

/// The top-level answer was already right (this arm fell through to the
/// container gate there) — pinned so the fix cannot move it.
#[test]
fn modifier_before_export_default_class_at_top_level_still_reports_ts1044() {
    assert_only_diagnostic(
        "public export default class Widget {}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

/// `readonly` carries its own fixed-message TS1024 rather than the formatted
/// TS1044, so the family's second message shape needs its own row.
#[test]
fn readonly_before_export_default_class_in_namespace_body_reports_ts1024() {
    assert_only_diagnostic(
        "namespace Outer {\n  readonly export default class Widget {}\n}",
        "readonly",
        diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
    );
}

/// The answer is keyed on the node the `export default` begins, never on which
/// modifier was written — every member of the TS1044 family lands identically.
#[test]
fn every_ts1044_family_modifier_before_export_default_class_reports_ts1044() {
    for modifier in ["public", "private", "protected", "static"] {
        let source =
            format!("namespace Outer {{\n  {modifier} export default class Widget {{}}\n}}");
        assert_eq!(
            diagnostic_codes_at(&source, modifier),
            vec![diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT],
            "modifier {modifier:?} must not change the answer — it is the \
             ClassDeclaration node kind that decides it"
        );
    }
}

/// Renamed binder, to pin that nothing keys on the declared name — and the
/// anonymous form, which is the shape the `default` arm exists for.
#[test]
fn export_default_class_diagnostic_does_not_depend_on_the_class_name() {
    assert_only_diagnostic(
        "namespace Enclosing {\n  static export default class QuiteDifferentName {}\n}",
        "static",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
    assert_only_diagnostic(
        "namespace Enclosing {\n  static export default class {}\n}",
        "static",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

/// `default` may carry further modifiers of its own before the declaration
/// keyword; tsc gives those the same declaration answer.
#[test]
fn modifier_before_export_default_abstract_class_reports_the_declaration_answer() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export default abstract class Widget {}\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
    assert_only_diagnostic(
        "function collect() {\n  public export default abstract class Widget {}\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

#[test]
fn modifier_before_export_default_async_function_reports_the_declaration_answer() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export default async function build() {}\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
    assert_only_diagnostic(
        "function collect() {\n  public export default async function build() {}\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

// --------------------------------------------------------------------------
// The negative half of the same arm — a bare `export default <expr>` IS an
// `ExportAssignment` and keeps its silence. These are the rows a
// "reclassify the whole `default` arm" fix would break.
// --------------------------------------------------------------------------

#[test]
fn modifier_before_bare_export_default_expression_stays_silent_outside_top_level() {
    assert_no_parser_diagnostics("namespace Outer {\n  public export default 1;\n}");
    assert_no_parser_diagnostics("function collect() {\n  public export default 1;\n}");
}

/// The modifier-skipping lookahead must not swallow an `async` that begins an
/// arrow *expression* rather than a function declaration.
#[test]
fn modifier_before_export_default_async_arrow_stays_an_export_assignment() {
    assert_no_parser_diagnostics("namespace Outer {\n  public export default async () => 1;\n}");
    assert_no_parser_diagnostics("function collect() {\n  public export default async () => 1;\n}");
}

/// `abstract` / `async` used as a plain identifier expression after `default`
/// is an assignment too — the lookahead stops at the `;`.
///
/// The `abstract` row is anchored on the modifier rather than asserting a
/// diagnostic-free parse: `export default abstract;` already draws two
/// unrelated TS1005s with *no* preceding modifier at all (probed on this
/// branch's parent), a pre-existing recovery defect in the `export default
/// <expr>` path that is out of this slice's scope. What this row pins is the
/// classification — no modifier diagnostic is attributed to `public`.
#[test]
fn modifier_before_export_default_modifier_keyword_as_identifier_stays_silent() {
    assert!(
        diagnostic_codes_at(
            "namespace Outer {\n  public export default abstract;\n}",
            "public"
        )
        .is_empty(),
        "`export default abstract;` is an ExportAssignment — its own TS1319 wins \
         and no modifier diagnostic is attributed to `public`"
    );
    assert_no_parser_diagnostics("namespace Outer {\n  public export default async;\n}");
}

// --------------------------------------------------------------------------
// `export type { x } from "m"` / `export type * from "m"` — a type-only
// `ExportDeclaration`, not the type-alias declaration (#16403 slice 4).
//
// The classifier had no `type` arm at all, so both fell into the ordinary
// declaration bucket and drew TS1184 inside a Block, where tsc reports only
// the export declaration's own TS1233 and no modifier diagnostic. Outside a
// Block both buckets agree on TS1044/TS1024, so only the Block rows move.
// --------------------------------------------------------------------------

#[test]
fn modifier_before_type_only_export_list_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics(
        "function collect() {\n  public export type { Widget } from \"./source\";\n}",
    );
}

#[test]
fn modifier_before_type_only_export_star_in_function_body_reports_no_parser_diagnostic() {
    assert_no_parser_diagnostics(
        "function collect() {\n  public export type * from \"./source\";\n}",
    );
}

#[test]
fn modifier_before_type_only_export_list_at_top_level_reports_ts1044() {
    assert_only_diagnostic(
        "public export type { Widget } from \"./source\";",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

#[test]
fn modifier_before_type_only_export_list_in_namespace_body_reports_ts1044() {
    assert_only_diagnostic(
        "namespace Outer {\n  public export type { Widget } from \"./source\";\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
}

/// The type-alias form is an ordinary declaration and must keep the container
/// split — this is the row the new `type` lookahead has to leave alone.
#[test]
fn modifier_before_export_type_alias_keeps_the_declaration_answer() {
    assert_only_diagnostic(
        "public export type Widget = 1;",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
    assert_only_diagnostic(
        "namespace Outer {\n  public export type Widget = 1;\n}",
        "public",
        diagnostic_codes::MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
    );
    assert_only_diagnostic(
        "function collect() {\n  public export type Widget = 1;\n}",
        "public",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

/// `readonly`'s own message shape for the type-only forms.
#[test]
fn readonly_before_type_only_export_star_splits_by_container() {
    assert_only_diagnostic(
        "readonly export type * from \"./source\";",
        "readonly",
        diagnostic_codes::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
    );
    assert_no_parser_diagnostics(
        "function collect() {\n  readonly export type * from \"./source\";\n}",
    );
}

// --------------------------------------------------------------------------
// `accessor` x the two forms this slice reclassifies.
//
// Slice 5 routes `accessor` through the same `ModifiedExportForm` classifier
// as the TS1044/TS1024 families, so the `default class`/`default function` and
// type-only-export arms above move `accessor` too — in the same direction, and
// with `accessor`'s own TS1275 message. Landed after slice 4 was opened, so
// these rows are pinned here rather than left to the merge.
//
// Oracle (`typescript@7.0.2`): `accessor export default class {}` is TS1275 at
// top level and in a namespace body, TS1184 in a Block; a bare `accessor
// export default 1;` keeps the assignment's silence in both non-top
// containers; `accessor export type { x } from "m"` is TS1275 outside a Block
// and silent inside one.
// --------------------------------------------------------------------------

#[test]
fn accessor_before_export_default_class_in_namespace_body_reports_ts1275() {
    assert_only_diagnostic(
        "namespace Outer {\n  accessor export default class Widget {}\n}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
}

#[test]
fn accessor_before_export_default_function_in_function_body_reports_ts1184() {
    assert_only_diagnostic(
        "function collect() {\n  accessor export default function build() {}\n}",
        "accessor",
        diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
    );
}

/// The negative half — a bare `export default <expr>` is still the assignment
/// node for `accessor` too, so its own TS1319/TS1258 wins.
#[test]
fn accessor_before_bare_export_default_expression_stays_silent_outside_top_level() {
    assert_no_parser_diagnostics("namespace Outer {\n  accessor export default 1;\n}");
    assert_no_parser_diagnostics("function collect() {\n  accessor export default 1;\n}");
}

#[test]
fn accessor_before_type_only_export_splits_by_container() {
    assert_only_diagnostic(
        "namespace Outer {\n  accessor export type { Widget } from \"./source\";\n}",
        "accessor",
        diagnostic_codes::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
    );
    assert_no_parser_diagnostics(
        "function collect() {\n  accessor export type * from \"./source\";\n}",
    );
}

// --------------------------------------------------------------------------
// #16403 residual: the `NamespaceExportDeclaration` node built after a stray
// `static`/`public`/`protected`/`private`/`readonly` modifier must span from
// the MODIFIER, not from `export`. TS1184 (above) is unaffected — it anchors
// on the modifier token directly, before this node is even built — but the
// checker's own TS1314/TS1316 read this node's own `pos`/`end`
// (`crates/tsz-checker/src/state/state_checking/source_file.rs`), so a node
// that starts at `export` misreports their column even though the CODE is
// already right (a code-set comparison cannot see this). The accessor/async
// dispatch already threads the modifier's start position through
// `parse_export_declaration_from`; this dispatch (`static`/`public`/
// `protected`/`private`/`readonly`, `parse_statement_top_level_modifier`) used
// to drop straight to a fresh `parse_statement()` after reporting TS1184,
// which re-anchored the node at `export`. Oracle-pinned: tsc anchors both
// TS1184 and TS1314 at column 1 (the modifier) for every row below.
// --------------------------------------------------------------------------

fn assert_namespace_export_span_starts_at_modifier(modifier: &str) {
    let source = format!("{modifier} export as namespace Foo;");
    assert_span(
        &source,
        syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION,
        &source,
    );
}

#[test]
fn static_before_export_as_namespace_span_starts_at_modifier() {
    assert_namespace_export_span_starts_at_modifier("static");
}

#[test]
fn public_before_export_as_namespace_span_starts_at_modifier() {
    assert_namespace_export_span_starts_at_modifier("public");
}

#[test]
fn protected_before_export_as_namespace_span_starts_at_modifier() {
    assert_namespace_export_span_starts_at_modifier("protected");
}

#[test]
fn private_before_export_as_namespace_span_starts_at_modifier() {
    assert_namespace_export_span_starts_at_modifier("private");
}

#[test]
fn readonly_before_export_as_namespace_span_starts_at_modifier() {
    assert_namespace_export_span_starts_at_modifier("readonly");
}

/// Renamed-binder control: the exported name is arbitrary and must not affect
/// where the node's span starts.
#[test]
fn static_before_export_as_namespace_span_starts_at_modifier_renamed_binder() {
    let source = "static export as namespace qux$_0;";
    assert_span(
        source,
        syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION,
        source,
    );
}
