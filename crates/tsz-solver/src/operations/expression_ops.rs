//! Expression type computation operations.
//!
//! This module implements AST-agnostic type computation for expressions,
//! migrated from the Checker as part of the Solver-First architecture refactor.
//!
//! These functions operate purely on `TypeIds` and maintain no AST dependencies.

use crate::TypeDatabase;
use crate::TypeResolver;
use crate::is_subtype_of;
use crate::relations::subtype::SubtypeChecker;
use crate::types::{ObjectFlags, PropertyInfo, TemplateSpan, TypeData, TypeId};
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
        return interner.union2(true_type, false_type);
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

    interner.union2(true_type, false_type)
}

pub fn normalize_object_union_members_for_write_target(
    interner: &dyn TypeDatabase,
    members: &[TypeId],
) -> Option<Vec<TypeId>> {
    let mut object_members = Vec::with_capacity(members.len());
    let mut saw_fresh_member = false;

    for &member in members {
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
            let widened = interner.object_with_flags(shape.properties, shape.flags);
            if let Some(display_props) = interner.get_display_properties(original_type) {
                interner.store_display_properties(widened, display_props.as_ref().clone());
            }
            normalized.push(widened);
        } else {
            normalized.push(original_type);
        }
    }

    changed.then_some(normalized)
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

    let mut names: Vec<Atom> = Vec::new();
    for (_, shape) in &object_members {
        for prop in &shape.properties {
            if !names.contains(&prop.name) {
                names.push(prop.name);
            }
        }
    }

    if names.is_empty() {
        return None;
    }

    let mut changed = false;
    let mut normalized = Vec::with_capacity(object_members.len());
    for (original_type, shape) in object_members {
        // Don't enrich empty objects (`{}`) with optional properties from other
        // members. `{}` is already a valid structural supertype — adding phantom
        // `{ foo?: undefined }` properties alters its identity and breaks downstream
        // checks (e.g. TS2352 overlap, BCT display). tsc keeps `{}` as-is.
        if shape.properties.is_empty() {
            normalized.push(original_type);
            continue;
        }
        let completed = add_missing_optional_properties(&shape.properties, &names);
        if completed != shape.properties {
            changed = true;
            normalized.push(interner.object_with_flags(completed, shape.flags));
        } else {
            normalized.push(original_type);
        }
    }

    changed.then_some(normalized)
}

fn add_missing_optional_properties(existing: &[PropertyInfo], names: &[Atom]) -> Vec<PropertyInfo> {
    let mut out: Vec<PropertyInfo> = existing.to_vec();
    let mut next_order = out
        .iter()
        .map(|p| p.declaration_order)
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    for &name in names {
        if out.iter().any(|p| p.name == name) {
            continue;
        }
        let mut prop = PropertyInfo::opt(name, TypeId::UNDEFINED);
        prop.declaration_order = next_order;
        next_order = next_order.saturating_add(1);
        out.push(prop);
    }
    out
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
                if num.fract() == 0.0 && num.abs() < 1e15 {
                    let n = num as i64;
                    result.push_str(&format!("{n}"));
                } else {
                    result.push_str(&format!("{num}"));
                }
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
            let part = parts[i];
            spans.push(TemplateSpan::Type(part));
        }
    }

    db.template_literal(spans)
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
    if depth > 10 {
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
        return interner.union(normalized);
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

    // Step 2: Find the best common type from the candidate types
    // TypeScript rule: The best common type must be one of the input types
    // For example: [Dog, Cat] -> Dog | Cat (NOT Animal, even if both extend Animal)
    //              [Dog, Animal] -> Animal (Animal is in the set and is a supertype)
    //
    // OPTIMIZATION: Tournament-style O(N) reduction instead of O(N²) brute-force.
    // Pass 1 (O(N)): Find the "tournament winner" — iterate through candidates,
    //   replacing `best` whenever we find a candidate that is a supertype of it.
    // Pass 2 (O(N)): Verify the winner is truly a supertype of ALL types.
    // Total: O(2N) instead of O(N²). For 50 candidates: 100 checks vs 2,500.
    //
    // We handle the two cases (with/without resolver) separately because SubtypeChecker<R>
    // and SubtypeChecker<NoopResolver> are different types.
    if let Some(res) = resolver {
        let mut checker = SubtypeChecker::with_resolver(interner, res);
        // Pass 1: Tournament to find potential best candidate
        let mut best = widened[0];
        for &candidate in &widened[1..] {
            checker.guard.reset();
            if checker.is_subtype_of(best, candidate) {
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
    } else {
        let mut checker = SubtypeChecker::new(interner);
        // Pass 1: Tournament to find potential best candidate
        let mut best = widened[0];
        for &candidate in &widened[1..] {
            checker.guard.reset();
            if checker.is_subtype_of(best, candidate) {
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

    // Step 4: Default to union of all types
    interner.union(widened)
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
            if let Some(crate::types::TypeData::Literal(ref lit)) = interner.lookup(ty) {
                return lit.primitive_type_id();
            }
            ty // Non-literal types are preserved
        })
        .collect()
}

/// Get the base type of a type (for literals, this is the primitive type).
fn get_base_type(interner: &dyn TypeDatabase, ty: TypeId) -> Option<TypeId> {
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
