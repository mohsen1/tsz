//! Core implementation for contextual typing (reverse inference).
//!
//! See the parent [`contextual`](super) module for overview documentation.

use crate::TypeDatabase;
use crate::contextual::extractors::{
    ApplicationArgExtractor, ArrayElementExtractor, ParameterExtractor, ParameterForCallExtractor,
    PropertyExtractor, RestOrOptionalTailPositionExtractor, RestParameterExtractor,
    RestPositionCheckExtractor, ReturnTypeExtractor, ThisTypeExtractor, ThisTypeMarkerExtractor,
    TupleElementExtractor, collect_from_intersection, collect_single_or_union,
    collect_single_or_union_no_reduce, extract_param_type_at_for_call,
};
#[cfg(test)]
use crate::types::*;
use crate::types::{IntrinsicKind, TypeData, TypeId};

/// Context for contextual typing.
/// Holds the expected type and provides methods to extract type information.
pub struct ContextualTypeContext<'a> {
    interner: &'a dyn TypeDatabase,
    /// The expected type (contextual type)
    expected: Option<TypeId>,
    /// Whether noImplicitAny is enabled (affects contextual typing for multi-signature functions)
    no_implicit_any: bool,
}

/// Extract the per-argument contextual type from a rest parameter type.
///
/// For array rest params like `...args: Foo[]`, this returns `Foo`.
/// For tuple rest params, this returns the trailing rest element type when present.
/// Evaluatable wrappers such as `ConstructorParameters<T>` are normalized first so
/// generic call round-2 contextual typing doesn't pass the whole tuple application
/// through as a single argument type.
pub fn rest_argument_element_type(db: &dyn crate::TypeDatabase, type_id: TypeId) -> TypeId {
    fn rest_argument_element_type_inner(
        db: &dyn crate::TypeDatabase,
        type_id: TypeId,
        depth: usize,
    ) -> TypeId {
        if depth == 0 {
            return type_id;
        }

        match db.lookup(type_id) {
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                rest_argument_element_type_inner(db, inner, depth - 1)
            }
            Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
                .constraint
                .filter(|&constraint| constraint != type_id)
                .map(|constraint| rest_argument_element_type_inner(db, constraint, depth - 1))
                .unwrap_or(type_id),
            Some(TypeData::Union(members_id)) => {
                let members = db.type_list(members_id);
                let extracted: Vec<_> = members
                    .iter()
                    .map(|&member| rest_argument_element_type_inner(db, member, depth - 1))
                    .collect();
                crate::utils::union_or_single(db, extracted)
            }
            Some(TypeData::Array(elem)) => elem,
            Some(TypeData::Tuple(elements_id)) => {
                let elements = db.tuple_list(elements_id);
                if let Some(last) = elements.last() {
                    if last.rest {
                        match db.lookup(last.type_id) {
                            Some(TypeData::Array(elem)) => elem,
                            _ => last.type_id,
                        }
                    } else {
                        last.type_id
                    }
                } else {
                    type_id
                }
            }
            Some(
                TypeData::Application(_)
                | TypeData::Conditional(_)
                | TypeData::Mapped(_)
                | TypeData::Lazy(_)
                | TypeData::IndexAccess(_, _),
            ) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(db, type_id);
                if evaluated != type_id {
                    rest_argument_element_type_inner(db, evaluated, depth - 1)
                } else {
                    type_id
                }
            }
            _ => type_id,
        }
    }

    rest_argument_element_type_inner(db, type_id, 8)
}

impl<'a> ContextualTypeContext<'a> {
    fn property_name_to_key_type(&self, name: &str) -> TypeId {
        if let Some(symbol_ref) = name.strip_prefix("__unique_")
            && let Ok(id) = symbol_ref.parse::<u32>()
        {
            return self.interner.unique_symbol(crate::types::SymbolRef(id));
        }
        self.interner.literal_string(name)
    }

    /// Create a new contextual type context.
    /// Defaults to `no_implicit_any: false` for compatibility.
    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        ContextualTypeContext {
            interner,
            expected: None,
            no_implicit_any: false,
        }
    }

    /// Create a context with an expected type.
    /// Defaults to `no_implicit_any: false` for compatibility.
    pub fn with_expected(interner: &'a dyn TypeDatabase, expected: TypeId) -> Self {
        ContextualTypeContext {
            interner,
            expected: Some(expected),
            no_implicit_any: false,
        }
    }

    /// Create a context with an expected type and explicit noImplicitAny setting.
    pub fn with_expected_and_options(
        interner: &'a dyn TypeDatabase,
        expected: TypeId,
        no_implicit_any: bool,
    ) -> Self {
        ContextualTypeContext {
            interner,
            expected: Some(expected),
            no_implicit_any,
        }
    }

    /// Get the expected type.
    pub const fn expected(&self) -> Option<TypeId> {
        self.expected
    }

    /// Check if we have a contextual type.
    pub const fn has_context(&self) -> bool {
        self.expected.is_some()
    }

    /// Get the contextual type for a function parameter at the given index.
    ///
    /// Example:
    /// ```typescript
    /// type Handler = (e: string, i: number) => void;
    /// const h: Handler = (x, y) => {};  // x: string, y: number from context
    /// ```
    pub fn get_parameter_type(&self, index: usize) -> Option<TypeId> {
        let expected = self.expected?;

        // `Function` (intrinsic or boxed) is callable with `any` parameters.
        // Returning `None` here causes false TS7006 for callbacks constrained by
        // `T extends Function` (e.g. deprecate wrappers).
        if self.is_function_boxed_or_intrinsic(expected) {
            return Some(TypeId::ANY);
        }

        // Handle Union explicitly - collect parameter types from callable members.
        // Per TypeScript spec: "If S is not empty and the sets of call signatures of the
        // types in S are identical ignoring return types, U has the same set of call
        // signatures." If parameter types differ across callable members at the same
        // index, no contextual type is provided (triggers TS7006 under noImplicitAny).
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let mut param_types: Vec<TypeId> = Vec::new();
            let mut has_callable_member = false;

            for &m in members.iter() {
                // Check if this member is callable (has call signatures)
                let is_callable = crate::type_queries::is_callable_type(self.interner, m);
                if !is_callable {
                    // Non-callable member — excluded from set S per spec
                    continue;
                }
                has_callable_member = true;

                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    m,
                    self.no_implicit_any,
                );
                match ctx.get_parameter_type(index) {
                    Some(ty) => param_types.push(ty),
                    None => {
                        // Callable member returned None — either:
                        // - Multiple overloads with disagreeing param types
                        // - No parameter at this index for any signature
                        // In either case, the signatures are not identical
                        // across members, so no contextual type is provided.
                        // However, for arity differences (member has fewer params),
                        // tsc still provides contextual types from other members.
                        // We check: does this callable have ANY signature with a
                        // param at this index? If yes → internal disagreement → None.
                        // If no → arity gap → skip this member.
                        if self.callable_has_param_at_index(m, index) {
                            return None;
                        }
                        // Arity gap — this member simply has fewer params; skip.
                    }
                }
            }

            if !has_callable_member || param_types.is_empty() {
                return None;
            }
            // When all callable union members agree on the parameter type, return it directly.
            // When they disagree, a direct union of callable types does not provide a
            // contextual parameter type. This matches conformance cases like
            // `IWithCallSignatures | IWithCallSignatures3`, where the callback
            // parameter should remain implicit-any and report TS7006.
            let first = param_types[0];
            if param_types.iter().all(|&t| t == first) {
                return Some(first);
            }
            return None;
        }

        // Handle Application explicitly.
        // First try evaluating the applied type so type arguments are preserved
        // (e.g. GenericFn<string> -> (x: string) => ...). Falling back directly
        // to `base` loses substitution information and causes false TS7006.
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    evaluated,
                    self.no_implicit_any,
                );
                return ctx.get_parameter_type(index);
            }
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected_and_options(
                self.interner,
                app.base,
                self.no_implicit_any,
            );
            return ctx.get_parameter_type(index);
        }

        // Handle Intersection explicitly - combine member contributions and
        // ignore broad `any` fallbacks when a more specific contextual type exists.
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected_and_options(
                        self.interner,
                        m,
                        self.no_implicit_any,
                    );
                    ctx.get_parameter_type(index)
                })
                .collect();
            return collect_from_intersection(self.interner, param_types, |db, tys| db.union(tys));
        }

        // Handle Mapped, Conditional, Lazy, and IndexAccess types by evaluating them first.
        // PERF: Single lookup for both the guard check and Conditional extraction.
        if let Some(expected_key) = self.interner.lookup(expected)
            && matches!(
                expected_key,
                TypeData::Mapped(_)
                    | TypeData::Conditional(_)
                    | TypeData::Lazy(_)
                    | TypeData::IndexAccess(_, _)
            )
        {
            if let TypeData::Conditional(cond_id) = expected_key {
                let cond = self.interner.get_conditional(cond_id);
                let mut branch_param_types = Vec::new();
                for branch in [cond.true_type, cond.false_type] {
                    // Guard against self-recursive aliases.
                    if branch == expected {
                        continue;
                    }
                    let ctx = ContextualTypeContext::with_expected_and_options(
                        self.interner,
                        branch,
                        self.no_implicit_any,
                    );
                    if let Some(ty) = ctx.get_parameter_type(index) {
                        branch_param_types.push(ty);
                    }
                }
                if let Some(resolved) = collect_single_or_union(self.interner, branch_param_types) {
                    return Some(resolved);
                }
            }
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    evaluated,
                    self.no_implicit_any,
                );
                return ctx.get_parameter_type(index);
            }
        }

        // Handle TypeParameter - use its constraint for parameter type extraction.
        // Example: f<T extends (p1: number) => number>(callback: T)
        // When the contextual type is T (a TypeParameter), use its constraint
        // (p1: number) => number to extract the parameter type.
        // Without this, callbacks passed to generic functions get TS7006 (false positive)
        // because get_parameter_type() returns None for unhandled TypeParameter variants.
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected_and_options(
                self.interner,
                constraint,
                self.no_implicit_any,
            );
            return ctx.get_parameter_type(index);
        }

        // Use visitor for Function/Callable types
        let mut extractor = ParameterExtractor::new(self.interner, index, self.no_implicit_any);
        extractor.extract(expected)
    }

    /// Get the contextual type for a **rest** callback parameter at position `index`.
    ///
    /// Unlike `get_parameter_type` which returns the element type at a specific position,
    /// this returns the full tuple/array type that a rest parameter should receive.
    ///
    /// Example: contextual type `(a: string, b: number, c: boolean) => void`
    /// - `get_parameter_type(0)` → `string` (for non-rest param `a`)
    /// - `get_rest_parameter_type(0)` → `[string, number, boolean]` (for rest param `...x`)
    pub fn get_rest_parameter_type(&self, index: usize) -> Option<TypeId> {
        let expected = self.expected?;

        if self.is_function_boxed_or_intrinsic(expected) {
            return Some(TypeId::ANY);
        }

        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let rest_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    let ctx = ContextualTypeContext::with_expected_and_options(
                        self.interner,
                        member,
                        self.no_implicit_any,
                    );
                    ctx.get_rest_parameter_type(index)
                })
                .collect();
            return collect_single_or_union(self.interner, rest_types);
        }

        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    evaluated,
                    self.no_implicit_any,
                );
                return ctx.get_rest_parameter_type(index);
            }
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected_and_options(
                self.interner,
                app.base,
                self.no_implicit_any,
            );
            return ctx.get_rest_parameter_type(index);
        }

        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let rest_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&member| {
                    let ctx = ContextualTypeContext::with_expected_and_options(
                        self.interner,
                        member,
                        self.no_implicit_any,
                    );
                    ctx.get_rest_parameter_type(index)
                })
                .collect();
            return collect_from_intersection(self.interner, rest_types, |db, tys| db.union(tys));
        }

        if let Some(
            TypeData::Mapped(_)
            | TypeData::Conditional(_)
            | TypeData::Lazy(_)
            | TypeData::IndexAccess(_, _),
        ) = self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    evaluated,
                    self.no_implicit_any,
                );
                return ctx.get_rest_parameter_type(index);
            }
        }

        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected_and_options(
                self.interner,
                constraint,
                self.no_implicit_any,
            );
            return ctx.get_rest_parameter_type(index);
        }

        // Use visitor for Function/Callable types
        let mut extractor = RestParameterExtractor::new(self.interner, index);
        extractor.extract(expected)
    }

    /// Get the contextual type for a call argument at the given index and arity.
    pub fn get_parameter_type_for_call(&self, index: usize, arg_count: usize) -> Option<TypeId> {
        let expected = self.expected?;

        // `Function` (intrinsic or boxed) accepts arbitrary arguments of type `any`.
        if self.is_function_boxed_or_intrinsic(expected) {
            return Some(TypeId::ANY);
        }

        // Handle Union explicitly - collect parameter types from all members.
        // Use literal-only union reduction (no subtype reduction) to preserve
        // all callback type variants. Full subtype reduction can incorrectly
        // absorb callback types due to parameter contravariance. For example,
        // with `Array<string>.map | Array<never>.map`, the callback
        // `(value: string) => U` is a subtype of `(value: never) => U`
        // (contravariant params), so subtype reduction would discard the
        // string variant, losing contextual type information for parameters.
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_parameter_type_for_call(index, arg_count)
                        .or_else(|| {
                            let evaluated =
                                crate::evaluation::evaluate::evaluate_type(self.interner, m);
                            if evaluated != m {
                                let evaluated_ctx =
                                    ContextualTypeContext::with_expected(self.interner, evaluated);
                                evaluated_ctx.get_parameter_type_for_call(index, arg_count)
                            } else {
                                None
                            }
                        })
                })
                .collect();

            return collect_single_or_union_no_reduce(self.interner, param_types);
        }

        // Handle Application explicitly.
        // Preserve application-instantiated signatures. Unwrapping to the base type
        // discards instantiated parameter types like `Iterable<readonly [K, V]>`,
        // which in turn breaks nested generic call contextual typing.
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            if let Some(shape) = crate::get_contextual_signature_for_arity_with_compat_checker(
                self.interner,
                expected,
                arg_count,
            ) {
                return extract_param_type_at_for_call(
                    self.interner,
                    &shape.params,
                    index,
                    arg_count,
                );
            }

            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_parameter_type_for_call(index, arg_count);
            }

            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_parameter_type_for_call(index, arg_count);
        }

        // Handle Intersection explicitly - combine member contributions and
        // ignore broad `any` fallbacks when a more specific contextual type exists.
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_parameter_type_for_call(index, arg_count)
                })
                .collect();
            return collect_from_intersection(self.interner, param_types, |db, tys| db.union(tys));
        }

        // Handle TypeParameter - use its constraint for parameter type extraction.
        // Example: f<T extends (p1: number) => number>(callback: T) called as f(x => x)
        // When the contextual type is T (TypeParameter), use its constraint to get the
        // parameter type, so x is contextually typed as number (not any).
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_parameter_type_for_call(index, arg_count);
        }

        // Handle Mapped, Conditional, Lazy, and IndexAccess types by evaluating them first.
        // PERF: Single lookup for both the guard check and Conditional extraction.
        if let Some(expected_key) = self.interner.lookup(expected)
            && matches!(
                expected_key,
                TypeData::Mapped(_)
                    | TypeData::Conditional(_)
                    | TypeData::Lazy(_)
                    | TypeData::IndexAccess(_, _)
            )
        {
            if let TypeData::Conditional(cond_id) = expected_key {
                let cond = self.interner.get_conditional(cond_id);
                let mut branch_param_types = Vec::new();
                for (is_true_branch, branch) in [(true, cond.true_type), (false, cond.false_type)] {
                    // Guard against self-recursive aliases.
                    if branch == expected {
                        continue;
                    }
                    let ctx = ContextualTypeContext::with_expected(self.interner, branch);
                    if let Some(ty) = ctx.get_parameter_type_for_call(index, arg_count) {
                        if is_true_branch {
                            // Mirror tsc's conditional true-branch substitution for
                            // nested callback parameter positions:
                            //   (n: Check) => ...  becomes  (n: Check & Extends) => ...
                            // This prevents false TS2345 inside callbacks while preserving
                            // direct-argument contextual types like `arg(10)`.
                            branch_param_types.push(
                                self.apply_conditional_true_branch_param_substitution(ty, &cond),
                            );
                        } else {
                            branch_param_types.push(ty);
                        }
                    }
                }
                if let Some(resolved) = collect_single_or_union(self.interner, branch_param_types) {
                    return Some(resolved);
                }
            }
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_parameter_type_for_call(index, arg_count);
            }
        }

        // Use visitor for Function/Callable types
        let mut extractor = ParameterForCallExtractor::new(self.interner, index, arg_count);
        extractor.extract(expected)
    }

    /// Check if argument at `index` falls at a rest parameter position in the expected callable.
    ///
    /// Returns `true` if the expected callable's parameter at `index` is a rest parameter.
    /// For overloaded callables, returns `true` only if ALL matching signatures agree.
    /// Returns `false` if the expected type is not callable or `index` is at a non-rest position.
    pub fn is_rest_parameter_position(&self, index: usize, arg_count: usize) -> bool {
        let Some(expected) = self.expected else {
            return false;
        };
        // Unwrap Application to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.is_rest_parameter_position(index, arg_count);
        }

        // Handle TypeParameter via constraint
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.is_rest_parameter_position(index, arg_count);
        }

        // Evaluate Mapped/Conditional/Lazy/IndexAccess
        if let Some(
            TypeData::Mapped(_)
            | TypeData::Conditional(_)
            | TypeData::Lazy(_)
            | TypeData::IndexAccess(_, _),
        ) = self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.is_rest_parameter_position(index, arg_count);
            }
            // IndexAccess couldn't be evaluated — it still contains type parameters
            // (e.g., T[K] where T or K is generic).
            // When the callable shape is unknowable at the generic call site, tsc
            // gives the benefit of the doubt and does NOT emit TS2556.
            // Returning `true` suppresses the false positive.
            if let Some(TypeData::IndexAccess(_, _)) = self.interner.lookup(expected) {
                return true;
            }
        }

        let mut extractor = RestPositionCheckExtractor::new(self.interner, index, arg_count);
        extractor.extract(expected)
    }

    /// Check if a non-tuple spread is allowed because it lands on a rest
    /// parameter or only covers optional trailing parameters.
    pub fn allows_non_tuple_spread_position(&self, index: usize, arg_count: usize) -> bool {
        let Some(expected) = self.expected else {
            return false;
        };
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.allows_non_tuple_spread_position(index, arg_count);
        }

        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.allows_non_tuple_spread_position(index, arg_count);
        }

        // PERF: Single lookup for guard + IndexAccess check
        if let Some(expected_key) = self.interner.lookup(expected)
            && matches!(
                expected_key,
                TypeData::Mapped(_)
                    | TypeData::Conditional(_)
                    | TypeData::Lazy(_)
                    | TypeData::IndexAccess(_, _)
            )
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.allows_non_tuple_spread_position(index, arg_count);
            }
            if matches!(expected_key, TypeData::IndexAccess(_, _)) {
                return true;
            }
        }

        let mut extractor =
            RestOrOptionalTailPositionExtractor::new(self.interner, index, arg_count);
        extractor.extract(expected)
    }

    /// Get the contextual type for a `this` parameter, if present on the expected type.
    pub fn get_this_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect this types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let this_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_this_type()
                })
                .collect();

            return collect_single_or_union(self.interner, this_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_this_type();
        }

        // Use visitor for Function/Callable types
        let mut extractor = ThisTypeExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the type T from a `ThisType`<T> marker in the contextual type.
    ///
    /// This is used for the Vue 2 / Options API pattern where object literal
    /// methods have their `this` type overridden by contextual markers.
    ///
    /// Example:
    /// ```typescript
    /// type ObjectDescriptor<D, M> = {
    ///     methods?: M & ThisType<D & M>;
    /// };
    /// const obj: ObjectDescriptor<{x: number}, {greet(): void}> = {
    ///     methods: {
    ///         greet() { console.log(this.x); } // this is D & M
    ///     }
    /// };
    /// ```
    pub fn get_this_type_from_marker(&self) -> Option<TypeId> {
        let expected = self.expected?;
        let mut extractor = ThisTypeMarkerExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Extract `ThisType<T>` from the contextual type, expanding type alias
    /// applications if needed.
    ///
    /// When the contextual type is a type alias application like
    /// `ConstructorOptions<Data>` whose body is `Props<Data> & ThisType<Instance<Data>>`,
    /// the basic extractor can't see through the `Lazy(DefId)` base. This method
    /// uses the provided `TypeResolver` to expand the alias body, instantiate it
    /// with the application arguments, and retry the `ThisType` extraction.
    pub fn get_this_type_from_marker_with_resolver(
        &self,
        resolver: &dyn crate::TypeResolver,
    ) -> Option<TypeId> {
        // First try the simple extraction (no expansion needed).
        if let Some(result) = self.get_this_type_from_marker() {
            return Some(result);
        }

        let expected = self.expected?;

        // Check if the expected type is an Application whose base is a Lazy (type alias).
        // If so, expand the alias body and retry.
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base)
                && let Some(body) = resolver.resolve_lazy(def_id, self.interner)
            {
                let type_params = resolver.get_lazy_type_params(def_id).unwrap_or_default();
                let expanded =
                    crate::instantiate_generic(self.interner, body, &type_params, &app.args);
                let expanded_ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    expanded,
                    self.no_implicit_any,
                );
                return expanded_ctx.get_this_type_from_marker();
            }
        }

        None
    }

    /// Alias for [`get_this_type_from_marker_with_resolver`] — kept for
    /// callers that reference the previous name.
    #[inline]
    pub fn get_this_type_from_marker_expanding(
        &self,
        resolver: &dyn crate::TypeResolver,
    ) -> Option<TypeId> {
        self.get_this_type_from_marker_with_resolver(resolver)
    }

    /// Get the contextual return type for a function.
    pub fn get_return_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect return types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let return_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_return_type()
                })
                .collect();

            return collect_single_or_union(self.interner, return_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            if let Some(shape) =
                crate::get_contextual_signature_with_compat_checker(self.interner, expected)
            {
                return Some(shape.return_type);
            }
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_return_type();
        }

        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_return_type();
        }

        // Handle Lazy, Mapped, and Conditional types by evaluating first
        if let Some(
            TypeData::Lazy(_)
            | TypeData::Mapped(_)
            | TypeData::Conditional(_)
            | TypeData::IndexAccess(_, _),
        ) = self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_return_type();
            }
        }

        // Use visitor for Function/Callable types
        let mut extractor = ReturnTypeExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Get the contextual element type for an array.
    ///
    /// Example:
    /// ```typescript
    /// const arr: number[] = [1, 2, 3];  // elements are contextually typed as number
    /// ```
    pub fn get_array_element_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect element types from all array members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let elem_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_array_element_type()
                })
                .collect();
            return collect_single_or_union(self.interner, elem_types);
        }

        // Handle Application explicitly - evaluate to resolve type aliases.
        // For generic iterable-like types (Iterable<T>, ReadonlyArray<T>, ArrayLike<T>, etc.),
        // try to extract the element type from the first type argument, since these types
        // all use T as their element type.
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            // First try evaluating to see if it resolves to an array type
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                if let Some(elem) = ctx.get_array_element_type() {
                    return Some(elem);
                }
                // Check if the evaluated type has iterable-like structure
                if !app.args.is_empty() && self.is_iterable_like_object(evaluated) {
                    return Some(app.args[0]);
                }
            }
            // If evaluation didn't change the type (e.g., Lazy(DefId) base that can't
            // be resolved without TypeEnvironment), use the first type argument as
            // the element type. This is a reasonable heuristic because assigning an
            // array literal to a generic type like Iterable<T> or ArrayLike<T> means
            // T is the expected element type.
            if !app.args.is_empty() && evaluated == expected {
                return Some(app.args[0]);
            }
        }

        // Handle Mapped/Conditional/Lazy types
        if let Some(TypeData::Mapped(_) | TypeData::Conditional(_) | TypeData::Lazy(_)) =
            self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_array_element_type();
            }
        }

        // Handle TypeParameter - use its constraint for element extraction
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_array_element_type();
        }

        // Handle Intersection - intersect element types from all array members
        // For `Array<{key, value}> & Array<{value}>`, the contextual element type
        // should be `{key, value} & {value}` (= `{key, value}`), not just the first
        // member's element type. This prevents false excess property errors when
        // a property exists in one intersection member but not another.
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let elem_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_array_element_type()
                })
                .collect();
            return match elem_types.len() {
                0 => None,
                1 => Some(elem_types[0]),
                _ => Some(self.interner.intersection(elem_types)),
            };
        }

        let mut extractor = ArrayElementExtractor::new(self.interner);
        extractor.extract(expected)
    }

    /// Check if a type looks like an iterable or array-like object type.
    /// This is used as a heuristic to determine whether the first type argument
    /// of an Application is the element type (for contextual typing of array literals).
    fn is_iterable_like_object(&self, type_id: TypeId) -> bool {
        use crate::types::TypeData;

        // Check if type is an object with properties suggesting iterable/array-like behavior
        match self.interner.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = self.interner.object_shape(shape_id);
                // Has number index → array-like (ArrayLike<T>, ReadonlyArray<T>)
                if shape.number_index.is_some() {
                    return true;
                }
                // Has Symbol.iterator property → iterable (Iterable<T>)
                for prop in &shape.properties {
                    let name = self.interner.resolve_atom(prop.name);
                    if name == "__@iterator" || name == "[Symbol.iterator]" {
                        return true;
                    }
                }
                false
            }
            Some(TypeData::Intersection(members)) => {
                let members = self.interner.type_list(members);
                members.iter().any(|&m| self.is_iterable_like_object(m))
            }
            _ => false,
        }
    }

    /// Get the contextual type for a specific tuple element.
    pub fn get_tuple_element_type(&self, index: usize) -> Option<TypeId> {
        self.get_tuple_element_type_inner(index, None)
    }

    /// Get the contextual type for a tuple element, with knowledge of the total element count.
    /// This enables correct mapping for variadic tuple types like `[...T[], U]`.
    pub fn get_tuple_element_type_with_count(
        &self,
        index: usize,
        element_count: usize,
    ) -> Option<TypeId> {
        self.get_tuple_element_type_inner(index, Some(element_count))
    }

    fn get_tuple_element_type_inner(
        &self,
        index: usize,
        element_count: Option<usize>,
    ) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect tuple element types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let elem_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_tuple_element_type_inner(index, element_count)
                })
                .collect();
            return collect_single_or_union(self.interner, elem_types);
        }

        // Handle Intersection explicitly - collect tuple element types from all members
        // and intersect them. This ensures that when the contextual type is an intersection
        // of mapped types like `Results<T> & Errors<E>`, the element contextual type
        // includes properties from ALL members, enabling contextual typing of callbacks
        // in every intersection member.
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let elem_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_tuple_element_type_inner(index, element_count)
                })
                .collect();
            return match elem_types.len() {
                0 => None,
                1 => Some(elem_types[0]),
                _ => Some(self.interner.intersection(elem_types)),
            };
        }

        // Handle Application explicitly - evaluate to resolve type aliases
        if let Some(TypeData::Application(_)) = self.interner.lookup(expected) {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_tuple_element_type_inner(index, element_count);
            }
        }

        // Handle TypeParameter - use its constraint
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_tuple_element_type_inner(index, element_count);
        }

        // Handle Mapped, Conditional, and Lazy types by evaluating them first.
        // PERF: Single lookup for guard + Conditional extraction.
        if let Some(expected_key) = self.interner.lookup(expected)
            && matches!(
                expected_key,
                TypeData::Mapped(_) | TypeData::Conditional(_) | TypeData::Lazy(_)
            )
        {
            if let TypeData::Conditional(cond_id) = expected_key {
                let cond = self.interner.get_conditional(cond_id);
                let mut branch_elem_types = Vec::new();
                for branch in [cond.true_type, cond.false_type] {
                    // Guard against self-recursive aliases.
                    if branch == expected {
                        continue;
                    }
                    let ctx = ContextualTypeContext::with_expected(self.interner, branch);
                    if let Some(ty) = ctx.get_tuple_element_type_inner(index, element_count) {
                        branch_elem_types.push(ty);
                    }
                }
                if let Some(resolved) = collect_single_or_union(self.interner, branch_elem_types) {
                    return Some(resolved);
                }
            }
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_tuple_element_type_inner(index, element_count);
            }
        }

        let mut extractor = TupleElementExtractor::new(self.interner, index, element_count);
        extractor.extract(expected)
    }

    /// Get the contextual type for an object property.
    ///
    /// Example:
    /// ```typescript
    /// const obj: {x: number, y: string} = {x: 1, y: "hi"};
    /// ```
    pub fn get_property_type(&self, name: &str) -> Option<TypeId> {
        self.get_property_type_inner(name, false)
    }

    /// Get the contextual type for an object-literal property assignment.
    ///
    /// This uses the declared property type for optional properties so a present
    /// assignment like `{ x: 1 }` in `{ x?: number }` is checked against `number`
    /// rather than the read-side `number | undefined`.
    pub fn get_property_assignment_type(&self, name: &str) -> Option<TypeId> {
        self.get_property_type_inner(name, true)
    }

    fn get_property_type_inner(
        &self,
        name: &str,
        strip_optional_undefined: bool,
    ) -> Option<TypeId> {
        let expected = self.expected?;

        // Single lookup to dispatch on the type shape. Avoids multiple DashMap
        // lookups for the common case (Object types fall through to the extractor).
        match self.interner.lookup(expected) {
            Some(TypeData::Union(members)) => {
                let members = self.interner.type_list(members);
                let prop_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_property_type_inner(name, strip_optional_undefined)
                    })
                    .collect();

                // When some union members contribute `any` (typically from index
                // signatures like `{ [k: string]: any }`) and others contribute
                // specific types, filter out the `any` values. The `any` from
                // index signatures doesn't carry useful contextual information and
                // would cause literal types to be widened incorrectly.
                let prop_types = if prop_types.len() > 1
                    && prop_types.contains(&TypeId::ANY)
                    && prop_types.iter().any(|&t| t != TypeId::ANY)
                {
                    prop_types
                        .into_iter()
                        .filter(|&t| t != TypeId::ANY)
                        .collect()
                } else {
                    prop_types
                };

                return if prop_types.is_empty() {
                    None
                } else if prop_types.len() == 1 {
                    Some(prop_types[0])
                } else {
                    // CRITICAL: Use union_preserve_members to keep literal types intact
                    // For discriminated unions like `{ success: false } | { success: true }`,
                    // the property type should be `false | true`, NOT widened to `boolean`.
                    Some(self.interner.union_preserve_members(prop_types))
                };
            }
            Some(TypeData::Intersection(members)) => {
                // Handle Intersection explicitly - collect property types from all members.
                // This must go through get_property_type() per member (not the PropertyExtractor
                // visitor) so that each member gets the full handling pipeline, including
                // mapped-type-template extraction for patterns like T & {[P in keyof T]: V}.
                let members = self.interner.type_list(members);
                let prop_types: Vec<TypeId> = members
                    .iter()
                    .filter_map(|&m| {
                        let ctx = ContextualTypeContext::with_expected(self.interner, m);
                        ctx.get_property_type_inner(name, strip_optional_undefined)
                    })
                    .collect();
                if let Some(result) =
                    collect_from_intersection(self.interner, prop_types, |db, tys| {
                        db.intersection(tys)
                    })
                {
                    return Some(result);
                }
            }
            Some(TypeData::Mapped(mapped_id)) => {
                let mapped = self.interner.get_mapped(mapped_id);
                if let Some(prop) = crate::type_queries::get_finite_mapped_property_type(
                    self.interner,
                    mapped_id,
                    name,
                ) {
                    return Some(prop);
                }
                // For remapped keys (`as ...`) that depend on value lookups like `T[P]`,
                // if the finite lookup fails then this property is absent from the mapped
                // result and must not acquire ghost context from the source constraint.
                // Key-only remaps like `K extends Uppercase<string> ? K : never` still
                // benefit from broader fallback contextual typing in tsc.
                if mapped.name_type.is_some_and(|name_type| {
                    crate::visitor::contains_type_matching(self.interner, name_type, |key| {
                        matches!(key, TypeData::IndexAccess(_, _))
                    })
                }) {
                    return None;
                }

                if mapped.name_type.is_some() {
                    let key_literal = self
                        .interner
                        .literal_string_atom(self.interner.intern_string(name));
                    let instantiated =
                        crate::type_queries::instantiate_mapped_template_for_property(
                            self.interner,
                            mapped.template,
                            mapped.type_param.name,
                            key_literal,
                        );
                    let evaluated =
                        crate::evaluation::evaluate::evaluate_type(self.interner, instantiated);
                    if evaluated != TypeId::ANY
                        && evaluated != TypeId::ERROR
                        && evaluated != TypeId::NEVER
                    {
                        return Some(evaluated);
                    }
                }

                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type_inner(name, strip_optional_undefined);
                }
                // If evaluation deferred (e.g. { [K in keyof T]: TakeString } where T is a type
                // parameter), use the mapped type's template as the contextual property type
                // IF the template doesn't reference the mapped type's bound parameter or
                // its iteration variable (TypeParameter with the same name).
                // Without this check, templates like `({ key }: { key: key }) => void`
                // would be returned uninstantiated, causing false TS2345 errors when the
                // iteration variable `key` should be substituted with a concrete literal.
                let mapped_param_name = mapped.type_param.name;
                if mapped.template != TypeId::ANY
                    && mapped.template != TypeId::ERROR
                    && mapped.template != TypeId::NEVER
                    && !crate::visitor::contains_type_matching(
                        self.interner,
                        mapped.template,
                        |key| match key {
                            TypeData::BoundParameter(_) => true,
                            TypeData::TypeParameter(info) => info.name == mapped_param_name,
                            _ => false,
                        },
                    )
                {
                    return Some(mapped.template);
                }
                // Fall back to the constraint of the mapped type's source.
                if let Some(TypeData::KeyOf(operand)) = self.interner.lookup(mapped.constraint) {
                    let resolved_operand =
                        crate::evaluation::evaluate::evaluate_type(self.interner, operand);
                    if let Some(constraint) = crate::type_queries::get_type_parameter_constraint(
                        self.interner,
                        resolved_operand,
                    ) {
                        let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
                        return ctx.get_property_type_inner(name, strip_optional_undefined);
                    }
                    if let Some(constraint) =
                        crate::type_queries::get_type_parameter_constraint(self.interner, operand)
                    {
                        let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
                        return ctx.get_property_type_inner(name, strip_optional_undefined);
                    }
                }
            }
            Some(TypeData::Application(app_id)) => {
                let prop_key = self.property_name_to_key_type(name);
                let indexed = self.interner.index_access(expected, prop_key);
                let indexed_evaluated =
                    crate::evaluation::evaluate::evaluate_type(self.interner, indexed);
                if indexed_evaluated != indexed
                    && indexed_evaluated != TypeId::ERROR
                    && indexed_evaluated != TypeId::NEVER
                {
                    return Some(indexed_evaluated);
                }

                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type_inner(name, strip_optional_undefined);
                }
                // Fallback for unevaluated Application types
                let app = self.interner.type_application(app_id);
                let base_ctx = ContextualTypeContext::with_expected(self.interner, app.base);
                if let Some(prop) = base_ctx.get_property_type_inner(name, strip_optional_undefined)
                {
                    return Some(prop);
                }
                if let Some(&arg0) = app.args.first() {
                    let ctx = ContextualTypeContext::with_expected(self.interner, arg0);
                    if let Some(prop) = ctx.get_property_type_inner(name, strip_optional_undefined)
                    {
                        return Some(prop);
                    }
                }
            }
            Some(TypeData::Conditional(_) | TypeData::Lazy(_) | TypeData::IndexAccess(_, _)) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type_inner(name, strip_optional_undefined);
                }
            }
            Some(TypeData::TypeParameter(ref info) | TypeData::Infer(ref info)) => {
                // Handle TypeParameter/Infer - use constraint for property extraction
                if let Some(constraint) = info.constraint {
                    let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
                    return ctx.get_property_type_inner(name, strip_optional_undefined);
                }
            }
            _ => {}
        }

        // Use visitor for Object types
        let mut extractor = if strip_optional_undefined {
            PropertyExtractor::new_for_assignment(self.interner, name)
        } else {
            PropertyExtractor::new(self.interner, name)
        };
        extractor.extract(expected)
    }

    /// Create a child context for a nested expression.
    /// This is used when checking nested structures with contextual types.
    pub fn for_property(&self, name: &str) -> Self {
        match self.get_property_type(name) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for an array element.
    pub fn for_array_element(&self) -> Self {
        match self.get_array_element_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a tuple element at the given index.
    pub fn for_tuple_element(&self, index: usize) -> Self {
        match self.get_tuple_element_type(index) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a function parameter at the given index.
    pub fn for_parameter(&self, index: usize) -> Self {
        match self.get_parameter_type(index) {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Create a child context for a function return expression.
    pub fn for_return(&self) -> Self {
        match self.get_return_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
        }
    }

    /// Get the contextual yield type for a generator function.
    ///
    /// If the expected type is `Generator<Y, R, N>`, this returns Y.
    /// This is used to contextually type yield expressions.
    ///
    /// Example:
    /// ```typescript
    /// function* gen(): Generator<number, void, unknown> {
    ///     yield 1;  // 1 is contextually typed as number
    /// }
    /// ```
    pub fn get_generator_yield_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect yield types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let yield_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_yield_type()
                })
                .collect();

            return collect_single_or_union(self.interner, yield_types);
        }

        // Handle Lazy, Mapped, and Conditional types by evaluating first
        if let Some(TypeData::Lazy(_) | TypeData::Mapped(_) | TypeData::Conditional(_)) =
            self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_generator_yield_type();
            }
        }

        // Generator<Y, R, N> — yield type is arg 0
        let mut extractor = ApplicationArgExtractor::new(self.interner, 0);
        extractor.extract(expected)
    }

    /// Get the contextual return type for a generator function (`TReturn` from Generator<Y, `TReturn`, N>).
    ///
    /// This is used to contextually type return statements in generators.
    pub fn get_generator_return_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect return types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let return_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_return_type()
                })
                .collect();

            return collect_single_or_union(self.interner, return_types);
        }

        // Handle Lazy, Mapped, and Conditional types by evaluating first
        if let Some(TypeData::Lazy(_) | TypeData::Mapped(_) | TypeData::Conditional(_)) =
            self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_generator_return_type();
            }
        }

        // Generator<Y, R, N> — return type is arg 1
        let mut extractor = ApplicationArgExtractor::new(self.interner, 1);
        extractor.extract(expected)
    }

    /// Get the contextual next type for a generator function (`TNext` from Generator<Y, R, `TNext`>).
    ///
    /// This is used to determine the type of values passed to .`next()` and
    /// the type of the yield expression result.
    pub fn get_generator_next_type(&self) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect next types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let next_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_generator_next_type()
                })
                .collect();

            return collect_single_or_union(self.interner, next_types);
        }

        // Handle Lazy, Mapped, and Conditional types by evaluating first
        if let Some(TypeData::Lazy(_) | TypeData::Mapped(_) | TypeData::Conditional(_)) =
            self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_generator_next_type();
            }
        }

        // Generator<Y, R, N> — next type is arg 2
        let mut extractor = ApplicationArgExtractor::new(self.interner, 2);
        extractor.extract(expected)
    }

    /// Check if a callable type has any call signature with a parameter at the given index.
    /// This distinguishes "no param at this index" (arity gap) from "params disagree."
    fn callable_has_param_at_index(&self, type_id: TypeId, index: usize) -> bool {
        use crate::types::TypeData;
        match self.interner.lookup(type_id) {
            Some(TypeData::Function(func_id)) => {
                let shape = self.interner.function_shape(func_id);
                shape.params.len() > index || shape.params.iter().any(|p| p.rest)
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = self.interner.callable_shape(callable_id);
                shape
                    .call_signatures
                    .iter()
                    .any(|sig| sig.params.len() > index || sig.params.iter().any(|p| p.rest))
            }
            _ => false,
        }
    }

    fn is_function_boxed_or_intrinsic(&self, type_id: TypeId) -> bool {
        if matches!(
            self.interner.lookup(type_id),
            Some(TypeData::Intrinsic(IntrinsicKind::Function))
        ) {
            return true;
        }
        if self
            .interner
            .get_boxed_type(IntrinsicKind::Function)
            .is_some_and(|boxed| boxed == type_id)
        {
            return true;
        }
        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(type_id)
            && self
                .interner
                .is_boxed_def_id(def_id, IntrinsicKind::Function)
        {
            return true;
        }
        false
    }

    fn apply_conditional_true_branch_param_substitution(
        &self,
        ty: TypeId,
        cond: &crate::types::ConditionalType,
    ) -> TypeId {
        use crate::types::TypeData;
        match self.interner.lookup(ty) {
            Some(TypeData::Function(func_id)) => {
                let mut shape = (*self.interner.function_shape(func_id)).clone();
                for p in &mut shape.params {
                    p.type_id = self.substitute_conditional_param_type(p.type_id, cond);
                }
                self.interner.function(shape)
            }
            Some(TypeData::Callable(callable_id)) => {
                let mut shape = (*self.interner.callable_shape(callable_id)).clone();
                for sig in &mut shape.call_signatures {
                    for p in &mut sig.params {
                        p.type_id = self.substitute_conditional_param_type(p.type_id, cond);
                    }
                }
                for sig in &mut shape.construct_signatures {
                    for p in &mut sig.params {
                        p.type_id = self.substitute_conditional_param_type(p.type_id, cond);
                    }
                }
                self.interner.callable(shape)
            }
            _ => ty,
        }
    }

    fn substitute_conditional_param_type(
        &self,
        param_type: TypeId,
        cond: &crate::types::ConditionalType,
    ) -> TypeId {
        if param_type == cond.check_type {
            self.interner.intersection2(param_type, cond.extends_type)
        } else {
            param_type
        }
    }
}

/// Apply contextual type to infer a more specific type.
///
/// This implements bidirectional type inference:
/// 1. If `expr_type` is any/unknown/error, use contextual type
/// 2. If `expr_type` is a literal and contextual type is a union containing that literal's base type, preserve literal
/// 3. If `expr_type` is assignable to contextual type and is more specific, use `expr_type`
/// 4. Otherwise, prefer `expr_type` (don't widen to contextual type)
pub fn apply_contextual_type(
    interner: &dyn TypeDatabase,
    expr_type: TypeId,
    contextual_type: Option<TypeId>,
) -> TypeId {
    let ctx_type = match contextual_type {
        Some(t) => t,
        None => return expr_type,
    };

    // If expression type is any, unknown, or error, use contextual type
    if expr_type.is_any_or_unknown() || expr_type.is_error() {
        return ctx_type;
    }

    // If expression type is the same, just return it
    if expr_type == ctx_type {
        return expr_type;
    }

    // Check if expr_type is a literal type that should be preserved
    // When contextual type is a union like string | number, we should preserve literal types
    if let Some(expr_key) = interner.lookup(expr_type) {
        // Literal types should be preserved when context is a union
        if matches!(expr_key, TypeData::Literal(_))
            && let Some(ctx_key) = interner.lookup(ctx_type)
            && matches!(ctx_key, TypeData::Union(_))
        {
            // Preserve the literal type - it's more specific than the union
            return expr_type;
        }
    }

    // PERF: Reuse a single SubtypeChecker across all subtype checks in this function
    let mut checker = crate::relations::subtype::SubtypeChecker::new(interner);

    // Check if contextual type is a union
    if let Some(TypeData::Union(members)) = interner.lookup(ctx_type) {
        let members = interner.type_list(members);
        // If expr_type is in the union, it's valid - use the more specific expr_type
        for &member in members.iter() {
            if member == expr_type {
                return expr_type;
            }
        }
        // If expr_type is assignable to any union member, use expr_type
        for &member in members.iter() {
            checker.reset();
            if checker.is_subtype_of(expr_type, member) {
                return expr_type;
            }
        }
    }

    // If expr_type is assignable to contextual type, use expr_type (it's more specific)
    checker.reset();
    if checker.is_subtype_of(expr_type, ctx_type) {
        return expr_type;
    }

    // Default: prefer the expression type.
    //
    // When the contextual type is narrower than the expression type (e.g.,
    // ctx = "foo", expr = string), we must NOT substitute the contextual type.
    // The expression genuinely has the wider type at runtime, and substituting
    // the narrower contextual type would mask real assignability errors like
    // TS2322: Type 'string' is not assignable to type '"foo"'.
    //
    // The assignability checker is responsible for catching mismatches between
    // the expression type and the target type — this function should not
    // pre-narrow the expression type to hide those mismatches.
    expr_type
}

#[cfg(test)]
#[path = "../../tests/contextual_tests.rs"]
mod tests;
