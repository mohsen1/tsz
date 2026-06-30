//! Indexed and key-derived type instantiation: the `TypeData::IndexAccess`,
//! `TypeData::KeyOf`, `TypeData::TemplateLiteral`, and
//! `TypeData::StringIntrinsic` arms of `instantiate_key`.

use crate::construction::TypeDatabase;
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
        let inst_obj = self.instantiate(*obj);
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
            // #14345 dormant re-reduce (default OFF, byte-parity): the
            // base/index are concrete (no type parameters reached this point)
            // but still hold a resolver-only meta-type (`Application`/`Lazy`/
            // etc.). With the flag on and a resolver-aware db threaded in,
            // route the re-reduce through it (resolving the cross-arena `Lazy`
            // base) instead of returning the deferred `IndexAccess`. The OFF
            // path below is the literal pre-existing deferred return.
            if super::flags::inst_resolver_rereduce_enabled() && self.query_db.is_some() {
                return self.evaluate_index_access(inst_obj, inst_idx);
            }
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
        if keyof_operand_needs_resolver(self.interner, inst_operand) {
            if super::flags::inst_resolver_rereduce_enabled() && self.query_db.is_some() {
                return self.evaluate_keyof(inst_operand);
            }
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

/// Check whether an instantiated `keyof` operand needs the outer resolver.
fn keyof_operand_needs_resolver(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match interner.lookup(type_id) {
        Some(
            TypeData::Application(_)
            | TypeData::Lazy(_)
            | TypeData::TypeQuery(_)
            | TypeData::IndexAccess(_, _),
        ) => true,
        Some(TypeData::Union(_) | TypeData::Intersection(_)) => {
            crate::type_queries::contains_lazy_or_recursive_db(interner, type_id)
        }
        _ => false,
    }
}
