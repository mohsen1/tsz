//! A fully-instantiated generic type-alias application whose declared body is a
//! *reducing operator* — a conditional, an indexed access, a `keyof`, a
//! template literal, or a string-mapping intrinsic — loses tsc's `aliasSymbol`
//! once the operator resolves, so the diagnostic renders the evaluated result
//! structurally rather than as `Name<Args>` (issue #15368).
//!
//! Before this fix the solver's reduction gate matched a *conditional* body
//! only, so an indexed-access-bodied application (`Idx<{ x: { deep } }>`) and a
//! `keyof`-bodied application (`Keys<{ p: 1; q: 2 }>`) leaked their
//! `Name<Args>` surface where tsc shows `{ deep: boolean; }` and `"p" | "q"`.
//!
//! Boundaries kept honest here:
//! * A *surviving constructor* body — mapped / union / intersection / object —
//!   keeps its alias symbol (`Keep<{ a: 1 }>` stays `Keep<…>`).
//! * A *still-generic* application (a free type parameter in the args) keeps its
//!   spelling: tsc never drops the name before the operator can resolve.
//!
//! Verified against `tsc` 6.0.2. Binder names are varied across the matrix so
//! the rule is proven structural, not keyed on a particular identifier.

use tsz_checker::diagnostics::Diagnostic;
use tsz_checker::test_utils::check_source_diagnostics;

fn collect(diags: Vec<Diagnostic>) -> Vec<String> {
    let mut out = Vec::new();
    for d in diags {
        out.push(d.message_text.clone());
        for r in &d.related_information {
            out.push(r.message_text.clone());
        }
    }
    out
}

#[track_caller]
fn messages(source: &str) -> Vec<String> {
    collect(check_source_diagnostics(source))
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

// ── Indexed-access-bodied applications reduce structurally ──

#[test]
fn indexed_access_application_reduces_to_property_object() {
    // `Idx<{ x: { deep: boolean } }>` resolves to the `x` property type; tsc
    // renders the resolved object `{ deep: boolean; }`, not `Idx<…>`.
    let source = r#"
type Idx<Src extends { x: unknown }> = Src["x"];
const value: Idx<{ x: { deep: boolean } }> = 5;
"#;
    assert_any_contains(source, "{ deep: boolean; }");
    assert_none_contains(source, "Idx<");
}

#[test]
fn indexed_access_application_reduces_to_primitive_renamed_binder() {
    // Same structural rule, different identifiers, primitive result — proves the
    // reduction is not keyed on a particular alias or type-parameter name.
    let source = r#"
type Lookup<Container extends { field: unknown }> = Container["field"];
const value: Lookup<{ field: string }> = 5;
"#;
    assert_any_contains(source, "type 'string'");
    assert_none_contains(source, "Lookup<");
}

// ── `keyof`-bodied applications reduce to the literal key union ──

#[test]
fn keyof_application_reduces_to_literal_key_union() {
    // `keyof { p: 1; q: 2 }` is the literal union `"p" | "q"`; tsc renders it
    // verbatim (no literal-union widening, unlike a conditional's union result).
    let source = r#"
type Keys<Shape> = keyof Shape;
const value: Keys<{ p: 1; q: 2 }> = 5;
"#;
    assert_any_contains(source, "\"p\" | \"q\"");
    assert_none_contains(source, "Keys<");
}

#[test]
fn keyof_application_reduces_to_literal_key_union_renamed_binder() {
    let source = r#"
type NameSet<Bag> = keyof Bag;
const value: NameSet<{ alpha: 0; beta: 0 }> = 5;
"#;
    assert_any_contains(source, "\"alpha\" | \"beta\"");
    assert_none_contains(source, "NameSet<");
}

// ── Nested elaboration position ──

#[test]
fn indexed_access_application_reduces_in_nested_elaboration() {
    // The reduced object is shared with a subterm of the application's own
    // argument, so it carries a `display_alias` back to the application. The
    // reduction must render the structural shape (`{ z: 1; }`) rather than
    // chasing that alias back into the still-visiting application and eliding to
    // `...` (the #15368 nested regression this locks out).
    let source = r#"
type Inner<Wrap extends { inner: unknown }> = Wrap["inner"];
const value: { p: Inner<{ inner: { z: 1 } }> } = { p: 5 };
"#;
    assert_any_contains(source, "{ z: 1; }");
    assert_none_contains(source, "Inner<");
    assert_none_contains(source, "type '...'");
}

// ── Negatives: the alias name is preserved ──

#[test]
fn still_generic_indexed_access_application_keeps_alias_name() {
    // A free type parameter in the args leaves the operator deferred; tsc keeps
    // the `Idx<U>` spelling rather than reducing to a concrete shape.
    let source = r#"
type Idx<Src extends { m: unknown }> = Src["m"];
function f<U extends { m: unknown }>(v: Idx<U>) {
  const y: number = v;
}
"#;
    assert_any_contains(source, "Idx<U>");
}

#[test]
fn mapped_bodied_application_keeps_alias_name() {
    // A mapped body is a *surviving constructor*: tsc stamps its alias symbol on
    // the freshly-constructed object, so the diagnostic keeps `Keep<…>`. Proves
    // the reducing-operator classifier does not over-reach into mapped bodies.
    let source = r#"
type Keep<M> = { [K in keyof M]: M[K] };
declare const src: { q: Keep<{ a: 1 }> };
const dst: { q: { z: 1 } } = src;
"#;
    assert_any_contains(source, "Keep<");
}
