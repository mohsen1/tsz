//! Focused tests for the `keyof` display-alias suppression rule (issue #9695).
//!
//! Kept in a dedicated shard rather than the oversized `tests.rs` so the
//! touched file stays well under the repository file-size cap.

use super::*;
use crate::construction::TypeInterner;
use crate::types::PropertyInfo;

#[test]
fn keyof_display_alias_does_not_repaint_unit_literal_union() {
    // `keyof R` evaluates to the interned literal union `"a" | "b"` and may
    // record a global `union -> KeyOf(R)` display alias. That alias must never
    // repaint a structurally identical user-written literal union: a bare
    // unit-literal union is what a user spells directly, and tsc renders it by
    // members. The `keyof Name` spelling is preserved through the `KeyOf` node,
    // not by repainting the shared union.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    // A named object `R` with a def so the keyof operand resolves to a name.
    let r_object = db.object(vec![
        PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER),
        PropertyInfo::new(db.intern_string("b"), TypeId::STRING),
    ]);
    let r_def = def_store.register(crate::def::DefinitionInfo::interface(
        db.intern_string("R"),
        vec![],
        vec![PropertyInfo::new(db.intern_string("a"), TypeId::NUMBER)],
    ));
    def_store.register_type_to_def(r_object, r_def);

    let keyof_r = db.keyof(r_object);
    let literal_union = db.union(vec![db.literal_string("a"), db.literal_string("b")]);
    db.store_display_alias(literal_union, keyof_r);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(literal_union),
        "\"a\" | \"b\"",
        "A unit-literal union must render by members, never as `keyof R`"
    );
}

#[test]
fn lazy_primitive_alias_renders_as_underlying_not_alias_name() {
    // `type N = number` used in a nested position arrives at the formatter as
    // `Lazy(N)`. tsc renders such a primitive-bodied alias as `number` (no
    // `aliasSymbol` is attached to the shared intrinsic), not as `N`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let n_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("N"),
        vec![],
        TypeId::NUMBER,
    ));
    let lazy_n = db.lazy(n_def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(lazy_n),
        "number",
        "A primitive-bodied type alias must render structurally, not by name"
    );
}

#[test]
fn lazy_literal_alias_renders_as_literal_not_alias_name() {
    // `type Greeting = "hello"` renders as `"hello"`, never `Greeting`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let body = db.literal_string("hello");
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Greeting"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(lazy), "\"hello\"");
}

#[test]
fn lazy_primitive_alias_chain_renders_as_underlying() {
    // `type A = B; type B = string` collapses to `string` through the chain.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let b_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("B"),
        vec![],
        TypeId::STRING,
    ));
    let a_def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("A"),
        vec![],
        db.lazy(b_def),
    ));
    let lazy_a = db.lazy(a_def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(lazy_a), "string");
}

#[test]
fn lazy_union_alias_keeps_its_name() {
    // A union-bodied alias is a freshly-constructed structural type and keeps
    // its alias name (`IdLike`), unlike a primitive-bodied alias.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let body = db.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("IdLike"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(lazy),
        "IdLike",
        "A union-bodied alias must keep its name"
    );
}
