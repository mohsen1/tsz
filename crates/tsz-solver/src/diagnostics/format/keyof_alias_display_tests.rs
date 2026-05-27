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
