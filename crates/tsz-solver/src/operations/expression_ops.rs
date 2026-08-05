//! Expression type computation operations.
//!
//! This module implements AST-agnostic type computation for expressions,
//! migrated from the Checker as part of the Solver-First architecture refactor.
//!
//! These functions operate purely on `TypeIds` and maintain no AST dependencies.

use crate::caches::db::QueryDatabase;
use crate::caches::subtype_reduction_cache::SubtypeReductionRequest;
use crate::construction::TypeDatabase;
use crate::relations::subtype::SubtypeChecker;
use crate::relations::subtype::TypeResolver;
use crate::relations::subtype::is_subtype_of;
use crate::types::{
    IntrinsicKind, ObjectFlags, PropertyInfo, TemplateSpan, TypeData, TypeId, Visibility,
};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

/// Computes the result type of a conditional expression: `condition ? true_branch : false_branch`.
///
/// # Arguments
/// * `interner` - The type database/interner
/// * `condition` - Type of the condition expression
/// * `true_type` - Type of the true branch (`when_true`)
/// * `false_type` - Type of the false branch (`when_false`)
///
/// # Returns
/// * If condition is definitely truthy: returns `true_type`
/// * If condition is definitely falsy: returns `false_type`
/// * Otherwise: returns union of `true_type` and `false_type`
pub fn compute_conditional_expression_type(
    interner: &dyn TypeDatabase,
    condition: TypeId,
    true_type: TypeId,
    false_type: TypeId,
) -> TypeId {
    compute_conditional_expression_type_with_resolver(
        interner,
        condition,
        true_type,
        false_type,
        None::<&crate::relations::subtype::NoopResolver>,
    )
}

/// Resolver-aware variant of [`compute_conditional_expression_type`].
///
/// A resolver lets subtype-reduction of the branch union see through alias /
/// application / mapped wrappers (e.g. `Record<string, unknown>` interns as a
/// `TypeData::Application`). Without it, the index signature that drives
/// `UnionReduction.Subtype` stays hidden behind the wrapper.
pub fn compute_conditional_expression_type_with_resolver<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    condition: TypeId,
    true_type: TypeId,
    false_type: TypeId,
    resolver: Option<&R>,
) -> TypeId {
    // Handle error propagation
    if condition == TypeId::ERROR {
        return TypeId::ERROR;
    }
    if true_type == TypeId::ERROR {
        return TypeId::ERROR;
    }
    if false_type == TypeId::ERROR {
        return TypeId::ERROR;
    }

    // Handle special type constants
    if condition == TypeId::ANY {
        // any ? A : B -> A | B
        if let Some(reduced) =
            reduce_fresh_empty_object_branch(interner, true_type, false_type, resolver)
        {
            return reduced;
        }
        let union = interner.union2(true_type, false_type);
        if let Some(res) = resolver {
            return reduce_class_subtype_union_members(interner, res, union);
        }
        return union;
    }
    if condition == TypeId::NEVER {
        // never ? A : B -> never (unreachable)
        return TypeId::NEVER;
    }

    // tsc always returns the union of both branch types, even when the
    // condition is a known literal boolean.  The checker already handles
    // diagnostic suppression for dead branches; the solver just computes
    // the result type as the union with subtype reduction.
    //
    // For null/undefined conditions, the false branch is still the
    // relevant type, but we union for consistency (never branches
    // disappear from unions automatically).
    //
    // Note: we do NOT short-circuit for literal true/false because
    // tsc's `checkConditionalExpression` always computes
    // `getUnionType([type1, type2], SubtypeReduction)`.

    // If both branches are the same type, no need for union
    if true_type == false_type {
        return true_type;
    }

    if let Some((adjusted_true, adjusted_false)) =
        complement_fresh_object_literal_union(interner, true_type, false_type)
    {
        return interner.union2(adjusted_true, adjusted_false);
    }

    if contains_unique_symbol(interner, true_type) || contains_unique_symbol(interner, false_type) {
        return interner.union_preserve_members(vec![true_type, false_type]);
    }

    if let Some(reduced) =
        reduce_fresh_empty_object_branch(interner, true_type, false_type, resolver)
    {
        return reduced;
    }

    // tsc computes `getUnionType([trueType, falseType], UnionReduction.Subtype)`,
    // which drops the empty array literal `[]` (`never[]`) in
    // `cond ? [1, 2, 3] : []` because it is a subtype of the sibling
    // `(1 | 2 | 3)[]`, leaving a single array that later widens to `number[]`.
    // Without this, the bare union keeps `never[]`, and a subsequent `.push`
    // contravariantly intersects the per-arm element types to `never`.
    //
    // Scoped to the empty-array constituent (`Array<never>` / empty tuple)
    // rather than general subtype reduction: full pairwise reduction over
    // structurally-related call signatures picks the wrong constituent (tsc's
    // `UnionReduction.Subtype` keeps the type that satisfies every member call,
    // not simply the structural supertype), so a broad reduction here regresses
    // `unionTypeReduction2`. The empty-array case is unambiguous because
    // `Array<never>` is a subtype of every `T[]`.
    if let Some(reduced) = reduce_empty_array_branch(interner, true_type, false_type) {
        return reduced;
    }

    let union = interner.union2(true_type, false_type);
    if let Some(res) = resolver {
        // `true ? new Base() : new Derived()` — the class-scoped slice of
        // UnionReduction.Subtype (heritage-guarded), which the broad
        // reduction above could not take without regressing
        // `unionTypeReduction2`.
        return reduce_class_subtype_union_members(interner, res, union);
    }
    union
}

/// Drop an empty array literal branch (`Array<never>` or the empty tuple `[]`)
/// from a conditional union when the sibling is an array/tuple it is a subtype
/// of, matching tsc's `UnionReduction.Subtype`. Returns the surviving sibling.
///
/// Limited to the empty-array constituent: `Array<never>` is unambiguously a
/// subtype of every `T[]`, so this never picks the wrong branch the way a
/// general pairwise subtype reduction would for structurally-related call
/// signatures.
fn reduce_empty_array_branch(
    interner: &dyn TypeDatabase,
    true_type: TypeId,
    false_type: TypeId,
) -> Option<TypeId> {
    // The sibling may be a bare array (`number[]`) or a union containing array
    // members (`Array<number> | Set<number>`, from `cond ? f<number>() : []`),
    // so the only guard is the subtype check: `Array<never>` is dropped exactly
    // when it is a subtype of the sibling. `cond ? 5 : []` keeps both members
    // because `never[]` is not a subtype of `number`.
    if is_empty_array_literal_type(interner, true_type)
        && !is_empty_array_literal_type(interner, false_type)
        && is_subtype_of(interner, true_type, false_type)
    {
        return Some(false_type);
    }
    if is_empty_array_literal_type(interner, false_type)
        && !is_empty_array_literal_type(interner, true_type)
        && is_subtype_of(interner, false_type, true_type)
    {
        return Some(true_type);
    }
    None
}

/// The empty array literal `[]` interns as `Array<never>` (an evolving-array
/// base in strict mode) or as the empty tuple. Either form is the bottom array
/// that subtype reduction folds into a sibling array.
fn is_empty_array_literal_type(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match interner.lookup(type_id) {
        Some(TypeData::Array(element)) => element == TypeId::NEVER,
        Some(TypeData::Tuple(list_id)) => interner.tuple_list(list_id).is_empty(),
        _ => false,
    }
}

/// tsc computes a conditional expression's type as
/// `getUnionType([trueType, falseType], UnionReduction.Subtype)`. A *fresh*
/// empty object literal `{}` is a strict subtype of any object type it is
/// assignable to (e.g. one with a string/number index signature, or with only
/// optional members), so subtype reduction drops it: `cond ? {} : rec` where
/// `rec: Record<string, unknown>` has type `Record<string, unknown>`, not
/// `{} | Record<string, unknown>`.
///
/// The freshness requirement mirrors tsc: a *declared* `{}` is the wide
/// empty-object supertype and survives reduction, so it is intentionally left
/// untouched here. A sibling that `{}` is not assignable to (e.g. `{ a: number }`,
/// which has a required property) is also preserved, because the fresh `{}` is
/// then the supertype rather than the subtype.
fn reduce_fresh_empty_object_branch<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    true_type: TypeId,
    false_type: TypeId,
    resolver: Option<&R>,
) -> Option<TypeId> {
    if is_fresh_empty_object_literal(interner, true_type)
        && !is_fresh_empty_object_literal(interner, false_type)
        && fresh_empty_is_subtype_of(interner, true_type, false_type, resolver)
    {
        return Some(false_type);
    }
    if is_fresh_empty_object_literal(interner, false_type)
        && !is_fresh_empty_object_literal(interner, true_type)
        && fresh_empty_is_subtype_of(interner, false_type, true_type, resolver)
    {
        return Some(true_type);
    }
    None
}

/// Decide whether a fresh empty `{}` is a subtype of `other`. `other` may be an
/// alias/application/mapped wrapper (e.g. `Record<string, unknown>` interns as a
/// `TypeData::Application`), so it is evaluated to its structural form first —
/// otherwise the index signature that makes `{}` a subtype stays hidden behind
/// the wrapper and the reduction is missed.
fn fresh_empty_is_subtype_of<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    empty: TypeId,
    other: TypeId,
    resolver: Option<&R>,
) -> bool {
    match resolver {
        Some(res) => {
            let resolved =
                crate::evaluation::evaluate::evaluate_type_with_resolver(interner, res, other);
            let mut checker = SubtypeChecker::with_resolver(interner, res);
            checker.is_subtype_of(empty, resolved)
        }
        None => {
            let resolved = crate::evaluation::evaluate::evaluate_type(interner, other);
            is_subtype_of(interner, empty, resolved)
        }
    }
}

/// A fresh empty object literal is `{}` written inline (carrying
/// `ObjectFlags::FRESH_LITERAL`) with no members and no index signatures.
/// Inspects the interned shape in place rather than cloning it, since this runs
/// for every conditional expression whose branch is a fresh object literal.
fn is_fresh_empty_object_literal(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    let shape_id = match interner.lookup(type_id) {
        Some(TypeData::Object(id) | TypeData::ObjectWithIndex(id)) => id,
        _ => return false,
    };
    let shape = interner.object_shape(shape_id);
    shape.flags.contains(ObjectFlags::FRESH_LITERAL)
        && shape.properties.is_empty()
        && shape.string_index.is_none()
        && shape.number_index.is_none()
        && shape.symbol.is_none()
}

fn contains_unique_symbol(interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
    match interner.lookup(type_id) {
        Some(TypeData::UniqueSymbol(_)) => true,
        Some(TypeData::Union(list_id)) => interner
            .type_list(list_id)
            .iter()
            .any(|&member| contains_unique_symbol(interner, member)),
        _ => false,
    }
}

pub fn normalize_object_union_members_for_write_target(
    interner: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Vec<TypeId>> {
    let mut object_members = Vec::with_capacity(members.len());
    let mut saw_fresh_member = false;

    for &member in members {
        if member.is_intrinsic() {
            return None;
        }
        let shape = fresh_literal_shape(interner, member).or_else(|| {
            let shape_id = match interner.lookup(member)? {
                TypeData::Object(id) | TypeData::ObjectWithIndex(id) => id,
                _ => return None,
            };
            Some((*interner.object_shape(shape_id)).clone())
        })?;
        if shape.flags.contains(ObjectFlags::FRESH_LITERAL) {
            saw_fresh_member = true;
        }
        if shape.symbol.is_some() || shape.string_index.is_some() || shape.number_index.is_some() {
            return None;
        }
        object_members.push((member, shape));
    }

    if !saw_fresh_member || object_members.len() < 2 {
        return None;
    }

    let mut all_props: Vec<PropertyInfo> = Vec::new();
    for (_, shape) in &object_members {
        for prop in &shape.properties {
            if !all_props.iter().any(|existing| existing.name == prop.name) {
                all_props.push(prop.clone());
            }
        }
    }

    if all_props.is_empty() {
        return None;
    }

    let mut changed = false;
    let mut normalized = Vec::with_capacity(object_members.len());
    for (original_type, mut shape) in object_members {
        let next_order = shape
            .properties
            .iter()
            .map(|p| p.declaration_order)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut append_order = next_order;

        for prop in &all_props {
            if shape
                .properties
                .iter()
                .any(|existing| existing.name == prop.name)
            {
                continue;
            }
            changed = true;
            let mut synthetic = PropertyInfo::opt(prop.name, TypeId::UNDEFINED);
            synthetic.declaration_order = append_order;
            append_order = append_order.saturating_add(1);
            shape.properties.push(synthetic);
        }

        if changed {
            shape.flags.remove(ObjectFlags::FRESH_LITERAL);
            let display_props =
                normalized_display_properties(interner, original_type, &shape.properties);
            let widened = interner.object_with_flags(shape.properties, shape.flags);
            interner.store_display_properties(widened, display_props);
            normalized.push(widened);
        } else {
            normalized.push(original_type);
        }
    }

    changed.then_some(normalized)
}

/// Merge a later object-spread property contribution with an earlier property.
///
/// This implements the AST-independent part of TypeScript's object spread merge
/// rule for a single property name:
/// - a later required property overrides the earlier contribution;
/// - a later optional property is unioned with the earlier contribution because
///   the runtime spread may omit it;
/// - when `exactOptionalPropertyTypes` is disabled, `undefined` is removed from
///   the later optional contribution before unioning with an earlier required
///   property.
pub fn merge_object_spread_property(
    db: &dyn TypeDatabase,
    exact_optional_property_types: bool,
    earlier: Option<&PropertyInfo>,
    spread: &PropertyInfo,
) -> PropertyInfo {
    let Some(earlier) = earlier else {
        return spread.clone();
    };

    if !spread.optional {
        return spread.clone();
    }

    let (spread_type, spread_write_type) = if !exact_optional_property_types && !earlier.optional {
        (
            crate::narrowing::utils::remove_undefined(db, spread.type_id),
            crate::narrowing::utils::remove_undefined(db, spread.write_type),
        )
    } else {
        (spread.type_id, spread.write_type)
    };

    PropertyInfo {
        name: spread.name,
        type_id: db.union2(earlier.type_id, spread_type),
        write_type: db.union2(earlier.write_type, spread_write_type),
        // Required wins on optionality.
        optional: earlier.optional && spread.optional,
        readonly: earlier.readonly && spread.readonly,
        is_method: spread.is_method,
        is_class_prototype: false,
        visibility: spread.visibility,
        parent_id: spread.parent_id,
        declaration_order: spread.declaration_order,
        is_string_named: spread.is_string_named,
        is_symbol_named: spread.is_symbol_named,
        single_quoted_name: spread.single_quoted_name,
        non_widening: spread.non_widening,
        declared_location: spread.declared_location,
    }
}

fn complement_fresh_object_literal_union(
    interner: &dyn TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> Option<(TypeId, TypeId)> {
    let normalized = normalize_fresh_object_literal_union_members(interner, &[left, right])?;
    if normalized.len() != 2 {
        return None;
    }
    Some((normalized[0], normalized[1]))
}

fn fresh_literal_shape(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::ObjectShape> {
    if type_id.is_intrinsic() {
        return None;
    }
    let shape_id = match interner.lookup(type_id)? {
        TypeData::Object(id) | TypeData::ObjectWithIndex(id) => id,
        _ => return None,
    };
    let shape = interner.object_shape(shape_id);
    if !shape.flags.contains(ObjectFlags::FRESH_LITERAL) {
        return None;
    }
    Some((*shape).clone())
}

#[derive(Clone, Copy)]
struct ContextPropertyOrigin {
    name: Atom,
    order: u32,
}

pub(crate) fn normalize_fresh_object_literal_union_members(
    interner: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Vec<TypeId>> {
    let mut object_members = Vec::with_capacity(members.len());

    for &member in members {
        let shape = fresh_literal_shape(interner, member)?;
        // Object literals containing spreads are not fresh, and open/symbol-backed
        // objects should not participate in this normalization.
        if shape.symbol.is_some() || shape.string_index.is_some() || shape.number_index.is_some() {
            return None;
        }
        object_members.push((member, shape));
    }

    if object_members.len() < 2 {
        return None;
    }

    // Collect each property's stable sibling-relative declaration origin.
    // `shape.properties` is Atom-sorted for canonical hashing, so first recover
    // each sibling's source order from `declaration_order`. A repeated name
    // overwrites its context prototype, matching tsc's `getPropertiesOfContext`
    // Map: a missing optional property inherits the declaration of the *last*
    // sibling that supplied that name.
    //
    // The rank is display-only. It is monotonic across the ordered sibling
    // list, so it reproduces the source-position comparison tsc applies to the
    // original and inherited declarations without changing property semantics.
    let mut context_properties: Vec<ContextPropertyOrigin> = Vec::new();
    let mut context_property_indices: FxHashMap<Atom, usize> = FxHashMap::default();
    let mut member_property_origins: Vec<FxHashMap<Atom, u32>> =
        Vec::with_capacity(object_members.len());
    let mut next_origin_order = 1_u32;

    for (_, shape) in &object_members {
        let mut props_by_decl: Vec<&PropertyInfo> = shape.properties.iter().collect();
        props_by_decl.sort_by_key(|p| p.declaration_order);
        let mut origins = FxHashMap::default();
        for prop in props_by_decl {
            let origin_order = next_origin_order;
            next_origin_order = next_origin_order.saturating_add(1);
            origins.insert(prop.name, origin_order);

            if let Some(&index) = context_property_indices.get(&prop.name) {
                context_properties[index].order = origin_order;
            } else {
                context_property_indices.insert(prop.name, context_properties.len());
                context_properties.push(ContextPropertyOrigin {
                    name: prop.name,
                    order: origin_order,
                });
            }
        }
        member_property_origins.push(origins);
    }

    if context_properties.is_empty() {
        return None;
    }

    let mut changed = false;
    let mut normalized = Vec::with_capacity(object_members.len());
    for ((original_type, shape), mut display_origins) in
        object_members.into_iter().zip(member_property_origins)
    {
        let mut completed = add_missing_optional_properties(&shape.properties, &context_properties);
        if completed != shape.properties {
            changed = true;
            for property in &context_properties {
                display_origins
                    .entry(property.name)
                    .or_insert(property.order);
            }
            // Capture source-order display properties before interning sorts the
            // canonical shape by Atom. Without this, the canonical shape may
            // dedupe to a previously-interned twin whose `declaration_order` is
            // zero, and the diagnostic printer falls back to Atom order — which
            // is non-deterministic across compilations and rarely matches the
            // source-written property order tsc preserves. Existing properties
            // keep their display-only literal types so normalized unions don't
            // repaint `{ c: true }` as `{ c: boolean }`.
            for prop in &mut completed {
                if let Some(&origin_order) = display_origins.get(&prop.name) {
                    prop.declaration_order = origin_order;
                }
            }
            crate::types::normalize_display_property_order(&mut completed);

            let display_props = normalized_display_properties(interner, original_type, &completed);
            // A normalized member can have the same semantic property shape in
            // multiple BCT contexts but a different inherited source order.
            // Give that order context-owned identity so a later partial
            // normalization cannot repaint another context's display side
            // table entry. Relations remain structural; the flag affects only
            // interning identity and diagnostics.
            let flags = shape.flags | ObjectFlags::PRESERVE_DECLARATION_ORDER;
            let new_type_id = interner.object_with_flags(completed, flags);
            interner.store_display_properties(new_type_id, display_props);
            normalized.push(new_type_id);
        } else {
            normalized.push(original_type);
        }
    }

    changed.then_some(normalized)
}

fn add_missing_optional_properties(
    existing: &[PropertyInfo],
    context_properties: &[ContextPropertyOrigin],
) -> Vec<PropertyInfo> {
    let mut out: Vec<PropertyInfo> = existing.to_vec();

    for property in context_properties {
        if out.iter().any(|p| p.name == property.name) {
            continue;
        }
        let mut prop = PropertyInfo::opt(property.name, TypeId::UNDEFINED);
        prop.declaration_order = property.order;
        out.push(prop);
    }
    out
}

type DisplayPropertyProvenance = (TypeId, TypeId, Option<SymbolId>);
type DisplayPropertiesByName = FxHashMap<Atom, DisplayPropertyProvenance>;

fn normalized_display_properties(
    interner: &dyn TypeDatabase,
    original_type: TypeId,
    normalized_properties: &[PropertyInfo],
) -> Vec<PropertyInfo> {
    let original_display = interner.get_display_properties(original_type);
    let original_display_by_name: Option<DisplayPropertiesByName> =
        original_display.as_ref().map(|props| {
            props
                .iter()
                .map(|prop| (prop.name, (prop.type_id, prop.write_type, prop.parent_id)))
                .collect()
        });
    let mut display_props: Vec<PropertyInfo> = normalized_properties
        .iter()
        .map(|prop| {
            let Some((display_type, display_write_type, display_parent_id)) =
                original_display_by_name
                    .as_ref()
                    .and_then(|props| props.get(&prop.name).copied())
            else {
                return prop.clone();
            };

            PropertyInfo {
                type_id: display_type,
                write_type: display_write_type,
                parent_id: display_parent_id,
                ..prop.clone()
            }
        })
        .collect();
    crate::types::normalize_display_property_order(&mut display_props);
    display_props
}

/// Computes the type of a template literal expression.
///
/// In TypeScript, template literal expressions produce:
/// - A concrete string literal type when all parts are literals (e.g., `hello ${42}` → "hello 42")
/// - A template literal type when in a template literal context (parameter expects template literal)
///   and parts include type parameters or other non-literal types
/// - `string` type otherwise
///
/// # Arguments
/// * `parts` - Slice of type IDs for each interpolated expression
///
/// # Returns
/// * `TypeId::STRING` - Template literals produce strings by default
pub fn compute_template_expression_type(
    db: &dyn TypeDatabase,
    texts: &[String],
    parts: &[TypeId],
) -> TypeId {
    // Check for error propagation
    for &part in parts {
        if part == TypeId::ERROR {
            return TypeId::ERROR;
        }
        if part == TypeId::NEVER {
            return TypeId::NEVER;
        }
    }

    // If all interpolated parts are literal types, produce a literal string type.
    // E.g., `abc${0}def` → "abc0def" when 0 has literal type 0.
    if !parts.is_empty() && texts.len() == parts.len() + 1 {
        let mut all_literal = true;
        let mut result = String::new();
        result.push_str(&texts[0]);

        for (i, &part) in parts.iter().enumerate() {
            if let Some(lit_atom) = crate::type_queries::get_string_literal_value(db, part) {
                result.push_str(&db.resolve_atom(lit_atom));
            } else if let Some(num) = crate::type_queries::get_number_literal_value(db, part) {
                result.push_str(&crate::utils::js_number_to_string(num));
            } else if part == TypeId::BOOLEAN_TRUE {
                result.push_str("true");
            } else if part == TypeId::BOOLEAN_FALSE {
                result.push_str("false");
            } else if part == TypeId::NULL {
                result.push_str("null");
            } else if part == TypeId::UNDEFINED {
                result.push_str("undefined");
            } else {
                all_literal = false;
                break;
            }
            result.push_str(&texts[i + 1]);
        }

        if all_literal {
            return db.literal_string(&result);
        }
    }

    if !parts.is_empty()
        && texts.len() == parts.len() + 1
        && parts
            .iter()
            .any(|&part| crate::type_queries::contains_type_parameters_db(db, part))
    {
        let mut spans = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            if !text.is_empty() {
                spans.push(TemplateSpan::Text(db.intern_string(text)));
            }
            if i < parts.len() {
                spans.push(TemplateSpan::Type(parts[i]));
            }
        }
        return db.template_literal(spans);
    }

    // Template literals produce string type by default
    TypeId::STRING
}

/// Computes the type of a template literal expression in a template literal context.
///
/// When the contextual type is a template literal type (e.g., parameter expects `` `${T}:${U}` ``),
/// the expression produces a template literal type instead of plain `string`.
///
/// # Arguments
/// * `db` - The type database for interning
/// * `texts` - The text parts of the template (head, middles, tail). Length = `parts.len()` + 1
/// * `parts` - The type of each interpolated expression
///
/// # Returns
/// A template literal type constructed from the texts and part types
pub fn compute_template_expression_type_contextual(
    db: &dyn TypeDatabase,
    texts: &[String],
    parts: &[TypeId],
) -> TypeId {
    // Check for error/never propagation
    for &part in parts {
        if part == TypeId::ERROR {
            return TypeId::ERROR;
        }
        if part == TypeId::NEVER {
            return TypeId::NEVER;
        }
    }

    // Build template spans: interleaved text and type parts
    let mut spans = Vec::new();
    for (i, text) in texts.iter().enumerate() {
        if !text.is_empty() {
            spans.push(TemplateSpan::Text(db.intern_string(text)));
        }
        if i < parts.len() {
            // For each interpolated part, check if it's assignable to the template constraint
            // (string | number | bigint | boolean | null | undefined).
            // If so, use the part type directly; otherwise widen to string.
            let part = template_substitution_type(db, parts[i]);
            spans.push(TemplateSpan::Type(part));
        }
    }

    db.template_literal(spans)
}

fn template_substitution_type(db: &dyn TypeDatabase, part: TypeId) -> TypeId {
    if template_substitution_type_is_valid(db, part, 0) {
        part
    } else {
        TypeId::STRING
    }
}

const MAX_TEMPLATE_CONTEXT_DEPTH: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateContextDepthState {
    Continue,
    LimitExceeded,
}

const fn template_context_depth_state(depth: u32) -> TemplateContextDepthState {
    if depth > MAX_TEMPLATE_CONTEXT_DEPTH {
        TemplateContextDepthState::LimitExceeded
    } else {
        TemplateContextDepthState::Continue
    }
}

fn template_substitution_type_is_valid(db: &dyn TypeDatabase, part: TypeId, depth: u32) -> bool {
    match template_context_depth_state(depth) {
        TemplateContextDepthState::Continue => {}
        TemplateContextDepthState::LimitExceeded => return false,
    }
    if matches!(
        part,
        TypeId::STRING
            | TypeId::NUMBER
            | TypeId::BIGINT
            | TypeId::BOOLEAN
            | TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::BOOLEAN_TRUE
            | TypeId::BOOLEAN_FALSE
    ) {
        return true;
    }
    if part.is_intrinsic() {
        return false;
    }

    match db.lookup(part) {
        Some(TypeData::Intrinsic(
            IntrinsicKind::String
            | IntrinsicKind::Number
            | IntrinsicKind::Bigint
            | IntrinsicKind::Boolean
            | IntrinsicKind::Null
            | IntrinsicKind::Undefined,
        ))
        | Some(TypeData::Literal(_))
        | Some(TypeData::TemplateLiteral(_)) => true,
        Some(TypeData::Union(list_id)) => db
            .type_list(list_id)
            .iter()
            .all(|&member| template_substitution_type_is_valid(db, member, depth + 1)),
        Some(TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&member| template_substitution_type_is_valid(db, member, depth + 1)),
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => {
            info.constraint.is_some_and(|constraint| {
                template_substitution_type_is_valid(db, constraint, depth + 1)
                    || template_substitution_constraint_is_dependent(db, constraint, depth + 1)
            })
        }
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            template_substitution_type_is_valid(db, inner, depth + 1)
        }
        _ => false,
    }
}

fn template_substitution_constraint_is_dependent(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: u32,
) -> bool {
    match template_context_depth_state(depth) {
        TemplateContextDepthState::Continue => {}
        TemplateContextDepthState::LimitExceeded => return false,
    }
    if type_id.is_intrinsic() {
        return false;
    }

    match db.lookup(type_id) {
        Some(
            TypeData::TypeParameter(_)
            | TypeData::Infer(_)
            | TypeData::Application(_)
            | TypeData::KeyOf(_)
            | TypeData::IndexAccess(_, _),
        ) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&member| template_substitution_constraint_is_dependent(db, member, depth + 1)),
        Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
            template_substitution_constraint_is_dependent(db, inner, depth + 1)
        }
        _ => false,
    }
}

/// Checks whether a type is or contains a template literal contextual type.
///
/// In tsc, this means: string literal, template literal, or an instantiable type
/// whose base constraint is a string literal/template literal, or a union/intersection
/// containing any of the above.
pub fn is_template_literal_contextual_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_template_literal_contextual_type_inner(db, type_id, 0)
}

fn is_template_literal_contextual_type_inner(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    depth: u32,
) -> bool {
    match template_context_depth_state(depth) {
        TemplateContextDepthState::Continue => {}
        TemplateContextDepthState::LimitExceeded => return false,
    }
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(
            TypeData::Literal(crate::types::LiteralValue::String(_)) | TypeData::TemplateLiteral(_),
        ) => true,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members
                .iter()
                .any(|m| is_template_literal_contextual_type_inner(db, *m, depth + 1))
        }
        _ => false,
    }
}

/// Computes the best common type (BCT) of a set of types.
///
/// This is used for array literal type inference and other contexts
/// where a single type must be inferred from multiple candidates.
///
/// # Arguments
/// * `interner` - The type database/interner
/// * `types` - Slice of type IDs to find the best common type of
/// * `resolver` - Optional `TypeResolver` for nominal hierarchy lookups (class inheritance)
///
/// # Returns
/// * Empty slice: Returns `TypeId::NEVER`
/// * Single type: Returns that type
/// * All same type: Returns that type
/// * Otherwise: Returns union of all types (or common base class if available)
///
/// # Note
/// When `resolver` is provided, this implements the full TypeScript BCT algorithm:
/// - Find the first candidate that is a supertype of all others
/// - Handle literal widening (via `TypeChecker`'s pre-widening)
/// - Handle base class relationships (Dog + Cat -> Animal)
pub fn compute_best_common_type<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: Option<&R>,
) -> TypeId {
    compute_best_common_type_cached(interner, None, types, resolver)
}

/// Cache-aware variant of [`compute_best_common_type`].
///
/// `query_db = Some(db)` enables the cross-call subtype-reduction cache on
/// `QueryCache`. The cache mirrors tsc's `subtypeReductionCache`
/// (`TypeScript/src/compiler/checker.ts:18128-18132`) and collapses the
/// O(N²) subtype loop in `remove_subtypes_for_bct` to O(1) when the same
/// candidate list shows up at multiple call sites in the same checker
/// pass.
///
/// All non-`remove_subtypes_for_bct` work happens before any cache probe
/// so the leaf fast paths (single-type, all-same, error/any propagation,
/// unit-type fast path, enum widening, etc.) remain allocation-free.
pub fn compute_best_common_type_cached<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    types: &[TypeId],
    resolver: Option<&R>,
) -> TypeId {
    // Handle empty cases
    if types.is_empty() {
        return TypeId::NEVER;
    }

    // Propagate errors
    for &ty in types {
        if ty == TypeId::ERROR {
            return TypeId::ERROR;
        }
        if ty == TypeId::ANY {
            return TypeId::ANY;
        }
    }

    // Single type: return it directly
    if types.len() == 1 {
        return types[0];
    }

    // If all types are the same, no need for union
    let first = types[0];
    if types.iter().all(|&ty| ty == first) {
        return first;
    }

    // Step 1: Apply literal widening for array literals
    // When we have multiple literal types of the same primitive kind, widen to the primitive
    // Example: [1, 2] -> number[], ["a", "b"] -> string[]
    let widened = widen_literals(interner, types);

    // Fresh plain object literals widen as a normalized union, not as the single
    // structural supertype candidate. Without this, `[ {a:0}, {a:1,b:"x"} ]`
    // collapses to `{ a: number }`, losing optionalized properties and causing
    // downstream TS2339/TS2353 drift.
    if let Some(normalized) = normalize_fresh_object_literal_union_members(interner, &widened) {
        let origin_members = normalized.clone();
        let result = interner.union(normalized);
        interner.store_union_origin(result, origin_members);
        return result;
    }

    // Constructor-valued arrays should preserve member unions. Collapsing
    // `[Concrete, Abstract]` to a single structurally-compatible constructor
    // loses abstractness and changes downstream `new` diagnostics inside
    // callbacks like `.map(cls => new cls())`.
    if widened.len() > 1
        && widened
            .iter()
            .all(|&ty| is_constructor_like(interner, ty, resolver))
    {
        return interner.union(widened);
    }

    // Step 1.5: Enum member widening
    // If all candidates are enum members from the same parent enum,
    // infer the parent enum type directly instead of a large union of members.
    // This matches TypeScript's behavior for expressions like [E.A, E.B] -> E[].
    if let Some(res) = resolver
        && let Some(common_enum_type) = common_parent_enum_type(interner, &widened, res)
    {
        return common_enum_type;
    }

    // OPTIMIZATION: Unit-type fast-path
    // If ALL types are unit types (tuples of literals/enums, or literals themselves),
    // no single type can be a supertype of the others (identity-comparable types are disjoint).
    // Skip the O(N²) subtype loop and go directly to union creation.
    // This turns O(N²) into O(N) for cases like enumLiteralsSubtypeReduction.ts
    // which has 500 distinct enum-tuple return types.
    if widened.len() > 2 {
        let all_unit = widened
            .iter()
            .all(|&ty| interner.is_identity_comparable_type(ty));
        if all_unit {
            // All identity-comparable types -> no common supertype exists, create union
            return interner.union(widened);
        }
    }

    // Preserve nullish members in best-common-type results. The subtype-based
    // tournament below can otherwise collapse `[T, undefined]` to `T`
    // (and `[T, null]` to `T`), which masks strict-null and overload failures
    // that should still see the nullable member.
    let has_nullable_member = widened
        .iter()
        .copied()
        .any(|ty| ty.is_nullable() || crate::narrowing::remove_nullish(interner, ty) != ty);
    let has_non_nullable_member = widened.iter().copied().any(|ty| {
        let non_nullish = crate::narrowing::remove_nullish(interner, ty);
        non_nullish != TypeId::NEVER
    });
    if has_nullable_member && has_non_nullable_member {
        return interner.union(widened);
    }

    // Resolver-backed class/interface BCT: sibling class instances that share
    // an `extends` chain should collapse to the nearest common base before we
    // enter the tournament and fallback subtype-reduction paths. This matches
    // the solver inference BCT path and avoids doing O(N²) pairwise relation
    // work for wide sibling-class arrays such as `BCT candidates=200`.
    if let Some(res) = resolver
        && let Some(common_base) = common_base_class_for_bct(interner, &widened, res)
    {
        return common_base;
    }

    // If every object candidate has a required primitive field name that no
    // sibling has, no candidate can be a supertype of all siblings: every other
    // sibling is missing that required field. The fallback subtype-reduction
    // pass would keep the same list, so skip both the tournament and the
    // fallback pairwise walk for wide disjoint object candidate sets.
    if bct_candidates_proven_pairwise_incomparable_by_unique_required_fields(interner, &widened) {
        if resolver.is_some() {
            let reduced = remove_subtypes_for_bct(interner, query_db, &widened, resolver);
            return interner.union_from_slice(&reduced);
        }
        return interner.union(widened);
    }

    // Step 2: Find the best common type from the candidate types
    // TypeScript rule: The best common type must be one of the input types
    // For example: [Dog, Cat] -> Dog | Cat (NOT Animal, even if both extend Animal)
    //              [Dog, Animal] -> Animal (Animal is in the set and is a supertype)
    //
    // OPTIMIZATION: Tournament-style O(N) reduction instead of O(N²) brute-force.
    // Pass 1 (O(N)): Find the "tournament winner" — iterate through candidates,
    //   replacing `best` whenever we find a candidate that STRICTLY dominates
    //   it (is a supertype but not also a subtype). Two candidates can be
    //   MUTUALLY related only through `any`'s absorption (e.g. `any[]` and
    //   `any[][]` are each a subtype of the other, since `any` is compatible
    //   with any element type); a plain "replace on any relation" tournament
    //   would then drift to whichever candidate appears LAST, but tsc keeps
    //   the first: `[[[null]],[undefined]]`'s sibling elements widen to
    //   `any[][]` and `any[]`, and tsc's `arrayLiteralWidened.ts` witness
    //   keeps `any[][]` (the first-declared element), not `any[]`.
    // Pass 2 (O(N)): Verify the winner is truly a supertype of ALL types.
    // Total: O(2N) instead of O(N²). For 50 candidates: 100 checks vs 2,500.
    //
    // We handle the two cases (with/without resolver) separately because SubtypeChecker<R>
    // and SubtypeChecker<NoopResolver> are different types.
    if let Some(res) = resolver {
        let mut checker = SubtypeChecker::with_resolver(interner, res);
        // tsc's removeSubtypes class guard, applied to the tournament: one
        // class reference only counts as a subtype of another for BCT
        // purposes when it genuinely derives from it. Without this,
        // `[new A(), new B()]` for unrelated same-shape classes crowns one of
        // them BCT winner where tsc keeps `(A | B)[]`.
        let related = |checker: &mut SubtypeChecker<_>, source: TypeId, target: TypeId| {
            if let (Some(src_def), Some(tgt_def)) = (
                class_ref_def(interner, res, source),
                class_ref_def(interner, res, target),
            ) && !class_derives_from(interner, res, source, src_def, target, tgt_def)
            {
                return false;
            }
            checker.guard.reset();
            checker.is_subtype_of(source, target)
        };
        // Pass 1: Tournament to find potential best candidate
        let mut best = widened[0];
        for &candidate in &widened[1..] {
            if related(&mut checker, best, candidate) && !related(&mut checker, candidate, best) {
                best = candidate;
            }
        }
        // Pass 2: Verify the winner is supertype of all
        let is_supertype = widened.iter().all(|&ty| related(&mut checker, ty, best));
        if is_supertype {
            return best;
        }
    } else {
        let mut checker = SubtypeChecker::new(interner);
        // Pass 1: Tournament to find potential best candidate
        let mut best = widened[0];
        for &candidate in &widened[1..] {
            checker.guard.reset();
            let best_to_candidate = checker.is_subtype_of(best, candidate);
            checker.guard.reset();
            let candidate_to_best = checker.is_subtype_of(candidate, best);
            if best_to_candidate && !candidate_to_best {
                best = candidate;
            }
        }
        // Pass 2: Verify the winner is supertype of all
        let is_supertype = widened.iter().all(|&ty| {
            checker.guard.reset();
            checker.is_subtype_of(ty, best)
        });
        if is_supertype {
            return best;
        }
    }

    // Step 3: Try to find a common base type for primitives/literals
    // For example, [string, "hello"] -> string
    if let Some(base) =
        crate::utils::find_common_base_type(&widened, |ty| get_base_type(interner, ty))
    {
        // All types share a common base type
        if all_types_are_narrower_than_base(interner, &widened, base) {
            return base;
        }
    }

    // Step 3.5: Remove subtypes before creating the fallback union.
    //
    // This matches tsc's UnionReduction.Subtype behavior used when computing
    // array literal element types: if A <: B, then A is redundant in A | B.
    // Example: [new C(), new C2(), new D<string>()] where C2 extends C
    //   → C2 <: C, so C2 is removed → union becomes C | D<string>.
    //
    // The interner's normalize_union/reduce_union_subtypes uses only a shallow
    // subtype check that cannot resolve Lazy types (class instances). Here we
    // use the full SubtypeChecker which handles class inheritance, generic
    // instantiations, and other relationships that require type resolution.
    let reduced = remove_subtypes_for_bct(interner, query_db, &widened, resolver);

    // Step 4: Default to union of all types
    interner.union_from_slice(&reduced)
}

fn common_base_class_for_bct<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: &R,
) -> Option<TypeId> {
    if types.len() < 2 {
        return None;
    }

    let mut candidates = nominal_hierarchy_for_bct(interner, types[0], resolver)?;
    // A single-entry hierarchy means the first candidate has no resolver-visible
    // base. Existing tournament/subtype-reduction logic handles those cases.
    if candidates.len() <= 1 {
        return None;
    }

    for &ty in types.iter().skip(1) {
        candidates.retain(|&candidate| nominally_extends_or_is(interner, ty, candidate, resolver));
        if candidates.is_empty() {
            return None;
        }
    }

    // `nominal_hierarchy_for_bct` is ordered most-derived to most-base, so the
    // first surviving candidate is the nearest common base.
    candidates.first().copied()
}

fn nominal_hierarchy_for_bct<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    resolver: &R,
) -> Option<Vec<TypeId>> {
    let mut hierarchy = Vec::new();
    let mut current = type_id;
    for _ in 0..32 {
        if hierarchy.contains(&current) {
            break;
        }
        hierarchy.push(current);
        let Some(base) = resolver.get_base_type(current, interner) else {
            break;
        };
        current = base;
    }

    (!hierarchy.is_empty()).then_some(hierarchy)
}

/// Identify a *class reference* union member: a `Lazy` ref whose def is a
/// class, a generic instantiation (`Application`) whose head is a class, or a
/// materialized class instance type registered with the resolver. This is the
/// tsz equivalent of tsc's `getObjectFlags(getTargetType(type)) & ObjectFlags.Class`
/// test inside `removeSubtypes`.
fn class_ref_def<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    type_id: TypeId,
) -> Option<crate::def::DefId> {
    let lazy_class = |ty: TypeId| match interner.lookup(ty) {
        Some(TypeData::Lazy(def_id)) => {
            (resolver.get_def_kind(def_id) == Some(crate::def::DefKind::Class)).then_some(def_id)
        }
        _ => None,
    };
    match interner.lookup(type_id) {
        Some(TypeData::Lazy(_)) => lazy_class(type_id),
        Some(TypeData::Application(app_id)) => lazy_class(interner.type_application(app_id).base),
        _ => resolver.class_def_for_instance_type(type_id),
    }
}

/// tsc's `isTypeDerivedFrom` for two *class references*: does `source` extend
/// `target` through its heritage chain?
///
/// Tries the instance-`TypeId` chain first (`get_base_type`, which covers
/// materialized instance types and non-generic `Lazy` refs), then the `DefId`
/// `extends` chain. The def-level hit deliberately ignores the target's type
/// arguments — tsc's `isTypeDerivedFrom` compares against
/// `getTargetType(target)` (argument-erased), and the strict-subtype half of
/// the removal conjunction is what rejects `GD extends GB<string>` against a
/// `GB<number>` sibling.
fn class_derives_from<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    source: TypeId,
    source_def: crate::def::DefId,
    target: TypeId,
    target_def: crate::def::DefId,
) -> bool {
    if nominally_extends_or_is(interner, source, target, resolver) {
        return true;
    }
    let mut current = source_def;
    for _ in 0..32 {
        let Some(parent) = resolver.get_class_extends(current) else {
            return false;
        };
        if parent == target_def {
            return true;
        }
        if parent == current {
            return false;
        }
        current = parent;
    }
    false
}

/// tsc `UnionReduction.Subtype` (checker.ts `removeSubtypes`) over an
/// already-constructed union, for member pairs the interner's shallow engine
/// treats as inert.
///
/// The default union constructor skips subtype reduction whenever a member is
/// `TypeData::Lazy` (class instance refs stay deferred), so `Base | Derived`
/// survives at tsc's Subtype-reduction sites (return-type-from-body unions,
/// conditional expressions). This entry point re-runs the pairwise sweep with
/// the resolver-backed `SubtypeChecker`, restricted to *class-reference
/// sources*: a source member is dropped when it is a subtype of a sibling AND
/// — when the sibling is itself a class reference — the source really derives
/// from it (tsc's `isTypeDerivedFrom` guard), so unrelated same-shape classes
/// coexist while `Derived extends Base` collapses to `Base`. Non-class
/// sources (interfaces, object literals, primitives) are left untouched: the
/// full-sweep variant historically picked the wrong constituent for
/// structurally-related call-signature unions (`unionTypeReduction2`).
///
/// Plain union type annotations (`var x: Base | Derived`) never route here;
/// only explicit Subtype-reduction call sites do — tsc's plain `getUnionType`
/// does not subtype-reduce either.
pub fn reduce_class_subtype_union_members<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    resolver: &R,
    union: TypeId,
) -> TypeId {
    let Some(TypeData::Union(list_id)) = interner.lookup(union) else {
        return union;
    };
    let members: Vec<TypeId> = interner.type_list(list_id).to_vec();
    let len = members.len();
    // Mirror tsc's removeSubtypes pairwise-iteration cap.
    if (len as u64) * (len as u64 - 1) >= 1_000_000 {
        return union;
    }
    let class_defs: Vec<Option<crate::def::DefId>> = members
        .iter()
        .map(|&m| class_ref_def(interner, resolver, m))
        .collect();
    tracing::trace!(
        ?members,
        ?class_defs,
        member_data = ?members.iter().map(|&m| interner.lookup(m)).collect::<Vec<_>>(),
        "reduce_class_subtype_union_members: entry"
    );
    if class_defs.iter().all(Option::is_none) {
        return union;
    }
    let mut keep = vec![true; len];
    let mut removed = false;
    let mut checker = SubtypeChecker::with_resolver(interner, resolver);
    for i in 0..len {
        if class_defs[i].is_none() {
            continue;
        }
        for j in 0..len {
            if i == j || !keep[j] {
                continue;
            }
            // Class-vs-class removal requires genuine heritage derivation,
            // not mere structural subtyping (tsc's isTypeDerivedFrom guard).
            if let (Some(src_def), Some(tgt_def)) = (class_defs[i], class_defs[j])
                && !class_derives_from(interner, resolver, members[i], src_def, members[j], tgt_def)
            {
                tracing::trace!(
                    src = ?members[i], tgt = ?members[j], ?src_def, ?tgt_def,
                    "reduce_class_subtype_union_members: derivation guard blocked"
                );
                continue;
            }
            checker.guard.reset();
            let subtype = checker.is_subtype_of(members[i], members[j]);
            tracing::trace!(
                src = ?members[i], tgt = ?members[j],
                src_data = ?interner.lookup(members[i]), tgt_data = ?interner.lookup(members[j]),
                src_class = ?class_defs[i], tgt_class = ?class_defs[j], subtype,
                "reduce_class_subtype_union_members: pair"
            );
            if subtype {
                keep[i] = false;
                removed = true;
                break;
            }
        }
    }
    if !removed {
        return union;
    }
    let kept: Vec<TypeId> = members
        .iter()
        .zip(keep.iter())
        .filter(|&(_, &k)| k)
        .map(|(&t, _)| t)
        .collect();
    interner.union(kept)
}

fn nominally_extends_or_is<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    resolver: &R,
) -> bool {
    if source == target {
        return true;
    }

    let mut current = source;
    for _ in 0..32 {
        let Some(base) = resolver.get_base_type(current, interner) else {
            return false;
        };
        if base == target {
            return true;
        }
        if base == current {
            return false;
        }
        current = base;
    }
    false
}

/// Remove subtypes from a type list using the full `SubtypeChecker`.
///
/// For each pair (i, j), if types[i] <: types[j] and i != j, types[i] is
/// redundant in the union and is removed. This matches tsc's `removeSubtypes`
/// used with `UnionReduction.Subtype` for array literal element types.
///
/// The interner's `reduce_union_subtypes` uses a shallow subtype check that
/// cannot handle class inheritance (it requires exact symbol equality for
/// nominal types). This function uses the full `SubtypeChecker` which correctly
/// resolves class hierarchies (e.g., C2 extends C → C2 <: C).
///
/// Uses O(N²) pairwise checks but N is typically small (array literal element count).
fn remove_subtypes_for_bct<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    query_db: Option<&dyn QueryDatabase>,
    types: &[TypeId],
    resolver: Option<&R>,
) -> Arc<[TypeId]> {
    if types.len() <= 1 {
        return Arc::from(types.to_vec());
    }

    // Guard: skip reduction for very large type lists to avoid O(N²) blowup.
    // tsc's removeSubtypes caps at 1,000,000 pairwise iterations.
    let len = types.len();
    if (len as u64) * (len as u64 - 1) >= 1_000_000 {
        return Arc::from(types.to_vec());
    }

    // Cross-call cache probe (mirrors tsc's `subtypeReductionCache`). The
    // typed request owns option-sensitive key construction, including the
    // nominal hierarchy option enabled when a resolver is available.
    let cache_key = query_db.map(|_| {
        SubtypeReductionRequest::new(types)
            .with_nominal_hierarchy_resolution(resolver.is_some())
            .cache_key()
    });
    if let (Some(db), Some(key)) = (query_db, cache_key.as_ref())
        && let Some(hit) = db.lookup_subtype_reduction_cache(key)
    {
        return hit;
    }

    if bct_candidates_proven_pairwise_incomparable_by_unique_required_fields(interner, types) {
        let result: Arc<[TypeId]> = Arc::from(types.to_vec());
        if let (Some(db), Some(key)) = (query_db, cache_key) {
            db.insert_subtype_reduction_cache(key, result.clone());
        }
        return result;
    }

    let result: Arc<[TypeId]> = if let Some(res) = resolver {
        let mut keep = vec![true; len];
        let class_defs: Vec<Option<crate::def::DefId>> = types
            .iter()
            .map(|&t| class_ref_def(interner, res, t))
            .collect();
        let mut checker = SubtypeChecker::with_resolver(interner, res);
        for i in 0..len {
            if !keep[i] {
                continue;
            }
            for j in 0..len {
                if i == j || !keep[j] {
                    continue;
                }
                // tsc's removeSubtypes class guard: when BOTH sides are class
                // references, structural subtyping alone must not drop a
                // member — removal additionally requires heritage derivation
                // (isTypeDerivedFrom), so unrelated same-shape classes keep
                // producing `(A | B)[]` from `[new A(), new B()]`.
                if let (Some(src_def), Some(tgt_def)) = (class_defs[i], class_defs[j])
                    && !class_derives_from(interner, res, types[i], src_def, types[j], tgt_def)
                {
                    continue;
                }
                checker.guard.reset();
                if checker.is_subtype_of(types[i], types[j]) {
                    // types[i] <: types[j], so types[i] is redundant
                    keep[i] = false;
                    break;
                }
            }
        }
        let kept: Vec<TypeId> = types
            .iter()
            .zip(keep.iter())
            .filter(|&(_, &k)| k)
            .map(|(&t, _)| t)
            .collect();
        Arc::from(kept)
    } else {
        let mut keep = vec![true; len];
        let mut checker = SubtypeChecker::new(interner);
        for i in 0..len {
            if !keep[i] {
                continue;
            }
            for j in 0..len {
                if i == j || !keep[j] {
                    continue;
                }
                checker.guard.reset();
                if checker.is_subtype_of(types[i], types[j]) {
                    keep[i] = false;
                    break;
                }
            }
        }
        let kept: Vec<TypeId> = types
            .iter()
            .zip(keep.iter())
            .filter(|&(_, &k)| k)
            .map(|(&t, _)| t)
            .collect();
        Arc::from(kept)
    };

    if let (Some(db), Some(key)) = (query_db, cache_key) {
        db.insert_subtype_reduction_cache(key, result.clone());
    }
    result
}

fn bct_candidates_proven_pairwise_incomparable_by_unique_required_fields(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
) -> bool {
    if types.len() <= 1 {
        return true;
    }

    let mut shapes = Vec::with_capacity(types.len());
    let mut property_counts = FxHashMap::default();

    for &type_id in types {
        let shape_id = match interner.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => shape_id,
            _ => return false,
        };
        let shape = interner.object_shape(shape_id);
        if shape.string_index.is_some() || shape.number_index.is_some() {
            return false;
        }
        for property in &shape.properties {
            *property_counts.entry(property.name).or_insert(0usize) += 1;
        }
        shapes.push(shape);
    }

    shapes.iter().all(|shape| {
        shape.properties.iter().any(|property| {
            property_can_prove_missing_required_field(property)
                && property_counts.get(&property.name).copied() == Some(1)
        })
    })
}

fn property_can_prove_missing_required_field(property: &PropertyInfo) -> bool {
    !property.optional
        && !property.is_method
        && !property.is_class_prototype
        && property.visibility == Visibility::Public
        && property_type_can_prove_object_base_incompatible(property.type_id)
}

const fn property_type_can_prove_object_base_incompatible(type_id: TypeId) -> bool {
    matches!(
        type_id,
        TypeId::BOOLEAN
            | TypeId::NUMBER
            | TypeId::STRING
            | TypeId::BIGINT
            | TypeId::SYMBOL
            | TypeId::BOOLEAN_TRUE
            | TypeId::BOOLEAN_FALSE
    )
}

fn is_constructor_like<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    type_id: TypeId,
    resolver: Option<&R>,
) -> bool {
    fn inner<R: TypeResolver>(
        interner: &dyn TypeDatabase,
        type_id: TypeId,
        resolver: Option<&R>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) -> bool {
        use crate::TypeData;

        if !visited.insert(type_id) {
            return false;
        }
        // Intrinsics are never Function/Callable/Application/Union/Intersection/Lazy.
        if type_id.is_intrinsic() {
            return false;
        }

        match interner.lookup(type_id) {
            Some(TypeData::Function(fn_id)) => interner.function_shape(fn_id).is_constructor,
            Some(TypeData::Callable(callable_id)) => !interner
                .callable_shape(callable_id)
                .construct_signatures
                .is_empty(),
            Some(TypeData::Application(app_id)) => {
                let app = interner.type_application(app_id);
                inner(interner, app.base, resolver, visited)
            }
            Some(TypeData::TypeParameter(info)) | Some(TypeData::Infer(info)) => info
                .constraint
                .is_some_and(|constraint| inner(interner, constraint, resolver, visited)),
            Some(TypeData::Union(list_id)) | Some(TypeData::Intersection(list_id)) => interner
                .type_list(list_id)
                .iter()
                .all(|&member| inner(interner, member, resolver, visited)),
            Some(TypeData::Lazy(def_id)) => resolver
                .and_then(|resolver| resolver.resolve_lazy(def_id, interner))
                .is_some_and(|resolved| {
                    resolved != type_id && inner(interner, resolved, resolver, visited)
                }),
            Some(TypeData::TypeQuery(sym_ref)) => resolver
                .and_then(|resolver| resolver.resolve_symbol_ref(sym_ref, interner))
                .is_some_and(|resolved| {
                    resolved != type_id && inner(interner, resolved, resolver, visited)
                }),
            _ => false,
        }
    }

    inner(
        interner,
        type_id,
        resolver,
        &mut rustc_hash::FxHashSet::default(),
    )
}

/// Widen literal types to their primitive base types when appropriate.
///
/// This implements Rule #10 (Literal Widening) for BCT:
/// - Fresh literals in arrays are widened to their primitive types
/// - Example: [1, 2] -> [number, number]
/// - Example: ["a", "b"] -> [string, string]
/// - Example: [1, "a"] -> [number, string] (mixed types)
///
/// The widening happens for each literal individually, even in mixed arrays.
/// Non-literal types are preserved as-is.
fn widen_literals(interner: &dyn TypeDatabase, types: &[TypeId]) -> Vec<TypeId> {
    // Widen each literal individually, regardless of what else is in the list.
    // This matches TypeScript's behavior where [1, "a"] infers as (number | string)[]
    types
        .iter()
        .map(|&ty| {
            // BOOLEAN_TRUE/FALSE are intrinsic IDs that resolve to Literal(Boolean).
            if ty == TypeId::BOOLEAN_TRUE || ty == TypeId::BOOLEAN_FALSE {
                return TypeId::BOOLEAN;
            }
            if ty.is_intrinsic() {
                return ty;
            }
            if let Some(crate::types::TypeData::Literal(ref lit)) = interner.lookup(ty) {
                return lit.primitive_type_id();
            }
            ty // Non-literal types are preserved
        })
        .collect()
}

/// Get the base type of a type (for literals, this is the primitive type).
fn get_base_type(interner: &dyn TypeDatabase, ty: TypeId) -> Option<TypeId> {
    if ty == TypeId::BOOLEAN_TRUE || ty == TypeId::BOOLEAN_FALSE {
        return Some(TypeId::BOOLEAN);
    }
    if ty.is_intrinsic() {
        return Some(ty);
    }
    match interner.lookup(ty) {
        Some(crate::types::TypeData::Literal(ref lit)) => Some(lit.primitive_type_id()),
        _ => Some(ty),
    }
}

/// Check if all types are narrower than (subtypes of) the given base type.
fn all_types_are_narrower_than_base(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    base: TypeId,
) -> bool {
    types.iter().all(|&ty| is_subtype_of(interner, ty, base))
}

/// Return the common parent enum type if all candidates are members of the same enum.
fn common_parent_enum_type<R: TypeResolver>(
    interner: &dyn TypeDatabase,
    types: &[TypeId],
    resolver: &R,
) -> Option<TypeId> {
    let mut parent_def = None;

    for &ty in types {
        let TypeData::Enum(def_id, _) = interner.lookup(ty)? else {
            return None;
        };

        let current_parent = resolver.get_enum_parent_def_id(def_id).unwrap_or(def_id);
        if let Some(existing) = parent_def {
            if existing != current_parent {
                return None;
            }
        } else {
            parent_def = Some(current_parent);
        }
    }

    let parent_def = parent_def?;
    resolver
        .resolve_lazy(parent_def, interner)
        .or_else(|| Some(interner.lazy(parent_def)))
}

#[cfg(test)]
#[path = "../../tests/expression_ops_tests.rs"]
mod tests;
