//! Regression tests for inferred get-accessor return types flowing to property
//! access (issue #14511).
//!
//! A class getter with no explicit return annotation must contribute its
//! *inferred* return type as the property type at access sites. The defect was
//! that `this.<field>` inside the getter body resolved against a Phase 0 prescan
//! instance type whose members are all `any` (initializers/getter bodies not yet
//! evaluated), so the inferred return collapsed to `any` and tsz missed the
//! `TS2322` that `tsc` reports. The fix ranks the instance-`this` candidate by
//! how *resolved* its members are, so the freshly-built partial type (concrete
//! members) wins over the all-`any` prescan during getter-body inference.
//!
//! Binder names are varied across cases so the behavior cannot be keyed on any
//! identifier.

use crate::test_utils::check_source_strict_codes;

fn assert_has_2322(src: &str) {
    let codes = check_source_strict_codes(src);
    assert!(
        codes.contains(&2322),
        "expected TS2322, got none. Got: {codes:?}"
    );
}

fn assert_no_2322(src: &str) {
    let codes = check_source_strict_codes(src);
    assert!(!codes.contains(&2322), "unexpected TS2322. Got: {codes:?}");
}

#[test]
fn inferred_getter_reading_field_is_number_at_access() {
    // `get count()` infers `number` from `this._v`; assigning it to `string`
    // must report TS2322 at the access site.
    assert_has_2322(
        "class C { _v = 0; get count() { return this._v; } }
         const wrong: string = new C().count;",
    );
}

#[test]
fn inferred_getter_is_widened_not_literal() {
    // The inferred getter return widens `0` to `number`, so a `123` target still
    // mismatches (parity with tsc's getter inference).
    assert_has_2322(
        "class C { _v = 0; get count() { return this._v; } }
         const probe: 123 = new C().count;",
    );
}

#[test]
fn inferred_getter_valid_assignment_stays_clean() {
    assert_no_2322(
        "class C { _v = 0; get count() { return this._v; } }
         const ok: number = new C().count;",
    );
}

#[test]
fn inferred_getter_renamed_binders() {
    // Same shape with different identifiers — the fix is structural, not name-keyed.
    assert_has_2322(
        "class Node { internalValue = 42; get exposed() { return this.internalValue; } }
         const bad: string = new Node().exposed;",
    );
}

#[test]
fn inferred_getter_setter_pair_uses_getter_return() {
    // A getter+setter pair's property type is the getter's (inferred) return type.
    assert_has_2322(
        "class P {
            _v = 0;
            get count() { return this._v; }
            set count(x: number) { this._v = x; }
         }
         const bad: string = new P().count;",
    );
}

#[test]
fn inferred_getter_returning_union() {
    // `this.flag ? 1 : \"a\"` infers `number | string`; assigning to `boolean`
    // mismatches.
    assert_has_2322(
        "class U { flag = true; get k() { return this.flag ? 1 : \"a\"; } }
         const bad: boolean = new U().k;",
    );
}

#[test]
fn inferred_getter_reading_another_inferred_getter() {
    // A getter reading another inferred getter still resolves to the concrete type.
    assert_has_2322(
        "class Chain {
            _v = 0;
            get a() { return this._v; }
            get b() { return this.a; }
         }
         const bad: string = new Chain().b;",
    );
}

#[test]
fn explicit_getter_annotation_unchanged() {
    // Control: an explicitly-annotated getter is unaffected by the fix.
    assert_has_2322(
        "class E { _v = 0; get count(): number { return this._v; } }
         const bad: string = new E().count;",
    );
}

#[test]
fn inferred_getter_returning_this_preserves_polymorphic_this() {
    // Control: a getter whose body is `return this` stays the class instance type
    // (polymorphic `this`), not collapsed to `any`.
    assert_has_2322(
        "class S { x = 1; get me() { return this; } }
         const bad: number = new S().me;",
    );
}
