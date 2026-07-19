//! Simultaneous exact-identity rewrites for cached type graphs.
//!
//! This is deliberately separate from the evaluator's distributive
//! substitution walker. That walker owns conditional/mapped evaluation policy;
//! this one only rebuilds the interned graph while replacing selected exact
//! [`TypeId`] identities.

use crate::construction::{QueryDatabase, TypeDatabase};
use crate::types::{
    CallSignature, CallableShape, ConditionalType, FunctionShape, IndexSignature, MappedType,
    ObjectShape, ParamInfo, PropertyInfo, TemplateSpan, TupleElement, TypeData, TypeId,
    TypeParamInfo, TypePredicate,
};
use rustc_hash::FxHashMap;
use std::collections::hash_map::Entry;

/// Replace aligned exact identities throughout `root` in one graph walk.
///
/// Replacements are simultaneous: a replacement value is terminal and is not
/// itself rewritten through another pair. The walk is `O(P + N)`, where `P` is
/// the number of non-identity replacement pairs and `N` is the reachable graph
/// size. Shared nodes are rebuilt once, and a no-op graph retains its original
/// interned identity.
pub fn substitute_exact_types(
    db: &dyn QueryDatabase,
    root: TypeId,
    from: &[TypeId],
    to: &[TypeId],
) -> TypeId {
    debug_assert_eq!(from.len(), to.len());
    if from.len() != to.len() || from.is_empty() {
        return root;
    }

    let mut rewritten = FxHashMap::with_capacity_and_hasher(from.len(), Default::default());
    for (&source, &replacement) in from.iter().zip(to) {
        if source == replacement {
            continue;
        }
        match rewritten.entry(source) {
            Entry::Vacant(entry) => {
                entry.insert(replacement);
            }
            Entry::Occupied(entry) => {
                debug_assert_eq!(
                    *entry.get(),
                    replacement,
                    "one exact type identity cannot have conflicting replacements",
                );
            }
        }
    }
    if rewritten.is_empty() {
        return root;
    }

    ExactTypeRewriter { db, rewritten }.rewrite(root)
}

/// Replace one exact interned identity throughout a type graph.
pub fn substitute_exact_type(
    db: &dyn QueryDatabase,
    root: TypeId,
    from: TypeId,
    to: TypeId,
) -> TypeId {
    substitute_exact_types(
        db,
        root,
        std::slice::from_ref(&from),
        std::slice::from_ref(&to),
    )
}

struct ExactTypeRewriter<'a> {
    db: &'a dyn QueryDatabase,
    /// Preloaded with direct replacements, then extended with per-node memoized
    /// results. Direct replacements therefore remain terminal.
    rewritten: FxHashMap<TypeId, TypeId>,
}

impl ExactTypeRewriter<'_> {
    fn rewrite(&mut self, type_id: TypeId) -> TypeId {
        if let Some(&cached) = self.rewritten.get(&type_id) {
            return cached;
        }
        if type_id.is_intrinsic() {
            return type_id;
        }

        // A self-map is the cycle placeholder. Interned types are normally a
        // DAG with `Lazy`/`Recursive` cut points, but provenance can add edges.
        self.rewritten.insert(type_id, type_id);

        let Some(data) = self.db.lookup(type_id) else {
            return type_id;
        };
        let result = match data {
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::BoundParameter(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::Error
            | TypeData::UnresolvedTypeName(_) => type_id,

            TypeData::Object(shape_id) => {
                let shape = self.db.object_shape(shape_id);
                self.rewrite_properties(&shape.properties)
                    .map_or(type_id, |properties| {
                        self.db
                            .object_with_flags_and_symbol(properties, shape.flags, shape.symbol)
                    })
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.db.object_shape(shape_id);
                let properties = self.rewrite_properties(&shape.properties);
                let string_index = shape
                    .string_index
                    .as_ref()
                    .and_then(|index| self.rewrite_index_signature(index));
                let number_index = shape
                    .number_index
                    .as_ref()
                    .and_then(|index| self.rewrite_index_signature(index));
                let symbol_index = shape
                    .symbol_index
                    .as_ref()
                    .and_then(|index| self.rewrite_index_signature(index));
                if properties.is_none()
                    && string_index.is_none()
                    && number_index.is_none()
                    && symbol_index.is_none()
                {
                    type_id
                } else {
                    self.db.object_with_index(ObjectShape {
                        flags: shape.flags,
                        properties: properties.unwrap_or_else(|| shape.properties.clone()),
                        string_index: string_index.or(shape.string_index),
                        number_index: number_index.or(shape.number_index),
                        symbol_index: symbol_index.or(shape.symbol_index),
                        symbol: shape.symbol,
                    })
                }
            }
            TypeData::Union(list_id) => self
                .rewrite_type_ids(self.db.type_list(list_id).as_ref())
                .map_or(type_id, |members| self.db.union(members)),
            TypeData::Intersection(list_id) => self
                .rewrite_type_ids(self.db.type_list(list_id).as_ref())
                .map_or(type_id, |members| self.db.intersection(members)),
            TypeData::Array(element) => {
                let rewritten = self.rewrite(element);
                if rewritten == element {
                    type_id
                } else {
                    self.db.array(rewritten)
                }
            }
            TypeData::Tuple(list_id) => self
                .rewrite_tuple_elements(self.db.tuple_list(list_id).as_ref())
                .map_or(type_id, |elements| self.db.tuple(elements)),
            TypeData::Function(shape_id) => {
                let shape = self.db.function_shape(shape_id);
                self.rewrite_function_shape(&shape)
                    .map_or(type_id, |shape| self.db.function(shape))
            }
            TypeData::Callable(shape_id) => {
                let shape = self.db.callable_shape(shape_id);
                self.rewrite_callable_shape(&shape)
                    .map_or(type_id, |shape| self.db.callable(shape))
            }
            TypeData::TypeParameter(info) => self
                .rewrite_type_param(info)
                .map_or(type_id, |info| self.db.fresh_type_param(info)),
            TypeData::Enum(def_id, member_type) => {
                let rewritten = self.rewrite(member_type);
                if rewritten == member_type {
                    type_id
                } else {
                    self.db.enum_type(def_id, rewritten)
                }
            }
            TypeData::Application(app_id) => {
                let app = self.db.type_application(app_id);
                let base = self.rewrite(app.base);
                let args = self.rewrite_type_ids(&app.args);
                if base == app.base && args.is_none() {
                    type_id
                } else {
                    self.db
                        .application(base, args.unwrap_or_else(|| app.args.clone()))
                }
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.db.get_conditional(cond_id);
                let rewritten = ConditionalType {
                    check_type: self.rewrite(cond.check_type),
                    extends_type: self.rewrite(cond.extends_type),
                    true_type: self.rewrite(cond.true_type),
                    false_type: self.rewrite(cond.false_type),
                    is_distributive: cond.is_distributive,
                };
                if rewritten == cond {
                    type_id
                } else {
                    self.db.conditional(rewritten)
                }
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.db.get_mapped(mapped_id);
                let type_param = self.rewrite_type_param(mapped.type_param);
                let constraint = self.rewrite(mapped.constraint);
                let name_type = mapped.name_type.map(|name_type| self.rewrite(name_type));
                let template = self.rewrite(mapped.template);
                if type_param.is_none()
                    && constraint == mapped.constraint
                    && name_type == mapped.name_type
                    && template == mapped.template
                {
                    type_id
                } else {
                    self.db.mapped(MappedType {
                        type_param: type_param.unwrap_or(mapped.type_param),
                        constraint,
                        name_type,
                        template,
                        readonly_modifier: mapped.readonly_modifier,
                        optional_modifier: mapped.optional_modifier,
                    })
                }
            }
            TypeData::IndexAccess(object_type, index_type) => {
                let object_type_rewritten = self.rewrite(object_type);
                let index_type_rewritten = self.rewrite(index_type);
                if object_type_rewritten == object_type && index_type_rewritten == index_type {
                    type_id
                } else {
                    self.db
                        .index_access(object_type_rewritten, index_type_rewritten)
                }
            }
            TypeData::TemplateLiteral(template_id) => self
                .rewrite_template_spans(self.db.template_list(template_id).as_ref())
                .map_or(type_id, |spans| self.db.template_literal(spans)),
            TypeData::KeyOf(inner) => {
                self.rewrite_unary(type_id, inner, |db, inner| db.keyof(inner))
            }
            TypeData::ReadonlyType(inner) => {
                self.rewrite_unary(type_id, inner, |db, inner| db.readonly_type(inner))
            }
            TypeData::Infer(info) => self
                .rewrite_type_param(info)
                .map_or(type_id, |info| self.db.infer(info)),
            TypeData::StringIntrinsic { kind, type_arg } => {
                self.rewrite_unary(type_id, type_arg, |db, inner| {
                    db.string_intrinsic(kind, inner)
                })
            }
            TypeData::NoInfer(inner) => {
                self.rewrite_unary(type_id, inner, |db, inner| db.no_infer(inner))
            }
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                let base_type_rewritten = self.rewrite(base_type);
                let constraint_rewritten = self.rewrite(constraint);
                if base_type_rewritten == base_type && constraint_rewritten == constraint {
                    type_id
                } else {
                    self.db
                        .substitution(base_type_rewritten, constraint_rewritten)
                }
            }
        };

        // Publish the structural result before walking side-table provenance;
        // provenance can point back into the source graph.
        self.rewritten.insert(type_id, result);
        if result != type_id {
            self.propagate_provenance(type_id, result);
        }
        result
    }

    fn rewrite_unary(
        &mut self,
        original: TypeId,
        inner: TypeId,
        build: impl FnOnce(&dyn TypeDatabase, TypeId) -> TypeId,
    ) -> TypeId {
        let rewritten = self.rewrite(inner);
        if rewritten == inner {
            original
        } else {
            build(self.db, rewritten)
        }
    }

    fn rewrite_type_ids(&mut self, ids: &[TypeId]) -> Option<Vec<TypeId>> {
        let mut changed: Option<Vec<TypeId>> = None;
        for (index, &type_id) in ids.iter().enumerate() {
            let rewritten = self.rewrite(type_id);
            if let Some(changed) = &mut changed {
                changed.push(rewritten);
            } else if rewritten != type_id {
                let mut values = Vec::with_capacity(ids.len());
                values.extend_from_slice(&ids[..index]);
                values.push(rewritten);
                changed = Some(values);
            }
        }
        changed
    }

    fn rewrite_tuple_elements(&mut self, elements: &[TupleElement]) -> Option<Vec<TupleElement>> {
        let mut changed: Option<Vec<TupleElement>> = None;
        for (index, element) in elements.iter().enumerate() {
            let type_id = self.rewrite(element.type_id);
            let rewritten = TupleElement {
                type_id,
                ..*element
            };
            if let Some(changed) = &mut changed {
                changed.push(rewritten);
            } else if rewritten != *element {
                let mut values = Vec::with_capacity(elements.len());
                values.extend_from_slice(&elements[..index]);
                values.push(rewritten);
                changed = Some(values);
            }
        }
        changed
    }

    fn rewrite_properties(&mut self, properties: &[PropertyInfo]) -> Option<Vec<PropertyInfo>> {
        let mut changed: Option<Vec<PropertyInfo>> = None;
        for (index, property) in properties.iter().enumerate() {
            let type_id = self.rewrite(property.type_id);
            let write_type = if property.write_type == property.type_id {
                type_id
            } else {
                self.rewrite(property.write_type)
            };
            let rewritten = PropertyInfo {
                type_id,
                write_type,
                ..property.clone()
            };
            if let Some(changed) = &mut changed {
                changed.push(rewritten);
            } else if type_id != property.type_id || write_type != property.write_type {
                let mut values = Vec::with_capacity(properties.len());
                values.extend_from_slice(&properties[..index]);
                values.push(rewritten);
                changed = Some(values);
            }
        }
        changed
    }

    fn rewrite_index_signature(&mut self, index: &IndexSignature) -> Option<IndexSignature> {
        let key_type = self.rewrite(index.key_type);
        let value_type = self.rewrite(index.value_type);
        (key_type != index.key_type || value_type != index.value_type).then_some(IndexSignature {
            key_type,
            value_type,
            ..*index
        })
    }

    fn rewrite_params(&mut self, params: &[ParamInfo]) -> Option<Vec<ParamInfo>> {
        let mut changed: Option<Vec<ParamInfo>> = None;
        for (index, param) in params.iter().enumerate() {
            let type_id = self.rewrite(param.type_id);
            let rewritten = ParamInfo { type_id, ..*param };
            if let Some(changed) = &mut changed {
                changed.push(rewritten);
            } else if rewritten != *param {
                let mut values = Vec::with_capacity(params.len());
                values.extend_from_slice(&params[..index]);
                values.push(rewritten);
                changed = Some(values);
            }
        }
        changed
    }

    fn rewrite_type_param(&mut self, param: TypeParamInfo) -> Option<TypeParamInfo> {
        let constraint = param.constraint.map(|constraint| self.rewrite(constraint));
        let default = param.default.map(|default| self.rewrite(default));
        let rewritten = TypeParamInfo {
            constraint,
            default,
            ..param
        };
        (rewritten != param).then_some(rewritten)
    }

    fn rewrite_type_params(&mut self, params: &[TypeParamInfo]) -> Option<Vec<TypeParamInfo>> {
        let mut changed: Option<Vec<TypeParamInfo>> = None;
        for (index, &param) in params.iter().enumerate() {
            let rewritten = self.rewrite_type_param(param).unwrap_or(param);
            if let Some(changed) = &mut changed {
                changed.push(rewritten);
            } else if rewritten != param {
                let mut values = Vec::with_capacity(params.len());
                values.extend_from_slice(&params[..index]);
                values.push(rewritten);
                changed = Some(values);
            }
        }
        changed
    }

    fn rewrite_predicate(&mut self, predicate: TypePredicate) -> Option<TypePredicate> {
        let type_id = predicate.type_id.map(|type_id| self.rewrite(type_id));
        (type_id != predicate.type_id).then_some(TypePredicate {
            type_id,
            ..predicate
        })
    }

    fn rewrite_function_shape(&mut self, shape: &FunctionShape) -> Option<FunctionShape> {
        let type_params = self.rewrite_type_params(&shape.type_params);
        let params = self.rewrite_params(&shape.params);
        let this_type = shape.this_type.map(|this_type| self.rewrite(this_type));
        let return_type = self.rewrite(shape.return_type);
        let type_predicate = shape
            .type_predicate
            .and_then(|predicate| self.rewrite_predicate(predicate));
        if type_params.is_none()
            && params.is_none()
            && this_type == shape.this_type
            && return_type == shape.return_type
            && type_predicate.is_none()
        {
            None
        } else {
            Some(FunctionShape {
                type_params: type_params.unwrap_or_else(|| shape.type_params.clone()),
                params: params.unwrap_or_else(|| shape.params.clone()),
                this_type,
                return_type,
                type_predicate: type_predicate.or(shape.type_predicate),
                is_constructor: shape.is_constructor,
                is_method: shape.is_method,
            })
        }
    }

    fn rewrite_call_signature(&mut self, signature: &CallSignature) -> Option<CallSignature> {
        let type_params = self.rewrite_type_params(&signature.type_params);
        let params = self.rewrite_params(&signature.params);
        let this_type = signature.this_type.map(|this_type| self.rewrite(this_type));
        let return_type = self.rewrite(signature.return_type);
        let type_predicate = signature
            .type_predicate
            .and_then(|predicate| self.rewrite_predicate(predicate));
        if type_params.is_none()
            && params.is_none()
            && this_type == signature.this_type
            && return_type == signature.return_type
            && type_predicate.is_none()
        {
            None
        } else {
            Some(CallSignature {
                type_params: type_params.unwrap_or_else(|| signature.type_params.clone()),
                params: params.unwrap_or_else(|| signature.params.clone()),
                this_type,
                return_type,
                type_predicate: type_predicate.or(signature.type_predicate),
                is_method: signature.is_method,
            })
        }
    }

    fn rewrite_signatures(&mut self, signatures: &[CallSignature]) -> Option<Vec<CallSignature>> {
        let mut changed: Option<Vec<CallSignature>> = None;
        for (index, signature) in signatures.iter().enumerate() {
            let rewritten = self
                .rewrite_call_signature(signature)
                .unwrap_or_else(|| signature.clone());
            if let Some(changed) = &mut changed {
                changed.push(rewritten);
            } else if rewritten != *signature {
                let mut values = Vec::with_capacity(signatures.len());
                values.extend_from_slice(&signatures[..index]);
                values.push(rewritten);
                changed = Some(values);
            }
        }
        changed
    }

    fn rewrite_callable_shape(&mut self, shape: &CallableShape) -> Option<CallableShape> {
        let call_signatures = self.rewrite_signatures(&shape.call_signatures);
        let construct_signatures = self.rewrite_signatures(&shape.construct_signatures);
        let properties = self.rewrite_properties(&shape.properties);
        let string_index = shape
            .string_index
            .as_ref()
            .and_then(|index| self.rewrite_index_signature(index));
        let number_index = shape
            .number_index
            .as_ref()
            .and_then(|index| self.rewrite_index_signature(index));
        if call_signatures.is_none()
            && construct_signatures.is_none()
            && properties.is_none()
            && string_index.is_none()
            && number_index.is_none()
        {
            None
        } else {
            Some(CallableShape {
                call_signatures: call_signatures.unwrap_or_else(|| shape.call_signatures.clone()),
                construct_signatures: construct_signatures
                    .unwrap_or_else(|| shape.construct_signatures.clone()),
                properties: properties.unwrap_or_else(|| shape.properties.clone()),
                string_index: string_index.or(shape.string_index),
                number_index: number_index.or(shape.number_index),
                symbol: shape.symbol,
                is_abstract: shape.is_abstract,
            })
        }
    }

    fn rewrite_template_spans(&mut self, spans: &[TemplateSpan]) -> Option<Vec<TemplateSpan>> {
        let mut changed: Option<Vec<TemplateSpan>> = None;
        for (index, span) in spans.iter().enumerate() {
            let rewritten = match span {
                TemplateSpan::Text(text) => TemplateSpan::Text(*text),
                TemplateSpan::Type(type_id) => TemplateSpan::Type(self.rewrite(*type_id)),
            };
            if let Some(changed) = &mut changed {
                changed.push(rewritten);
            } else if rewritten != *span {
                let mut values = Vec::with_capacity(spans.len());
                values.extend_from_slice(&spans[..index]);
                values.push(rewritten);
                changed = Some(values);
            }
        }
        changed
    }

    fn propagate_provenance(&mut self, source: TypeId, result: TypeId) {
        if let Some(properties) = self.db.get_display_properties(source) {
            let properties = self
                .rewrite_properties(properties.as_ref())
                .unwrap_or_else(|| properties.as_ref().clone());
            self.db.store_display_properties(result, properties);
        }

        if let Some(origin) = self.db.get_union_origin(source) {
            let origin = self
                .rewrite_type_ids(origin.as_ref())
                .unwrap_or_else(|| origin.as_ref().clone());
            self.db.store_union_origin(result, origin);
        }

        // Application provenance is first-write-wins. Publish it before
        // replaying a merged origin, whose member reconstruction can intern the
        // same structural result through a different application.
        if self.db.get_application_eval_origin(result).is_none()
            && let Some(origin) = self.db.get_application_eval_origin(source)
        {
            let origin = self.rewrite(origin);
            self.db.record_application_eval_origin(result, origin);
        }

        if self.db.get_merged_intersection_origin(result).is_none()
            && let Some(origin) = self.db.get_merged_intersection_origin(source)
        {
            let rewritten_origin = self.rewrite(origin);
            let raw_origin = self
                .db
                .get_merged_intersection_origin(rewritten_origin)
                .unwrap_or(rewritten_origin);
            self.db.store_merged_intersection_origin(result, raw_origin);
        }

        if let Some(alias) = self.db.get_display_alias(source) {
            let alias = self.rewrite(alias);
            if matches!(self.db.lookup(alias), Some(TypeData::Application(_))) {
                self.db
                    .store_display_alias_preferring_application(result, alias);
            } else if self.db.get_display_alias(result).is_none() {
                self.db.store_display_alias(result, alias);
            }
        }

        if self.db.is_conditional_alias_base(source) {
            self.db.mark_conditional_alias_base(result);
        }
        if self.db.is_global_this_surface_display(source) {
            self.db.mark_global_this_surface_display(result);
        }
        if self.db.is_literal_object_annotation(source) {
            self.db.mark_literal_object_annotation(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefId;
    use crate::intern::TypeInterner;

    fn fresh_param(db: &TypeInterner, name: &str) -> TypeId {
        db.fresh_type_param(TypeParamInfo::simple(db.intern_string(name)))
    }

    fn tuple_members(db: &TypeInterner, type_id: TypeId) -> Vec<TypeId> {
        let Some(TypeData::Tuple(list_id)) = db.lookup(type_id) else {
            panic!("expected tuple, got {:?}", db.lookup(type_id));
        };
        db.tuple_list(list_id)
            .iter()
            .map(|element| element.type_id)
            .collect()
    }

    #[test]
    fn exact_rewrite_batches_shared_nodes_and_is_simultaneous() {
        let db = TypeInterner::new();
        let first = fresh_param(&db, "First");
        let second = fresh_param(&db, "Second");
        let shared = db.application(TypeId::OBJECT, vec![first, second]);
        let root = db.tuple(vec![
            TupleElement::fixed(first),
            TupleElement::fixed(second),
            TupleElement::fixed(shared),
            TupleElement::fixed(shared),
        ]);

        let result = substitute_exact_types(&db, root, &[first, second], &[second, first]);
        let members = tuple_members(&db, result);
        assert_eq!(members[0], second);
        assert_eq!(members[1], first);
        assert_eq!(members[2], members[3]);

        let Some(TypeData::Application(app_id)) = db.lookup(members[2]) else {
            panic!("expected application");
        };
        let app = db.type_application(app_id);
        assert_eq!(app.args, vec![second, first]);
    }

    #[test]
    fn exact_rewrite_uses_identity_not_same_named_binder() {
        let db = TypeInterner::new();
        let declaration = fresh_param(&db, "Tail");
        let foreign = fresh_param(&db, "Tail");
        assert_ne!(declaration, foreign);
        let root = db.tuple(vec![
            TupleElement::fixed(declaration),
            TupleElement::fixed(foreign),
        ]);

        let result = substitute_exact_type(&db, root, declaration, TypeId::STRING);
        assert_eq!(tuple_members(&db, result), vec![TypeId::STRING, foreign]);

        let no_match = db.array(foreign);
        assert_eq!(
            substitute_exact_type(&db, no_match, declaration, TypeId::STRING),
            no_match,
        );
    }

    #[test]
    fn exact_rewrite_reaches_mapped_binder_and_surface_fields() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let iter_info = TypeParamInfo {
            name: db.intern_string("Key"),
            constraint: Some(outer),
            default: Some(db.array(outer)),
            is_const: true,
            origin: crate::types::TypeParamOrigin::User,
        };
        let mapped = db.mapped(MappedType {
            type_param: iter_info,
            constraint: outer,
            name_type: Some(db.readonly_type(outer)),
            template: db.array(outer),
            readonly_modifier: None,
            optional_modifier: None,
        });

        let result = substitute_exact_type(&db, mapped, outer, TypeId::STRING);
        let Some(TypeData::Mapped(mapped_id)) = db.lookup(result) else {
            panic!("expected mapped type");
        };
        let mapped = db.get_mapped(mapped_id);
        assert_eq!(mapped.type_param.constraint, Some(TypeId::STRING));
        assert_eq!(mapped.type_param.default, Some(db.array(TypeId::STRING)));
        assert!(mapped.type_param.is_const);
        assert_eq!(mapped.constraint, TypeId::STRING);
        assert_eq!(mapped.name_type, Some(db.readonly_type(TypeId::STRING)));
        assert_eq!(mapped.template, db.array(TypeId::STRING));
    }

    #[test]
    fn exact_rewrite_reaches_function_callable_and_index_metadata() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let signature_param = TypeParamInfo {
            name: db.intern_string("Inner"),
            constraint: Some(outer),
            default: Some(db.array(outer)),
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let predicate = TypePredicate {
            asserts: false,
            target: crate::types::TypePredicateTarget::This,
            type_id: Some(outer),
            parameter_index: None,
        };
        let function = db.function(FunctionShape {
            type_params: vec![signature_param],
            params: vec![ParamInfo {
                type_id: outer,
                ..ParamInfo::default()
            }],
            this_type: Some(outer),
            return_type: db.array(outer),
            type_predicate: Some(predicate),
            is_constructor: false,
            is_method: true,
        });
        let call_signature = CallSignature {
            type_params: vec![signature_param],
            params: vec![ParamInfo {
                type_id: outer,
                ..ParamInfo::default()
            }],
            this_type: Some(outer),
            return_type: outer,
            type_predicate: Some(predicate),
            is_method: true,
        };
        let callable = db.callable(CallableShape {
            call_signatures: vec![call_signature],
            construct_signatures: Vec::new(),
            properties: vec![PropertyInfo::new(db.intern_string("value"), outer)],
            string_index: Some(IndexSignature {
                key_type: outer,
                value_type: db.array(outer),
                readonly: true,
                param_name: None,
            }),
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let root = db.tuple(vec![
            TupleElement::fixed(function),
            TupleElement::fixed(callable),
        ]);

        let result = substitute_exact_type(&db, root, outer, TypeId::NUMBER);
        let members = tuple_members(&db, result);
        let Some(TypeData::Function(function_id)) = db.lookup(members[0]) else {
            panic!("expected function");
        };
        let function = db.function_shape(function_id);
        assert_eq!(function.type_params[0].constraint, Some(TypeId::NUMBER));
        assert_eq!(function.params[0].type_id, TypeId::NUMBER);
        assert_eq!(function.this_type, Some(TypeId::NUMBER));
        assert_eq!(function.return_type, db.array(TypeId::NUMBER));
        assert_eq!(
            function
                .type_predicate
                .expect("rewritten function should retain its predicate")
                .type_id,
            Some(TypeId::NUMBER)
        );

        let Some(TypeData::Callable(callable_id)) = db.lookup(members[1]) else {
            panic!("expected callable");
        };
        let callable = db.callable_shape(callable_id);
        assert_eq!(
            callable.call_signatures[0].type_params[0].default,
            Some(db.array(TypeId::NUMBER)),
        );
        assert_eq!(callable.properties[0].type_id, TypeId::NUMBER);
        let index = callable
            .string_index
            .expect("rewritten callable should retain its string index");
        assert_eq!(index.key_type, TypeId::NUMBER);
        assert_eq!(index.value_type, db.array(TypeId::NUMBER));
    }

    #[test]
    fn exact_rewrite_reaches_parameter_infer_enum_and_substitution_fields() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let base = fresh_param(&db, "Base");
        let info = TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(outer),
            default: Some(db.array(outer)),
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let nested_param = db.type_param(info);
        let infer = db.infer(info);
        let enum_type = db.enum_type(DefId(7), outer);
        let substitution = db.substitution(base, outer);
        assert!(matches!(
            db.lookup(substitution),
            Some(TypeData::Substitution { .. })
        ));
        let root = db.tuple(vec![
            TupleElement::fixed(nested_param),
            TupleElement::fixed(infer),
            TupleElement::fixed(enum_type),
            TupleElement::fixed(substitution),
        ]);

        let result = substitute_exact_type(&db, root, outer, TypeId::STRING);
        let members = tuple_members(&db, result);
        for member in &members[..2] {
            let info = match db.lookup(*member) {
                Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info,
                other => panic!("expected parameter metadata, got {other:?}"),
            };
            assert_eq!(info.constraint, Some(TypeId::STRING));
            assert_eq!(info.default, Some(db.array(TypeId::STRING)));
        }
        assert_eq!(
            db.lookup(members[2]),
            Some(TypeData::Enum(DefId(7), TypeId::STRING))
        );
        assert_eq!(
            db.lookup(members[3]),
            Some(TypeData::Substitution {
                base_type: base,
                constraint: TypeId::STRING,
            }),
        );
    }

    #[test]
    fn exact_rewrite_preserves_distinct_fresh_type_parameter_identities() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let nested_info = TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(outer),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let first = db.fresh_type_param(nested_info);
        let second = db.fresh_type_param(nested_info);
        assert_ne!(first, second);

        let function = db.function(FunctionShape {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                type_id: first,
                ..ParamInfo::default()
            }],
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        });
        let callable = db.callable(CallableShape {
            call_signatures: vec![CallSignature {
                type_params: Vec::new(),
                params: vec![ParamInfo {
                    type_id: second,
                    ..ParamInfo::default()
                }],
                this_type: None,
                return_type: TypeId::VOID,
                type_predicate: None,
                is_method: false,
            }],
            construct_signatures: Vec::new(),
            properties: Vec::new(),
            string_index: None,
            number_index: None,
            symbol: None,
            is_abstract: false,
        });
        let root = db.tuple(vec![
            TupleElement::fixed(function),
            TupleElement::fixed(callable),
        ]);

        let result = substitute_exact_type(&db, root, outer, TypeId::STRING);
        let members = tuple_members(&db, result);
        let Some(TypeData::Function(function_id)) = db.lookup(members[0]) else {
            panic!("expected function");
        };
        let rewritten_first = db.function_shape(function_id).params[0].type_id;
        let Some(TypeData::Callable(callable_id)) = db.lookup(members[1]) else {
            panic!("expected callable");
        };
        let rewritten_second = db.callable_shape(callable_id).call_signatures[0].params[0].type_id;

        assert_ne!(rewritten_first, rewritten_second);
        for rewritten in [rewritten_first, rewritten_second] {
            let Some(TypeData::TypeParameter(info)) = db.lookup(rewritten) else {
                panic!("expected fresh type parameter");
            };
            assert_eq!(info.constraint, Some(TypeId::STRING));
        }
    }

    #[test]
    fn exact_rewrite_preserves_rewritten_object_provenance() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        // Application display aliases are preferred only when they predate the
        // evaluated structural result, matching normal evaluator allocation.
        let application_origin = db.application(db.lazy(DefId(11)), vec![outer]);
        let left = db.object(vec![PropertyInfo::new(db.intern_string("left"), outer)]);
        let right = db.object(vec![PropertyInfo::new(
            db.intern_string("right"),
            TypeId::NUMBER,
        )]);
        let source = db.intersection(vec![left, right]);
        assert!(db.get_merged_intersection_origin(source).is_some());

        db.store_display_properties(
            source,
            vec![PropertyInfo::new(db.intern_string("shown"), outer)],
        );
        db.record_application_eval_origin(source, application_origin);
        db.store_display_alias_preferring_application(source, application_origin);

        let result = substitute_exact_type(&db, source, outer, TypeId::STRING);
        assert_ne!(result, source);
        assert_eq!(
            db.get_display_properties(result)
                .expect("rewritten object should retain display properties")[0]
                .type_id,
            TypeId::STRING,
        );
        assert!(db.get_merged_intersection_origin(result).is_some());

        let origin = db
            .get_application_eval_origin(result)
            .expect("rewritten object should retain its application origin");
        let Some(TypeData::Application(app_id)) = db.lookup(origin) else {
            panic!("expected application origin");
        };
        assert_eq!(db.type_application(app_id).args, vec![TypeId::STRING]);
        assert_eq!(db.get_display_alias(result), Some(origin));
    }
}
