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

use crate::interner::Atom;
use crate::solver::TypeDatabase;
use crate::solver::types::*;

#[cfg(test)]
use crate::solver::TypeInterner;

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
        let members = match self.interner.lookup(union_type) {
            Some(TypeKey::Union(m)) => self.interner.type_list(m),
            _ => return vec![],
        };

        if members.len() < 2 {
            return vec![];
        }

        // Collect all property names from all members
        let mut all_properties: Vec<Atom> = Vec::new();
        let mut member_props: Vec<Vec<(Atom, TypeId)>> = Vec::new();

        for &member in members.iter() {
            if let Some(TypeKey::Object(shape_id)) = self.interner.lookup(member) {
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
        let members = match self.interner.lookup(union_type) {
            Some(TypeKey::Union(m)) => self.interner.type_list(m),
            _ => return union_type,
        };

        let mut remaining: Vec<TypeId> = Vec::new();

        for &member in members.iter() {
            if let Some(TypeKey::Object(shape_id)) = self.interner.lookup(member) {
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
        // If source is the target, return it
        if source_type == target_type {
            return source_type;
        }

        // If source is a union, filter members
        if let Some(TypeKey::Union(members)) = self.interner.lookup(source_type) {
            let members = self.interner.type_list(members);
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
                return TypeId::NEVER;
            } else if matching.len() == 1 {
                return matching[0];
            } else {
                return self.interner.union(matching);
            }
        }

        if let Some(narrowed) = self.narrow_type_param(source_type, target_type) {
            return narrowed;
        }

        // Check if source is assignable to target
        if self.is_assignable_to(source_type, target_type) {
            source_type
        } else {
            TypeId::NEVER
        }
    }

    /// Narrow a type to exclude members assignable to target.
    pub fn narrow_excluding_type(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        if let Some(TypeKey::Intersection(members)) = self.interner.lookup(source_type) {
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
        if let Some(TypeKey::Union(members)) = self.interner.lookup(source_type) {
            let members = self.interner.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if matches!(self.interner.lookup(member), Some(TypeKey::Intersection(_))) {
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
        if let Some(TypeKey::Union(members)) = self.interner.lookup(source_type) {
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
        } else if let Some(TypeKey::Object(shape_id)) = self.interner.lookup(source_type) {
            let shape = self.interner.object_shape(shape_id);
            if shape.properties.is_empty() {
                self.function_type()
            } else {
                TypeId::NEVER
            }
        } else if let Some(TypeKey::ObjectWithIndex(shape_id)) = self.interner.lookup(source_type) {
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
    fn is_literal_type(&self, type_id: TypeId) -> bool {
        matches!(self.interner.lookup(type_id), Some(TypeKey::Literal(_)))
    }

    /// Check if a type is a function type.
    fn is_function_type(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Function(_) | TypeKey::Callable(_)) => true,
            Some(TypeKey::Intersection(members)) => {
                let members = self.interner.type_list(members);
                members.iter().any(|member| self.is_function_type(*member))
            }
            _ => false,
        }
    }

    /// Narrow a type to exclude function-like members (typeof !== "function").
    pub fn narrow_excluding_function(&self, source_type: TypeId) -> TypeId {
        if let Some(TypeKey::Union(members)) = self.interner.lookup(source_type) {
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

    fn is_object_typeof(&self, type_id: TypeId) -> bool {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Object(_))
            | Some(TypeKey::ObjectWithIndex(_))
            | Some(TypeKey::Array(_))
            | Some(TypeKey::Tuple(_))
            | Some(TypeKey::Mapped(_)) => true,
            Some(TypeKey::ReadonlyType(inner)) => self.is_object_typeof(inner),
            Some(TypeKey::Intersection(members)) => {
                let members = self.interner.type_list(members);
                members.iter().all(|member| self.is_object_typeof(*member))
            }
            Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info
                .constraint
                .map(|constraint| self.is_object_typeof(constraint))
                .unwrap_or(false),
            _ => false,
        }
    }

    fn narrow_type_param(&self, source: TypeId, target: TypeId) -> Option<TypeId> {
        let info = match self.interner.lookup(source) {
            Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info,
            _ => return None,
        };

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
        let info = match self.interner.lookup(source) {
            Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info,
            _ => return None,
        };

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
        let info = match self.interner.lookup(source) {
            Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info,
            _ => return None,
        };

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
        let info = match self.interner.lookup(source) {
            Some(TypeKey::TypeParameter(info)) | Some(TypeKey::Infer(info)) => info,
            _ => return None,
        };

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
        if let Some(TypeKey::Literal(lit)) = self.interner.lookup(source) {
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

        if let Some(TypeKey::Intersection(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            if members
                .iter()
                .any(|member| self.is_assignable_to(*member, target))
            {
                return true;
            }
        }

        if target == TypeId::STRING
            && matches!(
                self.interner.lookup(source),
                Some(TypeKey::TemplateLiteral(_))
            )
        {
            return true;
        }

        false
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

#[cfg(test)]
#[path = "narrowing_tests.rs"]
mod tests;
