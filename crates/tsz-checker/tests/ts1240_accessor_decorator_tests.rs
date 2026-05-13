//! Tests for TS1240 on TC39 auto-accessor decorators.
//!
//! Structural rule: when `experimentalDecorators` is off and a class
//! member declaration carries one or more decorators, the checker selects
//! the first-arg type from the member kind — `ClassAccessorDecoratorTarget<This, V>`
//! for `accessor` fields, `undefined` for plain fields — and rejects (TS1240)
//! decorators whose resolved signature cannot accept `(first_arg, ANY)`.
//!
//! These tests intentionally vary type-parameter and identifier names to
//! confirm the fix is not keyed on user-chosen spellings (anti-hardcoding
//! per CLAUDE.md §25 / §26).

use tsz_checker::test_utils::check_source_codes;

/// The reported repro from #6652: a TC39 accessor decorator that takes a
/// generic `ClassAccessorDecoratorTarget<This, Value>` must not produce
/// TS1240. The interface is declared locally; the checker resolves it from
/// `file_locals` just like a globally-scoped lib declaration.
#[test]
fn ts1240_accessor_decorator_with_class_accessor_decorator_target_accepted() {
    let codes = check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}
interface ClassAccessorDecoratorContext<This, Value> {}
interface ClassAccessorDecoratorResult<This, Value> {}

function bound<T, V>(
    target: ClassAccessorDecoratorTarget<T, V>,
    context: ClassAccessorDecoratorContext<T, V>
): ClassAccessorDecoratorResult<T, V> | void {
    return {} as any;
}

class WithAccessor {
    @bound
    accessor value: number = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Expected no TS1240 for accessor decorator typed against ClassAccessorDecoratorTarget, got: {codes:?}"
    );
}

/// Anti-hardcoding: rename the decorator's type parameters from `T/V` to
/// `A/B` and the field identifier from `value` to `field` — the fix must
/// rest on the structural rule (accessor modifier + decorator first-arg
/// shape), not on the spelling.
#[test]
fn ts1240_accessor_decorator_renamed_type_parameters_accepted() {
    let codes = check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}

function deco<A, B>(
    target: ClassAccessorDecoratorTarget<A, B>,
    context: any
): void {}

class Renamed {
    @deco
    accessor field: string = "";
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Expected no TS1240 with renamed type parameters, got: {codes:?}"
    );
}

/// The fix must not depend on the field being instance-level. Static
/// auto-accessors share the same TC39 calling convention.
#[test]
fn ts1240_static_accessor_decorator_accepted() {
    let codes = check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}

function bound<T, V>(
    target: ClassAccessorDecoratorTarget<T, V>,
    context: any
): void {}

class WithStaticAccessor {
    @bound
    static accessor flag: boolean = false;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Expected no TS1240 for static auto-accessor decorator, got: {codes:?}"
    );
}

/// A decorator that accepts `any` (or no annotation) for the first
/// argument must remain compatible with the accessor calling convention.
#[test]
fn ts1240_accessor_decorator_with_any_first_param_accepted() {
    let codes = check_source_codes(
        r#"
function loose(_target: any, _context: any): void {}

class C {
    @loose
    accessor x = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Expected no TS1240 for accessor decorator with `any` first param, got: {codes:?}"
    );
}

/// Structural rule check: a decorator whose first parameter has a shape
/// genuinely incompatible with `ClassAccessorDecoratorTarget` must still
/// be rejected. The first arg here is `string`, which cannot accept an
/// object with `get`/`set` methods, so TS1240 is correct.
#[test]
fn ts1240_accessor_decorator_with_incompatible_first_arg_still_rejected() {
    let codes = check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}

function bad(_target: string, _context: any): void {}

class C {
    @bad
    accessor x = 0;
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Expected TS1240 for accessor decorator with incompatible first arg, got: {codes:?}"
    );
}

/// Non-callable decorator value (a number literal) is still rejected:
/// the fix narrows the first-arg shape, it does not soften the basic
/// callable check.
#[test]
fn ts1240_non_callable_accessor_decorator_rejected() {
    let codes = check_source_codes(
        r#"
const dec = 42;

class C {
    @dec
    accessor x = 0;
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Expected TS1240 for non-callable accessor decorator, got: {codes:?}"
    );
}

/// Plain-field regression guard: the existing TS1240 behavior for plain
/// fields (decorator first arg must accept `undefined`) must be
/// unchanged. A decorator whose first arg is `string` is still rejected
/// on a plain field.
#[test]
fn ts1240_plain_field_with_non_undefined_first_arg_still_rejected() {
    let codes = check_source_codes(
        r#"
function dec(_target: string, _context: any): void {}

class C {
    @dec
    x = 0;
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Expected TS1240 for plain field with non-undefined first arg, got: {codes:?}"
    );
}

/// Plain-field acceptance regression guard: a decorator typed against
/// `ClassFieldDecoratorContext` (and `undefined` first arg) must continue
/// to be accepted on a plain field.
#[test]
fn ts1240_plain_field_with_undefined_first_arg_accepted() {
    let codes = check_source_codes(
        r#"
interface ClassFieldDecoratorContext<T, V> {}

function fieldDec<T, V>(
    _target: undefined,
    _context: ClassFieldDecoratorContext<T, V>
): void {}

class C {
    @fieldDec
    x = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Expected no TS1240 for plain field with undefined first arg, got: {codes:?}"
    );
}

/// Stacking decorators: the first-arg shape is loop-invariant per member,
/// so multiple decorators on the same accessor must each see the
/// `ClassAccessorDecoratorTarget` shape (not have the gate fall back to a
/// per-decorator computation).
#[test]
fn ts1240_multiple_accessor_decorators_all_typed_against_target_accepted() {
    let codes = check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}

function a<T, V>(_t: ClassAccessorDecoratorTarget<T, V>, _c: any): void {}
function b<T, V>(_t: ClassAccessorDecoratorTarget<T, V>, _c: any): void {}

class Stacked {
    @a
    @b
    accessor x = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Expected no TS1240 for stacked accessor decorators, got: {codes:?}"
    );
}

/// Decorator-factory form (`@bound()` rather than `@bound`): the check
/// resolves the call expression's return type as the decorator value, so
/// a factory returning a compatible decorator must also be accepted.
#[test]
fn ts1240_accessor_decorator_factory_call_accepted() {
    let codes = check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}

function make<T, V>(): (
    target: ClassAccessorDecoratorTarget<T, V>,
    context: any
) => void {
    return (_t, _c) => {};
}

class C {
    @make()
    accessor x = 0;
}
"#,
    );

    assert!(
        !codes.contains(&1240),
        "Expected no TS1240 for accessor decorator factory call, got: {codes:?}"
    );
}

/// The accessor decorator's first-arg shape must NOT be required on
/// plain-field decorators. A decorator typed against
/// `ClassAccessorDecoratorTarget` applied to a plain field is correctly
/// rejected (the runtime would pass `undefined`, not a target object).
#[test]
fn ts1240_accessor_typed_decorator_on_plain_field_rejected() {
    let codes = check_source_codes(
        r#"
interface ClassAccessorDecoratorTarget<This, Value> {
    get(this: This): Value;
    set(this: This, value: Value): void;
}

function accessorOnly<T, V>(
    _target: ClassAccessorDecoratorTarget<T, V>,
    _context: any
): void {}

class C {
    @accessorOnly
    x = 0;
}
"#,
    );

    assert!(
        codes.contains(&1240),
        "Expected TS1240 for accessor-typed decorator on plain field, got: {codes:?}"
    );
}
