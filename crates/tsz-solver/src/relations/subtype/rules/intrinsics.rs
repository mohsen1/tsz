//! Intrinsic (primitive) type subtype checking.
//!
//! This module handles subtyping for TypeScript's built-in primitive types:
//! - Intrinsic types (number, string, boolean, bigint, symbol, void, null, undefined)
//! - The `object` keyword type
//! - The `Function` type
//! - Apparent primitive shapes (for object-like operations on primitives)

use crate::TypeDatabase;
use crate::objects::apparent::apparent_primitive_shape;
use crate::types::{FunctionShape, IntrinsicKind, LiteralValue, ObjectShape, TypeId};
use crate::visitor::{
    application_id, array_element_type, callable_shape_id, function_shape_id, intersection_list_id,
    intrinsic_kind, is_this_type, lazy_def_id, literal_value, mapped_type_id, object_shape_id,
    object_with_index_shape_id, readonly_inner_type, template_literal_id, tuple_list_id,
    type_param_info, union_list_id,
};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

/// Create a function type with no parameters and the given return type.
///
/// Used for apparent method types on primitive wrappers during subtype checking.
/// Unlike the evaluator's `make_apparent_method_type` (which uses `...any[]`),
/// the subtype checker uses empty params because it only needs structural shape
/// matching, not full call-site compatibility.
fn make_subtype_method_type(db: &dyn TypeDatabase, return_type: TypeId) -> TypeId {
    db.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    })
}

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    /// Check if an intrinsic type is a subtype of another intrinsic type.
    ///
    /// Intrinsic types have a fixed subtyping hierarchy:
    /// - `never` <: everything
    /// - everything <: `any`
    /// - `undefined` <: `void`
    /// - Same types are subtypes of themselves
    ///
    /// ## TypeScript Soundness:
    /// - `never` is the bottom type (subtype of everything)
    /// - `any` is the top type (everything is a subtype of it)
    /// - `undefined` is a subtype of `void` (void functions can return undefined)
    ///
    /// ## Examples:
    /// ```typescript
    /// let x: void = undefined;  // ✅ undefined <: void
    /// let y: any = 42;          // ✅ number <: any
    /// let z: never;             // ⚠️ never has no values
    /// ```
    ///
    /// ## Note:
    /// The `object` keyword type has special handling in `check_subtype_inner`
    /// because it involves complex structural subtyping rules.
    pub(crate) fn check_intrinsic_subtype(
        &self,
        source: IntrinsicKind,
        target: IntrinsicKind,
    ) -> SubtypeResult {
        if source == target {
            return SubtypeResult::True;
        }

        // Everything is a subtype of any and unknown
        if target == IntrinsicKind::Any || target == IntrinsicKind::Unknown {
            return SubtypeResult::True;
        }

        // any is a subtype of everything (bottom type behavior in assignability)
        if source == IntrinsicKind::Any {
            return SubtypeResult::True;
        }

        // null and undefined are subtypes of their non-strict counterparts
        match (source, target) {
            // void accepts undefined
            (IntrinsicKind::Undefined, IntrinsicKind::Void) => SubtypeResult::True,

            // object keyword handling is in check_subtype_inner
            _ => SubtypeResult::False,
        }
    }

    /// Check if a type is assignable to the `object` keyword type.
    ///
    /// The `object` keyword represents non-primitive types in TypeScript.
    /// It accepts:
    /// - Objects (plain or with index signatures)
    /// - Arrays and tuples
    /// - Functions and callables
    /// - Mapped and application types
    /// - Class instances (via Ref)
    /// - `this` type
    /// - Special types: `any`, `never`, `error`, `object` itself
    ///
    /// It rejects:
    /// - Primitive types: `number`, `string`, `boolean`, `bigint`, `symbol`
    /// - `null`, `undefined`, `void`
    /// - `unknown`
    ///
    /// ## TypeScript Soundness:
    /// ```typescript
    /// let a: object = { x: 1 };              // ✅ object literal
    /// let b: object = [1, 2, 3];             // ✅ array
    /// let c: object = () => {};              // ✅ function
    /// let d: object = 42;                    // ❌ primitive
    /// let e: object = "hello";               // ❌ primitive
    /// let f: object = null;                  // ❌ null
    /// let g: object = undefined;             // ❌ undefined
    /// let h: object = class {};              // ✅ class
    /// let i: object = { foo: 42 } as const;  // ✅ readonly object
    /// let j: object = new Date();            // ✅ object instance
    /// let k: object = <T>() => {} as T;      // ❓ depends on T's constraint
    /// let l: object = <any>{};               // ✅ any matches everything
    /// ```
    ///
    /// This is used in subtype checking to determine when structural typing rules apply.
    #[allow(clippy::match_same_arms)]
    pub(crate) fn is_object_keyword_type(&mut self, source: TypeId) -> bool {
        let allow_any = self.any_propagation.allows_any_at_depth(self.guard.depth());
        match source {
            TypeId::ANY if allow_any => return true,
            TypeId::NEVER | TypeId::ERROR | TypeId::OBJECT => return true,
            TypeId::UNKNOWN
            | TypeId::VOID
            | TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::BOOLEAN
            | TypeId::NUMBER
            | TypeId::STRING
            | TypeId::BIGINT
            | TypeId::SYMBOL => return false,
            // Fall through to structural check for ANY in strict mode and all other types
            _ => {}
        }

        if object_shape_id(self.interner, source).is_some()
            || object_with_index_shape_id(self.interner, source).is_some()
            || array_element_type(self.interner, source).is_some()
            || tuple_list_id(self.interner, source).is_some()
            || function_shape_id(self.interner, source).is_some()
            || callable_shape_id(self.interner, source).is_some()
            || mapped_type_id(self.interner, source).is_some()
            || application_id(self.interner, source).is_some()
            || is_this_type(self.interner, source)
        {
            return true;
        }

        if let Some(inner) = readonly_inner_type(self.interner, source) {
            return self.check_subtype(inner, TypeId::OBJECT).is_true();
        }

        if let Some(members) = union_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            return members.iter().all(|&m| self.is_object_keyword_type(m));
        }

        if let Some(members) = intersection_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            return members.iter().any(|&m| self.is_object_keyword_type(m));
        }

        if let Some(info) = type_param_info(self.interner, source) {
            return info.constraint.is_some_and(|constraint| {
                self.check_subtype(constraint, TypeId::OBJECT).is_true()
            });
        }

        if let Some(def_id) = lazy_def_id(self.interner, source) {
            let resolved = self.resolver.resolve_lazy(def_id, self.interner);
            if let Some(resolved) = resolved {
                return self.check_subtype(resolved, TypeId::OBJECT).is_true();
            }
        }

        false
    }

    /// Check compatibility with the global `Object` interface type.
    ///
    /// TypeScript's uppercase `Object` accepts all non-nullish values, including
    /// primitives (unlike lowercase `object` which rejects primitives).
    pub(crate) fn is_global_object_interface_type(&mut self, source: TypeId) -> bool {
        match source {
            TypeId::ANY | TypeId::NEVER | TypeId::ERROR => return true,
            TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID | TypeId::UNKNOWN => return false,
            _ => {}
        }

        if let Some(members) = union_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            return members
                .iter()
                .all(|&member| self.is_global_object_interface_type(member));
        }

        if let Some(inner) = readonly_inner_type(self.interner, source) {
            return self.is_global_object_interface_type(inner);
        }

        if let Some(info) = type_param_info(self.interner, source) {
            return info
                .constraint
                .is_some_and(|constraint| self.is_global_object_interface_type(constraint));
        }

        true
    }

    /// Check if a type is callable (can be invoked as a function).
    ///
    /// Callable types represent values that can be called with parentheses syntax:
    /// - Functions: `(x: number) => void`
    /// - Function types: `Function` intrinsic
    /// - Callable objects: Objects with call signatures
    ///
    /// ## TypeScript Soundness:
    /// - **Union types**: All members must be callable (intersection semantics)
    /// - **Intersection types**: At least one member must be callable
    /// - **Type parameters**: Callable if their constraint is callable
    /// - **Special cases**: `any`, `never`, `error`, `Function` are always callable
    ///
    /// ## Examples:
    /// ```typescript
    /// // Callable types
    /// let a: Function = () => {};           // ✅ Function type
    /// let b: Function = function() {};       // ✅ Function expression
    ///
    /// // Call signatures
    /// interface Callable {
    ///     (x: number): void;
    /// }
    /// let c: Callable = (x: number) => {};   // ✅ Callable object
    ///
    /// // Unions and intersections
    /// type F = () => void;
    /// type G = () => void;
    /// type Union = F | G;                   // ✅ All members callable
    /// type Intersect = F & G;               // ✅ At least one callable
    ///
    /// // Non-callable types
    /// let d: Function = { x: 1 };           // ❌ Plain object
    /// let e: Function = 42;                 // ❌ Number
    /// ```
    ///
    /// Rule #29: Function intrinsic accepts any callable type as a subtype.
    #[allow(clippy::match_same_arms)]
    pub(crate) fn is_callable_type(&mut self, source: TypeId) -> bool {
        let allow_any = self.any_propagation.allows_any_at_depth(self.guard.depth());
        match source {
            TypeId::ANY if allow_any => return true,
            TypeId::NEVER | TypeId::ERROR | TypeId::FUNCTION => return true,
            // Fall through to structural check for ANY in strict mode and all other types
            _ => {}
        }

        if function_shape_id(self.interner, source).is_some()
            || callable_shape_id(self.interner, source).is_some()
        {
            return true;
        }

        if let Some(members) = union_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            return members.iter().all(|&m| self.is_callable_type(m));
        }

        if let Some(members) = intersection_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            return members.iter().any(|&m| self.is_callable_type(m));
        }

        if let Some(info) = type_param_info(self.interner, source) {
            return info
                .constraint
                .is_some_and(|constraint| self.is_callable_type(constraint));
        }

        false
    }

    /// Get the apparent primitive shape for a type.
    ///
    /// When primitives are used in object-like operations (e.g., `"hello".length`),
    /// TypeScript wraps them in their corresponding wrapper types. This function
    /// returns the object shape that represents those wrapper type members.
    pub(crate) fn apparent_primitive_shape_for_type(
        &mut self,
        type_id: TypeId,
    ) -> Option<ObjectShape> {
        let kind = self.apparent_primitive_kind(type_id)?;
        Some(self.apparent_primitive_shape(kind))
    }

    /// Get the intrinsic kind that a type represents (if it's a primitive).
    pub(crate) fn apparent_primitive_kind(&self, type_id: TypeId) -> Option<IntrinsicKind> {
        if let Some(kind) = intrinsic_kind(self.interner, type_id) {
            return match kind {
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol => Some(kind),
                _ => None,
            };
        }

        if let Some(literal) = literal_value(self.interner, type_id) {
            return match literal {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            };
        }

        if template_literal_id(self.interner, type_id).is_some() {
            return Some(IntrinsicKind::String);
        }

        None
    }

    /// Build the apparent object shape for a primitive type.
    ///
    /// Delegates to the shared `apparent_primitive_shape` with a simple
    /// method-type factory (no params, given return type).
    pub(crate) fn apparent_primitive_shape(&mut self, kind: IntrinsicKind) -> ObjectShape {
        apparent_primitive_shape(self.interner, kind, make_subtype_method_type)
    }

    /// Get the apparent primitive kind for a type (helper for template literal checking).
    ///
    /// Returns the `IntrinsicKind` if the type represents a primitive value.
    pub(crate) fn apparent_primitive_kind_for_type(
        &self,
        type_id: TypeId,
    ) -> Option<IntrinsicKind> {
        self.apparent_primitive_kind(type_id)
    }

    /// Check if a primitive intrinsic is a subtype of a boxed interface type (Rule #33).
    ///
    /// In TypeScript, primitive values can be assigned to their boxed interface types:
    /// - `number` is assignable to `Number`
    /// - `string` is assignable to `String`
    /// - `boolean` is assignable to `Boolean`
    /// - `bigint` is assignable to `BigInt`
    /// - `symbol` is assignable to `Symbol`
    ///
    /// This is because primitives auto-box when used in object contexts.
    /// However, the reverse is NOT true: `Number` is not assignable to `number`.
    ///
    /// ## Examples:
    /// ```typescript
    /// let n: Number = 42;           // ✅ number <: Number
    /// let m: number = new Number(); // ❌ Number is not assignable to number
    /// let o: Object = 42;           // ✅ number <: Number <: Object
    /// ```
    pub(crate) fn is_boxed_primitive_subtype(
        &mut self,
        source_kind: IntrinsicKind,
        target: TypeId,
    ) -> bool {
        // Only certain primitives have boxed equivalents
        let boxable = matches!(
            source_kind,
            IntrinsicKind::Number
                | IntrinsicKind::String
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol
        );

        if !boxable {
            return false;
        }

        // Ask the resolver for the boxed type
        if let Some(boxed_type) = self.resolver.get_boxed_type(source_kind) {
            // If target is exactly the boxed interface (e.g., Number)
            if target == boxed_type {
                return true;
            }
            // Or if target is a supertype of the boxed interface (e.g., Object)
            return self.check_subtype(boxed_type, target).is_true();
        }

        false
    }

    /// Check if `target` is the boxed wrapper type for the given intrinsic kind.
    ///
    /// Checks both the resolver and the interner (fallback) because the resolver
    /// may not always have boxed types registered (e.g., when `TypeEnvironment`
    /// is populated after the interner). When the registered boxed TypeId differs
    /// from the target (different interning paths for the same interface), falls
    /// back to a structural subtype check: `boxed_type` <: target.
    pub(crate) fn is_target_boxed_type(&mut self, target: TypeId, kind: IntrinsicKind) -> bool {
        // 1. Check resolver registry (identity)
        if self.resolver.is_boxed_type_id(target, kind) {
            return true;
        }
        if self
            .resolver
            .get_boxed_type(kind)
            .is_some_and(|b| b == target)
        {
            return true;
        }
        // 2. Check target Lazy DefId
        if lazy_def_id(self.interner, target)
            .is_some_and(|def_id| self.resolver.is_boxed_def_id(def_id, kind))
        {
            return true;
        }
        // 3. Interner fallback: the interner stores boxed types from register_boxed_type.
        //    The TypeId may differ from the target (different interning paths), so if
        //    identity doesn't match, do a structural subtype check.
        if let Some(boxed) = self.interner.get_boxed_type(kind) {
            if boxed == target {
                return true;
            }
            // Structural fallback: require bidirectional subtyping (structural equivalence).
            // Unidirectional `boxed <: target` is too permissive — any supertype of the
            // boxed wrapper (e.g., `object`, `{}`, `unknown`) would incorrectly match.
            // For example, `Number <: object` is true, but `object` is NOT the `Number`
            // boxed wrapper — `number` must NOT be assignable to `object`.
            if self.check_subtype(boxed, target).is_true()
                && self.check_subtype(target, boxed).is_true()
            {
                return true;
            }
            // Shape-level equivalence check: when both types are ObjectWithIndex shapes
            // from the same interface but with different interning (e.g., different
            // [Symbol.iterator] TypeIds due to separate resolution paths), compare by
            // property names + count as a proxy for interface identity.
            if let (Some(b_sid), Some(t_sid)) = (
                object_with_index_shape_id(self.interner, boxed),
                object_with_index_shape_id(self.interner, target),
            ) {
                let b_shape = self.interner.object_shape(b_sid);
                let t_shape = self.interner.object_shape(t_sid);
                if b_shape.properties.len() == t_shape.properties.len()
                    && b_shape
                        .properties
                        .iter()
                        .zip(t_shape.properties.iter())
                        .all(|(b, t)| b.name == t.name)
                {
                    return true;
                }
            }
        }
        false
    }
}

/// Map an intrinsic kind to its boxable equivalent (primitives with wrapper interfaces).
pub(crate) const fn boxable_intrinsic_kind(kind: IntrinsicKind) -> Option<IntrinsicKind> {
    match kind {
        IntrinsicKind::String
        | IntrinsicKind::Number
        | IntrinsicKind::Boolean
        | IntrinsicKind::Bigint
        | IntrinsicKind::Symbol => Some(kind),
        _ => None,
    }
}
