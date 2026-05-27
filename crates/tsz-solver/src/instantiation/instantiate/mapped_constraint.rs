use super::*;

impl<'a> TypeInstantiator<'a> {
    /// Instantiate a mapped type's constraint while preserving the
    /// `keyof <source>` structure of a homomorphic mapped type.
    ///
    /// A mapped type is *homomorphic* when its constraint is `keyof T`; the
    /// evaluator relies on that `KeyOf(source)` shape (`extract_source_from_keyof`)
    /// to recover the source object and inherit its `readonly`/optional property
    /// modifiers, mirroring tsc's `getHomomorphicTypeVariable` +
    /// `getModifiersTypeFromMappedType`.
    ///
    /// Routing `keyof T` through the generic `instantiate` path eagerly evaluates
    /// `keyof SQ` down to its concrete key set (e.g. the literal `"items"`) once
    /// the source resolves to a concrete object shape. That erases the homomorphic
    /// structure, so the expansion silently drops the modifiers and produces a
    /// mutable property — which made a `readonly` array (`ReadonlyArray`) property
    /// fail assignability against the inherited-`readonly` target. Re-wrapping the
    /// instantiated source in `keyof` keeps the mapped homomorphic so the
    /// modifiers survive.
    pub(super) fn instantiate_mapped_constraint(&mut self, constraint: TypeId) -> TypeId {
        if let Some(TypeData::KeyOf(source)) = self.interner.lookup(constraint) {
            let inst_source = self.instantiate(source);
            self.interner.keyof(inst_source)
        } else {
            self.instantiate(constraint)
        }
    }
}
