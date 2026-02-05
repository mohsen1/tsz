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
use crate::solver::operations_property::{PropertyAccessEvaluator, PropertyAccessResult};
use crate::solver::subtype::{TypeResolver, is_subtype_of};
use crate::solver::type_queries::{UnionMembersKind, classify_for_union_members};
use crate::solver::types::Visibility;
use crate::solver::types::*;
use crate::solver::visitor::{
    TypeVisitor, intersection_list_id, is_function_type_db, is_literal_type_db,
    is_object_like_type_db, lazy_def_id, literal_value, object_shape_id,
    object_with_index_shape_id, template_literal_id, type_param_info, union_list_id,
};
use crate::solver::{QueryDatabase, TypeDatabase};
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

    /// `x.prop === literal` or `x.payload.type === "value"` (Discriminated Union narrowing)
    ///
    /// Narrows a union of object types based on a discriminant property.
    ///
    /// # Examples
    /// - Top-level: `{ kind: "A" } | { kind: "B" }` with `path: ["kind"]` yields `{ kind: "A" }`
    /// - Nested: `{ payload: { type: "user" } } | { payload: { type: "product" } }`
    ///   with `path: ["payload", "type"]` yields `{ payload: { type: "user" } }`
    Discriminant {
        /// Property path from base to discriminant (e.g., ["payload", "type"])
        property_path: Vec<Atom>,
        /// The literal value to match against
        value_type: TypeId,
    },

    /// `prop in x`
    ///
    /// Narrows to types that have the specified property.
    InProperty(Atom),

    /// `x is T` or `asserts x is T` (User-Defined Type Guard)
    ///
    /// Narrows a type based on a user-defined type predicate function.
    ///
    /// # Examples
    /// ```typescript
    /// function isString(x: any): x is string { ... }
    /// function assertDefined(x: any): asserts x is Date { ... }
    ///
    /// if (isString(x)) { x; // string }
    /// assertDefined(x); x; // Date
    /// ```
    ///
    /// - `type_id: Some(T)`: The type to narrow to (e.g., `string` or `Date`)
    /// - `type_id: None`: Truthiness assertion (`asserts x`), behaves like `Truthy`
    /// - `asserts: true`: This is an assertion (throws if false), affects control flow
    Predicate {
        type_id: Option<TypeId>,
        asserts: bool,
    },

    /// `Array.isArray(x)`
    ///
    /// Narrows a type to only array-like types (arrays, tuples, readonly arrays).
    ///
    /// # Examples
    /// ```typescript
    /// function process(x: string[] | number | { length: number }) {
    ///   if (Array.isArray(x)) {
    ///     x; // string[] (not number or the object)
    ///   }
    /// }
    /// ```
    ///
    /// This preserves element types - `string[] | number[]` stays as `string[] | number[]`,
    /// it doesn't collapse to `any[]`.
    Array,
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
    db: &'a dyn QueryDatabase,
    /// Optional TypeResolver for resolving Lazy types (e.g., type aliases).
    /// When present, this enables proper narrowing of type aliases like `type Shape = Circle | Square`.
    resolver: Option<&'a dyn TypeResolver>,
}

impl<'a> NarrowingContext<'a> {
    pub fn new(db: &'a dyn QueryDatabase) -> Self {
        NarrowingContext { db, resolver: None }
    }

    /// Set the TypeResolver for this context.
    ///
    /// This enables proper resolution of Lazy types (type aliases) during narrowing.
    /// The resolver should be borrowed from the Checker's TypeEnvironment.
    pub fn with_resolver(mut self, resolver: &'a dyn TypeResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Resolve a type to its structural representation.
    ///
    /// Unwraps:
    /// - Lazy types (evaluates them using resolver if available, otherwise falls back to db)
    /// - Application types (evaluates the generic instantiation)
    ///
    /// This ensures that type aliases, interfaces, and generics are resolved
    /// to their actual structural types before performing narrowing operations.
    fn resolve_type(&self, mut type_id: TypeId) -> TypeId {
        // Prevent infinite loops with a fuel counter
        let mut fuel = 100;

        while fuel > 0 {
            fuel -= 1;

            // 1. Handle Lazy types (DefId-based, not SymbolRef)
            // If we have a TypeResolver, try to resolve Lazy types through it first
            if let Some(def_id) = lazy_def_id(self.db, type_id) {
                if let Some(resolver) = self.resolver {
                    if let Some(resolved) =
                        resolver.resolve_lazy(def_id, self.db.as_type_database())
                    {
                        type_id = resolved;
                        continue;
                    }
                }
                // Fallback to database evaluation if no resolver or resolution failed
                type_id = self.db.evaluate_type(type_id);
                continue;
            }

            // 2. Handle Application types (Generics)
            if let Some(TypeKey::Application(_app_id)) = self.db.lookup(type_id) {
                type_id = self.db.evaluate_type(type_id);
                continue;
            }

            // It's a structural type (Object, Union, Intersection, Primitive)
            break;
        }

        type_id
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

        let members = match union_list_id(self.db, union_type) {
            Some(members_id) => self.db.type_list(members_id),
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
            if let Some(shape_id) = object_shape_id(self.db, member) {
                let shape = self.db.object_shape(shape_id);
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

    /// Get the type of a property at a nested path within a type.
    ///
    /// # Examples
    /// - `get_type_at_path(type, ["payload"])` -> type of `payload` property
    /// - `get_type_at_path(type, ["payload", "type"])` -> type of `payload.type`
    ///
    /// Returns `None` if:
    /// - The type doesn't have the property at any level in the path
    /// - An intermediate type in the path is not an object type
    ///
    /// **NOTE**: Uses `resolve_property_access` which correctly handles optional properties.
    /// For optional properties that don't exist on a specific union member, returns
    /// `TypeId::UNDEFINED` to indicate the property could be undefined (not a definitive mismatch).
    fn get_type_at_path(&self, mut type_id: TypeId, path: &[Atom]) -> Option<TypeId> {
        let evaluator = PropertyAccessEvaluator::new(self.db); // TODO: Fix to use resolver when available

        for (i, &prop_name) in path.iter().enumerate() {
            // Handle ANY - any property access on any returns any
            if type_id == TypeId::ANY {
                return Some(TypeId::ANY);
            }

            // Resolve Lazy types
            type_id = self.resolve_type(type_id);

            // Handle Union - return union of property types from all members
            if let Some(members_id) = union_list_id(self.db, type_id) {
                let members = self.db.type_list(members_id);
                let remaining_path = &path[i..];
                let prop_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&member| self.get_type_at_path(member, remaining_path))
                    .collect();

                if prop_types.is_empty() {
                    return None;
                } else if prop_types.len() == 1 {
                    return Some(prop_types[0]);
                } else {
                    return Some(self.db.union(prop_types));
                }
            }

            // Use resolve_property_access for proper optional property handling
            // This correctly handles properties that are optional (prop?: type)
            let prop_name_arc = self.db.resolve_atom_ref(prop_name);
            let prop_name_str = prop_name_arc.as_ref();
            match evaluator.resolve_property_access(type_id, prop_name_str) {
                PropertyAccessResult::Success {
                    type_id: prop_type_id,
                    ..
                } => {
                    // Property found - use its type
                    // For optional properties, this already includes `undefined` in the union
                    type_id = prop_type_id;
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    // Property truly doesn't exist on this type
                    // This union member doesn't have the discriminant property, so filter it out
                    return None;
                }
                PropertyAccessResult::PossiblyNullOrUndefined { property_type, .. } => {
                    // CRITICAL FIX: For optional properties (prop?: type), we need to preserve
                    // both the property type AND undefined in the union.
                    // This ensures that is_subtype_of(circle, "circle" | undefined) works correctly.
                    if let Some(prop_ty) = property_type {
                        // Create union: property_type | undefined
                        type_id = self.db.union2(prop_ty, TypeId::UNDEFINED);
                    } else {
                        // No property type, just undefined
                        type_id = TypeId::UNDEFINED;
                    }
                }
                PropertyAccessResult::IsUnknown => {
                    return Some(TypeId::ANY);
                }
            }
        }

        Some(type_id)
    }

    /// Narrow a union type based on a discriminant property check.
    ///
    /// Example: `action.type === "add"` narrows `Action` to `{ type: "add", value: number }`
    ///
    /// Uses a filtering approach: checks each union member individually to see if
    /// the property could match the literal value. This is more flexible than the
    /// old `find_discriminants` approach which required ALL members to have the
    /// property with unique literal values.
    ///
    /// # Arguments
    /// - `union_type`: The union type to narrow
    /// - `property_path`: Path to the discriminant property (e.g., ["payload", "type"])
    /// - `literal_value`: The literal value to match
    pub fn narrow_by_discriminant(
        &self,
        union_type: TypeId,
        property_path: &[Atom],
        literal_value: TypeId,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_discriminant",
            union_type = union_type.0,
            property_path_len = property_path.len(),
            literal_value = literal_value.0
        )
        .entered();

        // CRITICAL: Resolve Lazy types before checking for union members
        // This ensures type aliases are resolved to their actual union types
        let resolved_type = self.resolve_type(union_type);

        // CRITICAL FIX: Use classify_for_union_members instead of union_list_id
        // This correctly handles intersections containing unions, nested unions, etc.
        let single_member_storage: Vec<TypeId>;
        let members: &[TypeId] = match classify_for_union_members(self.db, resolved_type) {
            UnionMembersKind::Union(members_list) => {
                // Convert Vec to slice for iteration
                single_member_storage = members_list.into_iter().collect::<Vec<_>>();
                &single_member_storage
            }
            UnionMembersKind::NotUnion => {
                // Not a union at all - treat as single member
                single_member_storage = vec![resolved_type];
                &single_member_storage
            }
        };

        trace!(
            "Checking {} member(s) for discriminant match",
            members.len()
        );

        trace!(
            "Narrowing union with {} members by discriminant property",
            members.len()
        );

        let mut matching: Vec<TypeId> = Vec::new();

        for &member in members.iter() {
            // Special case: any and unknown always match
            if member == TypeId::ANY || member == TypeId::UNKNOWN {
                trace!("Member {} is any/unknown, keeping in true branch", member.0);
                matching.push(member);
                continue;
            }

            // CRITICAL: Resolve Lazy types before checking for object shape
            // This ensures type aliases are resolved to their actual types
            let resolved_member = self.resolve_type(member);

            // Handle Intersection types: check all intersection members for the property
            let intersection_members =
                if let Some(members_id) = intersection_list_id(self.db, resolved_member) {
                    // Intersection type: check all members
                    Some(self.db.type_list(members_id).to_vec())
                } else {
                    // Not an intersection: treat as single member
                    None
                };

            // Helper function to check if a type has a matching property at the path
            let check_member_for_property = |check_type_id: TypeId| -> bool {
                // Get the type at the property path
                let prop_type = match self.get_type_at_path(check_type_id, property_path) {
                    Some(t) => t,
                    None => {
                        // Property doesn't exist on this member
                        trace!("Member {} does not have property path", check_type_id.0);
                        return false;
                    }
                };

                // CRITICAL: Use is_subtype_of(literal_value, property_type)
                // NOT the reverse! This was the bug in the reverted commit.
                let matches = is_subtype_of(self.db, literal_value, prop_type);

                if matches {
                    trace!(
                        "Member {} has property path with type {}, literal {} matches",
                        check_type_id.0, prop_type.0, literal_value.0
                    );
                } else {
                    trace!(
                        "Member {} has property path with type {}, literal {} does not match",
                        check_type_id.0, prop_type.0, literal_value.0
                    );
                }

                matches
            };

            // Check for property match
            let has_property_match = if let Some(ref intersection) = intersection_members {
                // For Intersection: at least one member must have the property
                intersection.iter().any(|&m| check_member_for_property(m))
            } else {
                // For non-Intersection: check the single member
                check_member_for_property(resolved_member)
            };

            if has_property_match {
                matching.push(member);
            }
        }

        // Return result based on matches
        let result = if matching.is_empty() {
            trace!("No members matched discriminant check, returning never");
            TypeId::NEVER
        } else if matching.len() == members.len() {
            trace!("All members matched, returning original");
            union_type
        } else if matching.len() == 1 {
            trace!("Narrowed to single member");
            matching[0]
        } else {
            trace!(
                "Narrowed to {} of {} members",
                matching.len(),
                members.len()
            );
            self.db.union(matching)
        };

        result
    }

    /// Narrow a union type by excluding variants with a specific discriminant value.
    ///
    /// Example: `action.type !== "add"` narrows to `{ type: "remove", ... } | { type: "clear" }`
    ///
    /// Uses the inverse logic of `narrow_by_discriminant`: we exclude a member
    /// ONLY if its property is definitely and only the excluded value.
    ///
    /// For example:
    /// - prop is "a", exclude "a" -> exclude (property is always "a")
    /// - prop is "a" | "b", exclude "a" -> keep (could be "b")
    /// - prop doesn't exist -> keep (property doesn't match excluded value)
    ///
    /// # Arguments
    /// - `union_type`: The union type to narrow
    /// - `property_path`: Path to the discriminant property (e.g., ["payload", "type"])
    /// - `excluded_value`: The literal value to exclude
    pub fn narrow_by_excluding_discriminant(
        &self,
        union_type: TypeId,
        property_path: &[Atom],
        excluded_value: TypeId,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_excluding_discriminant",
            union_type = union_type.0,
            property_path_len = property_path.len(),
            excluded_value = excluded_value.0
        )
        .entered();

        // CRITICAL: Resolve Lazy types before checking for union members
        // This ensures type aliases are resolved to their actual union types
        let resolved_type = self.resolve_type(union_type);

        // Get union members - normalize single types to "union of 1" slice
        // This allows single-object narrowing to work correctly
        let single_member_storage;
        let members_list_storage;
        let members: &[TypeId] = if let Some(members_id) = union_list_id(self.db, resolved_type) {
            members_list_storage = self.db.type_list(members_id);
            &members_list_storage
        } else {
            single_member_storage = [resolved_type];
            &single_member_storage[..]
        };

        trace!(
            "Excluding discriminant value {} from union with {} members",
            excluded_value.0,
            members.len()
        );

        let mut remaining: Vec<TypeId> = Vec::new();

        for &member in members.iter() {
            // Special case: any and unknown always kept (could have any property value)
            if member == TypeId::ANY || member == TypeId::UNKNOWN {
                trace!(
                    "Member {} is any/unknown, keeping in false branch",
                    member.0
                );
                remaining.push(member);
                continue;
            }

            // CRITICAL: Resolve Lazy types before checking for object shape
            let resolved_member = self.resolve_type(member);

            // Handle Intersection types: check all intersection members for the property
            let intersection_members =
                if let Some(members_id) = intersection_list_id(self.db, resolved_member) {
                    // Intersection type: check all members
                    Some(self.db.type_list(members_id).to_vec())
                } else {
                    // Not an intersection: treat as single member
                    None
                };

            // Helper function to check if a member should be excluded
            // Returns true if member should be KEPT (not excluded)
            let should_keep_member = |check_type_id: TypeId| -> bool {
                // Get the type at the property path
                let prop_type = match self.get_type_at_path(check_type_id, property_path) {
                    Some(t) => t,
                    None => {
                        // Property doesn't exist - keep the member
                        trace!(
                            "Member {} does not have property path, keeping",
                            check_type_id.0
                        );
                        return true;
                    }
                };

                // Exclude member ONLY if property type is subtype of excluded value
                // This means the property is ALWAYS the excluded value
                // REVERSE of narrow_by_discriminant logic
                let should_exclude = is_subtype_of(self.db, prop_type, excluded_value);

                if should_exclude {
                    trace!(
                        "Member {} has property path type {} which is subtype of excluded {}, excluding",
                        check_type_id.0, prop_type.0, excluded_value.0
                    );
                    false // Member should be excluded
                } else {
                    trace!(
                        "Member {} has property path type {} which is not subtype of excluded {}, keeping",
                        check_type_id.0, prop_type.0, excluded_value.0
                    );
                    true // Member should be kept
                }
            };

            // Check if member should be kept
            let keep_member = if let Some(ref intersection) = intersection_members {
                // CRITICAL: For Intersection exclusion, use ALL not ANY
                // If ANY intersection member has the excluded property value,
                // the ENTIRE intersection must be excluded.
                // Example: { kind: "A" } & { data: string } with x.kind !== "A"
                //   -> { kind: "A" } has "A" (excluded) -> exclude entire intersection
                intersection.iter().all(|&m| should_keep_member(m))
            } else {
                // For non-Intersection: check the single member
                should_keep_member(resolved_member)
            };

            if keep_member {
                remaining.push(member);
            }
        }

        let remaining_count = remaining.len();
        let result = if remaining.is_empty() {
            TypeId::NEVER
        } else if remaining_count == 1 {
            remaining[0]
        } else {
            self.db.union(remaining)
        };

        result
    }

    /// Narrow a type based on a typeof check.
    ///
    /// Example: `typeof x === "string"` narrows `string | number` to `string`
    pub fn narrow_by_typeof(&self, source_type: TypeId, typeof_result: &str) -> TypeId {
        let _span =
            span!(Level::TRACE, "narrow_by_typeof", source_type = source_type.0, %typeof_result)
                .entered();

        // CRITICAL FIX: Narrow `any` for typeof checks
        // TypeScript narrows `any` for typeof/instanceof/Array.isArray/user-defined guards
        // But NOT for equality/truthiness/in operator
        if source_type == TypeId::UNKNOWN || source_type == TypeId::ANY {
            return match typeof_result {
                "string" => TypeId::STRING,
                "number" => TypeId::NUMBER,
                "boolean" => TypeId::BOOLEAN,
                "bigint" => TypeId::BIGINT,
                "symbol" => TypeId::SYMBOL,
                "undefined" => TypeId::UNDEFINED,
                "object" => self.db.union2(TypeId::OBJECT, TypeId::NULL),
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

    /// Narrow a type based on an instanceof check.
    ///
    /// Example: `x instanceof MyClass` narrows `A | B` to include only `A` where `A` is an instance of `MyClass`
    pub fn narrow_by_instanceof(
        &self,
        source_type: TypeId,
        constructor_type: TypeId,
        sense: bool,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_instanceof",
            source_type = source_type.0,
            constructor_type = constructor_type.0,
            sense
        )
        .entered();

        // TODO: Check for static [Symbol.hasInstance] method which overrides standard narrowing
        // TypeScript allows classes to define custom instanceof behavior via:
        //   static [Symbol.hasInstance](value: any): boolean
        // This would require evaluating method calls and type predicates, which is
        // significantly more complex than the standard construct signature approach.

        // CRITICAL: Resolve Lazy types for both source and constructor
        // This ensures type aliases are resolved to their actual types
        let resolved_source = self.resolve_type(source_type);
        let resolved_constructor = self.resolve_type(constructor_type);

        // Extract the instance type from the constructor
        use crate::solver::type_queries_extended::InstanceTypeKind;
        use crate::solver::type_queries_extended::classify_for_instance_type;

        let instance_type = match classify_for_instance_type(self.db, resolved_constructor) {
            InstanceTypeKind::Callable(shape_id) => {
                // For callable types with construct signatures, get the return type of the construct signature
                let shape = self.db.callable_shape(shape_id);
                // Find a construct signature and get its return type (the instance type)
                if let Some(construct_sig) = shape.construct_signatures.first() {
                    construct_sig.return_type
                } else {
                    // No construct signature found, can't narrow
                    trace!("No construct signature found in callable type");
                    return source_type;
                }
            }
            InstanceTypeKind::Function(shape_id) => {
                // For function types, check if it's a constructor
                let shape = self.db.function_shape(shape_id);
                if shape.is_constructor {
                    // The return type is the instance type
                    shape.return_type
                } else {
                    trace!("Function is not a constructor");
                    return source_type;
                }
            }
            InstanceTypeKind::Intersection(members) => {
                // For intersection types, we need to extract instance types from all members
                // For now, create an intersection of the instance types
                let instance_types: Vec<TypeId> = members
                    .iter()
                    .map(|&member| self.narrow_by_instanceof(source_type, member, sense))
                    .collect();

                if sense {
                    if instance_types.is_empty() {
                        TypeId::NEVER
                    } else if instance_types.len() == 1 {
                        instance_types[0]
                    } else {
                        self.db.intersection(instance_types)
                    }
                } else {
                    // For negation with intersection, we can't easily exclude
                    // Fall back to returning the source type unchanged
                    source_type
                }
            }
            InstanceTypeKind::Union(members) => {
                // For union types, extract instance types from all members
                let instance_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&member| {
                        let result = self.narrow_by_instanceof(source_type, member, sense);
                        if result != TypeId::NEVER {
                            Some(result)
                        } else {
                            None
                        }
                    })
                    .collect();

                if sense {
                    if instance_types.is_empty() {
                        TypeId::NEVER
                    } else if instance_types.len() == 1 {
                        instance_types[0]
                    } else {
                        self.db.union(instance_types)
                    }
                } else {
                    // For negation with union, we can't easily exclude
                    // Fall back to returning the source type unchanged
                    source_type
                }
            }
            InstanceTypeKind::Readonly(inner) => {
                // Readonly wrapper - extract from inner type
                return self.narrow_by_instanceof(source_type, inner, sense);
            }
            InstanceTypeKind::TypeParameter { constraint } => {
                // Follow type parameter constraint
                if let Some(constraint) = constraint {
                    return self.narrow_by_instanceof(source_type, constraint, sense);
                } else {
                    trace!("Type parameter has no constraint");
                    return source_type;
                }
            }
            InstanceTypeKind::SymbolRef(_) | InstanceTypeKind::NeedsEvaluation => {
                // Complex cases that need further evaluation
                // For now, return the source type unchanged
                trace!("Complex instance type (SymbolRef or NeedsEvaluation), returning unchanged");
                return source_type;
            }
            InstanceTypeKind::NotConstructor => {
                trace!("Constructor type is not a valid constructor");
                return source_type;
            }
        };

        // Now narrow based on the sense (positive or negative)
        if sense {
            // CRITICAL: instanceof DOES narrow any/unknown (unlike equality checks)
            if resolved_source == TypeId::ANY {
                // any narrows to the instance type with instanceof
                trace!("Narrowing any to instance type via instanceof");
                return instance_type;
            }

            if resolved_source == TypeId::UNKNOWN {
                // unknown narrows to the instance type with instanceof
                trace!("Narrowing unknown to instance type via instanceof");
                return instance_type;
            }

            // Handle Union: filter members based on instanceof relationship
            if let Some(members_id) = union_list_id(self.db, resolved_source) {
                let members = self.db.type_list(members_id);
                let filtered_members: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&member| {
                        // Check if member is assignable to instance type
                        if is_subtype_of(self.db, member, instance_type) {
                            trace!(
                                "Union member {} is assignable to instance type {}, keeping",
                                member.0,
                                instance_type.0
                            );
                            return Some(member);
                        }

                        // Check if instance type is assignable to member (subclass case)
                        // If we have a Dog and instanceof Animal, Dog is an instance of Animal
                        if is_subtype_of(self.db, instance_type, member) {
                            trace!(
                                "Instance type {} is assignable to union member {} (subclass), narrowing to instance type",
                                instance_type.0,
                                member.0
                            );
                            return Some(instance_type);
                        }

                        // Interface overlap: both are object-like but not assignable
                        // Use intersection to preserve properties from both
                        if self.are_object_like(member) && self.are_object_like(instance_type) {
                            trace!(
                                "Interface overlap between {} and {}, using intersection",
                                member.0,
                                instance_type.0
                            );
                            return Some(self.db.intersection2(member, instance_type));
                        }

                        trace!(
                            "Union member {} excluded by instanceof check",
                            member.0
                        );
                        None
                    })
                    .collect();

                if filtered_members.is_empty() {
                    trace!("All union members excluded, resulting in NEVER");
                    TypeId::NEVER
                } else if filtered_members.len() == 1 {
                    filtered_members[0]
                } else {
                    self.db.union(filtered_members)
                }
            } else {
                // Non-union type: use standard narrowing with intersection fallback
                let narrowed = self.narrow_to_type(resolved_source, instance_type);

                // If that returns NEVER, try intersection approach for interface vs class cases
                // In TypeScript, instanceof on an interface narrows to intersection, not NEVER
                if narrowed == TypeId::NEVER && resolved_source != TypeId::NEVER {
                    // Check for interface overlap before using intersection
                    if self.are_object_like(resolved_source) && self.are_object_like(instance_type)
                    {
                        trace!("Interface vs class detected, using intersection instead of NEVER");
                        self.db.intersection2(resolved_source, instance_type)
                    } else {
                        narrowed
                    }
                } else {
                    narrowed
                }
            }
        } else {
            // Negative: !(x instanceof Constructor) - exclude the instance type
            // For unions, exclude members that are subtypes of the instance type
            if let Some(members_id) = union_list_id(self.db, resolved_source) {
                let members = self.db.type_list(members_id);
                let filtered_members: Vec<TypeId> = members
                    .iter()
                    .filter(|&&member| {
                        // Exclude members that are definitely subtypes of the instance type
                        !is_subtype_of(self.db, member, instance_type)
                    })
                    .copied()
                    .collect();

                if filtered_members.is_empty() {
                    trace!("All union members excluded, resulting in NEVER");
                    TypeId::NEVER
                } else if filtered_members.len() == 1 {
                    filtered_members[0]
                } else {
                    self.db.union(filtered_members)
                }
            } else {
                // Non-union: use standard exclusion
                self.narrow_excluding_type(resolved_source, instance_type)
            }
        }
    }

    /// Check if a type is object-like (has object structure)
    ///
    /// This is used to determine if two types can form an intersection
    /// for instanceof narrowing when they're not directly assignable.
    fn are_object_like(&self, type_id: TypeId) -> bool {
        use crate::solver::types::TypeKey;

        match self.db.lookup(type_id) {
            Some(TypeKey::Object(_))
            | Some(TypeKey::ObjectWithIndex(_))
            | Some(TypeKey::Function(_))
            | Some(TypeKey::Callable(_)) => true,

            // Interface and class types (which are object-like)
            Some(TypeKey::Application(_)) => {
                // Check if the application type has construct signatures or object structure
                use crate::solver::type_queries_extended::InstanceTypeKind;
                use crate::solver::type_queries_extended::classify_for_instance_type;

                matches!(
                    classify_for_instance_type(self.db, type_id),
                    InstanceTypeKind::Callable(_) | InstanceTypeKind::Function(_)
                )
            }

            // Type parameters - check their constraint
            Some(TypeKey::TypeParameter(info)) => {
                // For instanceof, generics with object constraints are treated as object-like
                // This allows intersection narrowing for cases like: T & MyClass
                info.constraint.is_none_or(|c| self.are_object_like(c))
            }

            // Intersection of object types
            Some(TypeKey::Intersection(members)) => {
                let members = self.db.type_list(members);
                members.iter().any(|&member| self.are_object_like(member))
            }

            _ => false,
        }
    }

    /// Narrow a type based on an `in` operator check.
    ///
    /// Example: `"a" in x` narrows `A | B` to include only types that have property `a`
    pub fn narrow_by_property_presence(
        &self,
        source_type: TypeId,
        property_name: Atom,
        present: bool,
    ) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_property_presence",
            source_type = source_type.0,
            ?property_name,
            present
        )
        .entered();

        // Handle special cases
        if source_type == TypeId::ANY {
            trace!("Source type is ANY, returning unchanged");
            return TypeId::ANY;
        }

        if source_type == TypeId::NEVER {
            trace!("Source type is NEVER, returning unchanged");
            return TypeId::NEVER;
        }

        if source_type == TypeId::UNKNOWN {
            if !present {
                // False branch: property is not present. Since unknown could be anything,
                // it remains unknown in the false branch.
                trace!("UNKNOWN in false branch for in operator, returning UNKNOWN");
                return TypeId::UNKNOWN;
            }

            // For unknown, narrow to object & { [prop]: unknown }
            // This matches TypeScript's behavior where `in` check on unknown
            // narrows to object type with the property
            let prop_type = TypeId::UNKNOWN;
            let required_prop = PropertyInfo {
                name: property_name,
                type_id: prop_type,
                write_type: prop_type,
                optional: false, // Property becomes required after `in` check
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            };
            let filter_obj = self.db.object(vec![required_prop]);
            let narrowed = self.db.intersection2(TypeId::OBJECT, filter_obj);
            trace!("Narrowing unknown to object & property = {}", narrowed.0);
            return narrowed;
        }

        // If source is a union, filter members based on property presence
        if let Some(members_id) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members_id);
            trace!(
                "Checking property {} in union with {} members",
                self.db.resolve_atom_ref(property_name),
                members.len()
            );

            let matching: Vec<TypeId> = members
                .iter()
                .map(|&member| {
                    // CRITICAL: Resolve Lazy types for each member
                    let resolved_member = self.resolve_type(member);

                    let has_property = self.type_has_property(resolved_member, property_name);
                    if present {
                        // Positive: "prop" in member
                        if has_property {
                            // Property exists: Promote to required
                            let prop_type = self.get_property_type(resolved_member, property_name);
                            let required_prop = PropertyInfo {
                                name: property_name,
                                type_id: prop_type.unwrap_or(TypeId::UNKNOWN),
                                write_type: prop_type.unwrap_or(TypeId::UNKNOWN),
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            };
                            let filter_obj = self.db.object(vec![required_prop]);
                            self.db.intersection2(member, filter_obj)
                        } else {
                            // Property not found: Intersect with { prop: unknown }
                            // This handles open objects and unresolved Lazy types
                            let required_prop = PropertyInfo {
                                name: property_name,
                                type_id: TypeId::UNKNOWN,
                                write_type: TypeId::UNKNOWN,
                                optional: false,
                                readonly: false,
                                is_method: false,
                                visibility: Visibility::Public,
                                parent_id: None,
                            };
                            let filter_obj = self.db.object(vec![required_prop]);
                            self.db.intersection2(member, filter_obj)
                        }
                    } else {
                        // Negative: !("prop" in member)
                        // Exclude member ONLY if property is required
                        if self.is_property_required(resolved_member, property_name) {
                            return TypeId::NEVER;
                        }
                        // Keep member (no required property found, or property is optional)
                        member
                    }
                })
                .collect();

            if matching.is_empty() {
                trace!("No members in union, returning NEVER");
                return TypeId::NEVER;
            } else if matching.len() == 1 {
                trace!("Found single member, returning {}", matching[0].0);
                return matching[0];
            } else {
                trace!("Created union with {} members", matching.len());
                return self.db.union(matching);
            }
        }

        // For non-union types, check if the property exists
        // CRITICAL: Resolve Lazy types before checking
        let resolved_type = self.resolve_type(source_type);
        let has_property = self.type_has_property(resolved_type, property_name);

        if present {
            // Positive: "prop" in x
            if has_property {
                // Property exists: Promote to required
                let prop_type = self.get_property_type(resolved_type, property_name);
                let required_prop = PropertyInfo {
                    name: property_name,
                    type_id: prop_type.unwrap_or(TypeId::UNKNOWN),
                    write_type: prop_type.unwrap_or(TypeId::UNKNOWN),
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                };
                let filter_obj = self.db.object(vec![required_prop]);
                self.db.intersection2(source_type, filter_obj)
            } else {
                // Property not found (or Lazy type): Intersect with { prop: unknown }
                // This handles open objects and unresolved Lazy types safely
                let required_prop = PropertyInfo {
                    name: property_name,
                    type_id: TypeId::UNKNOWN,
                    write_type: TypeId::UNKNOWN,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                };
                let filter_obj = self.db.object(vec![required_prop]);
                self.db.intersection2(source_type, filter_obj)
            }
        } else {
            // Negative: !("prop" in x)
            // Exclude ONLY if property is required (not optional)
            if self.is_property_required(resolved_type, property_name) {
                return TypeId::NEVER;
            }
            // Keep source_type (no required property found, or property is optional)
            source_type
        }
    }

    /// Check if a type has a specific property.
    ///
    /// Returns true if the type has the property (required or optional),
    /// or has an index signature that would match the property.
    fn type_has_property(&self, type_id: TypeId, property_name: Atom) -> bool {
        self.get_property_type(type_id, property_name).is_some()
    }

    /// Check if a property exists and is required on a type.
    ///
    /// Returns true if the property is required (not optional).
    /// This is used for negative narrowing: `!("prop" in x)` should
    /// exclude types where `prop` is required.
    fn is_property_required(&self, type_id: TypeId, property_name: Atom) -> bool {
        let resolved_type = self.resolve_type(type_id);

        // Helper to check a specific shape
        let check_shape = |shape_id: ObjectShapeId| -> bool {
            let shape = self.db.object_shape(shape_id);
            if let Some(prop) = shape.properties.iter().find(|p| p.name == property_name) {
                return !prop.optional;
            }
            false
        };

        // Check standard object shape
        if let Some(shape_id) = object_shape_id(self.db, resolved_type) {
            if check_shape(shape_id) {
                return true;
            }
        }

        // Check object with index shape (CRITICAL for interfaces/classes)
        if let Some(shape_id) = object_with_index_shape_id(self.db, resolved_type) {
            if check_shape(shape_id) {
                return true;
            }
        }

        // Check intersection members
        // If ANY member requires it, the intersection requires it
        if let Some(members_id) = intersection_list_id(self.db, resolved_type) {
            let members = self.db.type_list(members_id);
            return members
                .iter()
                .any(|&m| self.is_property_required(m, property_name));
        }

        false
    }

    /// Get the type of a property if it exists.
    ///
    /// Returns Some(type) if the property exists, None otherwise.
    fn get_property_type(&self, type_id: TypeId, property_name: Atom) -> Option<TypeId> {
        // CRITICAL: Resolve Lazy types before checking for properties
        // This ensures type aliases are resolved to their actual types
        let resolved_type = self.resolve_type(type_id);

        // Check intersection types - property exists if ANY member has it
        if let Some(members_id) = intersection_list_id(self.db, resolved_type) {
            let members = self.db.type_list(members_id);
            // Return the type from the first member that has the property
            for &member in members.iter() {
                // Resolve each member in the intersection
                let resolved_member = self.resolve_type(member);
                if let Some(prop_type) = self.get_property_type(resolved_member, property_name) {
                    return Some(prop_type);
                }
            }
            return None;
        }

        // Check object shape
        if let Some(shape_id) = object_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);

            // Check if the property exists in the object's properties
            if let Some(prop) = shape.properties.iter().find(|p| p.name == property_name) {
                return Some(prop.type_id);
            }

            // Check index signatures
            // If the object has a string index signature, it has any string property
            if let Some(ref string_idx) = shape.string_index {
                // String index signature matches any string property
                return Some(string_idx.value_type);
            }

            // If the object has a number index signature and the property name is numeric
            if let Some(ref number_idx) = shape.number_index {
                let prop_str = self.db.resolve_atom_ref(property_name);
                if prop_str.chars().all(|c| c.is_ascii_digit()) {
                    return Some(number_idx.value_type);
                }
            }

            return None;
        }

        // Check object with index signature
        if let Some(shape_id) = object_with_index_shape_id(self.db, resolved_type) {
            let shape = self.db.object_shape(shape_id);

            // Check properties first
            if let Some(prop) = shape.properties.iter().find(|p| p.name == property_name) {
                return Some(prop.type_id);
            }

            // Check index signatures
            if let Some(ref string_idx) = shape.string_index {
                return Some(string_idx.value_type);
            }

            if let Some(ref number_idx) = shape.number_index {
                let prop_str = self.db.resolve_atom_ref(property_name);
                if prop_str.chars().all(|c| c.is_ascii_digit()) {
                    return Some(number_idx.value_type);
                }
            }

            return None;
        }

        // For other types (functions, classes, arrays, etc.), assume they don't have arbitrary properties
        // unless they have been handled above (object shapes, etc.)
        None
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

        // CRITICAL FIX: Resolve Lazy/Ref types to inspect their structure.
        // This fixes the "Missing type resolution" bug where type aliases and
        // generics weren't being narrowed correctly.
        let resolved_source = self.resolve_type(source_type);

        // Gracefully handle resolution failures: if evaluation fails but the input
        // wasn't ERROR, we can't narrow structurally. Return original source to
        // avoid cascading ERRORs through the type system.
        if resolved_source == TypeId::ERROR && source_type != TypeId::ERROR {
            trace!("Source type resolution failed, returning original source");
            return source_type;
        }

        // Resolve target for consistency
        let resolved_target = self.resolve_type(target_type);
        if resolved_target == TypeId::ERROR && target_type != TypeId::ERROR {
            trace!("Target type resolution failed, returning original source");
            return source_type;
        }

        // If source is the target, return it
        if resolved_source == resolved_target {
            trace!("Source type equals target type, returning unchanged");
            return source_type;
        }

        // Special case: unknown can be narrowed to any type through type guards
        // This handles cases like: if (typeof x === "string") where x: unknown
        if resolved_source == TypeId::UNKNOWN {
            trace!("Narrowing unknown to specific type via type guard");
            return target_type;
        }

        // Special case: any can be narrowed to any type through type guards
        // This handles cases like: if (x === null) where x: any
        // CRITICAL: Unlike unknown, any MUST be narrowed to match target type
        if resolved_source == TypeId::ANY {
            trace!("Narrowing any to specific type via type guard");
            return target_type;
        }

        // If source is a union, filter members
        // Use resolved_source for structural inspection
        if let Some(members) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members);
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
                    // CRITICAL FIX: Check if target_type is a subtype of member
                    // This handles cases like narrowing string | number by "hello"
                    // where "hello" is a subtype of string, so we should narrow to "hello"
                    if crate::solver::subtype::is_subtype_of_with_db(self.db, target_type, member) {
                        return Some(target_type);
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
                return self.db.union(matching);
            }
        }

        // Check if this is a type parameter that needs narrowing
        // Use resolved_source to handle type parameters behind aliases
        if let Some(narrowed) = self.narrow_type_param(resolved_source, target_type) {
            trace!("Narrowed type parameter to {}", narrowed.0);
            return narrowed;
        }

        // Task 13: Handle boolean -> literal narrowing
        // When narrowing boolean to true or false, return the corresponding literal
        if resolved_source == TypeId::BOOLEAN {
            let is_target_true = if let Some(lit) = literal_value(self.db, resolved_target) {
                matches!(lit, LiteralValue::Boolean(true))
            } else {
                resolved_target == TypeId::BOOLEAN_TRUE
            };

            if is_target_true {
                trace!("Narrowing boolean to true");
                return TypeId::BOOLEAN_TRUE;
            }

            let is_target_false = if let Some(lit) = literal_value(self.db, resolved_target) {
                matches!(lit, LiteralValue::Boolean(false))
            } else {
                resolved_target == TypeId::BOOLEAN_FALSE
            };

            if is_target_false {
                trace!("Narrowing boolean to false");
                return TypeId::BOOLEAN_FALSE;
            }
        }

        // Check if source is assignable to target using resolved types for comparison
        if self.is_assignable_to(resolved_source, resolved_target) {
            trace!("Source type is assignable to target, returning source");
            source_type
        } else if crate::solver::subtype::is_subtype_of_with_db(
            self.db,
            resolved_target,
            resolved_source,
        ) {
            // CRITICAL FIX: Check if target is a subtype of source (reverse narrowing)
            // This handles cases like narrowing string to "hello" where "hello" is a subtype of string
            // The inference engine uses this to narrow upper bounds by lower bounds
            trace!("Target is subtype of source, returning target");
            target_type
        } else {
            trace!("Source type is not assignable to target, returning NEVER");
            TypeId::NEVER
        }
    }

    /// Narrow a type to exclude members assignable to target.
    pub fn narrow_excluding_type(&self, source_type: TypeId, excluded_type: TypeId) -> TypeId {
        if let Some(members) = intersection_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
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
            return self.db.intersection(narrowed_members);
        }

        // If source is a union, filter out matching members
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    if intersection_list_id(self.db, member).is_some() {
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
                return self.db.union(remaining);
            }
        }

        if let Some(narrowed) = self.narrow_type_param_excluding(source_type, excluded_type) {
            return narrowed;
        }

        // Special case: boolean type (treat as true | false union)
        // Task 13: Fix Boolean Narrowing Logic
        // When excluding true or false from boolean, return the other literal
        // When excluding both true and false from boolean, return never
        if source_type == TypeId::BOOLEAN
            || source_type == TypeId::BOOLEAN_TRUE
            || source_type == TypeId::BOOLEAN_FALSE
        {
            // Check if excluded_type is a boolean literal
            let is_excluding_true = if let Some(lit) = literal_value(self.db, excluded_type) {
                matches!(lit, LiteralValue::Boolean(true))
            } else {
                excluded_type == TypeId::BOOLEAN_TRUE
            };

            let is_excluding_false = if let Some(lit) = literal_value(self.db, excluded_type) {
                matches!(lit, LiteralValue::Boolean(false))
            } else {
                excluded_type == TypeId::BOOLEAN_FALSE
            };

            // Handle exclusion from boolean, true, or false
            if source_type == TypeId::BOOLEAN {
                if is_excluding_true {
                    // Excluding true from boolean -> return false
                    return TypeId::BOOLEAN_FALSE;
                } else if is_excluding_false {
                    // Excluding false from boolean -> return true
                    return TypeId::BOOLEAN_TRUE;
                }
                // If excluding BOOLEAN, let the final is_assignable_to check handle it below
            } else if source_type == TypeId::BOOLEAN_TRUE {
                if is_excluding_true {
                    // Excluding true from true -> return never
                    return TypeId::NEVER;
                }
                // For other cases (e.g., excluding BOOLEAN from TRUE),
                // let the final is_assignable_to check handle it below
            } else if source_type == TypeId::BOOLEAN_FALSE {
                if is_excluding_false {
                    // Excluding false from false -> return never
                    return TypeId::NEVER;
                }
                // For other cases, let the final is_assignable_to check handle it below
            }
            // CRITICAL: Do NOT return source_type here.
            // Fall through to the standard is_assignable_to check below.
            // This handles edge cases like narrow_excluding_type(TRUE, BOOLEAN) -> NEVER
        }

        // If source is assignable to excluded, return never
        if self.is_assignable_to(source_type, excluded_type) {
            TypeId::NEVER
        } else {
            source_type
        }
    }

    /// Narrow a type by excluding multiple types at once (batched version).
    ///
    /// This is an optimized version of `narrow_excluding_type` for cases like
    /// switch default clauses where we need to exclude many types at once.
    /// It avoids creating intermediate union types and reduces complexity from O(N²) to O(N).
    ///
    /// # Arguments
    /// * `source_type` - The type to narrow (typically a union)
    /// * `excluded_types` - Types to exclude from the source
    ///
    /// # Returns
    /// The narrowed type with all excluded types removed
    pub fn narrow_excluding_types(&self, source_type: TypeId, excluded_types: &[TypeId]) -> TypeId {
        if excluded_types.is_empty() {
            return source_type;
        }

        // For small lists, use sequential narrowing (avoids HashSet overhead)
        if excluded_types.len() <= 4 {
            let mut result = source_type;
            for &excluded in excluded_types {
                result = self.narrow_excluding_type(result, excluded);
                if result == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }
            return result;
        }

        // For larger lists, use HashSet for O(1) lookup
        let excluded_set: rustc_hash::FxHashSet<TypeId> = excluded_types.iter().copied().collect();

        // Handle union source type
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    // Fast path: direct identity check against the set
                    if excluded_set.contains(&member) {
                        return None;
                    }

                    // Handle intersection members
                    if intersection_list_id(self.db, member).is_some() {
                        let narrowed = self.narrow_excluding_types(member, excluded_types);
                        if narrowed == TypeId::NEVER {
                            return None;
                        }
                        return Some(narrowed);
                    }

                    // Handle type parameters
                    if let Some(narrowed) =
                        self.narrow_type_param_excluding_set(member, &excluded_set)
                    {
                        if narrowed == TypeId::NEVER {
                            return None;
                        }
                        return Some(narrowed);
                    }

                    // Slow path: check assignability for complex cases
                    // This handles cases where the member isn't identical to an excluded type
                    // but might still be assignable to one (e.g., literal subtypes)
                    for &excluded in &excluded_set {
                        if self.is_assignable_to(member, excluded) {
                            return None;
                        }
                    }
                    Some(member)
                })
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            } else {
                return self.db.union(remaining);
            }
        }

        // Handle single type (not a union)
        if excluded_set.contains(&source_type) {
            return TypeId::NEVER;
        }

        // Check assignability for single type
        for &excluded in &excluded_set {
            if self.is_assignable_to(source_type, excluded) {
                return TypeId::NEVER;
            }
        }

        source_type
    }

    /// Helper for narrow_excluding_types with type parameters
    fn narrow_type_param_excluding_set(
        &self,
        source: TypeId,
        excluded_set: &rustc_hash::FxHashSet<TypeId>,
    ) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint?;
        if constraint == source || constraint == TypeId::UNKNOWN {
            return None;
        }

        // Narrow the constraint by excluding all types in the set
        let excluded_vec: Vec<TypeId> = excluded_set.iter().copied().collect();
        let narrowed_constraint = self.narrow_excluding_types(constraint, &excluded_vec);

        if narrowed_constraint == constraint {
            return None;
        }
        if narrowed_constraint == TypeId::NEVER {
            return Some(TypeId::NEVER);
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    /// Narrow to function types only.
    fn narrow_to_function(&self, source_type: TypeId) -> TypeId {
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
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
                return self.db.union(functions);
            }
        }

        if let Some(narrowed) = self.narrow_type_param_to_function(source_type) {
            return narrowed;
        }

        if self.is_function_type(source_type) {
            source_type
        } else if source_type == TypeId::OBJECT {
            self.function_type()
        } else if let Some(shape_id) = object_shape_id(self.db, source_type) {
            let shape = self.db.object_shape(shape_id);
            if shape.properties.is_empty() {
                self.function_type()
            } else {
                TypeId::NEVER
            }
        } else if let Some(shape_id) = object_with_index_shape_id(self.db, source_type) {
            let shape = self.db.object_shape(shape_id);
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
        is_literal_type_db(self.db, type_id)
    }

    /// Check if a type is a function type.
    /// Uses the visitor pattern from solver::visitor.
    fn is_function_type(&self, type_id: TypeId) -> bool {
        is_function_type_db(self.db, type_id)
    }

    /// Narrow a type to exclude function-like members (typeof !== "function").
    pub fn narrow_excluding_function(&self, source_type: TypeId) -> TypeId {
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
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
                return self.db.union(remaining);
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
        is_object_like_type_db(self.db, type_id)
    }

    fn narrow_type_param(&self, source: TypeId, target: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

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

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_to_function(&self, source: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

        let constraint = info.constraint.unwrap_or(TypeId::UNKNOWN);
        if constraint == source || constraint == TypeId::UNKNOWN {
            let function_type = self.function_type();
            return Some(self.db.intersection2(source, function_type));
        }

        let narrowed_constraint = self.narrow_to_function(constraint);
        if narrowed_constraint == TypeId::NEVER {
            return None;
        }

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_excluding(&self, source: TypeId, excluded: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

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

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    fn narrow_type_param_excluding_function(&self, source: TypeId) -> Option<TypeId> {
        let info = type_param_info(self.db, source)?;

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

        Some(self.db.intersection2(source, narrowed_constraint))
    }

    pub(crate) fn function_type(&self) -> TypeId {
        let rest_array = self.db.array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.db.function(FunctionShape {
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
        if let Some(lit) = literal_value(self.db, source) {
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

        if let Some(members) = intersection_list_id(self.db, source) {
            let members = self.db.type_list(members);
            if members
                .iter()
                .any(|member| self.is_assignable_to(*member, target))
            {
                return true;
            }
        }

        if target == TypeId::STRING && template_literal_id(self.db, source).is_some() {
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

            TypeGuard::Instanceof(instance_type) => {
                if sense {
                    // Positive: x instanceof Class
                    // CRITICAL: The payload is already the Instance Type (extracted by Checker)
                    // We narrow to it directly using narrow_to_type, not narrow_by_instanceof
                    // which would try to extract the instance type again from a constructor.
                    let narrowed = self.narrow_to_type(source_type, *instance_type);

                    // Fallback: If standard narrowing returns NEVER but source wasn't NEVER,
                    // it might be an interface vs class check (which is allowed in TS).
                    // Use intersection in that case.
                    if narrowed == TypeId::NEVER && source_type != TypeId::NEVER {
                        self.db.intersection2(source_type, *instance_type)
                    } else {
                        narrowed
                    }
                } else {
                    // Negative: !(x instanceof Class)
                    // Exclude the instance type
                    self.narrow_excluding_type(source_type, *instance_type)
                }
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
                    self.db.union(vec![TypeId::NULL, TypeId::UNDEFINED])
                } else {
                    // Inequality: exclude null and undefined
                    let without_null = self.narrow_excluding_type(source_type, TypeId::NULL);
                    self.narrow_excluding_type(without_null, TypeId::UNDEFINED)
                }
            }

            TypeGuard::Truthy => {
                if sense {
                    // Truthy: remove null and undefined (TypeScript doesn't narrow other falsy values)
                    self.narrow_by_truthiness(source_type)
                } else {
                    // Falsy: narrow to the falsy component(s)
                    // This handles cases like: if (!x) where x: string → "" in false branch
                    self.narrow_to_falsy(source_type)
                }
            }

            TypeGuard::Discriminant {
                property_path,
                value_type,
            } => {
                if sense {
                    // Discriminant matches: narrow to matching union members
                    self.narrow_by_discriminant(source_type, property_path, *value_type)
                } else {
                    // Discriminant doesn't match: exclude matching union members
                    self.narrow_by_excluding_discriminant(source_type, property_path, *value_type)
                }
            }

            TypeGuard::InProperty(property_name) => {
                if sense {
                    // Positive: "prop" in x - narrow to types that have the property
                    self.narrow_by_property_presence(source_type, *property_name, true)
                } else {
                    // Negative: !("prop" in x) - narrow to types that don't have the property
                    self.narrow_by_property_presence(source_type, *property_name, false)
                }
            }

            TypeGuard::Predicate { type_id, asserts } => {
                match type_id {
                    Some(target_type) => {
                        // Type guard with specific type: is T or asserts T
                        if sense {
                            // True branch: narrow to the target type
                            self.narrow_to_type(source_type, *target_type)
                        } else if *asserts {
                            // CRITICAL: For assertion functions, the false branch is unreachable
                            // (the function throws if the assertion fails), so we don't narrow
                            source_type
                        } else {
                            // False branch for regular type guards: exclude the target type
                            self.narrow_excluding_type(source_type, *target_type)
                        }
                    }
                    None => {
                        // Truthiness assertion: asserts x
                        // Behaves like TypeGuard::Truthy (narrows to truthy in true branch)
                        if *asserts {
                            self.narrow_by_truthiness(source_type)
                        } else {
                            source_type
                        }
                    }
                }
            }

            TypeGuard::Array => {
                if sense {
                    // Positive: Array.isArray(x) - narrow to array-like types
                    self.narrow_to_array(source_type)
                } else {
                    // Negative: !Array.isArray(x) - exclude array-like types
                    self.narrow_excluding_array(source_type)
                }
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

    /// Check if a type is definitely falsy.
    ///
    /// Returns true for: null, undefined, void, false, 0, -0, NaN, "", 0n
    fn is_definitely_falsy(&self, type_id: TypeId) -> bool {
        let resolved = self.resolve_type(type_id);

        // 1. Check intrinsics that are always falsy
        if matches!(resolved, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID) {
            return true;
        }

        // 2. Check literals
        if let Some(lit) = literal_value(self.db, resolved) {
            return match lit {
                LiteralValue::Boolean(false) => true,
                LiteralValue::Number(n) => n.0 == 0.0 || n.0.is_nan(), // Handles 0, -0, and NaN
                LiteralValue::String(atom) => self.db.resolve_atom_ref(atom).is_empty(), // Handles ""
                LiteralValue::BigInt(atom) => self.db.resolve_atom_ref(atom).as_ref() == "0", // Handles 0n
                _ => false,
            };
        }

        false
    }

    /// Narrow a type by removing definitely falsy values (truthiness check).
    ///
    /// Narrow a type to its falsy component(s).
    ///
    /// This is used for the false branch of truthiness checks (e.g., `if (!x)`).
    /// Returns the union of all falsy values that the type could be.
    ///
    /// Falsy values in TypeScript:
    /// - null, undefined, void
    /// - false (boolean literal)
    /// - 0, -0, NaN (number literals)
    /// - "" (empty string)
    /// - 0n (bigint literal)
    ///
    /// CRITICAL: TypeScript does NOT narrow primitive types in falsy branches.
    /// For `boolean`, `number`, `string`, and `bigint`, they stay as their primitive type.
    /// For `unknown`, TypeScript does NOT narrow in falsy branches.
    ///
    /// Only literal types are narrowed (e.g., `0 | 1` -> `0`, `true | false` -> `false`).
    pub fn narrow_to_falsy(&self, type_id: TypeId) -> TypeId {
        let _span = span!(Level::TRACE, "narrow_to_falsy", type_id = type_id.0).entered();

        // Handle ANY - suppresses all narrowing
        if type_id == TypeId::ANY {
            return TypeId::ANY;
        }

        // Handle UNKNOWN - TypeScript does NOT narrow unknown in falsy branches
        if type_id == TypeId::UNKNOWN {
            return TypeId::UNKNOWN;
        }

        let resolved = self.resolve_type(type_id);

        // Handle Unions - recursively narrow each member and collect falsy components
        if let UnionMembersKind::Union(members) = classify_for_union_members(self.db, resolved) {
            let falsy_members: Vec<TypeId> = members
                .iter()
                .map(|&m| self.narrow_to_falsy(m))
                .filter(|&m| m != TypeId::NEVER)
                .collect();

            return if falsy_members.is_empty() {
                TypeId::NEVER
            } else if falsy_members.len() == 1 {
                falsy_members[0]
            } else {
                self.db.union(falsy_members)
            };
        }

        // Handle primitive types
        // CRITICAL: TypeScript does NOT narrow these primitives in falsy branches
        if matches!(
            resolved,
            TypeId::BOOLEAN | TypeId::STRING | TypeId::NUMBER | TypeId::BIGINT
        ) {
            return resolved;
        }
        if matches!(resolved, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID) {
            return resolved;
        }

        // Handle literals - check if they're falsy
        // This correctly handles `0` vs `1`, `""` vs `"a"`, `NaN` vs other numbers,
        // `true` vs `false`, etc.
        if let Some(_lit) = literal_value(self.db, resolved) {
            if self.is_definitely_falsy(resolved) {
                return type_id;
            }
        }

        TypeId::NEVER
    }

    /// This matches TypeScript's behavior where `if (x)` narrows out:
    /// - null, undefined, void
    /// - false (boolean literal)
    /// - 0, -0, NaN (number literals)
    /// - "" (empty string)
    /// - 0n (bigint literal)
    fn narrow_by_truthiness(&self, source_type: TypeId) -> TypeId {
        let _span = span!(
            Level::TRACE,
            "narrow_by_truthiness",
            source_type = source_type.0
        )
        .entered();

        // Handle special cases
        if source_type == TypeId::ANY {
            return source_type;
        }

        // CRITICAL FIX: unknown in truthy branch narrows to exclude null/undefined
        // TypeScript: if (x: unknown) { x } -> x is not null | undefined
        if source_type == TypeId::UNKNOWN {
            let narrowed = self.narrow_excluding_type(source_type, TypeId::NULL);
            return self.narrow_excluding_type(narrowed, TypeId::UNDEFINED);
        }

        let resolved = self.resolve_type(source_type);

        // Handle Intersections (recursive)
        // CRITICAL: If ANY part of intersection is falsy, the WHOLE intersection is falsy
        if let Some(members_id) = intersection_list_id(self.db, resolved) {
            let members = self.db.type_list(members_id);
            let mut narrowed_members = Vec::with_capacity(members.len());

            for &m in members.iter() {
                let narrowed = self.narrow_by_truthiness(m);
                // If any part is NEVER, the whole intersection is impossible
                if narrowed == TypeId::NEVER {
                    return TypeId::NEVER;
                }
                narrowed_members.push(narrowed);
            }

            if narrowed_members.len() == 1 {
                return narrowed_members[0];
            } else {
                return self.db.intersection(narrowed_members);
            }
        }

        // Handle Unions (filter out falsy members)
        if let Some(members_id) = union_list_id(self.db, resolved) {
            let members = self.db.type_list(members_id);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let narrowed = self.narrow_by_truthiness(m);
                    if narrowed == TypeId::NEVER {
                        None
                    } else {
                        Some(narrowed)
                    }
                })
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            } else {
                return self.db.union(remaining);
            }
        }

        // Base Case: Check if definitely falsy
        if self.is_definitely_falsy(source_type) {
            return TypeId::NEVER;
        }

        // Handle boolean -> true (TypeScript narrows boolean in truthy checks)
        if resolved == TypeId::BOOLEAN {
            return TypeId::BOOLEAN_TRUE;
        }

        // Handle Type Parameters (check constraint)
        if let Some(info) = type_param_info(self.db, resolved) {
            if let Some(constraint) = info.constraint {
                let narrowed_constraint = self.narrow_by_truthiness(constraint);
                if narrowed_constraint == TypeId::NEVER {
                    return TypeId::NEVER;
                }
                // If constraint narrowed, intersect source with it
                if narrowed_constraint != constraint {
                    return self.db.intersection2(source_type, narrowed_constraint);
                }
            }
        }

        source_type
    }

    /// Narrows a type by another type using the Visitor pattern.
    ///
    /// This is the general-purpose narrowing function that implements the
    /// Solver-First architecture (North Star Section 3.1). The Checker
    /// identifies WHERE narrowing happens (AST nodes) and the Solver
    /// calculates the RESULT.
    ///
    /// # Arguments
    /// * `type_id` - The type to narrow (e.g., a union type)
    /// * `narrower` - The type to narrow by (e.g., a literal type)
    ///
    /// # Returns
    /// The narrowed type. For unions, filters to members assignable to narrower.
    /// For type parameters, intersects with narrower.
    ///
    /// # Examples
    /// - `narrow("A" | "B", "A")` → `"A"`
    /// - `narrow(string | number, "hello")` → `"hello"`
    /// - `narrow(T | null, undefined)` → `null` (filters out T)
    pub fn narrow(&self, type_id: TypeId, narrower: TypeId) -> TypeId {
        // Fast path: already a subtype
        if is_subtype_of(self.db, type_id, narrower) {
            return type_id;
        }

        // Use visitor to perform narrowing
        let mut visitor = NarrowingVisitor {
            db: self.db,
            narrower,
        };
        visitor.visit_type(self.db, type_id)
    }

    /// Task 10: Narrow a type to only array-like types.
    ///
    /// Used for `Array.isArray(x)` in the true branch.
    /// Keeps only arrays, tuples, and readonly arrays - preserves element types.
    ///
    /// # Examples
    /// - `narrow_to_array(string[] | number)` → `string[]`
    /// - `narrow_to_array(unknown)` → `any[]`
    /// - `narrow_to_array(any)` → `any`
    /// - `narrow_to_array(readonly [number, string])` → `readonly [number, string]`
    fn narrow_to_array(&self, source_type: TypeId) -> TypeId {
        // Handle ANY and UNKNOWN first
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }

        if source_type == TypeId::UNKNOWN {
            // Unknown narrows to any[] (most general array type)
            return self.db.array(TypeId::ANY);
        }

        // Handle Union: filter members, keeping only array-like types
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let array_like: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    let narrowed = self.narrow_to_array(member);
                    if narrowed == TypeId::NEVER {
                        None
                    } else {
                        Some(narrowed)
                    }
                })
                .collect();

            if array_like.is_empty() {
                return TypeId::NEVER;
            } else if array_like.len() == 1 {
                return array_like[0];
            } else {
                return self.db.union(array_like);
            }
        }

        // Handle Intersections: if ANY member is array-like, the whole intersection is array-like
        // e.g., string[] & { foo: string } is an array-like type
        if let Some(members_id) = intersection_list_id(self.db, source_type) {
            let members = self.db.type_list(members_id);
            let is_array = members.iter().any(|&m| {
                let resolved = self.resolve_type(m);
                self.is_array_like(resolved) || self.narrow_to_array(resolved) != TypeId::NEVER
            });

            if is_array {
                return source_type;
            }
        }

        // Handle Type Parameters: intersect with any[]
        if let Some(_info) = type_param_info(self.db, source_type) {
            let any_array = self.db.array(TypeId::ANY);
            return self.db.intersection2(source_type, any_array);
        }

        // Check if type is array-like (Array, Tuple, or ReadonlyArray)
        if self.is_array_like(source_type) {
            return source_type;
        }

        // Not array-like
        TypeId::NEVER
    }

    /// Task 10: Exclude array-like types from a type.
    ///
    /// Used for `!Array.isArray(x)` in the false branch.
    /// Removes arrays, tuples, and readonly arrays.
    ///
    /// # Examples
    /// - `narrow_excluding_array(string[] | number)` → `number`
    /// - `narrow_excluding_array(string[])` → `NEVER`
    /// - `narrow_excluding_array(unknown)` → `unknown`
    fn narrow_excluding_array(&self, source_type: TypeId) -> TypeId {
        // Handle ANY and UNKNOWN
        if source_type == TypeId::ANY {
            return TypeId::ANY;
        }

        if source_type == TypeId::UNKNOWN {
            // Unknown doesn't have a "not array" type representation
            return TypeId::UNKNOWN;
        }

        // Handle Union: filter out array-like members
        if let Some(members) = union_list_id(self.db, source_type) {
            let members = self.db.type_list(members);
            let non_array: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    let narrowed = self.narrow_excluding_array(member);
                    if narrowed == TypeId::NEVER {
                        None
                    } else {
                        Some(narrowed)
                    }
                })
                .collect();

            if non_array.is_empty() {
                return TypeId::NEVER;
            } else if non_array.len() == 1 {
                return non_array[0];
            } else {
                return self.db.union(non_array);
            }
        }

        // Handle Type Parameters: check if constraint is definitely an array
        // e.g., if T extends string[] and we check !Array.isArray(x), then x is never
        if let Some(info) = type_param_info(self.db, source_type) {
            if let Some(constraint) = info.constraint {
                // If the constraint is definitely an array, then T is definitely an array.
                // So !Array.isArray(T) is NEVER.
                let narrowed_constraint = self.narrow_excluding_array(constraint);
                if narrowed_constraint == TypeId::NEVER {
                    return TypeId::NEVER;
                }
            }
        }

        // If array-like, return NEVER (excluded)
        if self.is_array_like(source_type) {
            return TypeId::NEVER;
        }

        // Not array-like, keep as-is
        source_type
    }

    /// Check if a type is array-like (Array, Tuple, or ReadonlyArray).
    ///
    /// This unwraps ReadonlyType recursively to check the underlying type.
    fn is_array_like(&self, type_id: TypeId) -> bool {
        use crate::solver::type_queries;

        // Check for ReadonlyType wrapper (unwrap recursively)
        if let Some(TypeKey::ReadonlyType(inner)) = self.db.lookup(type_id) {
            return self.is_array_like(inner);
        }

        // Check if type is Array, Tuple, or ReadonlyArray (wrapped)
        type_queries::is_array_type(self.db, type_id)
            || type_queries::is_tuple_type(self.db, type_id)
    }
}

/// Visitor that narrows a type by filtering/intersecting with a narrower type.
struct NarrowingVisitor<'a> {
    db: &'a dyn QueryDatabase,
    narrower: TypeId,
}

impl<'a> TypeVisitor for NarrowingVisitor<'a> {
    type Output = TypeId;

    fn visit_intrinsic(&mut self, kind: IntrinsicKind) -> Self::Output {
        match kind {
            IntrinsicKind::Any => {
                // Narrowing `any` by anything returns that type
                self.narrower
            }
            IntrinsicKind::Unknown => {
                // Narrowing `unknown` by anything returns that type
                self.narrower
            }
            IntrinsicKind::Never => {
                // Never stays never
                TypeId::NEVER
            }
            _ => {
                // For other intrinsics, we need to handle the overlap case
                // Narrowing primitive by primitive is effectively intersection
                let type_id = TypeId(kind as u32);

                // Case 1: narrower is subtype of type_id (e.g., narrow(string, "foo"))
                // Result: narrower
                if is_subtype_of(self.db, self.narrower, type_id) {
                    self.narrower
                }
                // Case 2: type_id is subtype of narrower (e.g., narrow("foo", string))
                // Result: type_id (the original)
                else if is_subtype_of(self.db, type_id, self.narrower) {
                    type_id
                }
                // Case 3: Disjoint types (e.g., narrow(string, number))
                // Result: never
                else {
                    TypeId::NEVER
                }
            }
        }
    }

    fn visit_literal(&mut self, _value: &LiteralValue) -> Self::Output {
        // For literal types, check if assignable to narrower
        // The literal type_id will be constructed and checked
        // For now, return the narrower (will be refined with actual type_id)
        self.narrower
    }

    fn visit_union(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));

        // CRITICAL: Recursively narrow each union member, don't just check subtype
        // This handles cases like: string narrowed by "foo" -> "foo"
        // where "foo" is NOT a subtype of string, but string contains "foo"
        let filtered: Vec<TypeId> = members
            .iter()
            .filter_map(|&member| {
                let narrowed = self.visit_type(self.db, member);
                if narrowed == TypeId::NEVER {
                    None
                } else {
                    Some(narrowed)
                }
            })
            .collect();

        if filtered.is_empty() {
            TypeId::NEVER
        } else if filtered.len() == members.len() {
            // All members matched - reconstruct the union
            self.db.union(filtered)
        } else if filtered.len() == 1 {
            filtered[0]
        } else {
            self.db.union(filtered)
        }
    }

    fn visit_intersection(&mut self, list_id: u32) -> Self::Output {
        let members = self.db.type_list(TypeListId(list_id));

        // For intersection, we need to check if ALL members are assignable to narrower
        let all_match = members
            .iter()
            .all(|&member| is_subtype_of(self.db, member, self.narrower));

        if all_match {
            // Intersection matches narrower, return the intersection
            // We need to reconstruct the intersection type
            self.db.intersection(members.to_vec())
        } else {
            // Intersection doesn't fully match - need to intersect with narrower
            // For now, conservatively return the intersection as-is
            // TODO: Implement proper intersection narrowing
            self.db.intersection(members.to_vec())
        }
    }

    fn visit_type_parameter(&mut self, info: &TypeParamInfo) -> Self::Output {
        // For type parameters, intersect with the narrower
        // This constrains the generic type variable
        if let Some(constraint) = info.constraint {
            self.db.intersection2(constraint, self.narrower)
        } else {
            // No constraint, so narrowing gives us the narrower
            self.narrower
        }
    }

    fn visit_lazy(&mut self, _def_id: u32) -> Self::Output {
        // CRITICAL: Must resolve lazy types before narrowing
        // For now, conservatively return narrower (may over-narrow but safe)
        // TODO: Track current type_id to resolve and recurse properly
        // The def_id corresponds to the lazy type being visited
        self.narrower
    }

    fn visit_ref(&mut self, _symbol_ref: u32) -> Self::Output {
        // CRITICAL: Must resolve ref types before narrowing
        // For now, conservatively return narrower (may over-narrow but safe)
        // TODO: Track current type_id to resolve and recurse properly
        self.narrower
    }

    fn visit_application(&mut self, _app_id: u32) -> Self::Output {
        // CRITICAL: Must resolve application types before narrowing
        // For now, conservatively return narrower (may over-narrow but safe)
        // TODO: Track current type_id to resolve and recurse properly
        self.narrower
    }

    fn visit_object(&mut self, _shape_id: u32) -> Self::Output {
        // For object types, conservatively return the narrower
        // (Proper narrowing would check property compatibility)
        self.narrower
    }

    fn visit_function(&mut self, _shape_id: u32) -> Self::Output {
        // For function types, conservatively return the narrower
        self.narrower
    }

    fn visit_callable(&mut self, _shape_id: u32) -> Self::Output {
        // For callable types, conservatively return the narrower
        self.narrower
    }

    fn visit_tuple(&mut self, _list_id: u32) -> Self::Output {
        // For tuple types, conservatively return the narrower
        self.narrower
    }

    fn visit_array(&mut self, _element_type: TypeId) -> Self::Output {
        // For array types, conservatively return the narrower
        self.narrower
    }

    fn default_output() -> Self::Output {
        // Fallback for types not explicitly handled above
        // Conservative: return never (type doesn't match the narrower)
        // This is safe because:
        // - For unions, this member will be excluded from the filtered result
        // - For other contexts, never means "no match"
        TypeId::NEVER
    }
}

/// Convenience function for finding discriminants.
pub fn find_discriminants(
    interner: &dyn QueryDatabase,
    union_type: TypeId,
) -> Vec<DiscriminantInfo> {
    let ctx = NarrowingContext::new(interner);
    ctx.find_discriminants(union_type)
}

/// Convenience function for narrowing by discriminant.
pub fn narrow_by_discriminant(
    interner: &dyn QueryDatabase,
    union_type: TypeId,
    property_path: &[Atom],
    literal_value: TypeId,
) -> TypeId {
    let ctx = NarrowingContext::new(interner);
    ctx.narrow_by_discriminant(union_type, property_path, literal_value)
}

/// Convenience function for typeof narrowing.
pub fn narrow_by_typeof(
    interner: &dyn QueryDatabase,
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
