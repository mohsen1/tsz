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
//! ## `TypeGuard` Abstraction
//!
//! The `TypeGuard` enum provides an AST-agnostic representation of narrowing
//! conditions. This allows the Solver to perform pure type algebra without
//! depending on AST nodes.
//!
//! Architecture:
//! - **Checker**: Extracts `TypeGuard` from AST nodes (WHERE)
//! - **Solver**: Applies `TypeGuard` to types (WHAT)

// Re-export utility functions that were extracted to narrowing_utils
pub use crate::narrowing_utils::{
    can_be_nullish, find_discriminants, is_definitely_nullish, is_nullish_type,
    narrow_by_discriminant, narrow_by_typeof, remove_definitely_falsy_types, remove_nullish,
    split_nullish_type, type_contains_nullish, type_contains_undefined,
};

use crate::subtype::{SubtypeChecker, TypeResolver};
use crate::type_queries::{UnionMembersKind, classify_for_union_members};
#[cfg(test)]
use crate::types::*;
use crate::types::{FunctionShape, LiteralValue, ParamInfo, TypeData, TypeId};
use crate::utils::{TypeIdExt, intersection_or_single, union_or_single};
use crate::visitor::{
    index_access_parts, intersection_list_id, is_function_type_db, is_object_like_type_db,
    lazy_def_id, literal_value, object_shape_id, object_with_index_shape_id, template_literal_id,
    type_param_info, union_list_id,
};
use crate::{QueryDatabase, TypeDatabase};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use tracing::{Level, span, trace};
use tsz_common::interner::Atom;

/// The result of a `typeof` expression, restricted to the 8 standard JavaScript types.
///
/// Using an enum instead of `String` eliminates heap allocation per typeof guard.
/// TypeScript's `typeof` operator only returns these 8 values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeofKind {
    String,
    Number,
    Boolean,
    BigInt,
    Symbol,
    Undefined,
    Object,
    Function,
}

impl TypeofKind {
    /// Parse a typeof result string into a `TypeofKind`.
    /// Returns None for non-standard typeof strings (which don't narrow).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "string" => Some(Self::String),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            "bigint" => Some(Self::BigInt),
            "symbol" => Some(Self::Symbol),
            "undefined" => Some(Self::Undefined),
            "object" => Some(Self::Object),
            "function" => Some(Self::Function),
            _ => None,
        }
    }

    /// Get the string representation of this typeof kind.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::BigInt => "bigint",
            Self::Symbol => "symbol",
            Self::Undefined => "undefined",
            Self::Object => "object",
            Self::Function => "function",
        }
    }
}

/// AST-agnostic representation of a type narrowing condition.
///
/// This enum represents various guards that can narrow a type, without
/// depending on AST nodes like `NodeIndex` or `SyntaxKind`.
///
/// # Examples
/// ```typescript
/// typeof x === "string"     -> TypeGuard::Typeof(TypeofKind::String)
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
    /// For example, narrowing `string | number` with `Typeof(TypeofKind::String)` yields `string`.
    Typeof(TypeofKind),

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

    /// `array.every(predicate)` where predicate has type predicate
    ///
    /// Narrows an array's element type based on a type predicate.
    ///
    /// # Examples
    /// ```typescript
    /// const arr: (number | string)[] = ['aaa'];
    /// const isString = (x: unknown): x is string => typeof x === 'string';
    /// if (arr.every(isString)) {
    ///   arr; // string[] (element type narrowed from number | string to string)
    /// }
    /// ```
    ///
    /// This only applies to arrays. For non-array types, the type is unchanged.
    ArrayElementPredicate {
        /// The type to narrow array elements to
        element_type: TypeId,
    },
}

#[inline]
pub(crate) fn union_or_single_preserve(db: &dyn TypeDatabase, types: Vec<TypeId>) -> TypeId {
    match types.len() {
        0 => TypeId::NEVER,
        1 => types[0],
        _ => db.union_from_sorted_vec(types),
    }
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
/// Shared across multiple narrowing contexts to persist resolution results.
#[derive(Default, Clone, Debug)]
pub struct NarrowingCache {
    /// Cache for type resolution (Lazy/App/Template -> Structural)
    pub resolve_cache: RefCell<FxHashMap<TypeId, TypeId>>,
    /// Cache for top-level property type lookups (TypeId, `PropName`) -> `PropType`
    pub property_cache: RefCell<FxHashMap<(TypeId, Atom), Option<TypeId>>>,
}

impl NarrowingCache {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Narrowing context for type guards and control flow analysis.
pub struct NarrowingContext<'a> {
    pub(crate) db: &'a dyn QueryDatabase,
    /// Optional `TypeResolver` for resolving Lazy types (e.g., type aliases).
    /// When present, this enables proper narrowing of type aliases like `type Shape = Circle | Square`.
    pub(crate) resolver: Option<&'a dyn TypeResolver>,
    /// Cache for narrowing operations.
    /// If provided, uses the shared cache; otherwise uses a local ephemeral cache.
    pub(crate) cache: std::borrow::Cow<'a, NarrowingCache>,
}

impl<'a> NarrowingContext<'a> {
    pub fn new(db: &'a dyn QueryDatabase) -> Self {
        NarrowingContext {
            db,
            resolver: None,
            cache: std::borrow::Cow::Owned(NarrowingCache::new()),
        }
    }

    /// Create a new context with a shared cache.
    pub fn with_cache(db: &'a dyn QueryDatabase, cache: &'a NarrowingCache) -> Self {
        NarrowingContext {
            db,
            resolver: None,
            cache: std::borrow::Cow::Borrowed(cache),
        }
    }

    /// Set the `TypeResolver` for this context.
    ///
    /// This enables proper resolution of Lazy types (type aliases) during narrowing.
    /// The resolver should be borrowed from the Checker's `TypeEnvironment`.
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
    pub(crate) fn resolve_type(&self, type_id: TypeId) -> TypeId {
        if let Some(&cached) = self.cache.resolve_cache.borrow().get(&type_id) {
            return cached;
        }

        let result = self.resolve_type_uncached(type_id);
        self.cache
            .resolve_cache
            .borrow_mut()
            .insert(type_id, result);
        result
    }

    fn resolve_type_uncached(&self, mut type_id: TypeId) -> TypeId {
        // Prevent infinite loops with a fuel counter
        let mut fuel = 100;

        while fuel > 0 {
            fuel -= 1;

            // 1. Handle Lazy types (DefId-based, not SymbolRef)
            // If we have a TypeResolver, try to resolve Lazy types through it first
            if let Some(def_id) = lazy_def_id(self.db, type_id) {
                if let Some(resolver) = self.resolver
                    && let Some(resolved) =
                        resolver.resolve_lazy(def_id, self.db.as_type_database())
                {
                    type_id = resolved;
                    continue;
                }
                // Fallback to database evaluation if no resolver or resolution failed
                type_id = self.db.evaluate_type(type_id);
                continue;
            }

            // 2. Handle Application types (Generics)
            // CRITICAL: When a resolver is available (from the checker's TypeEnvironment),
            // use it to resolve the Application's base type and instantiate with args.
            // Without the resolver, generic type aliases like `Box<number>` can't resolve
            // their DefId-based base types, causing narrowing to fail on discriminated
            // unions wrapped in generics.
            if let Some(TypeData::Application(app_id)) = self.db.lookup(type_id) {
                if let Some(resolver) = self.resolver {
                    let app = self.db.type_application(app_id);
                    // Try to resolve the base type's DefId and instantiate manually
                    if let Some(def_id) = lazy_def_id(self.db, app.base) {
                        let resolved_body =
                            resolver.resolve_lazy(def_id, self.db.as_type_database());
                        let type_params = resolver.get_lazy_type_params(def_id);
                        if let (Some(body), Some(params)) = (resolved_body, type_params) {
                            let instantiated = crate::instantiate::instantiate_generic(
                                self.db.as_type_database(),
                                body,
                                &params,
                                &app.args,
                            );
                            type_id = instantiated;
                            continue;
                        }
                    }
                }
                // Fallback: use db.evaluate_type (works when resolver isn't needed)
                type_id = self.db.evaluate_type(type_id);
                continue;
            }

            // 3. Handle TemplateLiteral types that can be fully evaluated to string literals.
            // Template literal spans may contain Lazy(DefId) types (e.g., `${EnumType.Member}`)
            // that must be resolved before evaluation. We resolve all lazy spans first,
            // rebuild the template literal, then let the evaluator handle it.
            if let Some(TypeData::TemplateLiteral(spans_id)) = self.db.lookup(type_id) {
                use crate::types::TemplateSpan;
                let spans = self.db.template_list(spans_id);
                let mut new_spans = Vec::with_capacity(spans.len());
                let mut changed = false;
                for span in spans.iter() {
                    match span {
                        TemplateSpan::Type(inner_id) => {
                            let resolved = self.resolve_type(*inner_id);
                            if resolved != *inner_id {
                                changed = true;
                            }
                            new_spans.push(TemplateSpan::Type(resolved));
                        }
                        other => new_spans.push(other.clone()),
                    }
                }
                let eval_input = if changed {
                    self.db.template_literal(new_spans)
                } else {
                    type_id
                };
                let evaluated = self.db.evaluate_type(eval_input);
                if evaluated != type_id {
                    type_id = evaluated;
                    continue;
                }
            }

            // It's a structural type (Object, Union, Intersection, Primitive)
            break;
        }

        type_id
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
        use crate::type_queries_extended::InstanceTypeKind;
        use crate::type_queries_extended::classify_for_instance_type;

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
                    intersection_or_single(self.db, instance_types)
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
                        self.narrow_by_instanceof(source_type, member, sense)
                            .non_never()
                    })
                    .collect();

                if sense {
                    union_or_single(self.db, instance_types)
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
                }
                trace!("Type parameter has no constraint");
                return source_type;
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
                // PERF: Reuse a single SubtypeChecker across all member checks
                // instead of allocating 4 hash sets per is_subtype_of call.
                let mut checker = SubtypeChecker::new(self.db.as_type_database());
                let mut filtered_members: Vec<TypeId> = Vec::new();
                for &member in &*members {
                    // Check if member is assignable to instance type
                    checker.reset();
                    if checker.is_subtype_of(member, instance_type) {
                        trace!(
                            "Union member {} is assignable to instance type {}, keeping",
                            member.0, instance_type.0
                        );
                        filtered_members.push(member);
                        continue;
                    }

                    // Check if instance type is assignable to member (subclass case)
                    // If we have a Dog and instanceof Animal, Dog is an instance of Animal
                    checker.reset();
                    if checker.is_subtype_of(instance_type, member) {
                        trace!(
                            "Instance type {} is assignable to union member {} (subclass), narrowing to instance type",
                            instance_type.0, member.0
                        );
                        filtered_members.push(instance_type);
                        continue;
                    }

                    // Interface overlap: both are object-like but not assignable
                    // Use intersection to preserve properties from both
                    if self.are_object_like(member) && self.are_object_like(instance_type) {
                        trace!(
                            "Interface overlap between {} and {}, using intersection",
                            member.0, instance_type.0
                        );
                        filtered_members.push(self.db.intersection2(member, instance_type));
                        continue;
                    }

                    trace!("Union member {} excluded by instanceof check", member.0);
                }

                union_or_single(self.db, filtered_members)
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
                // PERF: Reuse a single SubtypeChecker across all member checks
                let mut checker = SubtypeChecker::new(self.db.as_type_database());
                let mut filtered_members: Vec<TypeId> = Vec::new();
                for &member in &*members {
                    // Exclude members that are definitely subtypes of the instance type
                    checker.reset();
                    if !checker.is_subtype_of(member, instance_type) {
                        filtered_members.push(member);
                    }
                }

                union_or_single(self.db, filtered_members)
            } else {
                // Non-union: use standard exclusion
                self.narrow_excluding_type(resolved_source, instance_type)
            }
        }
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
                    if crate::subtype::is_subtype_of_with_db(self.db, target_type, member) {
                        return Some(target_type);
                    }
                    // CRITICAL FIX: instanceof Array matching
                    // When narrowing by `instanceof Array`, if the member is array-like and target
                    // is a Lazy/Application type (which includes Array<T> interface references),
                    // assume it's the global Array and match the member.
                    // This handles: `x: Message | Message[]` with `instanceof Array` should keep `Message[]`.
                    // At runtime, instanceof only checks prototype chain, not generic type arguments.
                    if self.is_array_like(member) {
                        use crate::type_queries;
                        // Check if target is a type reference or generic application (Array<T>)
                        let is_target_lazy_or_app = type_queries::is_type_reference(self.db, resolved_target)
                            || type_queries::is_generic_type(self.db, resolved_target);

                        trace!("Member is array-like: member={}, target={}, is_target_lazy_or_app={}",
                            member.0, resolved_target.0, is_target_lazy_or_app);

                        if is_target_lazy_or_app {
                            trace!("Array member with lazy/app target (likely Array interface), keeping member");
                            return Some(member);
                        }
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
            }
            trace!(
                "Found {} matching members, creating new union",
                matching.len()
            );
            return self.db.union(matching);
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
        } else if crate::subtype::is_subtype_of_with_db(self.db, resolved_target, resolved_source) {
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

    /// Narrow a type by instanceof check using the instance type.
    ///
    /// Unlike `narrow_to_type` which uses structural assignability to filter union members,
    /// this method uses instanceof-specific semantics:
    /// - Type parameters with constraints assignable to the target are kept (intersected)
    /// - When a type parameter absorbs the target, anonymous object types are excluded
    ///   since they cannot be class instances at runtime
    ///
    /// This prevents anonymous object types like `{ x: string }` from surviving instanceof
    /// narrowing when they happen to be structurally compatible with the class type.
    pub fn narrow_by_instance_type(&self, source_type: TypeId, instance_type: TypeId) -> TypeId {
        let resolved_source = self.resolve_type(source_type);

        if resolved_source == TypeId::ERROR && source_type != TypeId::ERROR {
            return source_type;
        }

        let resolved_target = self.resolve_type(instance_type);
        if resolved_target == TypeId::ERROR && instance_type != TypeId::ERROR {
            return source_type;
        }

        if resolved_source == resolved_target {
            return source_type;
        }

        // any/unknown narrow to instance type with instanceof
        if resolved_source == TypeId::ANY || resolved_source == TypeId::UNKNOWN {
            return instance_type;
        }

        // If source is a union, filter members using instanceof semantics
        if let Some(members) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members);
            trace!(
                "instanceof: narrowing union with {} members {:?} to instance type {}",
                members.len(),
                members.iter().map(|m| m.0).collect::<Vec<_>>(),
                instance_type.0
            );

            // First pass: check if any type parameter matches the instance type.
            let mut type_param_results: Vec<(usize, TypeId)> = Vec::new();
            for (i, &member) in members.iter().enumerate() {
                if let Some(narrowed) = self.narrow_type_param(member, instance_type) {
                    type_param_results.push((i, narrowed));
                }
            }

            let matching: Vec<TypeId> = if !type_param_results.is_empty() {
                // Type parameter(s) matched: keep type params and exclude anonymous
                // object types that can't be class instances at runtime.
                let mut result = Vec::new();
                let tp_indices: Vec<usize> = type_param_results.iter().map(|(i, _)| *i).collect();
                for &(_, narrowed) in &type_param_results {
                    result.push(narrowed);
                }
                for (i, &member) in members.iter().enumerate() {
                    if tp_indices.contains(&i) {
                        continue;
                    }
                    if crate::type_queries::is_object_type(self.db, member) {
                        trace!(
                            "instanceof: excluding anonymous object {} (type param absorbs)",
                            member.0
                        );
                        continue;
                    }
                    if crate::subtype::is_subtype_of_with_db(self.db, member, instance_type) {
                        result.push(member);
                    } else if crate::subtype::is_subtype_of_with_db(self.db, instance_type, member)
                    {
                        result.push(instance_type);
                    }
                }
                result
            } else {
                // No type parameter match: filter by instanceof semantics.
                // Primitives can never pass instanceof; non-primitives are
                // checked for assignability with the instance type.
                members
                    .iter()
                    .filter_map(|&member| {
                        // Primitive types can never pass `instanceof` at runtime.
                        if self.is_js_primitive(member) {
                            return None;
                        }
                        if let Some(narrowed) = self.narrow_type_param(member, instance_type) {
                            return Some(narrowed);
                        }
                        // Member assignable to instance type → keep member
                        if self.is_assignable_to(member, instance_type) {
                            return Some(member);
                        }
                        // Instance type assignable to member → narrow to instance
                        // (e.g., member=Animal, instance=Dog → Dog)
                        if self.is_assignable_to(instance_type, member) {
                            return Some(instance_type);
                        }
                        // Neither direction holds — create intersection per tsc
                        // semantics. This handles cases like Date instanceof Object
                        // where assignability checks may miss the relationship.
                        // The intersection preserves the member's shape while
                        // constraining it to the instance type.
                        Some(self.db.intersection2(member, instance_type))
                    })
                    .collect()
            };

            if matching.is_empty() {
                return self.narrow_to_type(source_type, instance_type);
            } else if matching.len() == 1 {
                return matching[0];
            }
            return self.db.union(matching);
        }

        // Non-union: use instanceof-specific semantics
        trace!(
            "instanceof: non-union path for source_type={}",
            source_type.0
        );

        // Try type parameter narrowing first (produces T & InstanceType)
        if let Some(narrowed) = self.narrow_type_param(resolved_source, instance_type) {
            return narrowed;
        }

        // For non-primitive, non-type-param source types, instanceof narrowing
        // should keep them when there's a potential runtime relationship.
        // This handles cases like `readonly number[]` narrowed by `instanceof Array`:
        // - readonly number[] is NOT a subtype of Array<T> (missing mutating methods)
        // - Array<T> is NOT a subtype of readonly number[] (unbound T)
        // - But at runtime, a readonly array IS an Array instance
        if !self.is_js_primitive(resolved_source) {
            if self.is_assignable_to(resolved_source, instance_type) {
                return source_type;
            }
            if self.is_assignable_to(instance_type, resolved_source) {
                return instance_type;
            }
            // Non-primitive types may still be instances at runtime
            // Keep the source type rather than returning NEVER.
            return source_type;
        }
        // Primitives can never pass instanceof
        TypeId::NEVER
    }

    /// Narrow a type for the false branch of `instanceof`.
    ///
    /// Keeps primitive types (which can never pass instanceof) and excludes
    /// non-primitive members that are subtypes of the instance type.
    /// For example, `string | number | Date` with `instanceof Object` false
    /// branch gives `string | number` (Date is excluded as it's an Object instance).
    pub fn narrow_by_instanceof_false(&self, source_type: TypeId, instance_type: TypeId) -> TypeId {
        let resolved_source = self.resolve_type(source_type);

        if let Some(members) = union_list_id(self.db, resolved_source) {
            let members = self.db.type_list(members);
            let remaining: Vec<TypeId> = members
                .iter()
                .filter(|&&member| {
                    // Primitives always survive the false branch of instanceof
                    if self.is_js_primitive(member) {
                        return true;
                    }
                    // Non-primitive: use the true-branch logic to determine if this
                    // member would be kept by instanceof narrowing. If the true branch
                    // would keep it, exclude it from the false branch.
                    let true_result = self.narrow_by_instance_type(member, instance_type);
                    // If true-branch narrows to NEVER, the member wouldn't pass instanceof
                    // → keep it in the false branch
                    true_result == TypeId::NEVER
                })
                .copied()
                .collect();

            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
        }

        // Non-union: can't narrow in the false branch
        source_type
    }

    /// Check if a literal type is assignable to a target for narrowing purposes.
    ///
    /// Handles union decomposition: if the target is a union, checks each member.
    /// Falls back to `narrow_to_type` to determine if the literal can narrow to the target.
    pub fn literal_assignable_to(&self, literal: TypeId, target: TypeId) -> bool {
        if literal == target || target == TypeId::ANY || target == TypeId::UNKNOWN {
            return true;
        }

        if let UnionMembersKind::Union(members) = classify_for_union_members(self.db, target) {
            return members
                .iter()
                .any(|&member| self.literal_assignable_to(literal, member));
        }

        self.narrow_to_type(literal, target) != TypeId::NEVER
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
                        return self
                            .narrow_excluding_type(member, excluded_type)
                            .non_never();
                    }
                    if let Some(narrowed) = self.narrow_type_param_excluding(member, excluded_type)
                    {
                        return narrowed.non_never();
                    }
                    if self.is_assignable_to(member, excluded_type) {
                        None
                    } else {
                        Some(member)
                    }
                })
                .collect();

            tracing::trace!(
                remaining_count = remaining.len(),
                remaining = ?remaining.iter().map(|t| t.0).collect::<Vec<_>>(),
                "narrow_excluding_type: union filter result"
            );
            if remaining.is_empty() {
                return TypeId::NEVER;
            } else if remaining.len() == 1 {
                return remaining[0];
            }
            return self.db.union(remaining);
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
            } else if source_type == TypeId::BOOLEAN_FALSE && is_excluding_false {
                // Excluding false from false -> return never
                return TypeId::NEVER;
            }
            // For other cases, let the final is_assignable_to check handle it below
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
                        return self
                            .narrow_excluding_types(member, excluded_types)
                            .non_never();
                    }

                    // Handle type parameters
                    if let Some(narrowed) =
                        self.narrow_type_param_excluding_set(member, &excluded_set)
                    {
                        return narrowed.non_never();
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
            }
            return self.db.union(remaining);
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

    /// Helper for `narrow_excluding_types` with type parameters
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
                        return narrowed.non_never();
                    }
                    self.is_function_type(member).then_some(member)
                })
                .collect();

            return union_or_single(self.db, functions);
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
        } else if index_access_parts(self.db, source_type).is_some() {
            // For indexed access types like T[K], narrow to T[K] & Function
            // This handles cases like: typeof obj[key] === 'function'
            let function_type = self.function_type();
            self.db.intersection2(source_type, function_type)
        } else {
            TypeId::NEVER
        }
    }

    /// Check if a type is a function type.
    /// Uses the visitor pattern from `solver::visitor`.
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
                        return narrowed.non_never();
                    }
                    if self.is_function_type(member) {
                        None
                    } else {
                        Some(member)
                    }
                })
                .collect();

            return union_or_single(self.db, remaining);
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
    /// Uses the visitor pattern from `solver::visitor`.
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

    /// Check if a type is a JS primitive that can never pass `instanceof`.
    /// Includes string, number, boolean, bigint, symbol, undefined, null,
    /// void, never, and their literal forms.
    fn is_js_primitive(&self, type_id: TypeId) -> bool {
        matches!(
            type_id,
            TypeId::STRING
                | TypeId::NUMBER
                | TypeId::BOOLEAN
                | TypeId::BIGINT
                | TypeId::SYMBOL
                | TypeId::UNDEFINED
                | TypeId::NULL
                | TypeId::VOID
                | TypeId::NEVER
                | TypeId::BOOLEAN_TRUE
                | TypeId::BOOLEAN_FALSE
        ) || matches!(self.db.lookup(type_id), Some(TypeData::Literal(_)))
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
        if target.is_any_or_unknown() {
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

        // Check if source is assignable to any member of a union target
        if let Some(members) = union_list_id(self.db, target) {
            let members = self.db.type_list(members);
            if members
                .iter()
                .any(|&member| self.is_assignable_to(source, member))
            {
                return true;
            }
        }

        // Fallback: use full structural/nominal subtype check.
        // This handles class inheritance (Derived extends Base), interface
        // implementations, and other structural relationships that the
        // fast-path checks above don't cover.
        // CRITICAL: Resolve Lazy(DefId) types before the subtype check.
        // Without resolution, two unrelated interfaces (e.g., Cat and Dog)
        // remain as opaque Lazy types and the SubtypeChecker can't distinguish them.
        let source = self.resolve_type(source);
        let target = self.resolve_type(target);
        if source == target {
            return true;
        }
        crate::subtype::is_subtype_of_with_db(self.db, source, target)
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
    /// let guard = TypeGuard::Typeof(TypeofKind::String);
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
            TypeGuard::Typeof(typeof_kind) => {
                let type_name = typeof_kind.as_str();
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
                    // Special case: `unknown` instanceof X narrows to X (or object if X unknown)
                    // This must be handled here in the solver, not in the checker.
                    if source_type == TypeId::UNKNOWN {
                        return *instance_type;
                    }

                    // CRITICAL: The payload is already the Instance Type (extracted by Checker)
                    // Use narrow_by_instance_type for instanceof-specific semantics:
                    // type parameters with matching constraints are kept, but anonymous
                    // object types that happen to be structurally compatible are excluded.
                    // Primitive types are filtered out since they can never pass instanceof.
                    let narrowed = self.narrow_by_instance_type(source_type, *instance_type);

                    if narrowed != TypeId::NEVER || source_type == TypeId::NEVER {
                        return narrowed;
                    }

                    // Fallback 1: If standard narrowing returns NEVER but source wasn't NEVER,
                    // it might be an interface vs class check (which is allowed in TS).
                    // Use intersection in that case.
                    let intersection = self.db.intersection2(source_type, *instance_type);
                    if intersection != TypeId::NEVER {
                        return intersection;
                    }

                    // Fallback 2: If even intersection fails, narrow to object-like types.
                    // On the true branch of instanceof, we know the value must be some
                    // kind of object (primitives can never pass instanceof).
                    self.narrow_to_objectish(source_type)
                } else {
                    // Negative: !(x instanceof Class)
                    // Keep primitives (they can never pass instanceof) and exclude
                    // non-primitive types assignable to the instance type.
                    if *instance_type == TypeId::OBJECT {
                        source_type
                    } else {
                        self.narrow_by_instanceof_false(source_type, *instance_type)
                    }
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
                // Use narrow_by_discriminant_for_type which handles type parameters
                // by narrowing the constraint and returning T & NarrowedConstraint
                self.narrow_by_discriminant_for_type(source_type, property_path, *value_type, sense)
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
                            // True branch: narrow source to the predicate type.
                            // Following TSC's narrowType logic:
                            // 1. For unions: filter members using narrow_to_type
                            // 2. For non-unions:
                            //    a. source <: target → return source
                            //    b. target <: source → return target
                            //    c. otherwise → return source & target
                            //
                            // Following TSC's narrowType logic which uses
                            // isTypeSubtypeOf (not isTypeAssignableTo) to decide
                            // whether source is already specific enough.
                            //
                            // If source is a strict subtype of the target, return
                            // source (it's already more specific). If target is a
                            // strict subtype of source, return target (narrowing
                            // down). Otherwise, return the intersection.
                            //
                            // narrow_to_type uses assignability internally, which is
                            // too loose for type predicates (e.g. {} is assignable to
                            // Record<string,unknown> but not a subtype).
                            let resolved_source = self.resolve_type(source_type);

                            if resolved_source == self.resolve_type(*target_type) {
                                source_type
                            } else if resolved_source == TypeId::UNKNOWN
                                || resolved_source == TypeId::ANY
                            {
                                *target_type
                            } else if union_list_id(self.db, resolved_source).is_some() {
                                // For unions: filter members, fall back to
                                // intersection if nothing matches.
                                let narrowed = self.narrow_to_type(source_type, *target_type);
                                if narrowed == TypeId::NEVER && source_type != TypeId::NEVER {
                                    self.db.intersection2(source_type, *target_type)
                                } else {
                                    narrowed
                                }
                            } else {
                                // Non-union source: use narrow_to_type first.
                                // If it returns source unchanged (assignable but
                                // possibly losing structural info) or NEVER (no
                                // overlap), fall back to intersection.
                                let narrowed = self.narrow_to_type(source_type, *target_type);
                                if narrowed == source_type && narrowed != *target_type {
                                    // Source was unchanged — intersect to preserve
                                    // target's structural info (index sigs, etc.)
                                    self.db.intersection2(source_type, *target_type)
                                } else if narrowed == TypeId::NEVER && source_type != TypeId::NEVER
                                {
                                    self.db.intersection2(source_type, *target_type)
                                } else {
                                    narrowed
                                }
                            }
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

            TypeGuard::ArrayElementPredicate { element_type } => {
                trace!(
                    ?element_type,
                    ?sense,
                    "Applying ArrayElementPredicate guard"
                );
                if sense {
                    // True branch: narrow array element type
                    let result = self.narrow_array_element_type(source_type, *element_type);
                    trace!(?result, "ArrayElementPredicate narrowing result");
                    result
                } else {
                    // False branch: we don't narrow (arr.every could be false for various reasons)
                    trace!("ArrayElementPredicate false branch, no narrowing");
                    source_type
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/narrowing_tests.rs"]
mod tests;
