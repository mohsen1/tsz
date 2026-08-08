//! Regression coverage for #16888: TS2403 ("Subsequent variable declarations
//! must have the same type") was silently dropped whenever both redeclared
//! types happened to render to the same simple display name, even though the
//! solver's `are_types_identical_for_redeclaration` (fixed for private/
//! protected brands in #16891) already correctly determined the two types
//! were NOT identical.
//!
//! The drop was not in the identity check itself but in the checker's
//! diagnostic-emission path: `error_subsequent_variable_declaration`
//! (`crates/tsz-checker/src/error_reporter/type_value.rs`) suppressed the
//! diagnostic whenever `prev_type_str == current_type_str` — a formatted
//! diagnostic string used as a semantic predicate, which the Anti-Hardcoding
//! Gate (`.claude/CLAUDE.md`) forbids. Two structurally-distinct classes that
//! both happen to be named `Foo` (in different namespaces/modules) rendered
//! identically and were wrongly suppressed, even though tsc reports TS2403
//! for exactly this shape (`isTypeIdenticalTo` does not care that both sides
//! print the same short name).
//!
//! This also fixes a byte-level message mismatch uncovered while unblocking
//! these cases: tsc's TS2403 template has two spaces after the first
//! sentence ("...must have the same type.  Variable ..."), oracle-verified
//! against pinned `typescript@7.0.2`; tsz's ad hoc `format!` call only had
//! one.
//!
//! All expected diagnostics oracle-verified against pinned `typescript@7.0.2`.

use tsz_checker::test_utils::check_source_diagnostics;

fn ts2403_count(source: &str) -> usize {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2403)
        .count()
}

fn ts2403_messages(source: &str) -> Vec<String> {
    check_source_diagnostics(source)
        .iter()
        .filter(|d| d.code == 2403)
        .map(|d| d.message_text.clone())
        .collect()
}

#[test]
fn same_simple_name_different_private_brand_emits_ts2403() {
    // The core #16888 repro: `A.Foo` and `B.Foo` are unrelated classes (each
    // carrying its own private member) that happen to share a display name.
    let source = r#"
namespace A { export class Foo { private n: number = 0; } }
namespace B { export class Foo { private n: number = 0; } }
var x: A.Foo;
var x: B.Foo;
"#;
    let messages = ts2403_messages(source);
    assert_eq!(
        messages.len(),
        1,
        "Expected exactly one TS2403 for distinct same-named private-branded classes: {messages:?}"
    );
    assert_eq!(
        messages[0],
        "Subsequent variable declarations must have the same type.  Variable 'x' must be of type 'Foo', but here has type 'Foo'.",
        "Message must byte-match tsc's two-space template"
    );
}

#[test]
fn same_simple_name_same_private_brand_stays_clean() {
    // Negative control: the SAME declaration referenced twice (via the same
    // namespace-qualified path) must not trigger TS2403 just because it has
    // a private member — it is genuinely identical, not merely same-named.
    let source = r#"
namespace A { export class Foo { private n: number = 0; } }
var x: A.Foo;
var x: A.Foo;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "Expected no TS2403: both declarations refer to the identical class"
    );
}

#[test]
fn same_simple_name_no_private_member_stays_clean() {
    // Negative control: two same-named, structurally identical PUBLIC-only
    // classes remain redeclaration-identical under plain structural
    // comparison (no private/protected brand involved), so this must not
    // regress now that the display-name suppression is gone.
    let source = r#"
namespace A { export class Foo { m: number = 0; } }
namespace B { export class Foo { m: number = 0; } }
var x: A.Foo;
var x: B.Foo;
"#;
    assert_eq!(
        ts2403_count(source),
        0,
        "Expected no TS2403: neither class has a private/protected member, so \
         plain structural identity applies even though both are named `Foo`"
    );
}
