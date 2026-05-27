use super::*;

impl<'a> TypeInstantiator<'a> {
    /// Instantiate a mapped type's constraint, preserving the `keyof <source>`
    /// structure for *non-identity* homomorphic mapped types.
    ///
    /// A mapped type is *homomorphic* when its constraint is `keyof T`; the
    /// evaluator inherits T's `readonly`/optional property modifiers, mirroring
    /// tsc's `getHomomorphicTypeVariable` + `getModifiersTypeFromMappedType`. It
    /// recovers the source object in one of two ways:
    /// 1. from the **template** `T[K]` (the identity case, e.g. `Readonly<T>`,
    ///    `Partial<T>`, `Required<T>`), or
    /// 2. from the **constraint** `keyof T` (the non-identity case, e.g.
    ///    `{ [P in keyof T]: unknown }`), via `extract_source_from_keyof`.
    ///
    /// Routing `keyof T` through the generic `instantiate` path eagerly evaluates
    /// `keyof SQ` down to its concrete key set (e.g. the literal `"items"`) once
    /// the source resolves to a concrete object shape, erasing the `KeyOf(source)`
    /// shape. For the identity case that is harmless — the evaluator still finds
    /// the source through the template — so those types keep the original path and
    /// their existing behavior. For the non-identity case the template carries no
    /// source, so collapsing the constraint silently drops the modifiers and yields
    /// a mutable property (which made a `readonly` array property fail
    /// assignability against the inherited-`readonly` target). Only there do we
    /// re-wrap the instantiated source in `keyof` to keep the mapped homomorphic.
    pub(super) fn instantiate_mapped_constraint(
        &mut self,
        constraint: TypeId,
        template: TypeId,
        iter_var: Atom,
    ) -> TypeId {
        if let Some(TypeData::KeyOf(source)) = self.interner.lookup(constraint)
            && !self.template_is_identity_index(template, source, iter_var)
        {
            let inst_source = self.instantiate(source);
            return self.interner.keyof(inst_source);
        }
        self.instantiate(constraint)
    }

    /// Whether `template` is the identity property access `source[iter_var]`
    /// (the `T[K]` template of `Readonly`/`Partial`/`Required`-style mapped
    /// types). For those the evaluator detects homomorphic-ness from the
    /// template alone, so the constraint may be collapsed without losing
    /// modifiers.
    fn template_is_identity_index(&self, template: TypeId, source: TypeId, iter_var: Atom) -> bool {
        matches!(
            self.interner.lookup(template),
            Some(TypeData::IndexAccess(obj, idx))
                if obj == source
                    && matches!(
                        self.interner.lookup(idx),
                        Some(TypeData::TypeParameter(p)) if p.name == iter_var
                    )
        )
    }
}
