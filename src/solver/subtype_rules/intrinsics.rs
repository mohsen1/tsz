//! Intrinsic (primitive) type subtype checking.
//!
//! This module handles subtyping for TypeScript's built-in primitive types:
//! - Intrinsic types (number, string, boolean, bigint, symbol, void, null, undefined)
//! - The `object` keyword type
//! - The `Function` type
//! - Apparent primitive shapes (for object-like operations on primitives)

use crate::solver::types::*;
use crate::solver::{ApparentMemberKind, apparent_primitive_members};

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

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match &key {
            TypeKey::Object(_)
            | TypeKey::ObjectWithIndex(_)
            | TypeKey::Array(_)
            | TypeKey::Tuple(_)
            | TypeKey::Function(_)
            | TypeKey::Callable(_)
            | TypeKey::Mapped(_)
            | TypeKey::Application(_)
            | TypeKey::ThisType => true,
            TypeKey::ReadonlyType(inner) => self.check_subtype(*inner, TypeId::OBJECT).is_true(),
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => match info.constraint {
                Some(constraint) => self.check_subtype(constraint, TypeId::OBJECT).is_true(),
                None => false,
            },
            TypeKey::Ref(sym) => {
                if let Some(resolved) = self.resolver.resolve_ref(*sym, self.interner) {
                    self.check_subtype(resolved, TypeId::OBJECT).is_true()
                } else {
                    false
                }
            }
            _ => false,
        }
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

        let key = match self.interner.lookup(source) {
            Some(key) => key,
            None => return false,
        };

        match &key {
            TypeKey::Function(_) | TypeKey::Callable(_) => true,
            TypeKey::Union(members) => {
                let members = self.interner.type_list(*members);
                // A union is callable if all members are callable
                members.iter().all(|&m| self.is_callable_type(m))
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(*members);
                // An intersection is callable if at least one member is callable
                members.iter().any(|&m| self.is_callable_type(m))
            }
            TypeKey::TypeParameter(info) | TypeKey::Infer(info) => {
                // Type parameters are not inherently callable without a callable constraint
                match info.constraint {
                    Some(constraint) => self.is_callable_type(constraint),
                    None => false,
                }
            }
            TypeKey::Ref(sym) => {
                if let Some(resolved) = self.resolver.resolve_ref(*sym, self.interner) {
                    self.is_callable_type(resolved)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Get the apparent primitive shape for a type key.
    ///
    /// When primitives are used in object-like operations (e.g., `"hello".length`),
    /// TypeScript wraps them in their corresponding wrapper types. This function
    /// returns the object shape that represents those wrapper type members.
    pub(crate) fn apparent_primitive_shape_for_key(&mut self, key: &TypeKey) -> Option<ObjectShape> {
        let kind = self.apparent_primitive_kind(key)?;
        Some(self.apparent_primitive_shape(kind))
    }

    /// Get the intrinsic kind that a type key represents (if it's a primitive).
    pub(crate) fn apparent_primitive_kind(&self, key: &TypeKey) -> Option<IntrinsicKind> {
        match key {
            TypeKey::Intrinsic(kind) => match kind {
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol => Some(*kind),
                _ => None,
            },
            TypeKey::Literal(literal) => match literal {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            },
            TypeKey::TemplateLiteral(_) => Some(IntrinsicKind::String),
            _ => None,
        }
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
    pub(crate) fn apparent_primitive_kind_for_type(&self, type_id: TypeId) -> Option<IntrinsicKind> {
        let key = self.interner.lookup(type_id);
        match key {
            Some(TypeKey::Intrinsic(kind)) => Some(kind),
            Some(TypeKey::Literal(literal)) => match literal {
                LiteralValue::String(_) => Some(IntrinsicKind::String),
                LiteralValue::Number(_) => Some(IntrinsicKind::Number),
                LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
                LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
            },
            Some(TypeKey::TemplateLiteral(_)) => Some(IntrinsicKind::String),
            _ => None,
        }
    }
}
