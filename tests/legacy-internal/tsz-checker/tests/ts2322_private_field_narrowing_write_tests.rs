//! Regression tests for false-positive TS2322 on private-field assignment after
//! a narrowing guard.
//!
//! When the checker resolves the type of an assignment target (`this.#field = v`
//! or `ClassName.#field = v`), it must use the *declared* type of the private
//! field, not the flow-narrowed (read) type. Previously
//! `get_type_of_private_property_access` called `apply_flow_narrowing`
//! unconditionally, so inside `if (!this.#field)` the assignment target type was
//! narrowed to `null`, producing a spurious TS2322.
//!
//! Issue: #5924 (instance/static via `this`)
//! Issue: #6185 (static via class name `ClassName.#field`)

use crate::test_utils::check_source_codes;

fn assert_no_2322(src: &str) {
    let c = check_source_codes(src);
    assert!(!c.contains(&2322), "unexpected TS2322. Got: {c:?}");
}

fn assert_has_2322(src: &str) {
    let c = check_source_codes(src);
    assert!(c.contains(&2322), "expected TS2322, got none. Got: {c:?}");
}

// ---------------------------------------------------------------------------
// Static private field — via `this` (original report #5924)
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_on_static_private_field_assignment_after_null_guard() {
    assert_no_2322(
        "
class Singleton {
    static #instance: Singleton | null = null;
    static get(): Singleton {
        if (!this.#instance) {
            this.#instance = new Singleton();
        }
        return this.#instance;
    }
}
",
    );
}

#[test]
fn no_false_positive_on_static_private_field_renamed() {
    assert_no_2322(
        "
class Cache {
    static #shared: Cache | null = null;
    static instance(): Cache {
        if (!this.#shared) {
            this.#shared = new Cache();
        }
        return this.#shared;
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// Static private field — via class name (#6185)
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_class_name_static_private_field_null_guard() {
    assert_no_2322(
        "
class Singleton {
    static #instance: Singleton | null = null;
    static getInstance(): Singleton {
        if (!Singleton.#instance) {
            Singleton.#instance = new Singleton();
        }
        return Singleton.#instance;
    }
}
",
    );
}

#[test]
fn no_false_positive_class_name_static_private_field_null_guard_renamed() {
    assert_no_2322(
        "
class Registry {
    static #current: Registry | null = null;
    static acquire(): Registry {
        if (!Registry.#current) {
            Registry.#current = new Registry();
        }
        return Registry.#current;
    }
}
",
    );
}

#[test]
fn no_false_positive_class_name_static_private_field_equality_guard() {
    assert_no_2322(
        "
class Pool {
    static #head: Pool | null = null;
    static get(): Pool {
        if (Pool.#head === null) {
            Pool.#head = new Pool();
        }
        return Pool.#head;
    }
}
",
    );
}

#[test]
fn no_false_positive_class_name_static_private_field_equality_guard_renamed() {
    assert_no_2322(
        "
class Store {
    static #active: Store | null = null;
    static get(): Store {
        if (Store.#active === null) {
            Store.#active = new Store();
        }
        return Store.#active;
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// Instance private field — via `this`
// ---------------------------------------------------------------------------

#[test]
fn no_false_positive_on_instance_private_field_assignment_after_null_guard() {
    assert_no_2322(
        "
class LazyInit {
    #value: string | null = null;
    getValue(): string {
        if (!this.#value) {
            this.#value = \"computed\";
        }
        return this.#value;
    }
}
",
    );
}

#[test]
fn no_false_positive_on_instance_private_field_number_guard() {
    assert_no_2322(
        "
class Counter {
    #count: number | null = null;
    init(): void {
        if (this.#count === null) {
            this.#count = 0;
        }
    }
}
",
    );
}

// ---------------------------------------------------------------------------
// Genuine TS2322 on private field must still fire
// ---------------------------------------------------------------------------

#[test]
fn genuine_ts2322_still_fires_on_private_field() {
    assert_has_2322(
        "
class Foo {
    #x: number = 0;
    set(): void {
        this.#x = \"oops\";
    }
}
",
    );
}

#[test]
fn genuine_ts2322_still_fires_class_name_private_field() {
    assert_has_2322(
        "
class Bar {
    static #y: number = 0;
    static set(): void {
        Bar.#y = \"oops\";
    }
}
",
    );
}
