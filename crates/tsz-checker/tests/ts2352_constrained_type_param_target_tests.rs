//! Tests for TS2352 type-assertion overlap when the assertion target is a
//! constrained type parameter `T extends C`.
//!
//! Reported via #10676 (kysely false-positive `Readonly<X> as T` assertions).
//!
//! Structural rule, matching tsc's `checkAssertionDeferred`: the cast
//! `source as T extends C` is permitted iff source's REQUIRED object members
//! structurally fit C (allowing primitive↔literal comparable narrowing).
//! Source's optional members are ignored. If source has any REQUIRED member
//! not present in C with a comparable type, tsc emits TS2352 with the
//! "T could be instantiated with a different subtype of constraint"
//! elaboration.
//!
//! The check runs at the checker level so that interface-shaped constraints
//! (stored as `Lazy(DefId)` references on the type-parameter info) are
//! resolved before the structural walk — purely solver-level checks see an
//! opaque constraint and miss the structural overlap that tsc accepts.

use crate::test_utils::check_source_strict_codes as check_strict;

// ---------------------------------------------------------------------------
// Source structurally fits constraint → no TS2352
// ---------------------------------------------------------------------------

/// `Readonly<X> as T extends C` where X conforms to C must NOT emit TS2352.
/// Reduced from kysely's `operation-node-transformer.ts` row.
#[test]
fn readonly_concrete_subtype_as_constrained_type_param_no_ts2352() {
    for (op_name, sn_name, t_name) in [
        ("OperationNode", "SelectNode", "T"),
        ("Op", "SN", "U"),
        ("Base", "Concrete", "K"),
    ] {
        let source = format!(
            r#"
interface {op_name} {{ readonly kind: string }}
interface {sn_name} {{ readonly kind: 'SelectNode'; readonly stuff?: string }}

function transform<{t_name} extends {op_name}>(node: Readonly<{sn_name}>): {t_name} {{
    return node as {t_name};
}}
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            ts2352.is_empty(),
            "[{op_name}/{sn_name}/{t_name}] no TS2352 expected — `Readonly<{sn_name}>` \
             structurally fits constraint `{op_name}`. Got: {codes:?}"
        );
    }
}

/// Inline object literal with a literal-narrowed property assigned to a
/// constrained type parameter must NOT emit TS2352 — the literal at the
/// matching key is comparable to the constraint's primitive at the same key.
#[test]
fn inline_literal_property_as_constrained_type_param_no_ts2352() {
    let source = r#"
interface Op { kind: string }
function f<T extends Op>(): T {
    return { kind: 'foo' } as T;
}
function g<U extends { kind: string }>(): U {
    return { kind: 'bar' } as U;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `kind:'foo'`/`kind:'bar'` are comparable to constraint's \
         `kind:string`. Got: {codes:?}"
    );
}

/// A source whose only required member matches the constraint exactly must
/// NOT emit TS2352, even when the source carries additional OPTIONAL members.
#[test]
fn source_with_optional_extras_no_ts2352() {
    let source = r#"
interface Op { kind: string }
function f<T extends Op>(x: { kind: string; extra?: number }): T {
    return x as T;
}
function g<T extends Op>(x: { kind: 'a'; extra?: number }): T {
    return x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `extra` is optional, not a required extra. Got: {codes:?}"
    );
}

/// `{}` source against a constrained type parameter whose constraint
/// contains an object-like member must remain comparable. Anti-regression
/// for the existing `{}`-special-case rule.
#[test]
fn empty_object_as_constrained_type_param_no_ts2352() {
    let source = r#"
function yes<T extends object | null | undefined>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — `{{}}` overlaps with `object` member of T's constraint. \
         Got: {codes:?}"
    );
}

/// Source assigned to a constrained type parameter whose constraint is an
/// intersection of object-likes — ANY intersection member providing a
/// matching property is sufficient (the intersection exposes all members'
/// properties).
#[test]
fn source_fits_intersection_constraint_no_ts2352() {
    for (op_name, t_name) in [("Op", "T"), ("Base", "U"), ("Node", "K")] {
        let source = format!(
            r#"
interface {op_name} {{ kind: string }}
function f<{t_name} extends {op_name} & {{ other: number }}>(x: {{ kind: 'a'; other: 1 }}): {t_name} {{
    return x as {t_name};
}}
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            ts2352.is_empty(),
            "[{op_name}/{t_name}] no TS2352 expected — both required source members fit \
             members of the intersection constraint. Got: {codes:?}"
        );
    }
}

/// Source assigned to a constrained type parameter whose constraint is a
/// union of object-likes — SOME union member must provide the required
/// property for the cast to be accepted.
#[test]
fn source_fits_some_union_member_in_constraint_no_ts2352() {
    for (op_name, other_name, t_name) in [
        ("Op", "Other", "T"),
        ("First", "Second", "U"),
        ("A", "B", "K"),
    ] {
        let source = format!(
            r#"
interface {op_name} {{ kind: string }}
interface {other_name} {{ otherKey: number }}
function f<{t_name} extends {op_name} | {other_name}>(x: {{ kind: 'a' }}): {t_name} {{
    return x as {t_name};
}}
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            ts2352.is_empty(),
            "[{op_name}/{other_name}/{t_name}] no TS2352 expected — `kind:'a'` fits the \
             `{op_name}` member of the constraint union. Got: {codes:?}"
        );
    }
}

/// Anti-regression for the Lazy-wrapped union: when the constraint is named
/// via a type alias whose body is a union, the structural fit still applies
/// — the helper must resolve the alias before decomposing the union.
#[test]
fn source_fits_lazy_aliased_union_constraint_no_ts2352() {
    let source = r#"
interface Op { kind: string }
interface Other { otherKey: number }
type OpOrOther = Op | Other;
function f<T extends OpOrOther>(x: { kind: 'a' }): T {
    return x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.is_empty(),
        "no TS2352 expected — alias `OpOrOther` resolves to a union whose `Op` member fits \
         `kind:'a'`. Got: {codes:?}"
    );
}

// ---------------------------------------------------------------------------
// Source has REQUIRED extras beyond constraint → TS2352 still fires
// ---------------------------------------------------------------------------

/// Source class has a required member that does not exist in the constraint.
/// tsc emits TS2352 with the "different subtype of constraint" elaboration;
/// tsz must continue to emit TS2352. Mirrors `genericTypeAssertions4.ts`.
#[test]
fn subclass_with_required_extras_as_constrained_type_param_emits_ts2352() {
    for (a_name, b_name) in [("A", "B"), ("Base", "Derived"), ("Animal", "Dog")] {
        let source = format!(
            r#"
class {a_name} {{ foo() {{ return ""; }} }}
class {b_name} extends {a_name} {{ bar() {{ return 1; }} }}
declare let b: {b_name};
function f<T extends {a_name}>() {{
    let y = b as T;
}}
"#
        );
        let codes = check_strict(&source);
        let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
        assert!(
            !ts2352.is_empty(),
            "[{a_name}/{b_name}] TS2352 expected — `{b_name}` has REQUIRED `bar` not in \
             constraint `{a_name}`. Got: {codes:?}"
        );
    }
}

/// Source object literal with a REQUIRED extra property emits TS2352, even
/// when the matching property fits the constraint.
#[test]
fn object_literal_with_required_extra_emits_ts2352() {
    let source = r#"
interface Op { kind: string }
function f<T extends Op>(x: { kind: string; extra: number }): T {
    return x as T;
}
function g<T extends Op>(x: { kind: 'a'; extra: number }): T {
    return x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.len() >= 2,
        "TS2352 expected on both casts — `extra` is required and absent from constraint. \
         Got: {codes:?}"
    );
}

/// Source completely unrelated to constraint still emits TS2352. The
/// constraint-fit rule does not over-permit primitive/object mismatches.
#[test]
fn unrelated_shape_emits_ts2352() {
    let source = r#"
interface Op { kind: string }
function a<T extends Op>(x: { foo: number }): T { return x as T; }
function b<T extends Op>(x: number): T { return x as T; }
function c<T extends Op>(x: boolean): T { return x as T; }
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        ts2352.len() >= 3,
        "TS2352 expected on all three casts. Got: {codes:?}"
    );
}

/// Source against a constrained type parameter whose constraint is
/// `null | undefined` (no object-like member) must emit TS2352 even when
/// source is `{}`. Anti-regression for the empty-object special case.
#[test]
fn empty_object_as_typeparam_without_object_in_constraint_emits_ts2352() {
    let source = r#"
function f<T extends null | undefined>() {
    let x = {};
    x as T;
}
"#;
    let codes = check_strict(source);
    let ts2352: Vec<&u32> = codes.iter().filter(|c| **c == 2352).collect();
    assert!(
        !ts2352.is_empty(),
        "TS2352 expected — `{{}}` has no overlap with `null | undefined`. Got: {codes:?}"
    );
}
