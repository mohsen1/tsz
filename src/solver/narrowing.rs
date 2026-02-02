//! Type narrowing for discriminated unions and type guards.
//!
//! Discriminated unions are unions where each member has a common "discriminant"
//! property with a literal type that uniquely identifies that member.
//!
//! Example:
//! ```typescript
//! type Action =
//!   | { type: "add", value: number }
//!   | { type: "remove", id: string }
//!   | { type: "clear" };
//!
//! function handle(action: Action) {
//!   if (action.type === "add") {
//!     // action is narrowed to { type: "add", value: number }
//!   }
//! }
//! ```
//!
//! ## TypeGuard Abstraction
//!
//! The `TypeGuard` enum provides an AST-agnostic representation of narrowing
//! conditions. This allows the Solver to perform pure type algebra without
//! depending on AST nodes.
//!
//! Architecture:
//! - **Checker**: Extracts `TypeGuard` from AST nodes (WHERE)
//! - **Solver**: Applies `TypeGuard` to types (WHAT)

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::types::*;
use crate::solver::visitor::{
    intersection_list_id, is_function_type_db, is_literal_type_db, is_object_like_type_db,
    literal_value, object_shape_id, object_with_index_shape_id, template_literal_id,
    type_param_info, union_list_id,
};
use tracing::{Level, span, trace};

#[cfg(test)]
use crate::solver::TypeInterner;

/// AST-agnostic representation of a type narrowing condition.
///
/// This enum represents various guards that can narrow a type, without
/// depending on AST nodes like `NodeIndex` or `SyntaxKind`.
///
/// # Examples
/// ```typescript
/// typeof x === "string"     -> TypeGuard::Typeof("string")
/// x instanceof MyClass      -> TypeGuard::Instanceof(MyClass_type)
/// x === null                -> TypeGuard::NullishEquality
/// x                         -> TypeGuard::Truthy
/// x.kind === "circle"       -> TypeGuard::Discriminant { property: "kind", value: "circle" }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub enum TypeGuard {
    /// `typeof x === "typename"`
    ///
    /// Narrows a union to only members matching the typeof result.
    /// For example, narrowing `string | number` with `Typeof("string")` yields `string`.
    Typeof(String),

    /// `x instanceof Class`
    ///
    /// Narrows to the class type or its subtypes.
    Instanceof(TypeId),

    /// `x === literal` or `x !== literal`
    ///
    /// Narrows to exactly that literal type (for equality) or excludes it (for inequality).
    LiteralEquality(TypeId),

    /// `x == null` or `x != null` (checks both null and undefined)
    ///
    /// JavaScript/TypeScript treats `== null` as matching both `null` and `undefined`.
    NullishEquality,

    /// `x` (truthiness check in a conditional)
    ///
    /// Removes falsy types from a union: `null`, `undefined`, `false`, `0`, `""`, `NaN`.
    Truthy,

    /// `x.prop === literal` (Discriminated Union narrowing)
    ///
    /// Narrows a union of object types based on a discriminant property.
    /// For example, narrowing `{ kind: "A" } | { kind: "B" }` with
    /// `Discriminant { property: "kind", value: "A" }` yields `{ kind: "A" }`.
    Discriminant {
        property_name: Atom,
        value_type: TypeId,
    },

    /// `prop in x`
    ///
    /// Narrows to types that have the specified property.
    InProperty(Atom),
}

/// Result of a narrowing operation.
///
/// Represents the types in both branches of a condition.
#[derive(Clone, Debug)]
pub struct NarrowingResult {
    /// The type in the "true" branch of the condition
    pub true_type: TypeId,
    /// The type in the "false" branch of the condition
    pub false_type: TypeId,
}

/// Result of finding discriminant properties in a union.
#[derive(Clone, Debug)]
pub struct DiscriminantInfo {
    /// The name of the discriminant property
    pub property_name: Atom,
    /// Map from literal value to the union member type
    pub variants: Vec<(TypeId, TypeId)>, // (literal_type, member_type)
}

/// Narrowing context for type guards and control flow analysis.
pub struct NarrowingContext<'a> {
    interner: &'a dyn TypeDatabase,
}

impl<'a> NarrowingContext<'a> {
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        NarrowingContext { interner }
    }

    /// Find discriminant properties in a union type.
    ///
    /// A discriminant property is one where:
    /// 1. All union members have the property
    /// 2. Each member has a unique literal type for that property
    pub fn find_discriminants(&self, union_type: TypeId) -> Vec<DiscriminantInfo> {
        let _span = span!(
            Level::TRACE,
            "find_discriminants",
            union_type = union_type.0
        )
        .entered();

        let members = match union_list_id(self.interner, union_type) {
            Some(members_id) => self.interner.type_list(members_id),
            None => return vec![],
        };

        if members.len() < 2 {
            trace!("Union has fewer than 2 members, skipping discriminant search");
            return vec![];
        }

        // Collect all property names from all members
        let mut all_properties: Vec<Atom> = Vec::new();
        let mut member_props: Vec<Vec<(Atom, TypeId)>> = Vec::new();

        for &member in members.iter() {
            if let Some(shape_id) = object_shape_id(self.interner, member) {
                let shape = self.interner.object_shape(shape_id);
                let props_vec: Vec<(Atom, TypeId)> = shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id))
                    .collect();

                // Track all property names
                for (name, _) in &props_vec {
                    if !all_properties.contains(name) {
                        all_properties.push(*name);
                    }
                }
                member_props.push(props_vec);
            } else {
                // Non-object member - can't have discriminants
                return vec![];
            }
        }

        // Check each property to see if it's a valid discriminant
        let mut discriminants = Vec::new();

        for prop_name in &all_properties {
            let mut is_discriminant = true;
            let mut variants: Vec<(TypeId, TypeId)> = Vec::new();
            let mut seen_literals: Vec<TypeId> = Vec::new();

            for (i, props) in member_props.iter().enumerate() {
                // Find this property in the member
                let prop_type = props
                    .iter()
                    .find(|(name, _)| name == prop_name)
                    .map(|(_, ty)| *ty);

                match prop_type {
                    Some(ty) => {
                        // Must be a literal type
                        if self.is_literal_type(ty) {
                            // Must be unique among members
                            if seen_literals.contains(&ty) {
                                is_discriminant = false;
                                break;
                            }
                            seen_literals.push(ty);
                            variants.push((ty, members[i]));
                        } else {
                            is_discriminant = false;
                            break;
                        }
                    }
                    None => {
                        // Property doesn't exist in this member
                        is_discriminant = false;
                        break;
                    }
                }
            }

            if is_discriminant && !variants.is_empty() {
                discriminants.push(DiscriminantInfo {
                    property_name: *prop_name,
                    variants,
                });
            }
        }

        discriminants
    }

    /// Narrow a union type based on a discriminant property check.
    ///
    /// Example: `action.type === "add"` narrows `Action` to `{ type: "add", value: number }`
    pub fn narrow_by_discriminant(
        &self,
        union_type: TypeId,
        property_name: Atom,
        literal_value: TypeId,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_discriminant",
            union_type = union_type.0,
            ?property_name,
            literal_value = literal_value.0
        )
        .entered();

        let discriminants = self.find_discriminants(union_type);

        for disc in &discriminants {
            if disc.property_name == property_name {
                // Find the variant matching this literal
                for (lit, member) in &disc.variants {
                    if *lit == literal_value {
                        return *member;
                    }
                }
            }
        }

        // No narrowing possible - return original
        union_type
    }

    /// Narrow a union type by excluding variants with a specific discriminant value.
    ///
    /// Example: `action.type !== "add"` narrows to `{ type: "remove", ... } | { type: "clear" }`
    pub fn narrow_by_excluding_discriminant(
        &self,
        union_type: TypeId,
        property_name: Atom,
        excluded_value: TypeId,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_excluding_discriminant",
            union_type = union_type.0,
            ?property_name,
            excluded_value = excluded_value.0
        )
        .entered();

        let members = match union_list_id(self.interner, union_type) {
            Some(members_id) => self.interner.type_list(members_id),
            None => return union_type,
        };

        let mut remaining: Vec<TypeId> = Vec::new();

        for &member in members.iter() {
            if let Some(shape_id) = object_shape_id(self.interner, member) {
                let shape = self.interner.object_shape(shape_id);
                let prop_type = shape
                    .properties
                    .iter()
                    .find(|p| p.name == property_name)
                    .map(|p| p.type_id);

                match prop_type {
                    Some(ty) if ty == excluded_value => {
                        // Exclude this member
                    }
                    _ => {
                        remaining.push(member);
                    }
                }
            } else {
                remaining.push(member);
            }
        }

        if remaining.is_empty() {
            TypeId::NEVER
        } else if remaining.len() == 1 {
            remaining[0]
        } else {
            self.interner.union(remaining)
        }
    }

    /// Narrow a type based on a typeof check.
    ///
    /// Example: `typeof x === "string"` narrows `string | number` to `string`
    pub fn narrow_by_typeof(&self, source_type: TypeId, typeof_result: &str) -> TypeId {
        let _span =
            span!(Level::TRACE, "narrow_by_typeof", source_type = source_type.0, %typeof_result)
                .entered();

        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }

        if source_type == TypeId::UNKNOWN {
            return match typeof_result {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "bigint" => TypeId::BIGINT,
                "symbol" => TypeId::SYMBOL,
                "undefined" => TypeId::UNDEFINED,
                "object" => self.interner.union2(TypeId::OBJECT, TypeId::NULL),
                "function" => self.function_type(),
                _ => source_type,
            };
        }

        let target_type = match typeof_result {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            "undefined" => TypeId::UNDEFINED,
            "object" => TypeId::OBJECT, // includes null
            "function" => return self.narrow_to_function(source_type),
            _ => return source_type,
        };

        self.narrow_to_type(source_type, target_type)
    }

    /// Narrow a type to include only members assignable to target.
    pub fn narrow_to_type(&self, source_type: TypeId, target_type: TypeId) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_to_type",
            source_type = source_type.0,
            target_type = target_type.0
        )
        .entered();

        // If source is the target, return it
        if source_type == target_type {
            trace!("Source type equals target type, returning unchanged");
            return source_type;
        }

        // Special case: unknown can be narrowed to any type through type guards
        // This handles cases like: if (typeof x === "string") where x: unknown
        if source_type == TypeId::UNKNOWN {
            trace!("Narrowing unknown to specific type via type guard");
            return target_type;
        }

        // If source is a union, filter members
        if let Some(members) = union_list_id(self.interner, source_type) {
            let members = self.interner.type_list(members);
            trace!(
                "Narrowing union with {} members to type {}",
                members.len(),
                target_type.0
            );
            let matching: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if let Some(narrowed) = self.narrow_type_param(member, target_type) {
                        return Some(narrowed);
                    }
                    if self.is_assignable_to(member, target_type) {
                        return Some(member);
                    }
                    None
                })
                .collect();

            if matching.is_empty() {
                trace!("No matching members found, returning NEVER");
                return TypeId::NEVER;
            } else if matching.len() == 1 {
                trace!("Found single matching member, returning {}", matching[0].0);
                return matching[0];
            } else {
                trace!(
                    "Found {} matching members, creating new union",
                    matching.len()
                );
                return self.interner.union(matching);
            }
        }

        if let Some(narrowed) = self.narrow_type_param(source_type, target_type) {
            trace!("Narrowed type parameter to {}", narrowed.0);
            return narrowed;
        }

        // Check if source is assignable to target
        if self.is_assignable_to(source_type, target_type) {
            trace!("Source type is assignable to target, returning source");
            source_type
        } else {
            trace!("Source type is not assignable to target, returning NEVER");
            TypeId::NEVER
        }
    }

    /// Narrow a type to exclude members assignable to target.
    pub fn narrow_excluding_type(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        if let Some(members) = intersection_list_id(self.interner, source_type) {
            let members = self.interner.type_list(members);
            let mut narrowed_members = Vec::with_capacity(members.len());
            let mut changed = false;
            for &member in members.iter() {
                let narrowed = self.narrow_excluding_type(member, excluded_type);
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
                if narrowed != member {
                    changed = true;
                }
                narrowed_members.push(narrowed);
            }
            if !changed {
                return source_type;
            }
            return self.interner.intersection(narrowed_members);
        }

        // If source is a union, filter out matching members
        if let Some(members) = union_list_id(self.interner, source_type) {
            let members = self.interner.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if intersection_list_id(self.interner, member).is_some() {
                        let narrowed = self.narrow_excluding_type(member, excluded_type);
                        if narrowed == TypeId::NEVER {
                            return None;
                        }
                        return Some(narrowed);
                    }
                    if let Some(narrowed) = self.narrow_type_param_excluding(member, excluded_type)
                    {
                        if narrowed == TypeId::NEVER {
                            return None;
                        }
                        return Some(narrowed);
                    }
                    if self.is_assignable_to(member, excluded_type) {
                        None
                    } else {
                        Some(member)
                    }
                })
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            } else {
                return self.interner.union(remaining);
            }
        }

        if let Some(narrowed) = self.narrow_type_param_excluding(source_type, excluded_type) {
            return narrowed;
        }

        // If source is assignable to excluded, return never
        if self.is_assignable_to(source_type, excluded_type) {
            TypeId::NEVER
        } else {
            source_type
        }
    }

    /// Narrow to function types only.
    fn narrow_to_function(&self, source_type: TypeId) -> TypeId {
        if let Some(members) = union_list_id(self.interner, source_type) {
            let members = self.interner.type_list(members);
            let functions: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if let Some(narrowed) = self.narrow_type_param_to_function(member) {
                        if narrowed == TypeId::NEVER {
                            return None;
                        }
                        return Some(narrowed);
                    }
                    if self.is_function_type(member) {
                        Some(member)
                    } else {
                        None
                    }
                })
                .collect();

            if functions.is_empty() {
                return TypeId::NEVER;
            } else if functions.len() == 1 {
                return functions[0];
            } else {
                return self.interner.union(functions);
            }
        }

        if let Some(narrowed) = self.narrow_type_param_to_function(source_type) {
            return narrowed;
        }

        if self.is_function_type(source_type) {
            source_type
        } else if source_type == TypeId::OBJECT {
            self.function_type()
        } else if let Some(shape_id) = object_shape_id(self.interner, source_type) {
            let shape = self.interner.object_shape(shape_id);
            if shape.properties.is_empty() {
                self.function_type()
            } else {
                TypeId::NEVER
            }
        } else if let Some(shape_id) = object_with_index_shape_id(self.interner, source_type) {
            let shape = self.interner.object_shape(shape_id);
            if shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
            {
                self.function_type()
            } else {
                TypeId::NEVER
            }
        } else {
            TypeId::NEVER
        }
    }

    /// Check if a type is a literal type.
    /// Uses the visitor pattern from solver::visitor.
    fn is_literal_type(&self, type_id: TypeId) -> bool {
        is_literal_type_db(self.interner, type_id)
    }

    /// Check if a type is a function type.
    /// Uses the visitor pattern from solver::visitor.
    fn is_function_type(&self, type_id: TypeId) -> bool {
        is_function_type_db(self.interner, type_id)
    }

    /// Narrow a type to exclude function-like members (typeof !== "function").
    pub fn narrow_excluding_function(&self, source_type: TypeId) -> TypeId {
        if let Some(members) = union_list_id(self.interner, source_type) {
            let members = self.interner.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if let Some(narrowed) = self.narrow_type_param_excluding_function(member) {
                        if narrowed == TypeId::NEVER {
                            return None;
                        }
                        return Some(narrowed);
                    }
                    if self.is_function_type(member) {
                        None
                    } else {
                        Some(member)
                    }
                })
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            } else {
                return self.interner.union(remaining);
            }
        }

        if let Some(narrowed) = self.narrow_type_param_excluding_function(source_type) {
            return narrowed;
        }

        if self.is_function_type(source_type) {
            TypeId::NEVER
        } else {
            source_type
        }
    }

    /// Check if a type has typeof "object".
    /// Uses the visitor pattern from solver::visitor.
    fn is_object_typeof(&self, type_id: TypeId) -> bool {
        is_object_like_type_db(self.interner, type_id)
    }

    fn narrow_type_param(&self, source: TypeId, target: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.interner, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source {
            return None;
        }

        let narrowed_constraint = if constraint == TypeId::UNKNOWN {
            target
        } else {
            self.narrow_to_type(constraint, target)
        };

        if narrowed_constraint == TypeId::NEVER {
            return None;
        }

        Some(self.interner.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_to_function(&self, source: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.interner, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source || constraint == TypeId::UNKNOWN {
            let function_type = self.function_type();
            return Some(self.interner.intersection2(source, function_type));
        }

        let narrowed_constraint = self.narrow_to_function(constraint);
        if narrowed_constraint == TypeId::NEVER {
            return None;
        }

        Some(self.interner.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_excluding(&self, source: TypeId, excluded: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.interner, source)?;

        let constraint = info.constraint?;
        if constraint == source || constraint == TypeId::UNKNOWN {
            return None;
        }

        let narrowed_constraint = self.narrow_excluding_type(constraint, excluded);
        if narrowed_constraint == constraint {
            return None;
        }
        if narrowed_constraint == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }

        Some(self.interner.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_excluding_function(&self, source: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.interner, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source || constraint == TypeId::UNKNOWN {
            return Some(source);
        }

        let narrowed_constraint = self.narrow_excluding_function(constraint);
        if narrowed_constraint == constraint {
            return Some(source);
        }
        if narrowed_constraint == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }

        Some(self.interner.intersection2(source, narrowed_constraint))
    }

    pub(crate) fn function_type(&self) -> TypeId {
        let rest_array = self.interner.array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.interner.function(FunctionShape {
            params: vec![rest_param],
            this_type: None,
            return_type: TypeId::ANY,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    /// Simple assignability check for narrowing purposes.
    fn is_assignable_to(&self, source: TypeId, target: TypeId) -> bool {
        if source == target {
            return true;
        }

        // never is assignable to everything
        if source == TypeId::NEVER {
            return true;
        }

        // everything is assignable to any/unknown
        if target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        // Literal to base type
        if let Some(lit) = literal_value(self.interner, source) {
            match (lit, target) {
                (LiteralValue::String(_), t) if t == TypeId::STRING => return true,
                (LiteralValue::Number(_), t) if t == TypeId::NUMBER => return true,
                (LiteralValue::Boolean(_), t) if t == TypeId::BOOLEAN => return true,
                (LiteralValue::BigInt(_), t) if t == TypeId::BIGINT => return true,
                _ => {}
            }
        }

        // object/null for typeof "object"
        if target == TypeId::OBJECT {
            if source == TypeId::NULL {
                return true;
            }
            if self.is_object_typeof(source) {
                return true;
            }
            return false;
        }

        if let Some(members) = intersection_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            if members
                .iter()
                .any(|member| self.is_assignable_to(*member, target))
            {
                return true;
            }
        }

        if target == TypeId::STRING && template_literal_id(self.interner, source).is_some() {
            return true;
        }

        false
    }

    /// Applies a type guard to narrow a type.
    ///
    /// This is the main entry point for AST-agnostic type narrowing.
    /// The Checker extracts a `TypeGuard` from AST nodes, and the Solver
    /// applies it to compute the narrowed type.
    ///
    /// # Arguments
    /// * `source_type` - The type to narrow
    /// * `guard` - The guard condition (extracted from AST by Checker)
    /// * `sense` - If true, narrow for the "true" branch; if false, narrow for the "false" branch
    ///
    /// # Returns
    /// The narrowed type after applying the guard.
    ///
    /// # Examples
    /// ```ignore
    /// // typeof x === "string"
    /// let guard = TypeGuard::Typeof("string".to_string());
    /// let narrowed = narrowing.narrow_type(string_or_number, &guard, true);
    /// assert_eq!(narrowed, TypeId::STRING);
    ///
    /// // x !== null (negated sense)
    /// let guard = TypeGuard::NullishEquality;
    /// let narrowed = narrowing.narrow_type(string_or_null, &guard, false);
    /// // Result should exclude null and undefined
    /// ```
    pub fn narrow_type(&self, source_type: TypeId, guard: &TypeGuard, sense: bool) -> TypeId {
        match guard {
            TypeGuard::Typeof(type_name) => {
                if sense {
                    self.narrow_by_typeof(source_type, type_name)
                } else {
                    // Negation: exclude typeof type
                    self.narrow_by_typeof_negation(source_type, type_name)
                }
            }

            TypeGuard::Instanceof(_class_type) => {
                // TODO: Implement instanceof narrowing
                // For now, return the source type unchanged
                source_type
            }

            TypeGuard::LiteralEquality(literal_type) => {
                if sense {
                    // Equality: narrow to the literal type
                    self.narrow_to_type(source_type, *literal_type)
                } else {
                    // Inequality: exclude the literal type
                    self.narrow_excluding_type(source_type, *literal_type)
                }
            }

            TypeGuard::NullishEquality => {
                if sense {
                    // Equality with null: narrow to null | undefined
                    self.interner.union(vec![TypeId::NULL, TypeId::UNDEFINED])
                } else {
                    // Inequality: exclude null and undefined
                    let without_null = self.narrow_excluding_type(source_type, TypeId::NULL);
                    self.narrow_excluding_type(without_null, TypeId::UNDEFINED)
                }
            }

            TypeGuard::Truthy => {
                if sense {
                    // Truthy: remove falsy types (null, undefined, false, 0, "", NaN)
                    self.narrow_by_truthiness(source_type)
                } else {
                    // Falsy: intersection with falsy types
                    // TODO: Implement proper falsy narrowing
                    source_type
                }
            }

            TypeGuard::Discriminant {
                property_name,
                value_type,
            } => {
                if sense {
                    // Discriminant matches: narrow to matching union members
                    self.narrow_by_discriminant(source_type, *property_name, *value_type)
                } else {
                    // Discriminant doesn't match: exclude matching union members
                    self.narrow_by_excluding_discriminant(source_type, *property_name, *value_type)
                }
            }

            TypeGuard::InProperty(_property_name) => {
                // TODO: Implement `in` operator narrowing
                // For now, return the source type unchanged
                source_type
            }
        }
    }

    /// Narrow a type by removing typeof-matching types.
    ///
    /// This is the negation of `narrow_by_typeof`.
    /// For example, narrowing `string | number` with `typeof "string"` (sense=false)
    /// yields `number`.
    fn narrow_by_typeof_negation(&self, source_type: TypeId, typeof_result: &str) -> TypeId {
        // For each typeof result, we exclude matching types
        let excluded = match typeof_result {
            "string" => TypeId::STRING,
            "number" => TypeId::NUMBER,
            "boolean" => TypeId::BOOLEAN,
            "bigint" => TypeId::BIGINT,
            "symbol" => TypeId::SYMBOL,
            "function" => {
                // Functions are more complex - handle separately
                return self.narrow_excluding_function(source_type);
            }
            "object" => {
                // Object excludes primitives
                // Exclude null, undefined, string, number, boolean, bigint, symbol
                let mut result = source_type;
                for &primitive in &[
                    TypeId::NULL,
                    TypeId::UNDEFINED,
                    TypeId::STRING,
                    TypeId::NUMBER,
                    TypeId::BOOLEAN,
                    TypeId::BIGINT,
                    TypeId::SYMBOL,
                ] {
                    result = self.narrow_excluding_type(result, primitive);
                }
                return result;
            }
            _ => return source_type,
        };

        self.narrow_excluding_type(source_type, excluded)
    }

    /// Narrow a type by removing falsy values.
    ///
    /// Removes: null, undefined, false, 0, "", NaN
    fn narrow_by_truthiness(&self, source_type: TypeId) -> TypeId {
        let mut result = source_type;

        // Remove nullish types
        result = self.narrow_excluding_type(result, TypeId::NULL);
        result = self.narrow_excluding_type(result, TypeId::UNDEFINED);

        // Remove false literal
        if let Some(_false_type) = self.interner.lookup(TypeId::BOOLEAN) {
            // For now, just keep the implementation simple
            // TODO: Remove false literal from unions
        }

        result
    }
}

/// Convenience function for finding discriminants.
pub fn find_discriminants(
    interner: &dyn TypeDatabase,
    union_type: TypeId,
) -> Vec<DiscriminantInfo> {
    let ctx = NarrowingContext::new(interner);
    ctx.find_discriminants(union_type)
}

/// Convenience function for narrowing by discriminant.
pub fn narrow_by_discriminant(
    interner: &dyn TypeDatabase,
    union_type: TypeId,
    property_name: Atom,
    literal_value: TypeId,
) -> TypeId {
    let ctx = NarrowingContext::new(interner);
    ctx.narrow_by_discriminant(union_type, property_name, literal_value)
}

/// Convenience function for typeof narrowing.
pub fn narrow_by_typeof(
    interner: &dyn TypeDatabase,
    source_type: TypeId,
    typeof_result: &str,
) -> TypeId {
    let ctx = NarrowingContext::new(interner);
    ctx.narrow_by_typeof(source_type, typeof_result)
}

// =============================================================================
// Nullish Type Helpers
// =============================================================================

fn top_level_union_members(types: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    union_list_id(types, type_id).map(|list_id| types.type_list(list_id).to_vec())
}

fn is_nullish_intrinsic(type_id: TypeId) -> bool {
    matches!(type_id, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID)
}

fn is_undefined_intrinsic(type_id: TypeId) -> bool {
    matches!(type_id, TypeId::UNDEFINED | TypeId::VOID)
}

fn normalize_nullish(type_id: TypeId) -> TypeId {
    if type_id == TypeId::VOID {
        TypeId::UNDEFINED
    } else {
        type_id
    }
}

/// Check if a type is nullish (null/undefined/void or union containing them).
pub fn is_nullish_type(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_nullish_intrinsic(type_id) {
        return true;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members.iter().any(|&member| is_nullish_type(types, member));
    }
    false
}

/// Check if a type (possibly a union) contains null or undefined.
pub fn type_contains_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_nullish_type(types, type_id)
}

/// Check if a type contains undefined (or void).
pub fn type_contains_undefined(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_undefined_intrinsic(type_id) {
        return true;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members
            .iter()
            .any(|&member| type_contains_undefined(types, member));
    }
    false
}

/// Check if a type is definitely nullish (only null/undefined/void).
pub fn is_definitely_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if is_nullish_intrinsic(type_id) {
        return true;
    }
    if let Some(members) = top_level_union_members(types, type_id) {
        return members
            .iter()
            .all(|&member| is_definitely_nullish(types, member));
    }
    false
}

/// Check if a type can be nullish (contains null/undefined/void).
pub fn can_be_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> bool {
    is_nullish_type(types, type_id)
}

fn split_nullish_members(
    types: &dyn TypeDatabase,
    type_id: TypeId,
    non_nullish: &mut Vec<TypeId>,
    nullish: &mut Vec<TypeId>,
) {
    if is_nullish_intrinsic(type_id) {
        nullish.push(normalize_nullish(type_id));
        return;
    }

    if let Some(members) = top_level_union_members(types, type_id) {
        for member in members {
            split_nullish_members(types, member, non_nullish, nullish);
        }
        return;
    }

    non_nullish.push(type_id);
}

/// Split a type into its non-nullish part and its nullish cause.
pub fn split_nullish_type(
    types: &dyn TypeDatabase,
    type_id: TypeId,
) -> (Option<TypeId>, Option<TypeId>) {
    let mut non_nullish = Vec::new();
    let mut nullish = Vec::new();

    split_nullish_members(types, type_id, &mut non_nullish, &mut nullish);

    if nullish.is_empty() {
        return (Some(type_id), None);
    }

    let non_nullish_type = if non_nullish.is_empty() {
        None
    } else if non_nullish.len() == 1 {
        Some(non_nullish[0])
    } else {
        Some(types.union(non_nullish))
    };

    let nullish_type = if nullish.len() == 1 {
        Some(nullish[0])
    } else {
        Some(types.union(nullish))
    };

    (non_nullish_type, nullish_type)
}

/// Remove nullish parts of a type (non-null assertion).
pub fn remove_nullish(types: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let (non_nullish, _) = split_nullish_type(types, type_id);
    non_nullish.unwrap_or(TypeId::NEVER)
}

#[cfg(test)]
#[path = "tests/narrowing_tests.rs"]
mod tests;
