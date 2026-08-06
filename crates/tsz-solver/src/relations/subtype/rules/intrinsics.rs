//! Intrinsic (primitive) type subtype checking.
//!
//! This module handles subtyping for TypeScript's built-in primitive types:
//! - Intrinsic types (number, string, boolean, bigint, symbol, void, null, undefined)
//! - The `object` keyword type
//! - The `Function` type
//! - Apparent primitive shapes (for object-like operations on primitives)

use std::sync::Arc;

use crate::construction::TypeDatabase;
use crate::objects::apparent::apparent_primitive_shape;
use crate::operations::iterators::{get_iterator_info, target_has_non_iterable_property_shape};
use crate::types::{FunctionShape, IntrinsicKind, LiteralValue, ObjectShape, TypeId};
use crate::visitor::{
    application_id, array_element_type, callable_shape_id, function_shape_id, intersection_list_id,
    intrinsic_kind, is_this_type, lazy_def_id, literal_value, mapped_type_id, object_shape_id,
    object_with_index_shape_id, readonly_inner_type, template_literal_id, tuple_list_id,
    type_param_info, union_list_id,
};

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};
use super::intrinsic_object::{IntrinsicObjectKind, intrinsic_vs_object_super};

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

const fn apparent_primitive_shape_slot(kind: IntrinsicKind) -> Option<usize> {
    match kind {
        IntrinsicKind::String => Some(0),
        IntrinsicKind::Number => Some(1),
        IntrinsicKind::Boolean => Some(2),
        IntrinsicKind::Bigint => Some(3),
        IntrinsicKind::Symbol => Some(4),
        _ => None,
    }
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

    /// Apply a structural predicate over a composite `source` — a union,
    /// intersection, or type parameter — using the variance-correct policy that
    /// every such predicate must share:
    ///
    /// - **Union (`all`)**: the predicate holds only if it holds for *every*
    ///   member (a union value could be any member, so all must qualify).
    /// - **Intersection (`any`)**: the predicate holds if it holds for *any*
    ///   member (an intersection value is all members at once, so one qualifying
    ///   member suffices). Omitted entirely when `check_intersection` is `false`
    ///   — `is_global_object_interface_type` deliberately has no intersection arm.
    /// - **Type parameter**: the predicate holds if the parameter has a
    ///   constraint that satisfies `constraint_leaf`.
    ///
    /// This is the all-over-union / any-over-intersection soundness contract;
    /// centralizing it means a future predicate cannot silently drop a branch.
    /// The recursion leaf is parameterized because it differs per predicate
    /// (some recurse into themselves, `is_object_keyword_type` falls back to a
    /// `check_subtype(_, OBJECT)` for its constraint arm), so this is a helper
    /// rather than a verbatim macro.
    ///
    /// Returns `Some(result)` when `source` matched one of the composite arms,
    /// or `None` when it is not a composite source so the caller falls through to
    /// its own leaf checks. The union/intersection/type-parameter `TypeData`
    /// variants are mutually exclusive, so the internal arm order does not change
    /// any outcome.
    fn predicate_over_composite(
        &mut self,
        source: TypeId,
        member_leaf: fn(&mut Self, TypeId) -> bool,
        constraint_leaf: fn(&mut Self, TypeId) -> bool,
        check_intersection: bool,
    ) -> Option<bool> {
        if let Some(members) = union_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            return Some(members.iter().all(|&m| member_leaf(self, m)));
        }

        if check_intersection && let Some(members) = intersection_list_id(self.interner, source) {
            let members = self.interner.type_list(members);
            return Some(members.iter().any(|&m| member_leaf(self, m)));
        }

        if let Some(info) = type_param_info(self.interner, source) {
            return Some(
                info.constraint
                    .is_some_and(|constraint| constraint_leaf(self, constraint)),
            );
        }

        None
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
        if source == TypeId::ERROR {
            return true;
        }
        if let Some(kind) = intrinsic_kind(self.interner, source) {
            // `source` is the relation source here, so consult the
            // source-side `any` allowance.
            let allow_any = self
                .any_propagation
                .allows_any_source_at_depth(self.guard.depth());
            return match intrinsic_vs_object_super(kind, IntrinsicObjectKind::ObjectKeyword) {
                Some(result) => result,
                // `any` is mode-dependent for the `object` keyword.
                None => allow_any,
            };
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

        // Union (all) / intersection (any) / type-parameter recursion. The
        // member leaf recurses into this predicate; the type-parameter
        // constraint instead defers to `check_subtype(_, OBJECT)`.
        if let Some(result) = self.predicate_over_composite(
            source,
            Self::is_object_keyword_type,
            |checker, constraint| checker.check_subtype(constraint, TypeId::OBJECT).is_true(),
            true,
        ) {
            return result;
        }

        if let Some(def_id) = lazy_def_id(self.interner, source) {
            let resolved = self.resolver.resolve_lazy(def_id, self.interner);
            if let Some(resolved) = resolved {
                return self.check_subtype(resolved, TypeId::OBJECT).is_true();
            }
            self.note_unresolved_lazy_relation_event();
        }

        false
    }

    /// Check compatibility with the global `Object` interface type.
    ///
    /// TypeScript's uppercase `Object` accepts all non-nullish values, including
    /// primitives (unlike lowercase `object` which rejects primitives).
    pub(crate) fn is_global_object_interface_type(&mut self, source: TypeId) -> bool {
        if source == TypeId::ERROR {
            return true;
        }
        if let Some(kind) = intrinsic_kind(self.interner, source) {
            // `any` (None) is always compatible with the global Object interface.
            return intrinsic_vs_object_super(kind, IntrinsicObjectKind::GlobalObject)
                .unwrap_or(true);
        }

        // Union (all) / type-parameter recursion, both recursing into this
        // predicate. The global `Object` interface accepts primitives, so unlike
        // `object`/`Function` it deliberately has *no* intersection arm
        // (`check_intersection` is `false`): an intersection is handled by the
        // `true` fallthrough below, not by an any-over-members check.
        if let Some(result) = self.predicate_over_composite(
            source,
            Self::is_global_object_interface_type,
            Self::is_global_object_interface_type,
            false,
        ) {
            return result;
        }

        if let Some(inner) = readonly_inner_type(self.interner, source) {
            return self.is_global_object_interface_type(inner);
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
    pub(crate) fn is_callable_type(&mut self, source: TypeId) -> bool {
        // `source` is the relation source (a candidate callable), so consult
        // the source-side `any` allowance.
        let allow_any = self
            .any_propagation
            .allows_any_source_at_depth(self.guard.depth());
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

        // Union (all) / intersection (any) / type-parameter recursion, all
        // recursing into this predicate.
        if let Some(result) = self.predicate_over_composite(
            source,
            Self::is_callable_type,
            Self::is_callable_type,
            true,
        ) {
            return result;
        }

        false
    }

    /// Structurally detect whether a type is the global `Function` interface.
    ///
    /// After pre-evaluation, `Function` from lib.d.ts becomes an `ObjectShape` and
    /// loses its identity. Delegates to the canonical shared structural fallback
    /// in `type_queries::global_interfaces` (issue #13090). Deliberately
    /// structural-only: callers like `core_dispatch` treat structural matches
    /// differently from boxed-registry identity matches.
    pub(crate) fn is_function_interface_structural(&self, target: TypeId) -> bool {
        crate::type_queries::matches_global_function_interface_shape(self.interner, target)
    }

    /// Whether a structural-`Function` target (apply/call/bind) additionally
    /// declares a numeric index signature that a bare function cannot satisfy.
    ///
    /// `tsc` models a function value's apparent type as its call signatures plus
    /// the members of the global `Function` interface, and that apparent type
    /// carries **no** numeric index signature. So a target that matches the
    /// Function surface but also declares `[n: number]: T` — the shape a user
    /// `interface Function { [n: number]: T }` augmentation produces — is not
    /// satisfied by a function no matter which other members it requires. The
    /// intrinsic `Function` and the un-augmented global interface carry no such
    /// index and are unaffected. Mirrors the dual-`any`-index waiver the object
    /// index path already honors (`indexSignaturesRelatedTo` short-circuits a
    /// target whose numeric and string indexes are both `any`), so the two paths
    /// agree on `{ [k: string]: any; [n: number]: any }`.
    pub(crate) fn function_structural_target_has_unwaived_number_index(
        &self,
        target: TypeId,
    ) -> bool {
        let shape_id = crate::visitors::visitor_extract::object_shape_id(self.interner, target)
            .or_else(|| {
                crate::visitors::visitor_extract::object_with_index_shape_id(self.interner, target)
            });
        let Some(shape_id) = shape_id else {
            return false;
        };
        let shape = self.interner.object_shape(shape_id);
        shape.number_index.is_some()
            && !self.target_dual_any_index_waives_missing_number_index(&shape)
    }

    /// Whether a structural-`Function` target (apply/call/bind) additionally
    /// declares a string index signature that a bare function cannot satisfy.
    ///
    /// Same rationale as [`Self::function_structural_target_has_unwaived_number_index`]
    /// (companion to #16473), extended to the string-index sibling: a function
    /// value's apparent type carries no string index either, so a target that
    /// matches the `Function` surface but also declares `[x: string]: T` is not
    /// satisfied by a function no matter which other members it requires.
    ///
    /// Unlike the number-index case, a lone `any`-valued string index waives the
    /// requirement outright (`tsc`'s `indexSignaturesRelatedTo` treats a target
    /// string index of `any` as satisfied by anything, string-indexed or not —
    /// [`Self::target_string_index_any_waives_missing_index`]); it does not need
    /// a paired `any`-valued number index the way the dual-any waiver does.
    pub(crate) fn function_structural_target_has_unwaived_string_index(
        &self,
        target: TypeId,
    ) -> bool {
        let shape_id = crate::visitors::visitor_extract::object_shape_id(self.interner, target)
            .or_else(|| {
                crate::visitors::visitor_extract::object_with_index_shape_id(self.interner, target)
            });
        let Some(shape_id) = shape_id else {
            return false;
        };
        let shape = self.interner.object_shape(shape_id);
        shape
            .string_index
            .as_ref()
            .is_some_and(|idx| !self.target_string_index_any_waives_missing_index(idx.value_type))
    }

    /// Get the apparent primitive shape for a type.
    ///
    /// When primitives are used in object-like operations (e.g., `"hello".length`),
    /// TypeScript wraps them in their corresponding wrapper types. This function
    /// returns the object shape that represents those wrapper type members.
    pub(crate) fn apparent_primitive_shape_for_type(
        &mut self,
        type_id: TypeId,
    ) -> Option<Arc<ObjectShape>> {
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
    pub(crate) fn apparent_primitive_shape(&mut self, kind: IntrinsicKind) -> Arc<ObjectShape> {
        let slot = apparent_primitive_shape_slot(kind)
            .expect("apparent primitive shapes are only defined for boxed primitives");
        if let Some(shape) = &self.apparent_primitive_shapes[slot] {
            return Arc::clone(shape);
        }

        let shape = Arc::new(apparent_primitive_shape(
            self.interner,
            kind,
            make_subtype_method_type,
        ));
        self.apparent_primitive_shapes[slot] = Some(Arc::clone(&shape));
        shape
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
    fn target_has_non_iterable_properties(&mut self, target: TypeId) -> bool {
        target_has_non_iterable_property_shape(self.interner, target, |t| self.evaluate_type(t))
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

        // Primitive wrapper fallback must not make primitives assignable to tuple
        // targets. Structural checks against boxed interfaces can otherwise
        // incorrectly accept cases like `string <: [any]`.
        let evaluated_target = self.evaluate_type(target);
        if tuple_list_id(self.interner, target).is_some()
            || tuple_list_id(self.interner, evaluated_target).is_some()
        {
            return false;
        }
        if readonly_inner_type(self.interner, target)
            .is_some_and(|inner| tuple_list_id(self.interner, inner).is_some())
            || readonly_inner_type(self.interner, evaluated_target)
                .is_some_and(|inner| tuple_list_id(self.interner, inner).is_some())
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::DefId;
    use crate::intern::TypeInterner;

    #[test]
    fn apparent_primitive_shape_is_cached_per_checker() {
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);

        let first = checker
            .apparent_primitive_shape_for_type(TypeId::STRING)
            .expect("string should have an apparent shape");
        let cached_after_first = checker.apparent_primitive_shapes.clone();
        let second = checker
            .apparent_primitive_shape_for_type(TypeId::STRING)
            .expect("string should reuse its apparent shape");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(
            cached_after_first, checker.apparent_primitive_shapes,
            "repeated lookup should not intern another apparent shape"
        );
        assert_eq!(
            checker
                .apparent_primitive_shapes
                .iter()
                .filter(|entry| entry.is_some())
                .count(),
            1
        );
    }

    #[test]
    fn unresolved_lazy_object_keyword_probe_records_relation_event() {
        crate::limits::reset_subtype_thread_local_state();
        let interner = TypeInterner::new();
        let mut checker = SubtypeChecker::new(&interner);
        let unresolved = interner.lazy(DefId(9001));
        let before = checker.unresolved_lazy_relation_event_count();

        assert!(!checker.is_object_keyword_type(unresolved));
        assert_ne!(
            checker.unresolved_lazy_relation_event_count(),
            before,
            "Lazy <: object miss must keep the relation result non-cacheable"
        );
        crate::limits::reset_subtype_thread_local_state();
    }
}
