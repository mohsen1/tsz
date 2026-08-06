//! Display parity for *generic* type-alias applications in assignability
//! diagnostics (issue #15368).
//!
//! tsc's `aliasSymbol` policy for a generic type-alias application is
//! structural: an application whose declared body is a *name-dropping reducing
//! operator* — a conditional, an indexed access, a `keyof`, a template literal,
//! a string-mapping intrinsic, or an alias chain forwarding to one — resolves
//! *into* the operator's result without stamping the enclosing alias, so once
//! the application is instantiated with concrete arguments tsc renders the
//! evaluated structural type. An application whose body is a *surviving
//! constructor* — mapped, union, intersection, object — keeps its alias symbol
//! and renders `Name<Args>`.
//!
//! The solver's `reducing_application_display` strategy (generalized
//! from conditional-only to the full reducing-operator set, with bounded
//! resolution of forwarded / recursive reductions) owns this for the direct
//! assignment-target render. Binder names vary so a hardcoded fix fails.
//!
//! Residuals (distinct owning mechanisms, tracked on #15368):
//! * recursive-conditional bodies (`Flatten<T>`) are kept as the annotation text
//!   by the checker's recursive-alias-application display guard, which exists to
//!   avoid unbounded `[42, [42, …]]` cascades;
//! * `keyof`-bodied applications reduce to a *union* whose member ordering tsz
//!   sorts canonically rather than by declaration position (the separate
//!   union-ordering concern), so they stay on the alias surface;
//! * mapped-bodied applications (`MappedAlias`) are over-reduced by an upstream
//!   checker path that hands the reporter an already-evaluated target — a
//!   separate mechanism from this display strategy;
//! * a union-bodied application (`UnionAlias<T> = T | undefined`) keeps its alias
//!   name *and* its literal source: the literal-source half was the last
//!   residual and is now fixed — a `T | undefined`/`T | null` target is
//!   singleton-capable (`undefined`/`null` are unit types), so the source `5`
//!   survives rather than generalizing to `number`.
use crate::test_utils::check_source_diagnostics;

#[track_caller]
fn ts2322_msg(source: &str) -> String {
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert_eq!(
        ts2322.len(),
        1,
        "expected exactly one TS2322, got: {:?}",
        diags
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
    ts2322[0].message_text.clone()
}

// --- Fixed by this change ------------------------------------------------

// Indexed-access-bodied application resolving to a single object shape drops the
// alias name and renders the resolved object.
#[test]
fn indexed_access_application_renders_resolved_object() {
    let msg = ts2322_msg(
        r#"
type IdxAlias<T> = T['x'];
const x: IdxAlias<{ x: { deep: boolean } }> = 0;
"#,
    );
    assert!(
        msg.contains("{ deep: boolean; }") && !msg.contains("IdxAlias"),
        "expected indexed-access application to render `{{ deep: boolean; }}`, got: {msg}"
    );
}

// Renamed binder, indexed access resolving to a scalar — proves the rule is
// structural, not keyed on a specific identifier.
#[test]
fn indexed_access_application_renders_resolved_scalar_renamed() {
    let msg = ts2322_msg(
        r#"
type Grab<Src> = Src['slot'];
const y: Grab<{ slot: string }> = 0;
"#,
    );
    assert!(
        msg.contains("type 'string'") && !msg.contains("Grab"),
        "expected indexed-access application to render `string`, got: {msg}"
    );
}

// Alias forwarding: a generic alias whose body is an *application* of another
// alias whose body is a conditional. tsc resolves through the chain and renders
// the resolved object.
#[test]
fn alias_forwarding_to_conditional_renders_resolved_object() {
    let msg = ts2322_msg(
        r#"
type CondResolved<T> = T extends unknown ? { a: string } : never;
type NestedCond<T> = CondResolved<T>;
const x: NestedCond<number> = 0;
"#,
    );
    assert!(
        msg.contains("{ a: string; }") && !msg.contains("NestedCond"),
        "expected forwarded conditional application to render `{{ a: string; }}`, got: {msg}"
    );
}

// Renamed forwarding chain reducing to a tuple.
#[test]
fn alias_forwarding_chain_renders_resolved_tuple_renamed() {
    let msg = ts2322_msg(
        r#"
type Pick2<Src> = Src extends unknown ? [string, number] : never;
type Forward<Src> = Pick2<Src>;
const z: Forward<boolean> = 0;
"#,
    );
    assert!(
        msg.contains("[string, number]") && !msg.contains("Forward"),
        "expected forwarded tuple application to render `[string, number]`, got: {msg}"
    );
}

// --- Negative / no-regression: surviving constructors keep the alias name ---

// A mapped-bodied application keeps its alias symbol in tsc; tsz currently
// over-reduces it (residual, distinct mechanism). Pin the current output so a
// future fix to that mechanism updates this deliberately rather than silently.
#[test]
fn mapped_application_current_behavior() {
    let msg = ts2322_msg(
        r#"
type MappedAlias<T> = { [K in keyof T]: T[K] };
const x: MappedAlias<{ m: string }> = 0;
"#,
    );
    // tsc: `MappedAlias<{ m: string; }>` — residual over-reduction (#15368).
    assert!(
        msg.contains("{ m: string; }"),
        "unexpected mapped-application render: {msg}"
    );
}

// A union-bodied application is a surviving constructor: tsc keeps its alias
// symbol (`UnionAlias<{ u: string; }>`) and the literal source `5`. The alias
// target was restored by the main #15368 fix; the *source* literal now survives
// too because the `T | undefined` target is singleton-capable (`undefined` is a
// unit type), so the diagnostic must not generalize `5` to `number`.
#[test]
fn union_application_keeps_literal_source_against_undefined_member() {
    let msg = ts2322_msg(
        r#"
type UnionAlias<T> = T | undefined;
const x: UnionAlias<{ u: string }> = 5;
"#,
    );
    assert!(
        msg.contains("Type '5'") && msg.contains("{ u: string; }"),
        "expected literal source `5` preserved against a singleton-capable union alias, got: {msg}"
    );
}

// The same rule with a `null` member — `null` is a unit type just like
// `undefined`, so the literal source is preserved. Renamed binder proves the
// decision is structural, not keyed on the alias/parameter spelling.
#[test]
fn union_application_keeps_literal_source_against_null_member_renamed() {
    let msg = ts2322_msg(
        r#"
type Nullable<Val> = Val | null;
const y: Nullable<{ u: string }> = 5;
"#,
    );
    assert!(
        msg.contains("Type '5'") && msg.contains("{ u: string; }"),
        "expected literal source `5` preserved against a `| null` union alias, got: {msg}"
    );
}

// An object-bodied generic application (a surviving constructor) keeps its alias
// name — the reducing strategy must not touch it.
#[test]
fn object_bodied_application_keeps_alias_name() {
    let msg = ts2322_msg(
        r#"
type Wrap<T> = { value: T };
const x: Wrap<number> = 0;
"#,
    );
    assert!(
        msg.contains("Wrap<number>") && !msg.contains("value"),
        "object-bodied application should keep its alias name, got: {msg}"
    );
}
