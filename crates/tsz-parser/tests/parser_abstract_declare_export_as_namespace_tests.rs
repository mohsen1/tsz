//! `abstract`/`declare` before `export as namespace Foo;` (#16389). Split out
//! of #16388, which fixed the same shape for the accessibility/`static`
//! modifiers but could not reach `abstract`/`declare`, since both have their
//! own statement dispatchers (`parse_statement_abstract_keyword`,
//! `parse_statement_declare_or_expression`) that never routed `export` at
//! all.
//!
//! A `NamespaceExportDeclaration` admits no modifiers in any container, so
//! unlike the sibling `abstract`/`declare` declarations (#16380, which split
//! their diagnostic by container — TS1184 in a Block, TS1242/none elsewhere),
//! tsc reports TS1184 across the whole statement **unconditionally** — top
//! level, namespace body, function body, nested block, and class static block
//! alike — and still parses the namespace export. Every row pinned against a
//! real `typescript@7.0.2` oracle.

use crate::parser::test_fixture::parse_source;
use tsz_common::diagnostics::diagnostic_codes;

fn assert_ts1184_over_whole_statement(source: &str) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let start = source
        .find("abstract")
        .or_else(|| source.find("declare"))
        .unwrap() as u32;
    // The span covers the whole `NamespaceExportDeclaration` statement, from
    // the leading modifier through its own semicolon — not the whole source
    // fixture, which may wrap the statement in an enclosing container.
    let namespace_kw = source.find("namespace").unwrap();
    let semicolon = source[namespace_kw..].find(';').unwrap();
    let end = (namespace_kw + semicolon + 1) as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE
                && d.start == start
                && d.start + d.length == end),
        "expected TS1184 spanning [{start}, {end}) for {source:?}, got {diagnostics:?}"
    );
}

fn assert_no_ts1184(source: &str) {
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
        "expected no TS1184 for {source:?}, got {diagnostics:?}"
    );
}

// -- `abstract`, every container --

#[test]
fn abstract_export_as_namespace_in_function_body_reports_ts1184() {
    assert_ts1184_over_whole_statement(
        "function collect() { abstract export as namespace Telemetry; }",
    );
}

#[test]
fn abstract_export_as_namespace_at_top_level_reports_ts1184() {
    assert_ts1184_over_whole_statement("abstract export as namespace Telemetry;");
}

#[test]
fn abstract_export_as_namespace_in_namespace_body_reports_ts1184() {
    assert_ts1184_over_whole_statement("namespace N { abstract export as namespace Telemetry; }");
}

#[test]
fn abstract_export_as_namespace_in_nested_block_reports_ts1184() {
    assert_ts1184_over_whole_statement(
        "function collect() { { abstract export as namespace Telemetry; } }",
    );
}

#[test]
fn abstract_export_as_namespace_in_class_static_block_reports_ts1184() {
    assert_ts1184_over_whole_statement(
        "class C { static { abstract export as namespace Telemetry; } }",
    );
}

// -- `declare`, every container --

#[test]
fn declare_export_as_namespace_in_function_body_reports_ts1184() {
    assert_ts1184_over_whole_statement(
        "function collect() { declare export as namespace Telemetry; }",
    );
}

#[test]
fn declare_export_as_namespace_at_top_level_reports_ts1184() {
    assert_ts1184_over_whole_statement("declare export as namespace Telemetry;");
}

#[test]
fn declare_export_as_namespace_in_namespace_body_reports_ts1184() {
    assert_ts1184_over_whole_statement("namespace N { declare export as namespace Telemetry; }");
}

#[test]
fn declare_export_as_namespace_in_nested_block_reports_ts1184() {
    assert_ts1184_over_whole_statement(
        "function collect() { { declare export as namespace Telemetry; } }",
    );
}

#[test]
fn declare_export_as_namespace_in_class_static_block_reports_ts1184() {
    assert_ts1184_over_whole_statement(
        "class C { static { declare export as namespace Telemetry; } }",
    );
}

// -- Binder-name independence: nothing should key on the namespace name --

#[test]
fn abstract_export_as_namespace_binder_name_does_not_matter() {
    assert_ts1184_over_whole_statement("abstract export as namespace _$weird1;");
}

// -- The `abstract`/`export` boundary is ASI-sensitive; `export`/`as` is not --

#[test]
fn abstract_on_its_own_line_before_export_as_namespace_is_not_ts1184() {
    // ASI cuts `abstract` into its own (invalid, but unrelated) expression
    // statement here — ASI applies at the `abstract`/`export` boundary, so
    // this must NOT report TS1184 for the (now unmodified) export statement.
    assert_no_ts1184("function collect() { abstract\nexport as namespace Telemetry; }");
}

#[test]
fn abstract_export_as_namespace_with_line_break_before_as_still_reports_ts1184() {
    // Unlike the `abstract`/`export` boundary, a line break between `export`
    // and `as` does not stop tsc from reading one `export as namespace`
    // statement, so this lookahead must not require the two to share a line.
    let source = "function collect() { abstract export\nas namespace Telemetry; }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let start = source.find("abstract").unwrap() as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE && d.start == start),
        "expected TS1184 at 'abstract' for {source:?}, got {diagnostics:?}"
    );
}

// -- Negative control: a well-formed, unmodified `export as namespace` --

#[test]
fn plain_export_as_namespace_reports_no_ts1184() {
    assert_no_ts1184("export as namespace Telemetry;");
}

// -- Negative control: `abstract`/`declare` before the OTHER export forms is
//    untouched by this fix (a distinct, still-open gap; see #16389's "worth
//    checking" note) — these lookaheads must not accidentally widen to match
//    `export {}` / `export *` / `export =` / `export default`. --

#[test]
fn abstract_export_brace_does_not_route_through_the_namespace_export_arm() {
    // Before this fix `abstract` here was silently swallowed as an
    // (erroneous) identifier expression; this fix must not change that
    // pre-existing, separately-tracked shape by accident.
    let source = "function collect() { abstract export {}; }";
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE),
        "abstract-before-`export {{}}` must not gain TS1184 from this fix, got {diagnostics:?}"
    );
}
