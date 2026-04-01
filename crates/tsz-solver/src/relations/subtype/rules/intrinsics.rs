//! Intrinsic (primitive) type subtype checking.
//!
//! This module handles subtyping for TypeScript's built-in primitive types:
//! - Intrinsic types (number, string, boolean, bigint, symbol, void, null, undefined)
//! - The `object` keyword type
//! - The `Function` type
//! - Apparent primitive shapes (for object-like operations on primitives)

use crate::TypeDatabase;
use crate::objects::apparent::apparent_primitive_shape;
use crate::operations::iterators::get_iterator_info;
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
    /// Extract the yield type from a target that has a `[Symbol.iterator]` method
    /// returning a type application (e.g., `ArrayIterator<any>`). This is a
    /// direct shape-level check used as a fallback when `get_iterator_info` fails.
    fn extract_iterable_yield_type_from_target(&self, target: TypeId) -> Option<TypeId> {
        let shape_id = object_shape_id(self.interner, target)
            .or_else(|| object_with_index_shape_id(self.interner, target))?;
        let shape = self.interner.object_shape(shape_id);
        let sym_iter_atom = self.interner.intern_string("[Symbol.iterator]");
        let iter_prop = shape
            .properties
            .binary_search_by_key(&sym_iter_atom, |p| p.name)
            .ok()
            .map(|idx| &shape.properties[idx])?;
        let callable_id = callable_shape_id(self.interner, iter_prop.type_id)?;
        let callable = self.interner.callable_shape(callable_id);
        let return_type = callable.call_signatures.first()?.return_type;
        let app_id = application_id(self.interner, return_type)?;
        let app = self.interner.type_application(app_id);
        app.args.first().copied()
    }

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

    /// Structurally detect whether a type is the global `Function` interface.
    ///
    /// After pre-evaluation, `Function` from lib.d.ts becomes an `ObjectShape` and
    /// loses its identity. This detects it by checking for the characteristic
    /// properties: `apply`, `call`, and `bind`, with a property count cap to
    /// avoid false matches on unrelated interfaces.
    pub(crate) fn is_function_interface_structural(&self, target: TypeId) -> bool {
        let shape_id = object_shape_id(self.interner, target)
            .or_else(|| object_with_index_shape_id(self.interner, target));
        let Some(shape_id) = shape_id else {
            return false;
        };
        let shape = self.interner.object_shape(shape_id);
        // Function interface has ~8 own properties + ~7 inherited Object properties = ~15.
        // Cap at 20 to avoid false positives on large interfaces.
        if shape.properties.len() > 20 {
            return false;
        }
        let apply = self.interner.intern_string("apply");
        let call = self.interner.intern_string("call");
        let bind = self.interner.intern_string("bind");
        shape.properties.iter().any(|p| p.name == apply)
            && shape.properties.iter().any(|p| p.name == call)
            && shape.properties.iter().any(|p| p.name == bind)
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
    /// Check if a target type has named properties beyond `[Symbol.iterator]`
    /// and `__@iterator` (the iterable protocol). Properties like `length` and
    /// numeric index signatures that `String` naturally has are also excluded.
    fn target_has_non_iterable_properties(&self, target: TypeId) -> bool {
        let shape = object_shape_id(self.interner, target)
            .or_else(|| object_with_index_shape_id(self.interner, target))
            .map(|id| self.interner.object_shape(id));
        let Some(shape) = shape else {
            return false;
        };
        let sym_iter = self.interner.intern_string("[Symbol.iterator]");
        let internal_iter = self.interner.intern_string("__@iterator");
        let length_atom = self.interner.intern_string("length");
        for prop in &shape.properties {
            if prop.name == sym_iter || prop.name == internal_iter || prop.name == length_atom {
                continue;
            }
            // This property is not part of the iterable protocol or String's natural shape
            return true;
        }
        false
    }

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

        // String-iterable shortcut: when the target is iterable with a yield type
        // compatible with `string`, check if the target is PURELY iterable (no extra
        // named properties beyond what String provides). This is needed because the
        // registered boxed `String` type may not include the es2015 `[Symbol.iterator]`
        // augmentation, so `String <: Iterable<string>` would fail structurally.
        //
        // However, we must NOT allow `string` to be assignable to types like `IArguments`
        // that are iterable but also have additional properties (e.g., `callee: Function`)
        // that `string`/`String` lacks.
        if source_kind == IntrinsicKind::String {
            let iterable_match = (|| {
                if let Some(db) = self.query_db
                    && let Some(iter_info) = get_iterator_info(db, target, false)
                    && self
                        .check_subtype(TypeId::STRING, iter_info.yield_type)
                        .is_true()
                {
                    return true;
                }
                if let Some(yield_type) = self.extract_iterable_yield_type_from_target(target)
                    && self.check_subtype(TypeId::STRING, yield_type).is_true()
                {
                    return true;
                }
                false
            })();

            if iterable_match {
                // The target is iterable with compatible yield type. Now check
                // whether the target has additional properties that the boxed
                // String type cannot satisfy. If the boxed type check passes,
                // the shortcut is valid. If it fails, only allow the shortcut
                // when the target has NO extra named properties beyond what
                // the iterable protocol requires.
                let boxed_type = self
                    .resolver
                    .get_boxed_type(source_kind)
                    .or_else(|| self.interner.get_boxed_type(source_kind));
                if let Some(boxed_type) = boxed_type {
                    let saved = self.in_intersection_member_check;
                    self.in_intersection_member_check = false;
                    let ok = self.check_subtype(boxed_type, target).is_true();
                    self.in_intersection_member_check = saved;
                    if ok {
                        return true;
                    }
                }
                // Boxed type doesn't satisfy all target properties. Check if
                // the target only has iterable-related properties (no extras).
                let target_has_extra_props = self.target_has_non_iterable_properties(target);
                if !target_has_extra_props {
                    return true;
                }
                // Target has extra properties — fall through to normal boxed check.
            }
        }

        // Ask the resolver for the boxed type, falling back to the interner
        // when the resolver can't provide it (e.g., type_env borrow conflict).
        let boxed_type = self
            .resolver
            .get_boxed_type(source_kind)
            .or_else(|| self.interner.get_boxed_type(source_kind));
        if let Some(boxed_type) = boxed_type {
            // If target is exactly the boxed interface (e.g., Number)
            if target == boxed_type {
                return true;
            }
            // Reset `in_intersection_member_check` for the boxed structural check.
            // The boxed type comparison is a fresh structural query — the boxed
            // wrapper should NOT bypass weak type detection.
            let saved = self.in_intersection_member_check;
            self.in_intersection_member_check = false;
            let result = self.check_subtype(boxed_type, target).is_true();
            self.in_intersection_member_check = saved;
            return result;
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
            // Shape-level property check: verify target contains all properties of the
            // boxed type by name. This handles two cases:
            // 1. Exact match: same interface resolved through different interning paths
            //    (e.g., different [Symbol.iterator] TypeIds).
            // 2. Augmented superset: user augmented a built-in interface with additional
            //    heritage members (e.g., `interface Number extends ICloneable {}`).
            //    The boxed type may be resolved from lib declarations only, while the
            //    target includes augmented heritage members. In this case target has all
            //    of boxed's properties PLUS the augmentation extras.
            // Both Object and ObjectWithIndex shapes are checked.
            let b_sid = object_with_index_shape_id(self.interner, boxed)
                .or_else(|| object_shape_id(self.interner, boxed));
            let t_sid = object_with_index_shape_id(self.interner, target)
                .or_else(|| object_shape_id(self.interner, target));
            if let (Some(b_sid), Some(t_sid)) = (b_sid, t_sid) {
                let b_shape = self.interner.object_shape(b_sid);
                let t_shape = self.interner.object_shape(t_sid);
                // Target must have at least as many properties as boxed, and ALL
                // of boxed's property names must appear in target's properties.
                if t_shape.properties.len() >= b_shape.properties.len()
                    && !b_shape.properties.is_empty()
                    && b_shape
                        .properties
                        .iter()
                        .all(|bp| t_shape.properties.iter().any(|tp| tp.name == bp.name))
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
