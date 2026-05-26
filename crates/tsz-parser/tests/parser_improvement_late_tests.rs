//! Tests for parser improvements to reduce TS1005 and TS2300 false positives

use crate::parser::test_fixture::{parse_source, parse_source_named};
use tsz_common::diagnostics::diagnostic_codes;
use tsz_common::position::LineMap;

#[test]
fn test_orphan_finally_block_emits_ts1005() {
    // finally block without try should emit TS1005: 'try' expected
    let source = r"
function fn() {
    finally { }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 error for orphan finally block, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );

    // Check that the error message mentions 'try'
    let has_try_expected = diagnostics
        .iter()
        .any(|d| d.code == 1005 && d.message.to_lowercase().contains("try"));
    assert!(
        has_try_expected,
        "Expected TS1005 error to mention 'try', got diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_multiple_orphan_blocks_emit_separate_ts1005() {
    // Multiple orphan catch/finally blocks should each emit TS1005
    let source = r"
function fn() {
    finally { }
    catch (x) { }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();

    assert_eq!(
        ts1005_count, 2,
        "Expected 2 TS1005 errors for two orphan blocks, got {ts1005_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_ts1131_emitted_for_invalid_interface_member() {
    // Invalid token inside an interface body should emit TS1131
    // "Property or signature expected."
    let source = r"
interface Foo {
    ?;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert!(
        ts1131_count >= 1,
        "Expected at least 1 TS1131 for invalid interface member, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_ts1131_emitted_for_invalid_type_literal_member() {
    // Invalid token inside a type literal should emit TS1131
    let source = r"
type T = {
    !;
};
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert!(
        ts1131_count >= 1,
        "Expected at least 1 TS1131 for invalid type literal member, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_postfix_optional_method_signature_recovers_with_semicolon_expected() {
    let source = r"
type T = { x()?: number; };
interface I { y()?: string; }
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let x_question = source.find("x()?").expect("x method") as u32 + 3;
    let y_question = source.find("y()?").expect("y method") as u32 + 3;
    let question_positions = vec![x_question, y_question];
    let colon_positions = vec![x_question + 1, y_question + 1];

    let actual_positions: Vec<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED && diag.message == "';' expected.")
        .map(|diag| diag.start)
        .collect();
    assert_eq!(
        actual_positions, question_positions,
        "Expected TS1005 ';' expected at postfix optional method markers, got {diagnostics:?}",
    );

    let actual_ts1131_positions: Vec<u32> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED)
        .map(|diag| diag.start)
        .collect();
    assert_eq!(
        actual_ts1131_positions, colon_positions,
        "Expected TS1131 at the colon following postfix optional method markers: {diagnostics:?}",
    );

    let ts1131_at_question = diagnostics
        .iter()
        .filter(|diag| {
            diag.code == diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED
                && question_positions.contains(&diag.start)
        })
        .count();
    assert_eq!(
        ts1131_at_question, 0,
        "Postfix optional method markers should not fall through to TS1131: {diagnostics:?}",
    );
}

#[test]
fn test_type_literal_statement_recovery_matches_interface_extending_class2() {
    let source = r"
class Foo {
    x: string;
    y() { }
    get Z() {
        return 1;
    }
    [x: string]: Object;
}

interface I2 extends Foo {
    a: {
        toString: () => {
            return 1;
        };
    }
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<_> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1131, 1128, 1128],
        "Expected parser recovery to match tsc for malformed type literal member body, got {diagnostics:?}"
    );
}

#[test]
fn test_ts1131_not_emitted_for_valid_interface() {
    // Valid interface should not emit TS1131
    let source = r"
interface Foo {
    x: number;
    y: string;
    z(): void;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1131_count = diagnostics.iter().filter(|d| d.code == 1131).count();
    assert_eq!(
        ts1131_count, 0,
        "Expected no TS1131 for valid interface, got {ts1131_count}. Diagnostics: {diagnostics:?}",
    );
}

#[test]
fn test_computed_property_signature_after_array_type_line_break_does_not_emit_ts1131() {
    let source = r"
const IGNORE_LIST = 'ignoreList';

interface SourceMap {
  sources: string[]
  [IGNORE_LIST]: number[]
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .all(|d| d.code != diagnostic_codes::PROPERTY_OR_SIGNATURE_EXPECTED),
        "A line-broken computed property signature should not be parsed as indexed access: {diagnostics:?}"
    );
}

#[test]
fn test_class_computed_property_after_type_annotation_line_break_uses_asi() {
    let source = r"
class C {
    [e]: number
    [e2]: number
}
";
    let (parser, root) = parse_source(source);

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    assert_eq!(
        class_data.members.nodes.len(),
        2,
        "line-broken class computed members should not become one indexed-access type"
    );
}

#[test]
fn test_class_computed_method_after_return_type_line_break_uses_asi() {
    let source = r#"
class C {
    ["foo"](): void
    ["bar"](): void;
    ["foo"]() {}
}
"#;
    let (parser, root) = parse_source(source);

    let codes: Vec<_> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        !codes.contains(&diagnostic_codes::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED)
            && !codes.contains(&diagnostic_codes::OR_EXPECTED),
        "line-broken computed method signatures should remain separate members, got {codes:?}",
    );

    let source_file = parser.get_arena().get_source_file_at(root).unwrap();
    let class_idx = source_file.statements.nodes[0];
    let class_node = parser.get_arena().get(class_idx).unwrap();
    let class_data = parser.get_arena().get_class(class_node).unwrap();
    assert_eq!(
        class_data.members.nodes.len(),
        3,
        "computed method signatures should not become indexed-access return types"
    );
}

#[test]
fn test_empty_index_signature_after_type_member_annotation_line_break_uses_asi() {
    let source = r"
var v: {
   a: B
   [];
};
";
    let (parser, _root) = parse_source(source);

    let codes: Vec<_> = parser.get_diagnostics().iter().map(|d| d.code).collect();
    assert!(
        codes.contains(&diagnostic_codes::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER),
        "empty bracket member should recover as an index signature, got {codes:?}",
    );
}

// =============================================================================
// Import Defer Tests
// =============================================================================

#[test]
fn test_import_defer_namespace_parses_clean() {
    // `import defer * as ns from "mod"` is valid — no parse errors
    let source = r#"import defer * as ns from "./a";"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for valid defer namespace import, got {parse_errors:?}",
    );
}

#[test]
fn test_import_defer_as_binding_name() {
    // `import defer from "mod"` — defer is the default import NAME, not a modifier
    let source = r#"import defer from "./a";"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors when 'defer' is used as binding name, got {parse_errors:?}",
    );
}

#[test]
fn test_import_dot_defer_call_no_parse_error() {
    // `import.defer("./a")` — valid dynamic defer import, no parse error
    let source = r#"import.defer("./a.js");"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for import.defer() call, got {parse_errors:?}",
    );
}

#[test]
fn test_import_dot_defer_standalone_emits_ts1005() {
    // `import.defer` without () should emit TS1005 "'(' expected."
    let source = r"const x = import.defer;";
    let (parser, _root) = parse_source(source);

    let ts1005_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 1005)
        .count();
    assert_eq!(
        ts1005_count, 1,
        "Expected 1 TS1005 for standalone import.defer, got {ts1005_count}",
    );
}

#[test]
fn test_import_dot_invalid_meta_property_ts17012() {
    // `import.foo` (not in call) should emit TS17012
    let source = r"const x = import.foo;";
    let (parser, _root) = parse_source(source);

    let ts17012_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 17012)
        .count();
    assert_eq!(
        ts17012_count, 1,
        "Expected 1 TS17012 for invalid import.foo, got {ts17012_count}",
    );
}

#[test]
fn test_import_dot_invalid_meta_property_call_ts18061() {
    // `import.foo()` (in call) should emit TS18061
    let source = r#"import.foo("./a");"#;
    let (parser, _root) = parse_source(source);

    let ts18061_count = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code == 18061)
        .count();
    assert_eq!(
        ts18061_count, 1,
        "Expected 1 TS18061 for import.foo() call, got {ts18061_count}",
    );
}

#[test]
fn test_import_defer_with_default_sets_deferred_flag() {
    // `import defer foo from "./a"` — defer is modifier, foo is default name
    // Parser should set is_deferred = true
    let source = r#"import defer foo from "./a";"#;
    let (parser, root) = parse_source(source);

    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt = sf.statements.nodes[0];
    let stmt_node = arena.get(stmt).unwrap();
    let import = arena.get_import_decl(stmt_node).unwrap();
    let clause_node = arena.get(import.import_clause).unwrap();
    let clause = arena.get_import_clause(clause_node).unwrap();
    assert!(
        clause.is_deferred,
        "Expected is_deferred to be true for 'import defer foo from'"
    );
    assert!(
        clause.name.is_some(),
        "Expected default import name to be present"
    );
}

#[test]
fn test_import_defer_from_as_name_not_deferred() {
    // `import defer from "./a"` — defer is the import NAME, not modifier
    // Parser should NOT set is_deferred = true
    let source = r#"import defer from "./a";"#;
    let (parser, root) = parse_source(source);

    let arena = parser.get_arena();
    let sf = arena.get_source_file_at(root).unwrap();
    let stmt = sf.statements.nodes[0];
    let stmt_node = arena.get(stmt).unwrap();
    let import = arena.get_import_decl(stmt_node).unwrap();
    let clause_node = arena.get(import.import_clause).unwrap();
    let clause = arena.get_import_clause(clause_node).unwrap();
    assert!(
        !clause.is_deferred,
        "Expected is_deferred to be false for 'import defer from' (defer is name)"
    );
}

#[test]
fn test_import_defer_type_modifier_conflict_anchors_from_at_namespace_token() {
    // `import defer type * as ns from "./a"` is illegal (defer + type modifier
    // conflict) but tsc still parses it as: `defer` modifier, `type` as the
    // default-import name (contextual keyword), then expects `from`. The
    // resulting `'from' expected` diagnostic anchors at the `*` (column 19),
    // not at the `type` keyword (column 14) or with an incorrect `'='
    // expected` from the import-equals lookahead path.
    let source = r#"import defer type * as ns from "./a";"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();
    assert!(
        !ts1005.is_empty(),
        "Expected at least one TS1005 for `import defer type *`, got {diagnostics:?}"
    );
    // No `'=' expected.` (would be the import-equals lookahead misroute).
    assert!(
        !ts1005.iter().any(|d| d.message.contains("'=' expected")),
        "Should not emit `'=' expected.` for `import defer type *`: {ts1005:?}"
    );
    // The `'from' expected.` should anchor at column 19 (the `*`), 0-indexed
    // start = 18.
    let from_expected: Vec<_> = ts1005
        .iter()
        .filter(|d| d.message.contains("'from' expected"))
        .collect();
    assert!(
        from_expected.iter().any(|d| d.start == 18),
        "Expected `'from' expected.` anchored at column 19 (start=18), got {from_expected:?}"
    );
}

#[test]
fn test_import_defer_from_equals_routes_to_import_declaration() {
    // `import defer from = require("m")` — `defer` has no import-equals form,
    // so the lookahead must route this to import-declaration. tsc parses it as
    // `defer` modifier + `from` binding name, then expects the `from` keyword
    // and finds `=` at column 19 (start=18). The lookahead must NOT route to
    // import-equals (which would emit `'=' expected.` at column 14 plus a
    // trailing `';' expected.`).
    let source = r#"import defer from = require("m");"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005: Vec<_> = diagnostics.iter().filter(|d| d.code == 1005).collect();
    assert!(
        !ts1005.iter().any(|d| d.message.contains("'=' expected")),
        "Should not emit `'=' expected.` for `import defer from = require(...)`: {ts1005:?}"
    );
    assert!(
        !ts1005.iter().any(|d| d.message.contains("';' expected")),
        "Should not emit `';' expected.` for `import defer from = require(...)`: {ts1005:?}"
    );
    let from_expected: Vec<_> = ts1005
        .iter()
        .filter(|d| d.message.contains("'from' expected"))
        .collect();
    assert!(
        from_expected.iter().any(|d| d.start == 18),
        "Expected `'from' expected.` anchored at column 19 (start=18), got {from_expected:?} (all ts1005: {ts1005:?})"
    );
}

#[test]
fn test_import_type_from_equals_still_routes_to_import_equals() {
    // Regression for the sibling case: `import type from = require("m")` IS
    // valid type-only import-equals (with `from` as the binding name). The
    // narrow `defer` fix must not regress the `type` branch, so the parser
    // should accept this without parser-level recovery diagnostics.
    let source = r#"import type from = require("m");"#;
    let (parser, _root) = parse_source(source);

    let parse_errors: Vec<_> = parser
        .get_diagnostics()
        .iter()
        .filter(|d| d.code < 2000)
        .collect();
    assert!(
        parse_errors.is_empty(),
        "Expected no parse errors for `import type from = require(...)`, got {parse_errors:?}"
    );
}

#[test]
fn test_regex_named_capturing_groups_do_not_emit_unexpected_paren() {
    let source = r#"const re = /(?<year>\d{4})-(?<month>\d{2})-(?<day>\d{2})/u;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1508: Vec<_> = diagnostics.iter().filter(|d| d.code == 1508).collect();
    assert!(
        ts1508.is_empty(),
        "Expected valid named capturing groups to avoid TS1508, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_unicode_brace_escape_variants_do_not_emit_ts1125() {
    let source = r#"
const a = /\u{-DDDD}/gu;
const b = /\u{r}\u{n}\u{t}/gu;
const c = /\u{}/gu;
"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();
    let ts1125: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();
    assert!(
        ts1125.is_empty(),
        "Expected brace-form regex unicode escapes to avoid TS1125, got {diagnostics:?}"
    );
}

// =============================================================================
// Bare Hash Character Recovery (TS1127)
// =============================================================================

#[test]
fn test_bare_hash_at_top_level_emits_ts1127() {
    // Bare `#` at top level should emit TS1127, not cascading errors
    let source = "# foo";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#', got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_bare_hash_in_class_emits_ts1127() {
    // Bare `#` in class body should emit TS1127, not cascading errors
    let source = r"
class C {
    # name;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert!(
        ts1127_count >= 1,
        "Expected TS1127 for bare '#' in class body, got diagnostics: {diagnostics:?}"
    );
    // Should NOT cascade into TS1003/TS1005/TS1068/TS1128
    let cascade_count = diagnostics
        .iter()
        .filter(|d| matches!(d.code, 1003 | 1005 | 1068 | 1128))
        .count();
    assert_eq!(
        cascade_count, 0,
        "Bare '#' should not cascade into other errors, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_valid_private_name_no_ts1127() {
    // Valid private names should not emit TS1127
    let source = r"
class C {
    #name = 42;
    get #value() { return this.#name; }
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1127_count = diagnostics.iter().filter(|d| d.code == 1127).count();
    assert_eq!(
        ts1127_count, 0,
        "Valid private names should not emit TS1127, got diagnostics: {diagnostics:?}"
    );
}

// =============================================================================
// Nullable Type Syntax Recovery (TS17019/TS17020)
// =============================================================================

#[test]
fn test_postfix_question_emits_ts17019() {
    // `string?` should emit TS17019, not TS1005 or TS1110
    let source = "let x: string?;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    assert!(
        ts17019_count >= 1,
        "Expected TS17019 for postfix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1005 or TS1110 cascade
    let ts1005_count = diagnostics.iter().filter(|d| d.code == 1005).count();
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1005_count, 0,
        "Should not emit TS1005 for nullable type, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_question_emits_ts17020() {
    // `?string` should emit TS17020, not TS1110
    let source = "let x: ?string;";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for prefix '?' on type, got diagnostics: {diagnostics:?}"
    );
    // Should NOT emit TS1110 cascade
    let ts1110_count = diagnostics.iter().filter(|d| d.code == 1110).count();
    assert_eq!(
        ts1110_count, 0,
        "Should not emit TS1110 for nullable type, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_question_simplifies_ts17020_suggestions() {
    for (input, expected) in [
        ("unknown", "unknown"),
        ("never", "never"),
        ("void", "void"),
        ("undefined", "null | undefined"),
        ("null", "null | undefined"),
        ("number", "number | null | undefined"),
    ] {
        let source = format!("let x: ?{input};");
        let (parser, _root) = parse_source_named(&format!("{input}.ts"), &source);

        let diagnostic = parser
            .get_diagnostics()
            .iter()
            .find(|d| d.code == 17020)
            .unwrap_or_else(|| {
                panic!(
                    "Expected TS17020 for ?{input}, got {:?}",
                    parser.get_diagnostics()
                )
            });
        assert_eq!(
            diagnostic.message,
            format!(
                "'?' at the start of a type is not valid TypeScript syntax. Did you mean to write '{expected}'?"
            ),
            "wrong TS17020 suggestion for ?{input}"
        );
    }
}

#[test]
fn test_multiple_nullable_types() {
    // Multiple nullable types in different positions
    let source = r"
function f(x: string?): ?number {
    return null;
}
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17019_count = diagnostics.iter().filter(|d| d.code == 17019).count();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17019_count >= 1,
        "Expected at least 1 TS17019 for postfix '?', got diagnostics: {diagnostics:?}"
    );
    assert!(
        ts17020_count >= 1,
        "Expected at least 1 TS17020 for prefix '?', got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_in_type_predicate() {
    // `x is ?string` should emit TS17020
    let source = "function f(x: any): x is ?string { return true; }";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts17020_count = diagnostics.iter().filter(|d| d.code == 17020).count();
    assert!(
        ts17020_count >= 1,
        "Expected TS17020 for '?string' in type predicate, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_nullable_type_no_cascade() {
    // Nullable type should not cause cascading errors
    let source = r#"
let a: string? = "hello";
let b: ?number = 42;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    // Should only have TS17019 and TS17020, no cascade
    let cascade_codes: Vec<u32> = diagnostics
        .iter()
        .filter(|d| d.code == 1005 || d.code == 1109 || d.code == 1110 || d.code == 1128)
        .map(|d| d.code)
        .collect();
    assert!(
        cascade_codes.is_empty(),
        "Nullable types should not cause cascading errors, got: {cascade_codes:?}. All: {diagnostics:?}"
    );
}

#[test]
fn test_adjacent_jsx_roots_in_tsx_report_ts2657() {
    let source = r"
declare namespace JSX { interface Element { } }

<div></div>
<div></div>

var x = <div></div><div></div>
";
    let (parser, _root) = parse_source_named("test.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let ts2657_count = diagnostics.iter().filter(|d| d.code == 2657).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    // tsc emits TS2657 for adjacent JSX roots in ALL JSX files (.tsx, .jsx, .js)
    assert!(
        ts2657_count >= 1,
        "Expected TS2657 for adjacent JSX siblings in TSX, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Adjacent JSX recovery should not leak TS1003, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 0,
        "Adjacent JSX recovery should not leak TS1109, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_jsx_type_arguments_in_js_report_ts2657() {
    let source = r#"
/// <reference path="/.lib/react.d.ts" />
import { MyComp, Prop } from "./component";
import * as React from "react";

let x = <MyComp<Prop> a={10} b="hi" />; // error, no type arguments in js
"#;
    let (parser, _root) = parse_source_named("file.jsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&2657),
        "Expected TS2657 for JSX type arguments in JS recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&1003),
        "Expected TS1003 alongside TS2657 for illegal JSX type-argument syntax, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_call_type_argument_syntax_prefers_relational_parsing() {
    let source = r#"
Foo<number>();
Foo<number>(1);
Foo<number>``;
"#;
    let (parser, _root) = parse_source_named("a.jsx", source);

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();

    assert_eq!(
        ts1109_count, 1,
        "Expected only the empty-call JS generic syntax case to emit TS1109, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 0,
        "Non-JSX JS generic-call syntax should not leak JSX TS1003 recovery diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_jsx_type_arguments_in_js_with_closing_tag_report_ts17002() {
    let source = r#"
<Foo<number>></Foo>;
"#;
    let (parser, _root) = parse_source_named("a.jsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17002),
        "Expected TS17002 for the mismatched closing tag after JS JSX type-argument recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&2657),
        "Expected TS2657 for the recovered adjacent JSX roots, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_let_array_ambiguity_reports_ts1181_then_statement_recovery() {
    let source = r#"
var let: any;
let[0] = 100;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1181, 1005, 1128],
        "Expected TS1181/TS1005/TS1128 recovery for ambiguous `let[` statement, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_for_header_let_disambiguation_matches_invalid_for_of_recovery() {
    let source = r#"
var let = 10;
for (let of [1,2,3]) {}

for (let in [1,2,3]) {}
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![1005, 1181, 1005, 1128],
        "Expected TS1005/TS1181/TS1005/TS1128 recovery for `for (let of [...])`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_invalid_nonnullable_type_recovery_reports_ts17019_and_ts17020() {
    let source = r#"
function f1(a: string): a is string! { return true; }
function f2(a: string): a is !string { return true; }
const a = 1 as any!;
const b = 1 as !any;
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![17019, 17020, 17019, 17020],
        "Expected TS17019/TS17020 recovery for invalid non-nullable type syntax, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_unclosed_jsx_fragment_after_unary_plus_in_tsx_reports_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let (parser, _root) = parse_source_named("index.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17014),
        "Expected TSX unary `+ <>` recovery to report TS17014, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unclosed_jsx_fragment_after_unary_plus_reports_ts17014() {
    let source = r#"
const x = "oops";
const y = + <> x;
"#;
    let (parser, _root) = parse_source_named("index.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&17014),
        "Expected TS17014 for JS unary `+ <>` JSX-fragment recovery, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unary_tilde_then_malformed_jsx_reports_ts1003() {
    let source = "~< <";
    let (parser, _root) = parse_source_named("a.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let ts1109_count = diagnostics.iter().filter(|d| d.code == 1109).count();

    assert!(
        codes.contains(&1003),
        "Expected TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1003_count, 1,
        "Expected exactly one TS1003 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 1,
        "Expected exactly one trailing TS1109 for malformed JSX after unary `~`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unary_plus_then_numeric_jsx_head_reports_ts1003_without_ts1109() {
    let source = r#"
const x = "oops";
const y = + <1234> x;
"#;
    let (parser, _root) = parse_source_named("index.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed JSX tag head `<1234>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 fallback for malformed numeric JSX tag head, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_unary_plus_mixed_type_assertion_and_fragment_matches_conformance_shape() {
    let source = r#"
const x = "oops";

const a = + <number> x;
const b = + <> x;
const c = + <1234> x;
"#;
    let (parser, _root) = parse_source_named("index.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17008 from unary `+ <number> x` JSX recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17014 from unary `+ <> x` JSX recovery, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed numeric JSX tag head `<1234>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT),
        "Expected TS1382 on malformed numeric JSX tag head close token, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPECTED),
        "Expected TS1005 recovery tail after malformed JSX unary expressions, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_js_unary_bang_then_braced_jsx_head_reports_ts17008_without_ts1109() {
    let source = "!< {:>";
    let (parser, _root) = parse_source_named("a.js", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::IDENTIFIER_EXPECTED),
        "Expected TS1003 for malformed braced JSX tag head, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::JSX_ELEMENT_HAS_NO_CORRESPONDING_CLOSING_TAG),
        "Expected TS17008 unclosed JSX element recovery for `!< {{:>`, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 fallback for malformed braced JSX tag head, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_malformed_extends_in_generic_arrow_ambiguity_prefers_jsx_ts1382() {
    let source = r#"
declare namespace JSX {
    interface Element { isElement; }
}

var x4 = <T extends={true}>() => {}</T>;
x4.isElement;

var x5 = <T extends>() => {}</T>;
x5.isElement;
"#;
    let (parser, _root) = parse_source_named("file.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
    let ts1382_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::UNEXPECTED_TOKEN_DID_YOU_MEAN_OR_GT)
        .count();

    assert!(
        ts1382_count >= 2,
        "Expected malformed `extends` TSX ambiguity to emit TS1382 on both forms, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::TYPE_EXPECTED),
        "Expected no TS1110 Type expected diagnostics for malformed `extends` JSX ambiguity, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected no TS1109 diagnostics for malformed `extends` JSX ambiguity, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_jsx_and_type_assertion_conformance_codes_exclude_ts1003() {
    let source = r#"
declare var createElement: any;

class foo {}

var x: any;
x = <any> { test: <any></any> };

x = <any><any></any>;
 
x = <foo>hello {<foo>{}} </foo>;

x = <foo test={<foo>{}}>hello</foo>;

x = <foo test={<foo>{}}>hello{<foo>{}}</foo>;

x = <foo>x</foo>, x = <foo/>;

<foo>{<foo><foo>{/foo/.test(x) ? <foo><foo></foo> : <foo><foo></foo>}</foo>}</foo>
"#;
    let (parser, _root) = parse_source_named("jsxAndTypeAssertion.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let ts1003_count = diagnostics.iter().filter(|d| d.code == 1003).count();
    let malformed_jsx_statement_terminators = [
        "x = <foo>hello {<foo>{}} </foo>;",
        "x = <foo test={<foo>{}}>hello</foo>;",
        "x = <foo test={<foo>{}}>hello{<foo>{}}</foo>;",
    ]
    .into_iter()
    .map(|statement| {
        source
            .find(statement)
            .map(|start| start as u32 + statement.len() as u32 - 1)
            .expect("target JSX statement should exist")
    })
    .collect::<Vec<_>>();

    assert_eq!(
        ts1003_count, 0,
        "Expected no TS1003 for jsxAndTypeAssertion.tsx parser diagnostics, got diagnostics: {diagnostics:?}"
    );
    for semicolon_pos in malformed_jsx_statement_terminators {
        assert!(
            diagnostics.iter().any(|diag| {
                diag.code == diagnostic_codes::EXPECTED
                    && diag.start == semicolon_pos
                    && diag.message == "'}' expected."
            }),
            "Expected TS1005 \"'}}' expected.\" at malformed JSX statement terminator pos {semicolon_pos}, got diagnostics: {diagnostics:?}"
        );
    }
}

#[test]
fn test_type_literal_invalid_member_lt_minus_reports_ts1109_not_ts1128() {
    let source = r#"
var f: {
    x: number;
    <-
};
"#;
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&diagnostic_codes::TYPE_PARAMETER_DECLARATION_EXPECTED),
        "Expected TS1139 from malformed call-signature type parameters, got diagnostics: {diagnostics:?}"
    );
    assert!(
        codes.contains(&diagnostic_codes::EXPRESSION_EXPECTED),
        "Expected TS1109 at the type-literal synchronizing close brace, got diagnostics: {diagnostics:?}"
    );
    assert!(
        !codes.contains(&diagnostic_codes::DECLARATION_OR_STATEMENT_EXPECTED),
        "Expected no top-level TS1128 stray-brace cascade for `<-` type-member recovery, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_fragment_errors_conformance_shape_matches_mismatch_then_eof_sequence() {
    let source = r#"
declare namespace JSX {
	interface Element { }
	interface IntrinsicElements {
		[s: string]: any;
	}
}
declare var React: any;

<>hi</div>

<>eof
"#;
    let (parser, _root) = parse_source_named("file.tsx", source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
            diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            diagnostic_codes::EXPECTED,
        ],
        "Expected TS17015/TS17014/TS1005 recovery for malformed + EOF JSX fragments, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_fragment_errors_actual_conformance_file_matches_expected_codes() {
    let fixture_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../TypeScript/tests/cases/conformance/jsx/tsxFragmentErrors.tsx"
    );
    let source = match std::fs::read_to_string(fixture_path) {
        Ok(source) => source,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return;
        }
        Err(err) => {
            panic!("failed to read tsxFragmentErrors conformance fixture {fixture_path}: {err}")
        }
    };
    let (parser, _root) = parse_source_named("file.tsx", &source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
            diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
            diagnostic_codes::EXPECTED,
        ],
        "Expected TS17015/TS17014/TS1005 on actual tsxFragmentErrors conformance file, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_tsx_fragment_errors_stripped_source_matches_expected_positions() {
    let source = r#"
declare namespace JSX {
	interface Element { }
	interface IntrinsicElements {
		[s: string]: any;
	}
}
declare var React: any;

<>hi</div> // Error

<>eof   // Error
"#
    .to_string();
    let line_map = LineMap::build(&source);
    let (parser, _root) = parse_source_named("file.tsx", &source);

    let diagnostics = parser.get_diagnostics();
    let actual: Vec<(u32, u32, u32)> = diagnostics
        .iter()
        .filter(|diag| {
            matches!(
                diag.code,
                diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT
                    | diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG
            )
        })
        .map(|diag| {
            let pos = line_map.offset_to_position(diag.start, &source);
            (diag.code, pos.line + 1, pos.character + 1)
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (
                diagnostic_codes::EXPECTED_CORRESPONDING_CLOSING_TAG_FOR_JSX_FRAGMENT,
                10,
                7,
            ),
            (
                diagnostic_codes::JSX_FRAGMENT_HAS_NO_CORRESPONDING_CLOSING_TAG,
                10,
                11,
            ),
        ],
        "Expected JSX fragment recovery positions to match tsc for tsxFragmentErrors.tsx, got {diagnostics:?}"
    );
}

#[test]
fn test_trailing_decimal_numeric_literal_recovery_matches_conformance_shape() {
    let source = "1.toString();\nvar test2 = 2.toString();\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();

    assert_eq!(
        codes,
        vec![
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::AN_IDENTIFIER_OR_KEYWORD_CANNOT_IMMEDIATELY_FOLLOW_A_NUMERIC_LITERAL,
            diagnostic_codes::EXPECTED,
            diagnostic_codes::EXPRESSION_EXPECTED,
        ],
        "Trailing-decimal recovery should match the numeric literal conformance shape, got diagnostics: {diagnostics:?}"
    );

    let standalone_identifier_pos = source.find("toString").unwrap();
    assert!(
        diagnostics
            .iter()
            .all(|diag| !(diag.code == diagnostic_codes::EXPECTED
                && diag.start as usize == standalone_identifier_pos)),
        "Standalone `1.toString()` should not emit a spurious missing-semicolon diagnostic: {diagnostics:?}"
    );

    let var_stmt_start = source.find("var test2 = 2.toString();").unwrap();
    let open_paren_pos = var_stmt_start + "var test2 = 2.toString".len();
    let close_paren_pos = open_paren_pos + 1;

    let ts1005 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPECTED)
        .expect("expected TS1005 for the recovered call tail");
    assert_eq!(
        ts1005.start as usize, open_paren_pos,
        "TS1005 should anchor at the opening paren after the recovered identifier tail: {diagnostics:?}"
    );

    let ts1109 = diagnostics
        .iter()
        .find(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .expect("expected TS1109 for the empty recovered call expression");
    assert_eq!(
        ts1109.start as usize, close_paren_pos,
        "TS1109 should anchor at the closing paren after the recovered empty call: {diagnostics:?}"
    );
}

#[test]
fn test_decorator_type_assertion_reports_brace_expected_and_expression_expected_at_end_of_type_token()
 {
    let source = "@<[[import(obju2c77,\n";
    let (parser, _root) = parse_source_named("parseUnmatchedTypeAssertion.ts", source);

    let diagnostics = parser.get_diagnostics();
    let ts1109_positions: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .map(|diag| diag.start as usize)
        .collect();
    let ts1005_positions: Vec<_> = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED)
        .map(|diag| diag.start)
        .collect();

    let ts1109_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();
    assert_eq!(
        ts1109_count, 1,
        "Decorator type assertion recovery should emit one TS1109 diagnostic at the type assertion start, got {diagnostics:?}"
    );
    assert_eq!(
        ts1109_positions,
        vec![1],
        "TS1109 should anchor at the decorator type assertion start, got positions: {ts1109_positions:?}. Full diagnostics: {diagnostics:?}"
    );

    let ts1005_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::EXPECTED)
        .count();
    assert_eq!(
        ts1005_count, 1,
        "Decorator type assertion recovery should emit a single TS1005 for the missing class body brace, got {diagnostics:?}"
    );
    assert_eq!(
        ts1005_positions,
        vec![21],
        "TS1005 should anchor at decorator tail, got positions: {ts1005_positions:?}. Full diagnostics: {diagnostics:?}"
    );
    let ts1146_count = diagnostics
        .iter()
        .filter(|diag| diag.code == diagnostic_codes::DECLARATION_EXPECTED)
        .count();
    assert_eq!(
        ts1146_count, 0,
        "Decorator type assertion recovery should not emit TS1146, got {diagnostics:?}"
    );
}

#[test]
fn test_ts1125_tagged_template_does_not_emit_errors() {
    // Tagged templates (ES2018+) allow invalid escape sequences per spec.
    // tsc does NOT emit TS1125 for tagged templates — only for untagged templates.
    let source =
        r#"const x = tag`\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // Tagged templates should NOT get TS1125 errors
    assert_eq!(
        ts1125_diagnostics.len(),
        0,
        "Expected 0 TS1125 errors for tagged template, got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}

#[test]
fn test_ts1125_untagged_template_emits_errors() {
    // Untagged templates with invalid escape sequences DO get TS1125.
    let source =
        r#"const y = `\u{hello} ${ 100 } \xtraordinary ${ 200 } wonderful ${ 300 } \uworld`;"#;
    let (parser, _root) = parse_source(source);
    let diagnostics = parser.get_diagnostics();

    let ts1125_diagnostics: Vec<_> = diagnostics.iter().filter(|d| d.code == 1125).collect();

    // We should get 3 TS1125 errors (for \u{hello}, \xtraordinary, \uworld)
    assert_eq!(
        ts1125_diagnostics.len(),
        3,
        "Expected 3 TS1125 errors (for \\u{{hello}}, \\xtraordinary, \\uworld), got {}: {:?}",
        ts1125_diagnostics.len(),
        ts1125_diagnostics
    );
}

#[test]
fn test_prefix_unary_without_operand_emits_ts1109_after_prior_ts1005() {
    // `var a = q~;` — after parsing `var a = q`, the `~` triggers TS1005
    // (',' expected) in the variable declaration list. Recovery then re-enters
    // statement parsing and treats `~;` as a prefix-unary expression with a
    // missing operand. tsc emits TS1109 at the `;` even though TS1005 was just
    // reported one column earlier; our distance-based error suppression used
    // to swallow the TS1109 because the two positions are within three
    // characters. Verify both diagnostics are now emitted.
    let source = "var a = q~;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1005_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPECTED)
        .count();
    let ts1109_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();

    assert_eq!(
        ts1005_count, 1,
        "Expected exactly one TS1005 (',' expected) for `var a = q~;`, got diagnostics: {diagnostics:?}"
    );
    assert_eq!(
        ts1109_count, 1,
        "Expected TS1109 (Expression expected) at the `;` after `~` for `var a = q~;`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_prefix_unary_tilde_missing_operand_emits_ts1109_after_initializer() {
    // `var b =~;` — the initializer is parsed as `~` with a missing operand.
    // tsc emits TS1109 at the `;`. This path has no prior parser error so it
    // does not exercise the suppression-bypass, but it pins down the baseline
    // behaviour alongside the prior-error variant above.
    let source = "var b =~;\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let ts1109_count = diagnostics
        .iter()
        .filter(|d| d.code == diagnostic_codes::EXPRESSION_EXPECTED)
        .count();

    assert_eq!(
        ts1109_count, 1,
        "Expected exactly one TS1109 at the `;` for `var b =~;`, got diagnostics: {diagnostics:?}"
    );
}

#[test]
fn test_bitwise_not_invalid_operations_matches_tsc_diagnostics() {
    // Matches the conformance test
    // TypeScript/tests/cases/conformance/expressions/unaryOperators/
    // bitwiseNotOperator/bitwiseNotOperatorInvalidOperations.ts after the
    // test runner strips the `// @target:` directive. tsc emits exactly four
    // diagnostics:
    //   (5,10) TS1005 ',' expected.
    //   (5,11) TS1109 Expression expected.
    //   (8,27) TS1134 Variable declaration expected.
    //   (11,9) TS1109 Expression expected.
    let source = "\
// Unary operator ~
var q;

// operand before ~
var a = q~;  //expect error

// multiple operands after ~
var mul = ~[1, 2, \"abc\"], \"\";  //expect error

// miss an operand
var b =~;
";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut fingerprints: Vec<(u32, u32, u32)> = diagnostics
        .iter()
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (d.code, pos.line + 1, pos.character + 1)
        })
        .collect();
    fingerprints.sort();

    let mut expected = vec![
        (diagnostic_codes::EXPECTED, 5, 10),
        (diagnostic_codes::EXPRESSION_EXPECTED, 5, 11),
        (diagnostic_codes::VARIABLE_DECLARATION_EXPECTED, 8, 27),
        (diagnostic_codes::EXPRESSION_EXPECTED, 11, 9),
    ];
    expected.sort();

    assert_eq!(
        fingerprints, expected,
        "Diagnostic fingerprints must match tsc exactly, got: {diagnostics:?}"
    );
}

#[test]
fn test_array_terminated_by_close_paren_emits_comma_expected() {
    // Regression for conformance test
    // `destructuringParameterDeclaration2.ts` line 8:
    //   `a0([1, "string", [["world"]]);`
    // The outer `[` is never closed before the `)`. tsc reports a single TS1005
    // `',' expected.` at the `)`. Before this fix, we reported `']' expected.`
    // because the array-literal loop broke without first emitting the missing-
    // separator diagnostic that tsc's parseDelimitedList unconditionally emits.
    let source = "a0([1, \"string\", [[\"world\"]]);\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    let close_paren_pos = source.find(')').expect("`)` is in the source") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_paren_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at the `)`, got {diagnostics:?}"
    );
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_paren_pos
                && d.message == "']' expected."),
        "TS1005 `']' expected.` at the `)` should be dedup'd by the comma error, got {diagnostics:?}"
    );
}

#[test]
fn test_array_terminated_by_close_brace_emits_comma_expected() {
    // Sibling case: array literal terminated by an enclosing `}` (e.g. block
    // boundary). Same expectation — tsc reports `,' expected` rather than
    // `]' expected`.
    let source = "{ const x = [1, 2 }\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();

    let close_brace_pos = source.find('}').expect("`}` is in the source") as u32;
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == close_brace_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at the `}}`, got {diagnostics:?}"
    );
}

#[test]
fn test_array_terminated_by_close_bracket_keeps_clean_close() {
    // Sanity guard: a normal `[1, 2]` must not gain a spurious comma diagnostic.
    let source = "var a = [1, 2];\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics.is_empty(),
        "well-formed array literal must not emit diagnostics, got {diagnostics:?}"
    );
}

#[test]
fn test_parameter_list_stray_colon_recovers_through_object_binding_tail() {
    // Regression for `parametersSyntaxErrorNoCrash1.ts`. After the stray second
    // colon, tsc keeps parsing the following `{ return arg; }` as a malformed
    // object binding parameter, producing the full recovery tail.
    let source = "\n// https://github.com/microsoft/TypeScript/issues/59422\n\nfunction identity<T>(arg: T: T {\n    return arg;\n}";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line_map = LineMap::build(source);

    let mut fingerprints: Vec<(u32, u32, u32, String)> = diagnostics
        .iter()
        .map(|d| {
            let pos = line_map.offset_to_position(d.start, source);
            (d.code, pos.line + 1, pos.character + 1, d.message.clone())
        })
        .collect();
    fingerprints.sort();

    let mut expected = vec![
        (
            diagnostic_codes::EXPECTED,
            4,
            28,
            "',' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            4,
            32,
            "',' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            5,
            12,
            "':' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            5,
            15,
            "',' expected.".to_string(),
        ),
        (
            diagnostic_codes::EXPECTED,
            6,
            2,
            "')' expected.".to_string(),
        ),
    ];
    expected.sort();

    assert_eq!(
        fingerprints, expected,
        "parameter-list recovery fingerprints must match tsc, got: {diagnostics:?}"
    );
}

#[test]
fn test_object_literal_comma_recovery_after_short_distance_colon_error() {
    // Regression for conformance test
    // `conformance/classes/nestedClassDeclaration.ts`:
    //   `var x = {\n    class C4 {\n    }\n}`
    // tsc emits TWO TS1005 errors here:
    //   - `':' expected.` at column 11 (the `C` of `C4`)
    //   - `',' expected.` at column 14 (the `{`)
    // We previously emitted only the first because our `error_comma_expected`
    // applies a 3-byte distance suppression that swallows the legitimate comma
    // diagnostic when the gap is exactly 3 columns. tsc's `parseErrorAtPosition`
    // dedups only on exact same position; the unexpected-token recovery path in
    // `parse_object_literal` now bypasses the distance gate so it emits.
    let source = "var x = {\n    class C4 {\n    }\n}\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    let line2_offset = source.find("    class C4").expect("C4 line is in source") as u32;
    let c4_pos = line2_offset + "    class ".len() as u32; // position of `C` in `C4`
    let open_brace_pos = source.find("C4 {").expect("C4 { is in source") as u32 + 3; // position of `{`

    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == c4_pos
                && d.message == "':' expected."),
        "expected TS1005 `':' expected.` at `C4`, got {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::EXPECTED
                && d.start == open_brace_pos
                && d.message == "',' expected."),
        "expected TS1005 `',' expected.` at `{{` after `C4`, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_hex_escape_with_numeric_separator_no_ts1125() {
    // Regression for conformance test
    // `conformance/parser/ecmascript2021/numericSeparators/parser.numericSeparators.unicodeEscape.ts`:
    // tsc accepts `_` as a numeric-separator placeholder inside regex `\x` and
    // `\u` escapes (deferring strict hex grammar to the regex runtime), and
    // emits NO TS1125 for `/\xf_f/u` or `/\u_ffff/u`. We previously rejected
    // `_` at every hex-digit slot in the parser-level regex escape validator.
    let source = "/\\xf_f/u\n/\\uff_ff/u\n/\\u_ffff/u\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
        "regex `\\x`/`\\u` escapes with `_` separator must not emit TS1125, got {diagnostics:?}"
    );
}

#[test]
fn test_regex_hex_escape_keeps_real_hex_digit_validation() {
    // Sanity guard: `_` relaxation must not silence genuine non-hex chars.
    // For `/\u\i\c/` the `\u` is followed by `\` (not hex, not `_`), so TS1125
    // must still fire — matching tsc's `regularExpressionAnnexB.ts`.
    let source = "/\\u\\i\\c/\n";
    let (parser, _root) = parse_source(source);

    let diagnostics = parser.get_diagnostics();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code == diagnostic_codes::HEXADECIMAL_DIGIT_EXPECTED),
        "regex `\\u\\i...` must still emit TS1125 for non-hex non-separator chars, got {diagnostics:?}"
    );
}

// ---------------------------------------------------------------------------
// Invalid let-array recovery (state_statements_recovery module)
// ---------------------------------------------------------------------------

#[test]
fn invalid_let_array_reserved_word_emits_destructuring_diagnostic() {
    // `let [while]` — `while` is a reserved word; not a valid binding element.
    let source = "let [while];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [while]`, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_for_keyword_emits_destructuring_diagnostic() {
    // `let [for]` — different reserved word; same structural rule.
    let source = "let [for];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [for]`, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_numeric_literal_emits_destructuring_diagnostic() {
    // `let [42]` — numeric literal; not a binding name.
    let source = "let [42];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [42]`, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_string_literal_emits_destructuring_diagnostic() {
    // `let ["key"]` — string literal; not a binding name.
    let source = r#"let ["key"];"#;
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "expected array-element-destructuring diagnostic for `let [\"key\"]`, got {diags:?}",
    );
}

#[test]
fn valid_let_array_identifier_does_not_trigger_recovery() {
    // `let [x] = []` — valid destructuring; no recovery diagnostic.
    let source = "let [x] = [];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "valid `let [x] = []` must not emit array-element-destructuring diagnostic, got {diags:?}",
    );
}

#[test]
fn valid_let_array_empty_brackets_does_not_trigger_recovery() {
    // `let [] = []` — valid empty destructuring.
    let source = "let [] = [];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "valid `let [] = []` must not emit array-element-destructuring diagnostic, got {diags:?}",
    );
}

#[test]
fn valid_let_array_rest_element_does_not_trigger_recovery() {
    // `let [...rest] = []` — valid rest-element pattern.
    let source = "let [...rest] = [];\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags
            .iter()
            .any(|d| d.code == diagnostic_codes::ARRAY_ELEMENT_DESTRUCTURING_PATTERN_EXPECTED),
        "valid `let [...rest] = []` must not emit array-element-destructuring diagnostic, got {diags:?}",
    );
}

#[test]
fn invalid_let_array_recovery_does_not_crash_on_assignment() {
    // `let [+] = 1` — bad first element followed by `=`; parser must not panic.
    let source = "let [+] = 1;\n";
    let (parser, _root) = parse_source(source);
    let diags = parser.get_diagnostics();
    assert!(
        !diags.is_empty(),
        "expected at least one diagnostic for `let [+] = 1`, got none",
    );
}
