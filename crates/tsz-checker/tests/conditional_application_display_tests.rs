//! A conditional-bodied generic type-alias application loses tsc's `aliasSymbol`
//! once the conditional reduces, so the solver formatter renders the evaluated
//! result structurally for any concrete shape (tuple, array, object, primitive,
//! `never`) in the nested elaboration positions of assignment diagnostics —
//! `{ p: TupleBox<string> }` shows `{ p: [string]; }`, not
//! `{ p: TupleBox<string>; }`. Previously only object results expanded.
//!
//! Two boundaries keep this honest:
//! * Bare literal / union results stay on the application surface because tsc
//!   applies literal-union display widening there (a separate display concern).
//! * A non-converged recursive reduction (a truncated cycle) keeps the alias
//!   name rather than rendering a partial expansion.
//!
//! Verified against `tsc` 6.0.2. Binder names are varied across the matrix so the
//! rule is proven structural, not keyed on a particular identifier.

use tsz_checker::test_utils::check_source_diagnostics;

/// Collect the rendered messages (primary + nested) for inspection.
#[track_caller]
fn messages(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for d in check_source_diagnostics(source) {
        out.push(d.message_text.clone());
        for r in &d.related_information {
            out.push(r.message_text.clone());
        }
    }
    out
}

#[track_caller]
fn assert_any_contains(source: &str, needle: &str) {
    let msgs = messages(source);
    assert!(
        msgs.iter().any(|m| m.contains(needle)),
        "expected a diagnostic containing {needle:?}, got: {msgs:#?}",
    );
}

#[track_caller]
fn assert_none_contains(source: &str, needle: &str) {
    let msgs = messages(source);
    assert!(
        !msgs.iter().any(|m| m.contains(needle)),
        "expected no diagnostic containing {needle:?}, got: {msgs:#?}",
    );
}

// ── Nested elaboration positions (rendered by the solver formatter) ──

#[test]
fn nested_conditional_application_tuple_renders_structurally() {
    let source = r#"
type TupleBox<T> = T extends string ? [T] : never;
declare const x: { p: TupleBox<string> };
const y: { p: number } = x;
"#;
    assert_any_contains(source, "[string]");
    assert_none_contains(source, "TupleBox<string>");
}

#[test]
fn nested_conditional_application_object_renders_structurally() {
    // Renamed binder (`Cell`/`E`) — structural, not identifier-keyed.
    let source = r#"
type Cell<E> = E extends number ? { v: E } : never;
declare const x: { p: Cell<1> };
const y: { p: string } = x;
"#;
    assert_any_contains(source, "{ v: 1; }");
    assert_none_contains(source, "Cell<1>");
}

#[test]
fn nested_conditional_application_array_renders_structurally() {
    let source = r#"
type Arr<T> = T extends number ? T[] : never;
declare const x: { p: Arr<1> };
const y: { p: string } = x;
"#;
    assert_any_contains(source, "1[]");
    assert_none_contains(source, "Arr<1>");
}

// ── Negative controls: mapped/object-bodied applications keep their name ──

#[test]
fn nested_mapped_application_keeps_alias_name() {
    // A mapped body (not conditional) keeps tsc's alias symbol, so the
    // application surface is preserved rather than expanded to its structural
    // object. Defined locally so the harness does not depend on `lib.es5`.
    let source = r#"
type Keep<T> = { [K in keyof T]: T[K] };
declare const x: { q: Keep<{ a: 1 }> };
const y: { q: { z: 1 } } = x;
"#;
    assert_any_contains(source, "Keep<");
}

#[test]
fn deferred_generic_conditional_keeps_branch_union() {
    // Still generic (free `T`): tsc shows the branch union, never expanding to a
    // concrete shape. Matches today's behavior; locks in the no-over-reach guard.
    let source = r#"
type F<T> = T extends number ? string : boolean;
function g<T>(p: F<T>): void { const y: number = p; }
"#;
    assert_any_contains(source, "string | boolean");
}
