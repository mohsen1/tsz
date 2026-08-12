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

/// Reusable mapping produced by one completed exact-identity rewrite.
///
/// The representation is intentionally opaque. It retains direct replacement
/// pairs and structurally changed source-to-target pairs, but drops unchanged
/// visitation entries. One memo is reusable for every root that shares the
/// same exact replacement pairs; root results and shared structural mappings
/// are retained once per session. Cache owners use
/// [`Self::refresh_provenance`] before returning an associated rewritten type.
#[derive(Clone, Debug)]
pub struct ExactRewriteMemo {
    mapped: FxHashMap<TypeId, TypeId>,
    provenance_sources: Vec<TypeId>,
    root_results: FxHashMap<TypeId, TypeId>,
    provenance_generation: u64,
}

/// A shared solver-frame limit prevented an exact rewrite from completing.
///
/// Aborted attempts do not publish provenance or reusable memo state. Callers
/// must preserve the original type and may retry under a fresh frame budget.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactRewriteAborted;

impl ExactRewriteMemo {
    fn merge_delta(&mut self, delta: ExactRewriteDelta) {
        self.mapped.extend(delta.mapped);
        self.provenance_sources.extend(delta.provenance_sources);
    }

    /// Replay all currently attached source provenance onto the previously
    /// mapped target nodes.
    ///
    /// Existing structural mappings seed the walk, so refreshing provenance
    /// never remints a nested fresh type parameter that the completed rewrite
    /// already rebuilt. New provenance-only subgraphs extend the memo only after
    /// the whole refresh completes. On [`ExactRewriteAborted`], neither this memo
    /// nor any provenance side table is mutated.
    pub fn refresh_provenance(
        &mut self,
        db: &dyn QueryDatabase,
    ) -> Result<(), ExactRewriteAborted> {
        let provenance_generation = db.display_provenance_generation();
        if provenance_generation == self.provenance_generation {
            return Ok(());
        }

        let delta = {
            let mut rewriter = ExactTypeRewriter {
                db,
                base_mappings: Some(&self.mapped),
                rewritten: FxHashMap::default(),
                provenance_sources: Vec::new(),
                aborted: false,
                pending_provenance: Vec::new(),
            };
            for &source in &self.provenance_sources {
                debug_assert!(
                    self.mapped.contains_key(&source),
                    "provenance source must have a retained mapping",
                );
                let Some(&result) = self.mapped.get(&source) else {
                    continue;
                };
                rewriter.propagate_provenance(source, result);
                if rewriter.aborted {
                    break;
                }
            }
            rewriter.commit()?
        };

        self.merge_delta(delta);
        // Retain the pre-scan snapshot. Provenance committed by this replay or
        // written concurrently advances the universe generation and therefore
        // forces another scan; a no-op scan then converges to the current value.
        // Reading after commit could acknowledge a write that landed behind the
        // scan cursor and permanently skip it.
        self.provenance_generation = provenance_generation;
        Ok(())
    }

    /// Rewrite another root with this session's exact replacement pairs.
    ///
    /// A previously completed root is an `O(1)` result lookup plus the
    /// provenance-generation check. A new root reuses all retained structural
    /// mappings, so shared subgraphs and nested fresh binders keep their exact
    /// rewritten identities. Provenance refresh and the new root walk commit as
    /// one transaction: on [`ExactRewriteAborted`], neither the memo nor any
    /// provenance side table is mutated. Callers must not cache the failed root
    /// result and may retry it under a fresh frame budget.
    pub fn rewrite_root(
        &mut self,
        db: &dyn QueryDatabase,
        root: TypeId,
    ) -> Result<TypeId, ExactRewriteAborted> {
        if let Some(&result) = self.root_results.get(&root) {
            self.refresh_provenance(db)?;
            return Ok(result);
        }

        let provenance_generation = db.display_provenance_generation();
        let refresh_provenance = provenance_generation != self.provenance_generation;
        let (result, delta) = {
            let mut rewriter = ExactTypeRewriter {
                db,
                base_mappings: Some(&self.mapped),
                rewritten: FxHashMap::default(),
                provenance_sources: Vec::new(),
                aborted: false,
                pending_provenance: Vec::new(),
            };
            if refresh_provenance {
                for &source in &self.provenance_sources {
                    debug_assert!(
                        self.mapped.contains_key(&source),
                        "provenance source must have a retained mapping",
                    );
                    let Some(&mapped) = self.mapped.get(&source) else {
                        continue;
                    };
                    rewriter.propagate_provenance(source, mapped);
                    if rewriter.aborted {
                        break;
                    }
                }
            }
            let result = rewriter.rewrite(root);
            let delta = rewriter.commit()?;
            (result, delta)
        };

        self.merge_delta(delta);
        self.root_results.insert(root, result);
        self.provenance_generation = provenance_generation;
        Ok(result)
    }
}

/// Replace aligned exact identities throughout `root` in one graph walk.
///
/// Replacements are simultaneous: a replacement value is terminal and is not
/// itself rewritten through another pair. The walk is
/// `O(P + V + E + Σ Uₖ log Uₖ)`, where
/// `P` is the number of non-identity replacement pairs, `V` is the number of
/// reachable interned nodes, and `E` is the number of structural and provenance
/// slots traversed; each `Uₖ` is the size of a changed union whose canonical
/// member order is restored. Shared nodes are rebuilt once, intersection member
/// lists are reconstructed in one linear pass, and a no-op graph retains its
/// original interned identity. Auxiliary storage is `O(P + V + Q)`, where `Q`
/// is the provenance transfer count staged for all-or-nothing commit.
pub fn substitute_exact_types(
    db: &dyn QueryDatabase,
    root: TypeId,
    from: &[TypeId],
    to: &[TypeId],
) -> TypeId {
    substitute_exact_types_with_memo(db, root, from, to).map_or(root, |(result, _memo)| result)
}

/// Replace aligned exact identities and retain the completed structural map
/// for provenance refreshes by cache owners.
///
/// [`ExactRewriteAborted`] is distinct from a successful no-op rewrite. No
/// provenance is published and no memo is returned when the shared solver-frame
/// budget aborts the walk. The aligned pairs must be scoped binder identities:
/// direct replacements are terminal and their source provenance is deliberately
/// not copied onto the replacement identity. Copying it would repaint a shared
/// active binder that belongs to the destination scope.
pub fn substitute_exact_types_with_memo(
    db: &dyn QueryDatabase,
    root: TypeId,
    from: &[TypeId],
    to: &[TypeId],
) -> Result<(TypeId, ExactRewriteMemo), ExactRewriteAborted> {
    debug_assert_eq!(from.len(), to.len());
    let provenance_generation = db.display_provenance_generation();
    if from.len() != to.len() || from.is_empty() {
        let mut root_results = FxHashMap::default();
        root_results.insert(root, root);
        return Ok((
            root,
            ExactRewriteMemo {
                mapped: FxHashMap::default(),
                provenance_sources: Vec::new(),
                root_results,
                provenance_generation,
            },
        ));
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
        let mut root_results = FxHashMap::default();
        root_results.insert(root, root);
        return Ok((
            root,
            ExactRewriteMemo {
                mapped: rewritten,
                provenance_sources: Vec::new(),
                root_results,
                provenance_generation,
            },
        ));
    }

    let mut rewriter = ExactTypeRewriter {
        db,
        base_mappings: None,
        rewritten,
        provenance_sources: Vec::new(),
        aborted: false,
        pending_provenance: Vec::new(),
    };
    let result = rewriter.rewrite(root);
    let delta = rewriter.commit()?;
    let mut root_results = FxHashMap::default();
    root_results.insert(root, result);
    Ok((
        result,
        ExactRewriteMemo {
            mapped: delta.mapped,
            provenance_sources: delta.provenance_sources,
            root_results,
            provenance_generation,
        },
    ))
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
    /// Retained mappings from a previous completed walk. These are read-only
    /// until this attempt commits, so an abort cannot poison reusable state.
    base_mappings: Option<&'a FxHashMap<TypeId, TypeId>>,
    /// Preloaded with direct replacements on the first walk, then extended with
    /// this attempt's per-node results. Direct replacements remain terminal.
    rewritten: FxHashMap<TypeId, TypeId>,
    /// Structurally changed sources discovered by this attempt. Only these
    /// sources can need future provenance replay.
    provenance_sources: Vec<TypeId>,
    /// Sticky shared-frame bailout. Once set, every active caller preserves its
    /// source node and the public operation returns its original root.
    aborted: bool,
    /// Provenance writes are transactional: speculative structural nodes may be
    /// interned during a walk, but no side table is mutated unless the whole
    /// reachable graph completes within the shared solver-frame budget.
    pending_provenance: Vec<PendingProvenance>,
}

struct ExactRewriteDelta {
    mapped: FxHashMap<TypeId, TypeId>,
    provenance_sources: Vec<TypeId>,
}

enum PendingProvenance {
    DisplayProperties(TypeId, Vec<PropertyInfo>),
    RewrittenUnionOrigin(TypeId, Vec<TypeId>, bool),
    ApplicationEvalOrigin(TypeId, TypeId),
    MergedIntersectionOrigin(TypeId, TypeId),
    RewrittenApplicationDisplayAlias(TypeId, TypeId),
    DisplayAliasIfAbsent(TypeId, TypeId),
    ConditionalAliasBase(TypeId),
    GlobalThisSurfaceDisplay(TypeId),
    LiteralObjectAnnotation(TypeId),
}

impl ExactTypeRewriter<'_> {
    fn rewrite(&mut self, type_id: TypeId) -> TypeId {
        if self.aborted {
            return type_id;
        }
        if let Some(&cached) = self.rewritten.get(&type_id) {
            return cached;
        }
        if let Some(&cached) = self
            .base_mappings
            .and_then(|mappings| mappings.get(&type_id))
        {
            return cached;
        }
        if type_id.is_intrinsic() {
            return type_id;
        }

        let Some(result) = crate::recursion::with_solver_frame(|| self.rewrite_in_frame(type_id))
        else {
            self.aborted = true;
            return type_id;
        };
        result
    }

    fn rewrite_in_frame(&mut self, type_id: TypeId) -> TypeId {
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
                if self.aborted
                    || (properties.is_none()
                        && string_index.is_none()
                        && number_index.is_none()
                        && symbol_index.is_none())
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
            TypeData::Union(list_id) => {
                let Some(members) = self.rewrite_type_ids(self.db.type_list(list_id).as_ref())
                else {
                    return type_id;
                };
                let result = self.db.union_preserve_members(members.clone());
                if self.db.get_union_origin(type_id).is_none() {
                    self.pending_provenance
                        .push(PendingProvenance::RewrittenUnionOrigin(
                            result, members, true,
                        ));
                }
                result
            }
            TypeData::Intersection(list_id) => self
                .rewrite_type_ids(self.db.type_list(list_id).as_ref())
                .map_or(type_id, |members| {
                    self.db.intersect_types_raw_for_replay(members)
                }),
            TypeData::Array(element) => {
                let rewritten = self.rewrite(element);
                if self.aborted || rewritten == element {
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
                if self.aborted || rewritten == member_type {
                    type_id
                } else {
                    self.db.enum_type(def_id, rewritten)
                }
            }
            TypeData::Application(app_id) => {
                let app = self.db.type_application(app_id);
                let base = self.rewrite(app.base);
                let args = self.rewrite_type_ids(&app.args);
                if self.aborted || (base == app.base && args.is_none()) {
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
                if self.aborted || rewritten == cond {
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
                if self.aborted
                    || (type_param.is_none()
                        && constraint == mapped.constraint
                        && name_type == mapped.name_type
                        && template == mapped.template)
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
                if self.aborted
                    || (object_type_rewritten == object_type && index_type_rewritten == index_type)
                {
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
                if self.aborted
                    || (base_type_rewritten == base_type && constraint_rewritten == constraint)
                {
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
            self.provenance_sources.push(type_id);
            self.propagate_provenance(type_id, result);
        }
        if self.aborted { type_id } else { result }
    }

    fn commit(self) -> Result<ExactRewriteDelta, ExactRewriteAborted> {
        if self.aborted {
            return Err(ExactRewriteAborted);
        }
        for provenance in self.pending_provenance {
            match provenance {
                PendingProvenance::DisplayProperties(type_id, properties) => {
                    self.db.store_display_properties(type_id, properties);
                }
                PendingProvenance::RewrittenUnionOrigin(type_id, members, is_fallback) => {
                    self.db
                        .store_rewritten_union_origin(type_id, members, is_fallback);
                }
                PendingProvenance::ApplicationEvalOrigin(type_id, origin) => {
                    self.db.record_application_eval_origin(type_id, origin);
                }
                PendingProvenance::MergedIntersectionOrigin(type_id, origin) => {
                    self.db.store_merged_intersection_origin(type_id, origin);
                }
                PendingProvenance::RewrittenApplicationDisplayAlias(type_id, alias) => {
                    self.db
                        .transfer_rewritten_application_display_alias(type_id, alias);
                }
                PendingProvenance::DisplayAliasIfAbsent(type_id, alias) => {
                    if self.db.get_display_alias(type_id).is_none() {
                        self.db.store_display_alias(type_id, alias);
                    }
                }
                PendingProvenance::ConditionalAliasBase(type_id) => {
                    self.db.mark_conditional_alias_base(type_id);
                }
                PendingProvenance::GlobalThisSurfaceDisplay(type_id) => {
                    self.db.mark_global_this_surface_display(type_id);
                }
                PendingProvenance::LiteralObjectAnnotation(type_id) => {
                    self.db.mark_literal_object_annotation(type_id);
                }
            }
        }
        let mapped = self
            .rewritten
            .into_iter()
            .filter(|(source, result)| source != result)
            .collect();
        Ok(ExactRewriteDelta {
            mapped,
            provenance_sources: self.provenance_sources,
        })
    }

    fn rewrite_unary(
        &mut self,
        original: TypeId,
        inner: TypeId,
        build: impl FnOnce(&dyn TypeDatabase, TypeId) -> TypeId,
    ) -> TypeId {
        let rewritten = self.rewrite(inner);
        if self.aborted || rewritten == inner {
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
        (!self.aborted).then_some(changed).flatten()
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
        (!self.aborted).then_some(changed).flatten()
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
        (!self.aborted).then_some(changed).flatten()
    }

    fn rewrite_index_signature(&mut self, index: &IndexSignature) -> Option<IndexSignature> {
        let key_type = self.rewrite(index.key_type);
        let value_type = self.rewrite(index.value_type);
        (!self.aborted && (key_type != index.key_type || value_type != index.value_type)).then_some(
            IndexSignature {
                key_type,
                value_type,
                ..*index
            },
        )
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
        (!self.aborted).then_some(changed).flatten()
    }

    fn rewrite_type_param(&mut self, param: TypeParamInfo) -> Option<TypeParamInfo> {
        let constraint = param.constraint.map(|constraint| self.rewrite(constraint));
        let default = param.default.map(|default| self.rewrite(default));
        let rewritten = TypeParamInfo {
            constraint,
            default,
            ..param
        };
        (!self.aborted && rewritten != param).then_some(rewritten)
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
        (!self.aborted).then_some(changed).flatten()
    }

    fn rewrite_predicate(&mut self, predicate: TypePredicate) -> Option<TypePredicate> {
        let type_id = predicate.type_id.map(|type_id| self.rewrite(type_id));
        (!self.aborted && type_id != predicate.type_id).then_some(TypePredicate {
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
        if self.aborted
            || (type_params.is_none()
                && params.is_none()
                && this_type == shape.this_type
                && return_type == shape.return_type
                && type_predicate.is_none())
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
        if self.aborted
            || (type_params.is_none()
                && params.is_none()
                && this_type == signature.this_type
                && return_type == signature.return_type
                && type_predicate.is_none())
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
        (!self.aborted).then_some(changed).flatten()
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
        if self.aborted
            || (call_signatures.is_none()
                && construct_signatures.is_none()
                && properties.is_none()
                && string_index.is_none()
                && number_index.is_none())
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
        (!self.aborted).then_some(changed).flatten()
    }

    fn propagate_provenance(&mut self, source: TypeId, result: TypeId) {
        if let Some(properties) = self.db.get_display_properties(source) {
            let properties = self
                .rewrite_properties(properties.as_ref())
                .unwrap_or_else(|| properties.as_ref().clone());
            self.pending_provenance
                .push(PendingProvenance::DisplayProperties(result, properties));
        }

        if let Some(origin) = self.db.get_union_origin(source) {
            let origin = self
                .rewrite_type_ids(origin.as_ref())
                .unwrap_or_else(|| origin.as_ref().clone());
            self.pending_provenance
                .push(PendingProvenance::RewrittenUnionOrigin(
                    result, origin, false,
                ));
        }

        // Application provenance is first-write-wins. Stage it before replaying
        // a merged origin, whose member reconstruction can intern the same
        // structural result through a different application.
        if self.db.get_application_eval_origin(result).is_none()
            && let Some(origin) = self.db.get_application_eval_origin(source)
        {
            let origin = self.rewrite(origin);
            self.pending_provenance
                .push(PendingProvenance::ApplicationEvalOrigin(result, origin));
        }

        if self.db.get_merged_intersection_origin(result).is_none()
            && let Some(origin) = self.db.get_merged_intersection_origin(source)
        {
            let rewritten_origin = self.rewrite(origin);
            let raw_origin = self
                .db
                .get_merged_intersection_origin(rewritten_origin)
                .unwrap_or(rewritten_origin);
            self.pending_provenance
                .push(PendingProvenance::MergedIntersectionOrigin(
                    result, raw_origin,
                ));
        }

        if let Some(alias) = self.db.get_display_alias(source) {
            let alias = self.rewrite(alias);
            if matches!(self.db.lookup(alias), Some(TypeData::Application(_))) {
                self.pending_provenance
                    .push(PendingProvenance::RewrittenApplicationDisplayAlias(
                        result, alias,
                    ));
            } else if self.db.get_display_alias(result).is_none() {
                self.pending_provenance
                    .push(PendingProvenance::DisplayAliasIfAbsent(result, alias));
            }
        }

        if self.db.is_conditional_alias_base(source) {
            self.pending_provenance
                .push(PendingProvenance::ConditionalAliasBase(result));
        }
        if self.db.is_global_this_surface_display(source) {
            self.pending_provenance
                .push(PendingProvenance::GlobalThisSurfaceDisplay(result));
        }
        if self.db.is_literal_object_annotation(source) {
            self.pending_provenance
                .push(PendingProvenance::LiteralObjectAnnotation(result));
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

    const COMPLEX_REPLAY_UNION_WIDTH: usize = 317;
    const _: () = assert!(COMPLEX_REPLAY_UNION_WIDTH * COMPLEX_REPLAY_UNION_WIDTH >= 100_000);

    fn complex_replay_intersection(db: &TypeInterner, outer: TypeId) -> TypeId {
        let mut left = Vec::with_capacity(COMPLEX_REPLAY_UNION_WIDTH);
        left.push(outer);
        for index in 1..COMPLEX_REPLAY_UNION_WIDTH {
            left.push(db.literal_string(&format!("left{index}")));
        }
        let left = db.union_preserve_members(left);

        let right = (0..COMPLEX_REPLAY_UNION_WIDTH)
            .map(|index| db.literal_number(index as f64))
            .collect();
        let right = db.union_preserve_members(right);

        db.intersect_types_raw_for_replay(vec![left, right])
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
    fn exact_rewrite_preserves_union_members_and_raw_intersection_shape() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let subtype_member = db.literal_string("member");
        assert_eq!(
            db.union(vec![subtype_member, TypeId::STRING]),
            TypeId::STRING,
            "ordinary union construction absorbs the literal subtype",
        );
        let union = db.union_preserve_members(vec![outer, TypeId::STRING]);

        let rewritten_union = substitute_exact_type(&db, union, outer, subtype_member);
        let Some(TypeData::Union(list_id)) = db.lookup(rewritten_union) else {
            panic!("exact replay must not subtype-reduce the literal member");
        };
        assert_eq!(db.type_list(list_id).len(), 2);

        let left = db.object(vec![PropertyInfo::new(db.intern_string("left"), outer)]);
        let right = db.object(vec![PropertyInfo::new(
            db.intern_string("right"),
            TypeId::NUMBER,
        )]);
        let intersection = db.intersect_types_raw(vec![left, right]);
        let rewritten_intersection =
            substitute_exact_type(&db, intersection, outer, subtype_member);
        let Some(TypeData::Intersection(list_id)) = db.lookup(rewritten_intersection) else {
            panic!("exact replay must not normalize raw object intersections");
        };
        assert_eq!(db.type_list(list_id).len(), 2);
    }

    #[test]
    fn exact_rewrite_preserves_pre_sort_union_member_order() {
        let db = TypeInterner::new();
        let source = fresh_param(&db, "Source");
        let other = fresh_param(&db, "Other");
        let union = db.union_preserve_members(vec![source, other]);
        assert_eq!(db.get_union_origin(union), None);

        let replacement = fresh_param(&db, "Replacement");
        let result = substitute_exact_type(&db, union, source, replacement);

        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(vec![replacement, other]),
        );
    }

    #[test]
    fn exact_rewrite_prefers_an_existing_union_origin() {
        let db = TypeInterner::new();
        let first = fresh_param(&db, "First");
        let source = fresh_param(&db, "Source");
        let union = db.union_preserve_members(vec![first, source]);
        db.store_union_origin(union, vec![source, first]);
        assert_eq!(
            db.get_union_origin(union).map(|origin| origin.to_vec()),
            Some(vec![source, first]),
        );

        let replacement = fresh_param(&db, "Replacement");
        let result = substitute_exact_type(&db, union, source, replacement);

        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(vec![replacement, first]),
        );
    }

    #[test]
    fn exact_rewrite_complex_intersection_replay_does_not_signal_union_complexity() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let source = complex_replay_intersection(&db, outer);
        assert!(!db.is_union_too_complex());

        let result = substitute_exact_type(&db, source, outer, db.literal_string("replacement"));
        assert_ne!(result, source);
        assert!(matches!(db.lookup(result), Some(TypeData::Intersection(_))));
        assert!(
            !db.is_union_too_complex(),
            "replaying admitted structure must not request TS2590",
        );
    }

    #[test]
    fn exact_rewrite_complex_intersection_before_depth_bail_does_not_leak_flag() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let intersection = complex_replay_intersection(&db, outer);
        assert!(!db.is_union_too_complex());

        let mut deep = outer;
        for _ in 0..=crate::recursion::MAX_SOLVER_STACK_FRAMES {
            deep = db.array(deep);
        }
        let root = db.tuple(vec![
            TupleElement::fixed(intersection),
            TupleElement::fixed(deep),
        ]);

        assert_eq!(
            substitute_exact_type(&db, root, outer, TypeId::STRING),
            root,
        );
        assert!(
            !db.is_union_too_complex(),
            "discarded replay work must not leak a TS2590 signal",
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
                suppress_display_optional: false,
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
                suppress_display_optional: false,
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
    fn exact_rewrite_concretizes_outer_constraint_and_keeps_nested_local_identity() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let key = fresh_param(&db, "Key");
        let local_info = TypeParamInfo {
            name: db.intern_string("Local"),
            constraint: Some(db.union(vec![outer, key])),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        };
        let local = db.fresh_type_param(local_info);
        let method = db.function(FunctionShape {
            type_params: vec![local_info],
            params: vec![ParamInfo::required(db.intern_string("value"), local)],
            this_type: None,
            return_type: local,
            type_predicate: None,
            is_constructor: false,
            is_method: true,
        });
        let concrete_key = db.literal_string("table");

        let result =
            substitute_exact_types(&db, method, &[outer, key], &[TypeId::NUMBER, concrete_key]);
        let Some(TypeData::Function(shape_id)) = db.lookup(result) else {
            panic!("expected materialized method, got {:?}", db.lookup(result));
        };
        let shape = db.function_shape(shape_id);
        let rewritten_local = shape.params[0].type_id;
        assert_eq!(shape.return_type, rewritten_local);
        assert_eq!(shape.type_params.len(), 1);
        assert_eq!(
            db.lookup(rewritten_local),
            Some(TypeData::TypeParameter(shape.type_params[0])),
        );
        assert_eq!(
            shape.type_params[0].constraint,
            Some(db.union(vec![TypeId::NUMBER, concrete_key])),
        );
        let constraint_members = crate::visitor::collect_all_types(
            &db,
            shape.type_params[0]
                .constraint
                .expect("local constraint should remain present"),
        );
        assert!(!constraint_members.contains(&outer));
        assert!(!constraint_members.contains(&key));
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
                suppress_display_optional: false,
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
                    suppress_display_optional: false,
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

    #[test]
    fn exact_rewrite_transfers_generic_display_alias_in_both_allocation_orders() {
        fn run(alias_before_result: bool) {
            let db = TypeInterner::new();
            let source_param = fresh_param(&db, "Source");
            let replacement = fresh_param(&db, "Replacement");
            let base = db.lazy(DefId(21));

            // Seed valid source provenance in the ordinary evaluator order:
            // the application exists before its evaluated structural result.
            let source_alias = db.application(base, vec![source_param]);
            let source = db.array(source_param);
            db.store_display_alias_preferring_application(source, source_alias);
            assert_eq!(db.get_display_alias(source), Some(source_alias));

            let expected_alias =
                alias_before_result.then(|| db.application(base, vec![replacement]));
            let expected_result = (!alias_before_result).then(|| db.array(replacement));

            let result = substitute_exact_type(&db, source, source_param, replacement);
            let expected_result = expected_result.unwrap_or_else(|| db.array(replacement));
            let expected_alias =
                expected_alias.unwrap_or_else(|| db.application(base, vec![replacement]));

            assert_eq!(result, expected_result);
            assert_eq!(db.get_display_alias(result), Some(expected_alias));
        }

        run(true);
        run(false);
    }

    #[test]
    fn exact_rewrite_does_not_repaint_an_existing_application_alias() {
        let db = TypeInterner::new();
        let source_param = fresh_param(&db, "Source");
        let replacement = fresh_param(&db, "Replacement");
        let source_base = db.lazy(DefId(24));
        let existing_base = db.lazy(DefId(25));

        let source_alias = db.application(source_base, vec![source_param]);
        let source = db.array(source_param);
        db.store_display_alias_preferring_application(source, source_alias);

        let existing_alias = db.application(existing_base, vec![replacement]);
        let expected = db.array(replacement);
        db.store_display_alias_preferring_application(expected, existing_alias);
        assert_eq!(db.get_display_alias(expected), Some(existing_alias));

        let result = substitute_exact_type(&db, source, source_param, replacement);

        assert_eq!(result, expected);
        assert_eq!(db.get_display_alias(result), Some(existing_alias));
    }

    #[test]
    fn rewritten_display_alias_transfer_retains_global_identity_and_cycle_guards() {
        let db = TypeInterner::new();
        let parameter = fresh_param(&db, "Parameter");
        let base = db.lazy(DefId(23));
        let safe_alias = db.application(base, vec![TypeId::STRING]);

        db.transfer_rewritten_application_display_alias(TypeId::STRING, safe_alias);
        db.transfer_rewritten_application_display_alias(parameter, safe_alias);
        assert_eq!(db.get_display_alias(TypeId::STRING), None);
        assert_eq!(db.get_display_alias(parameter), None);

        let evaluated = db.array(TypeId::STRING);
        let cyclic_alias = db.application(base, vec![evaluated]);
        db.transfer_rewritten_application_display_alias(evaluated, cyclic_alias);
        assert_eq!(db.get_display_alias(evaluated), None);
    }

    #[test]
    fn rewritten_application_alias_replaces_structural_provenance() {
        let db = TypeInterner::new();
        let evaluated = db.array(TypeId::STRING);
        let structural_alias = db.object(vec![PropertyInfo::new(
            db.intern_string("value"),
            TypeId::STRING,
        )]);
        db.store_display_alias(evaluated, structural_alias);
        assert_eq!(db.get_display_alias(evaluated), Some(structural_alias));

        let application = db.application(db.lazy(DefId(26)), vec![TypeId::STRING]);
        db.transfer_rewritten_application_display_alias(evaluated, application);

        assert_eq!(db.get_display_alias(evaluated), Some(application));
    }

    #[test]
    fn exact_rewrite_depth_bail_returns_original_without_provenance_writes() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let base = db.lazy(DefId(22));
        let source_alias = db.application(base, vec![outer]);
        let property = db.intern_string("value");
        let shallow = db.object(vec![PropertyInfo::new(property, outer)]);
        db.store_display_alias_preferring_application(shallow, source_alias);
        assert_eq!(db.get_display_alias(shallow), Some(source_alias));

        // This canonical node is the speculative shallow rewrite the first
        // tuple slot would produce before the second slot exceeds the shared
        // solver-frame budget. Its provenance must remain untouched on bail.
        let rewritten_shallow = db.object(vec![PropertyInfo::new(property, TypeId::STRING)]);
        assert_eq!(db.get_display_alias(rewritten_shallow), None);

        let mut deep = outer;
        for _ in 0..=crate::recursion::MAX_SOLVER_STACK_FRAMES {
            deep = db.array(deep);
        }
        let root = db.tuple(vec![
            TupleElement::fixed(shallow),
            TupleElement::fixed(deep),
        ]);

        assert_eq!(
            substitute_exact_type(&db, root, outer, TypeId::STRING),
            root,
        );
        assert_eq!(db.get_display_alias(rewritten_shallow), None);

        // The RAII frame budget and sticky bailout are request-scoped.
        let shallow_array = db.array(outer);
        assert_eq!(
            substitute_exact_type(&db, shallow_array, outer, TypeId::STRING),
            db.array(TypeId::STRING),
        );
    }

    #[test]
    fn exact_rewrite_memo_refreshes_late_provenance_and_converges_generation() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let other = fresh_param(&db, "Other");
        let third = fresh_param(&db, "Third");
        let alias_base = db.lazy(DefId(31));
        let source_application = db.application(alias_base, vec![outer]);
        let nested_source = db.array(outer);
        let source = db.union_preserve_members(vec![outer, other, third]);
        let replacement = fresh_param(&db, "Replacement");

        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[outer], &[replacement])
                .expect("the initial exact rewrite should complete");
        let rewritten_nested = db.array(replacement);
        let rewritten_application = db.application(alias_base, vec![replacement]);
        assert!(db.get_display_properties(result).is_none());
        let synthesized_union_fallback = db
            .get_union_origin(result)
            .expect("changed union should retain its pre-sort rewritten members");
        assert!(db.get_application_eval_origin(result).is_none());
        assert!(db.get_display_alias(result).is_none());

        let shown = db.intern_string("shown");
        db.store_display_properties(
            source,
            vec![PropertyInfo {
                declaration_order: 1,
                ..PropertyInfo::new(shown, nested_source)
            }],
        );
        db.replace_union_origin_for_display(source, vec![third, other, outer]);
        db.record_application_eval_origin(source, source_application);
        db.store_display_alias_preferring_application(source, source_application);

        memo.refresh_provenance(&db)
            .expect("late provenance replay should complete");
        let properties = db
            .get_display_properties(result)
            .expect("late display properties should reach the rewritten root");
        assert_eq!(properties[0].type_id, rewritten_nested);
        assert_eq!(properties[0].declaration_order, 1);
        assert_ne!(
            synthesized_union_fallback.as_slice(),
            &[third, other, replacement],
            "the test must replace a distinct synthesized fallback",
        );
        assert_eq!(
            db.get_union_origin(result)
                .expect("late union origin should reach the rewritten root")
                .as_slice(),
            &[third, other, replacement],
        );
        assert_eq!(
            db.get_application_eval_origin(result),
            Some(rewritten_application),
        );
        assert_eq!(db.get_display_alias(result), Some(rewritten_application));

        // A replay can advance the universe generation with its own target
        // writes. One no-op scan converges; later hits take the `O(1)` gate.
        let replay_generation = db.display_provenance_generation();
        memo.refresh_provenance(&db)
            .expect("the convergence replay should complete");
        assert_eq!(db.display_provenance_generation(), replay_generation);
        assert_eq!(memo.provenance_generation, replay_generation);
        memo.refresh_provenance(&db)
            .expect("an unchanged generation should be an immediate hit");
        assert_eq!(db.display_provenance_generation(), replay_generation);

        // `PropertyInfo` structural equality intentionally ignores declaration
        // order. The provenance epoch must still notice this display-only edit.
        db.store_display_properties(
            source,
            vec![PropertyInfo {
                declaration_order: 9,
                ..PropertyInfo::new(shown, nested_source)
            }],
        );
        assert_ne!(db.display_provenance_generation(), replay_generation);
        memo.refresh_provenance(&db)
            .expect("display-only metadata changes must replay");
        assert_eq!(
            db.get_display_properties(result)
                .expect("rewritten display properties should be replaced")[0]
                .declaration_order,
            9,
        );
    }

    #[test]
    fn exact_rewrite_union_fallback_never_repaints_unrelated_real_origin() {
        let db = TypeInterner::new();
        let source_param = fresh_param(&db, "Source");
        let other = fresh_param(&db, "Other");
        let replacement = fresh_param(&db, "Replacement");
        let source = db.union_preserve_members(vec![source_param, other]);
        let expected = db.union_preserve_members(vec![replacement, other]);
        let unrelated_target_origin = vec![replacement, other];
        db.replace_union_origin_for_display(expected, unrelated_target_origin.clone());

        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[source_param], &[replacement])
                .expect("the initial exact rewrite should complete");
        assert_eq!(result, expected);
        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(unrelated_target_origin.clone()),
            "a synthesized fallback must not replace a real target origin",
        );

        db.replace_union_origin_for_display(source, vec![other, source_param]);
        memo.refresh_provenance(&db)
            .expect("late real source provenance should replay");
        assert_eq!(
            db.get_union_origin(result).map(|origin| origin.to_vec()),
            Some(unrelated_target_origin),
            "late provenance from another rewrite session must remain sticky",
        );
    }

    #[test]
    fn exact_rewrite_late_canonical_union_origin_clears_fallback() {
        let db = TypeInterner::new();
        let source_param = fresh_param(&db, "Source");
        let other = fresh_param(&db, "Other");
        let replacement = fresh_param(&db, "Replacement");
        let source = db.union_preserve_members(vec![source_param, other]);
        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[source_param], &[replacement])
                .expect("the initial exact rewrite should complete");
        assert!(db.get_union_origin(result).is_some());

        let Some(TypeData::Union(result_list)) = db.lookup(result) else {
            panic!("expected rewritten union");
        };
        let canonical_source_origin = db
            .type_list(result_list)
            .iter()
            .map(|&member| {
                if member == replacement {
                    source_param
                } else {
                    member
                }
            })
            .collect();
        db.replace_union_origin_for_display(source, canonical_source_origin);
        memo.refresh_provenance(&db)
            .expect("late canonical provenance should replay");
        assert_eq!(
            db.get_union_origin(result),
            None,
            "a canonical real origin should clear the stale tagged fallback",
        );
    }

    #[test]
    fn exact_rewrite_memo_reuses_nested_fresh_binders_across_roots() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let nested = db.fresh_type_param(TypeParamInfo {
            name: db.intern_string("Nested"),
            constraint: Some(outer),
            default: None,
            is_const: false,
            origin: crate::types::TypeParamOrigin::User,
        });
        let first_root = db.tuple(vec![
            TupleElement::fixed(nested),
            TupleElement::fixed(db.array(nested)),
        ]);

        let (first_result, mut memo) =
            substitute_exact_types_with_memo(&db, first_root, &[outer], &[TypeId::STRING])
                .expect("the initial exact rewrite should complete");
        let rewritten_nested = tuple_members(&db, first_result)[0];
        assert_ne!(rewritten_nested, nested);
        let Some(TypeData::TypeParameter(info)) = db.lookup(rewritten_nested) else {
            panic!("expected a rewritten fresh type parameter");
        };
        assert_eq!(info.constraint, Some(TypeId::STRING));

        db.store_display_properties(
            first_root,
            vec![PropertyInfo::new(db.intern_string("shown"), nested)],
        );
        memo.refresh_provenance(&db)
            .expect("late provenance replay should complete");
        assert_eq!(
            db.get_display_properties(first_result)
                .expect("rewritten root should receive late display properties")[0]
                .type_id,
            rewritten_nested,
        );
        memo.refresh_provenance(&db)
            .expect("the generation should converge after target writes");

        let second_root = db.array(nested);
        let second_result = memo
            .rewrite_root(&db, second_root)
            .expect("a second root should reuse the completed session");
        assert_eq!(second_result, db.array(rewritten_nested));
        assert_eq!(
            memo.rewrite_root(&db, second_root)
                .expect("a completed root should be reusable"),
            second_result,
        );
    }

    #[test]
    fn exact_rewrite_direct_binder_pairs_do_not_repaint_replacements() {
        let db = TypeInterner::new();
        let source_binder = fresh_param(&db, "Source");
        let active_binder = fresh_param(&db, "Active");
        let shown = db.intern_string("shown");
        db.store_display_properties(
            source_binder,
            vec![PropertyInfo::new(shown, TypeId::STRING)],
        );

        let root = db.array(source_binder);
        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, root, &[source_binder], &[active_binder])
                .expect("the binder rewrite should complete");
        assert_eq!(result, db.array(active_binder));
        assert!(db.get_display_properties(active_binder).is_none());

        db.store_display_properties(
            source_binder,
            vec![PropertyInfo::new(shown, TypeId::NUMBER)],
        );
        memo.refresh_provenance(&db)
            .expect("late structural provenance should refresh");
        assert!(
            db.get_display_properties(active_binder).is_none(),
            "a terminal direct pair must not repaint the destination binder",
        );
    }

    #[test]
    fn exact_rewrite_abort_is_retryable_and_refresh_is_transactional() {
        let db = TypeInterner::new();
        let outer = fresh_param(&db, "Outer");
        let shown = db.intern_string("shown");
        let source = db.array(db.array(outer));
        let expected = db.array(db.array(TypeId::STRING));
        db.store_display_properties(source, vec![PropertyInfo::new(shown, outer)]);

        let held_frames: Vec<_> = (0..crate::recursion::MAX_SOLVER_STACK_FRAMES - 1)
            .map(|_| {
                crate::recursion::try_enter_solver_frame()
                    .expect("test should reserve all but one solver frame")
            })
            .collect();
        assert!(matches!(
            substitute_exact_types_with_memo(&db, source, &[outer], &[TypeId::STRING]),
            Err(ExactRewriteAborted),
        ));
        assert!(db.get_display_properties(expected).is_none());
        drop(held_frames);

        let (result, mut memo) =
            substitute_exact_types_with_memo(&db, source, &[outer], &[TypeId::STRING])
                .expect("the same rewrite should retry under a fresh frame budget");
        assert_eq!(result, expected);
        assert_eq!(
            db.get_display_properties(result)
                .expect("the completed retry should commit provenance")[0]
                .type_id,
            TypeId::STRING,
        );

        let late_source = db.readonly_type(db.tuple(vec![TupleElement::fixed(outer)]));
        let late_result = db.readonly_type(db.tuple(vec![TupleElement::fixed(TypeId::STRING)]));
        db.store_display_properties(source, vec![PropertyInfo::new(shown, late_source)]);
        let mapped_before = memo.mapped.clone();
        let sources_before = memo.provenance_sources.clone();
        let roots_before = memo.root_results.clone();
        let generation_before = memo.provenance_generation;
        let held_frames: Vec<_> = (0..crate::recursion::MAX_SOLVER_STACK_FRAMES - 1)
            .map(|_| {
                crate::recursion::try_enter_solver_frame()
                    .expect("test should reserve all but one solver frame")
            })
            .collect();
        assert_eq!(memo.refresh_provenance(&db), Err(ExactRewriteAborted));
        assert_eq!(memo.mapped, mapped_before);
        assert_eq!(memo.provenance_sources, sources_before);
        assert_eq!(memo.root_results, roots_before);
        assert_eq!(memo.provenance_generation, generation_before);
        assert_eq!(
            db.get_display_properties(result)
                .expect("failed refresh must preserve prior target provenance")[0]
                .type_id,
            TypeId::STRING,
        );
        drop(held_frames);

        memo.refresh_provenance(&db)
            .expect("the provenance refresh should retry after frames unwind");
        assert_eq!(
            db.get_display_properties(result)
                .expect("successful retry should commit late provenance")[0]
                .type_id,
            late_result,
        );
    }
}
