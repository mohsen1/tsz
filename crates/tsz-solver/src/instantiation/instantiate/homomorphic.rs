use super::{
    MappedType, TypeData, TypeId, TypeInstantiator, mapped_constraint_needs_resolver,
    template_has_lazy_application_in_composite, type_contains_lazy_application,
};

impl<'a> TypeInstantiator<'a> {
    pub(super) fn try_expand_substituted_homomorphic_object_mapped(
        &mut self,
        mapped: &MappedType,
        resolved_source: TypeId,
        has_identity_name_type: bool,
    ) -> Option<TypeId> {
        if self.preserve_meta_types || !(mapped.name_type.is_none() || has_identity_name_type) {
            return None;
        }

        match self.interner.lookup(resolved_source) {
            Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_) | TypeData::Callable(_)) => {}
            _ => return None,
        }

        let constraint = self.interner.keyof(resolved_source);
        if mapped_constraint_needs_resolver(self.interner, constraint) {
            return None;
        }

        let template = self.instantiate(mapped.template);
        if template_has_lazy_application_in_composite(self.interner, template)
            || self
                .interner
                .lookup(template)
                .is_some_and(|data| matches!(data, TypeData::Conditional(_)))
        {
            return None;
        }

        let name_type = mapped
            .name_type
            .map(|name_type| self.instantiate(name_type));
        if name_type
            .is_some_and(|name_type| type_contains_lazy_application(self.interner, name_type))
        {
            return None;
        }

        let expanded = self.interner.mapped(MappedType {
            type_param: mapped.type_param,
            constraint,
            name_type,
            template,
            readonly_modifier: mapped.readonly_modifier,
            optional_modifier: mapped.optional_modifier,
        });
        let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expanded);
        if evaluated == TypeId::ERROR
            || matches!(self.interner.lookup(evaluated), Some(TypeData::Mapped(_)))
        {
            None
        } else {
            Some(evaluated)
        }
    }
}
