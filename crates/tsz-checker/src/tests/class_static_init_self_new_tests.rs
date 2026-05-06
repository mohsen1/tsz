//! `new Derived(args)` inside Derived's own static-property initializer
//! must see the inherited construct signatures. Without this, the rough
//! partial constructor type used during static-member processing falls back
//! to a default 0-arg constructor, producing a false TS2554.

use crate::test_utils::check_source_diagnostics;

#[test]
fn new_derived_in_static_property_initializer_inherits_base_construct_arity() {
    let diags = check_source_diagnostics(
        r#"
class Base<Def> {
    constructor(def: Def) {}
}

class Derived extends Base<{ count: number }> {
    static create = (): Derived => {
        return new Derived({ count: 1 });
    };
}
"#,
    );

    let ts2554: Vec<_> = diags.iter().filter(|d| d.code == 2554).collect();
    assert!(
        ts2554.is_empty(),
        "Expected no TS2554 for `new Derived(...)` in static-property initializer; got: {diags:?}"
    );
}

#[test]
fn new_derived_in_static_method_inherits_base_construct_arity() {
    let diags = check_source_diagnostics(
        r#"
class Base<Def> {
    constructor(def: Def) {}
}

class Derived extends Base<{ count: number }> {
    static create(): Derived {
        return new Derived({ count: 1 });
    }
}
"#,
    );

    let ts2554: Vec<_> = diags.iter().filter(|d| d.code == 2554).collect();
    assert!(
        ts2554.is_empty(),
        "Expected no TS2554 for `new Derived(...)` in static method; got: {diags:?}"
    );
}
