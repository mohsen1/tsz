//! Indexed and key-derived type instantiation: the `TypeData::IndexAccess`,
//! `TypeData::KeyOf`, `TypeData::TemplateLiteral`, and
//! `TypeData::StringIntrinsic` arms of `instantiate_key`.

use crate::types::{
    IntrinsicKind, LiteralValue, StringIntrinsicKind, TemplateLiteralId, TemplateSpan, TypeData,
    TypeId,
};

use super::{TypeInstantiator, index_access_operand_needs_resolver};

impl<'a> TypeInstantiator<'a> {
    /// Instantiate an index access: instantiate both parts and evaluate
    /// immediately.
    ///
    /// Task #46: Meta-type reduction for O(1) equality
    pub(super) fn instantiate_index_access(&mut self, obj: &TypeId, idx: &TypeId) -> TypeId {
        // For homomorphic -? mapped type evaluation, T[K] must use the
        // declared property type (without the `| undefined` that
        // `optional_property_type` adds for read access). Check *idx
        // BEFORE instantiation so we can detect the iteration variable
        // (K → key_literal substitution hasn't happened yet).
        let is_iter_var = self.declared_index_type.is_some_and(|(_, iter_var, _)| {
            matches!(
                self.interner.lookup(*idx),
                Some(TypeData::TypeParameter(p)) if p.name == iter_var
            )
        });
        let inst_obj = self.instantiate(*obj);
        if let Some((override_source, _, replacement)) = self.declared_index_type
            && is_iter_var
            && inst_obj == override_source
        {
            return replacement;
        }
        let inst_idx = self.instantiate(*idx);
        // Don't eagerly evaluate if either part still contains type parameters.
        // This prevents premature evaluation of `T[K]` or `T[keyof T]` where T
        // is an inference placeholder, which would resolve through the constraint
        // instead of waiting for the actual inferred type.
        if crate::visitor::contains_type_parameters(self.interner, inst_obj)
            || crate::visitor::contains_type_parameters(self.interner, inst_idx)
        {
            return self.interner.index_access(inst_obj, inst_idx);
        }
        if self.preserve_meta_types
            || index_access_operand_needs_resolver(self.interner, inst_obj)
            || index_access_operand_needs_resolver(self.interner, inst_idx)
        {
            return self.interner.index_access(inst_obj, inst_idx);
        }
        // Evaluate immediately to achieve O(1) equality
        self.evaluate_index_access(inst_obj, inst_idx)
    }

    /// Instantiate a `keyof`: instantiate the operand and evaluate immediately.
    ///
    /// Task #46: Meta-type reduction for O(1) equality
    pub(super) fn instantiate_keyof(&mut self, operand: &TypeId) -> TypeId {
        tracing::trace!(
            operand = operand.0,
            operand_key = ?self.interner.lookup(*operand),
            subst = ?self.substitution.map.iter().map(|(k, v)| (self.interner.resolve_atom_ref(*k), v.0)).collect::<Vec<_>>(),
            "instantiate KeyOf: about to instantiate operand"
        );
        let inst_operand = self.instantiate(*operand);
        tracing::trace!(
            operand = operand.0,
            inst_operand = inst_operand.0,
            inst_operand_key = ?self.interner.lookup(inst_operand),
            has_type_params = crate::visitor::contains_type_parameters(self.interner, inst_operand),
            "instantiate KeyOf: result"
        );
        // Don't eagerly evaluate if the operand still contains type parameters.
        // This prevents premature evaluation of `keyof T` where T is an inference
        // placeholder (e.g. during compute_contextual_types), which would resolve
        // to `keyof <constraint>` instead of waiting for T to be inferred.
        // Without this, mapped types like `{ [P in keyof T]: ... }` collapse to `{}`
        // because `keyof object` = `never`.
        if crate::visitor::contains_type_parameters(self.interner, inst_operand) {
            return self.interner.keyof(inst_operand);
        }
        if self.preserve_meta_types {
            return self.interner.keyof(inst_operand);
        }
        if matches!(
            self.interner.lookup(inst_operand),
            Some(
                TypeData::TypeQuery(_)
                    | TypeData::Lazy(_)
                    | TypeData::Application(_)
                    | TypeData::IndexAccess(_, _)
            )
        ) {
            return self.interner.keyof(inst_operand);
        }
        // Union/intersection operands whose members are semantic refs
        // (`Lazy(DefId)`), generic applications, or recursive aliases
        // cannot be flattened to a finite key set by the resolver-less
        // `evaluate_keyof` reached from this instantiation path: the
        // member refs stay unresolved, so the keyof collapses to a
        // deferred, structurally-detached form that loses the source's
        // properties (and their optional/readonly modifiers) when the
        // mapped type later expands. Keep the keyof deferred over the
        // instantiated operand so the resolver-aware key extraction in
        // `extract_mapped_keys`/`collect_properties` can resolve the
        // member refs and recover the full key set. Fully concrete
        // unions/intersections (e.g. `keyof ({ a: 1 } & { b: 2 })`)
        // have no such refs and continue to evaluate eagerly below.
        if matches!(
            self.interner.lookup(inst_operand),
            Some(TypeData::Union(_) | TypeData::Intersection(_))
        ) && crate::type_queries::contains_lazy_or_recursive_db(self.interner, inst_operand)
        {
            return self.interner.keyof(inst_operand);
        }
        // Evaluate immediately to expand keyof { a: 1 } -> "a"
        let result = self.evaluate_keyof(inst_operand);

        // Store display alias so the formatter shows "keyof Shape" instead
        // of the expanded union. Only store when the result is non-trivial
        // and the operand is a named type (has a def-store mapping via the
        // Object/Callable shape → def reverse lookup in the formatter).
        if result != TypeId::NEVER && !result.is_intrinsic() {
            let keyof_type = self.interner.keyof(inst_operand);
            if result != keyof_type {
                self.interner.store_display_alias(result, keyof_type);
            }
        }

        result
    }

    /// Instantiate a template literal: instantiate embedded types.
    ///
    /// After substitution, if any type span becomes a union of string literals,
    /// we trigger evaluation to expand the template literal into a union of strings.
    pub(super) fn instantiate_template_literal(&mut self, spans: &TemplateLiteralId) -> TypeId {
        let spans = self.interner.template_list(*spans);
        let mut instantiated: Vec<TemplateSpan> = Vec::with_capacity(spans.len());
        let mut needs_evaluation = false;

        for span in spans.iter() {
            match span {
                TemplateSpan::Text(t) => instantiated.push(TemplateSpan::Text(*t)),
                TemplateSpan::Type(t) => {
                    let inst_type = self.instantiate(*t);
                    // Check if this type became something that can be evaluated:
                    // - A union of string literals
                    // - A single string literal
                    // - The string intrinsic type
                    if let Some(
                        TypeData::Union(_)
                        | TypeData::Literal(
                            LiteralValue::String(_)
                            | LiteralValue::Number(_)
                            | LiteralValue::Boolean(_),
                        )
                        | TypeData::Intrinsic(
                            IntrinsicKind::String | IntrinsicKind::Number | IntrinsicKind::Boolean,
                        ),
                    ) = self.interner.lookup(inst_type)
                    {
                        needs_evaluation = true;
                    }
                    instantiated.push(TemplateSpan::Type(inst_type));
                }
            }
        }

        let template_type = self.interner.template_literal(instantiated);

        // If we detected types that can be evaluated, trigger evaluation
        // to potentially expand the template literal to a union of string literals
        if needs_evaluation {
            self.evaluate_type(template_type)
        } else {
            template_type
        }
    }

    /// Instantiate a string intrinsic: instantiate the type argument.
    ///
    /// After substitution, if the type argument becomes a concrete type that can
    /// be evaluated (like a string literal or union), trigger evaluation.
    pub(super) fn instantiate_string_intrinsic(
        &mut self,
        kind: &StringIntrinsicKind,
        type_arg: &TypeId,
    ) -> TypeId {
        let inst_arg = self.instantiate(*type_arg);
        let string_intrinsic = self.interner.string_intrinsic(*kind, inst_arg);

        // Check if we can evaluate the result
        if let Some(key) = self.interner.lookup(inst_arg) {
            match key {
                TypeData::Union(_)
                | TypeData::Literal(LiteralValue::String(_))
                | TypeData::TemplateLiteral(_)
                | TypeData::Intrinsic(IntrinsicKind::String) => {
                    self.evaluate_type(string_intrinsic)
                }
                _ => string_intrinsic,
            }
        } else {
            string_intrinsic
        }
    }
}
