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

#[test]
fn lazy_conditional_alias_resolving_to_intrinsic_renders_underlying() {
    // `type X = string extends string ? string : number` reduces to the shared
    // `string` singleton. tsc attaches no `aliasSymbol` to a conditional result,
    // so the diagnostic surface is `string`, never `X`. The body is a
    // `Conditional`, so the syntactic intrinsic/literal check does not catch it;
    // the formatter must evaluate the computed body to discover the scalar.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let body = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: TypeId::STRING,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("X"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(lazy),
        "string",
        "A conditional-bodied alias that reduces to an intrinsic must render structurally"
    );
}

#[test]
fn lazy_conditional_alias_resolving_to_literal_renders_underlying() {
    // `type Y = string extends string ? "yes" : "no"` reduces to the literal
    // `"yes"`; tsc shows `"yes"`, not `Y`.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let body = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: db.literal_string("yes"),
        false_type: db.literal_string("no"),
        is_distributive: false,
    });
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Y"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(lazy), "\"yes\"");
}

#[test]
fn lazy_conditional_alias_resolving_to_never_renders_underlying() {
    // A conditional that reduces to `never` displays as `never`, not the alias.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let body = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Z"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(lazy), "never");
}

#[test]
fn lazy_conditional_alias_resolving_to_tuple_renders_underlying() {
    // A *conditional* resolves away into its branch type and never carries the
    // alias's `aliasSymbol`, so tsc renders the underlying structural result —
    // here `[string, number]` — rather than the alias name `Pair`. This holds
    // for any resolved shape, not only shared-singleton scalars. (Verified
    // against tsc 6.0.2: `type Pair = string extends string ? [string, number]
    // : never` elaborates as `[string, number]`.)
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let tuple = db.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let body = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: tuple,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("Pair"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(
        fmt.format(lazy),
        "[string, number]",
        "A conditional alias that reduces to a tuple renders the underlying tuple"
    );
}

#[test]
fn lazy_conditional_alias_resolving_to_object_renders_underlying() {
    // `type C = string extends string ? { a: 1 } : never` → tsc shows
    // `{ a: 1; }`, never `C`: the resolved conditional drops the alias symbol.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let object = db.object(vec![PropertyInfo::new(
        db.intern_string("a"),
        db.literal_number(1.0),
    )]);
    let body = db.conditional(crate::types::ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::STRING,
        true_type: object,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("C"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(lazy), "{ a: 1; }");
}

#[test]
fn lazy_keyof_alias_renders_underlying_literal_union() {
    // `type K = keyof { a: 1; b: 2 }` → tsc shows `"a" | "b"`, never `K`:
    // `keyof` constructs its result without the enclosing alias symbol.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let object = db.object(vec![
        PropertyInfo::new(db.intern_string("a"), db.literal_number(1.0)),
        PropertyInfo::new(db.intern_string("b"), db.literal_number(2.0)),
    ]);
    let body = db.keyof(object);
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("K"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(lazy), "\"a\" | \"b\"");
}

#[test]
fn lazy_index_access_alias_renders_underlying_object() {
    // `type IA = { p: { a: 1 } }["p"]` → tsc shows `{ a: 1; }`, never `IA`:
    // the indexed access resolves to its element type with no alias symbol.
    let db = TypeInterner::new();
    let def_store = crate::def::DefinitionStore::new();

    let inner = db.object(vec![PropertyInfo::new(
        db.intern_string("a"),
        db.literal_number(1.0),
    )]);
    let outer = db.object(vec![PropertyInfo::new(db.intern_string("p"), inner)]);
    let body = db.index_access(outer, db.literal_string("p"));
    let def = def_store.register(crate::def::DefinitionInfo::type_alias(
        db.intern_string("IA"),
        vec![],
        body,
    ));
    let lazy = db.lazy(def);

    let mut fmt = TypeFormatter::new(&db).with_def_store(&def_store);
    assert_eq!(fmt.format(lazy), "{ a: 1; }");
}
