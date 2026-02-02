//! Intrinsic (primitive) type subtype checking.
//!
//! This module handles subtyping for TypeScript's built-in primitive types:
//! - Intrinsic types (number, string, boolean, bigint, symbol, void, null, undefined)
//! - The `object` keyword type
//! - The `Function` type
//! - Apparent primitive shapes (for object-like operations on primitives)

use crate::solver::types::*;
use crate::solver::{ApparentMemberKind, apparent_primitive_members};
use crate::solver::visitor::{
    application_id, array_element_type, callable_shape_id, function_shape_id,
    intersection_list_id, intrinsic_kind, is_this_type, literal_value, mapped_type_id,
    object_shape_id, object_with_index_shape_id, readonly_inner_type, ref_symbol,
    template_literal_id, tuple_list_id, type_param_info, union_list_id,
};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

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
    pub(crate) fn is_object_keyword_type(&mut self, source: TypeId) -> bool {
        match source {
            TypeId::ANY | TypeId::NEVER | TypeId::ERROR | TypeId::OBJECT => return true,
            TypeId::UNKNOWN
            | TypeId::VOID
            | TypeId::NULL
            | TypeId::UNDEFINED
            | TypeId::BOOLEAN
            | TypeId::NUMBER
            | TypeId::STRING
            | TypeId::BIGINT
            | TypeId::SYMBOL => return false,
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
            return info
                .constraint
                .map(|constraint| self.check_subtype(constraint, TypeId::OBJECT).is_true())
                .unwrap_or(false);
        }

        if let Some(sym) = ref_symbol(self.interner, source) {
            if let Some(resolved) = self.resolver.resolve_ref(sym, self.interner) {
                return self.check_subtype(resolved, TypeId::OBJECT).is_true();
            }
        }

        false
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
    pub(crate) fn is_callable_type(&mut self, source: TypeId) -> bool {
        match source {
            TypeId::ANY | TypeId::NEVER | TypeId::ERROR => return true,
            TypeId::FUNCTION => return true,
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
                .map(|constraint| self.is_callable_type(constraint))
                .unwrap_or(false);
        }

        if let Some(sym) = ref_symbol(self.interner, source) {
            if let Some(resolved) = self.resolver.resolve_ref(sym, self.interner) {
                return self.is_callable_type(resolved);
            }
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
    /// This creates an ObjectShape representing the wrapper type's members:
    /// - String: length, charAt, concat, etc.
    /// - Number: toFixed, toPrecision, etc.
    /// - Boolean: valueOf
    /// - BigInt: toString, valueOf, etc.
    /// - Symbol: description, toString, valueOf
    pub(crate) fn apparent_primitive_shape(&mut self, kind: IntrinsicKind) -> ObjectShape {
        let members = apparent_primitive_members(self.interner, kind);
        let mut properties = Vec::with_capacity(members.len());

        for member in members {
            let name = self.interner.intern_string(member.name);
            match member.kind {
                ApparentMemberKind::Value(type_id) => properties.push(PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional: false,
                    readonly: false,
                    is_method: false,
                }),
                ApparentMemberKind::Method(return_type) => properties.push(PropertyInfo {
                    name,
                    type_id: self.apparent_method_type(return_type),
                    write_type: self.apparent_method_type(return_type),
                    optional: false,
                    readonly: false,
                    is_method: true,
                }),
            }
        }

        let number_index = if kind == IntrinsicKind::String {
            Some(IndexSignature {
                key_type: TypeId::NUMBER,
                value_type: TypeId::STRING,
                // Keep string index signature assignable to mutable targets for TS compat.
                readonly: false,
            })
        } else {
            None
        };

        ObjectShape {
            flags: ObjectFlags::empty(),
            properties,
            string_index: None,
            number_index,
        }
    }

    /// Create a function type with no parameters and the given return type.
    ///
    /// Used for apparent method types on primitive wrappers.
    pub(crate) fn apparent_method_type(&mut self, return_type: TypeId) -> TypeId {
        self.interner.function(FunctionShape {
            params: Vec::new(),
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    /// Get the apparent primitive kind for a type (helper for template literal checking).
    ///
    /// Returns the IntrinsicKind if the type represents a primitive value.
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
}
