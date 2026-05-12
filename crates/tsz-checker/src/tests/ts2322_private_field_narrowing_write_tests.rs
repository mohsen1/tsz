//! Regression tests for false-positive TS2322 on private-field assignment after
//! a narrowing guard.
//!
//! When the checker resolves the type of an assignment target (`this.#field = v`),
//! it must use the *declared* type of the private field, not the flow-narrowed
//! (read) type. Previously `get_type_of_private_property_access` called
//! `apply_flow_narrowing` unconditionally, so inside `if (!this.#field)` the
//! assignment target type was narrowed to `null`, producing a spurious TS2322.
//!
//! Issue: #5924

use crate::test_utils::check_source_codes;

// ---------------------------------------------------------------------------
// Static private field (the original report)
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_on_static_private_field_assignment_after_null_guard() {
    let src = "
class Singleton {
    static #instance: Singleton | null = null;
    static get(): Singleton {
        if (!this.#instance) {
            this.#instance = new Singleton();
        }
        return this.#instance;
    }
}
";
    let c = check_source_codes(src);
    assert!(
        !c.contains(&2322),
        "Static private field: no TS2322 expected. Got: {c:?}"
    );
}

#[test]
fn no_false_positive_on_static_private_field_renamed() {
    let src = "
class Cache {
    static #shared: Cache | null = null;
    static instance(): Cache {
        if (!this.#shared) {
            this.#shared = new Cache();
        }
        return this.#shared;
    }
}
";
    let c = check_source_codes(src);
    assert!(
        !c.contains(&2322),
        "Renamed static private field: no TS2322 expected. Got: {c:?}"
    );
}

// ---------------------------------------------------------------------------
// Instance private field
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_on_instance_private_field_assignment_after_null_guard() {
    let src = "
class LazyInit {
    #value: string | null = null;
    getValue(): string {
        if (!this.#value) {
            this.#value = \"computed\";
        }
        return this.#value;
    }
}
";
    let c = check_source_codes(src);
    assert!(
        !c.contains(&2322),
        "Instance private field: no TS2322 expected. Got: {c:?}"
    );
}

#[test]
fn no_false_positive_on_instance_private_field_number_guard() {
    let src = "
class Counter {
    #count: number | null = null;
    init(): void {
        if (this.#count === null) {
            this.#count = 0;
        }
    }
}
";
    let c = check_source_codes(src);
    assert!(
        !c.contains(&2322),
        "Instance private number field: no TS2322 expected. Got: {c:?}"
    );
}

// ---------------------------------------------------------------------------
// Genuine TS2322 on private field must still fire
// ---------------------------------------------------------------------------

#[test]
fn genuine_ts2322_still_fires_on_private_field() {
    let src = "
class Foo {
    #x: number = 0;
    set(): void {
        this.#x = \"oops\";
    }
}
";
    let c = check_source_codes(src);
    assert!(
        c.contains(&2322),
        "Genuine type mismatch on private field: TS2322 must still fire. Got: {c:?}"
    );
}
