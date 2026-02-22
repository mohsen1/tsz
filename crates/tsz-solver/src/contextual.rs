//! Contextual typing (reverse inference).
//!
//! Contextual typing allows type information to flow "backwards" from
//! an expected type to an expression. This is used for:
//! - Arrow function parameters: `const f: (x: string) => void = (x) => ...`
//! - Array literals: `const arr: number[] = [1, 2, 3]`
//! - Object literals: `const obj: {x: number} = {x: 1}`
//!
//! The key insight is that when we have an expected type, we can use it
//! to infer types for parts of the expression that would otherwise be unknown.
//!
//! The visitor-based type extractors used by [`ContextualTypeContext`] are in
//! the sibling [`contextual_extractors`](super::contextual_extractors) module.

use crate::TypeDatabase;
use crate::contextual_extractors::{
    ApplicationArgExtractor, ArrayElementExtractor, ParameterExtractor, ParameterForCallExtractor,
    PropertyExtractor, ReturnTypeExtractor, ThisTypeExtractor, ThisTypeMarkerExtractor,
    TupleElementExtractor, collect_single_or_union,
};
#[cfg(test)]
use crate::types::*;
use crate::types::{TypeData, TypeId};

/// Context for contextual typing.
/// Holds the expected type and provides methods to extract type information.
pub struct ContextualTypeContext<'a> {
    interner: &'a dyn TypeDatabase,
    /// The expected type (contextual type)
    expected: Option<TypeId>,
    /// Whether noImplicitAny is enabled (affects contextual typing for multi-signature functions)
    no_implicit_any: bool,
}

impl<'a> ContextualTypeContext<'a> {
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

        // Handle Union explicitly - collect parameter types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
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

            return collect_single_or_union(self.interner, param_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected_and_options(
                self.interner,
                app.base,
                self.no_implicit_any,
            );
            return ctx.get_parameter_type(index);
        }

        // Handle Intersection explicitly - pick the first callable member's parameter type
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            for &m in members.iter() {
                let ctx = ContextualTypeContext::with_expected_and_options(
                    self.interner,
                    m,
                    self.no_implicit_any,
                );
                if let Some(param_type) = ctx.get_parameter_type(index) {
                    return Some(param_type);
                }
            }
            return None;
        }

        // Handle Mapped, Conditional, and Lazy types by evaluating them first
        if let Some(TypeData::Mapped(_) | TypeData::Conditional(_) | TypeData::Lazy(_)) =
            self.interner.lookup(expected)
        {
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

        // Use visitor for Function/Callable types
        let mut extractor = ParameterExtractor::new(self.interner, index, self.no_implicit_any);
        extractor.extract(expected)
    }

    /// Get the contextual type for a call argument at the given index and arity.
    pub fn get_parameter_type_for_call(&self, index: usize, arg_count: usize) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect parameter types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let param_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_parameter_type_for_call(index, arg_count)
                })
                .collect();

            return collect_single_or_union(self.interner, param_types);
        }

        // Handle Application explicitly - unwrap to base type
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(expected) {
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_parameter_type_for_call(index, arg_count);
        }

        // Handle Intersection explicitly - pick the first callable member's parameter type
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            for &m in members.iter() {
                let ctx = ContextualTypeContext::with_expected(self.interner, m);
                if let Some(param_type) = ctx.get_parameter_type_for_call(index, arg_count) {
                    return Some(param_type);
                }
            }
            return None;
        }

        // Use visitor for Function/Callable types
        let mut extractor = ParameterForCallExtractor::new(self.interner, index, arg_count);
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
            let app = self.interner.type_application(app_id);
            let ctx = ContextualTypeContext::with_expected(self.interner, app.base);
            return ctx.get_return_type();
        }

        // Handle Lazy, Mapped, and Conditional types by evaluating first
        if let Some(TypeData::Lazy(_) | TypeData::Mapped(_) | TypeData::Conditional(_)) =
            self.interner.lookup(expected)
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

        // Handle Intersection - pick the first array member's element type
        if let Some(TypeData::Intersection(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            for &m in members.iter() {
                let ctx = ContextualTypeContext::with_expected(self.interner, m);
                if let Some(elem_type) = ctx.get_array_element_type() {
                    return Some(elem_type);
                }
            }
            return None;
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
        let expected = self.expected?;

        // Handle Union explicitly - collect tuple element types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let elem_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_tuple_element_type(index)
                })
                .collect();
            return collect_single_or_union(self.interner, elem_types);
        }

        // Handle Application explicitly - evaluate to resolve type aliases
        if let Some(TypeData::Application(_)) = self.interner.lookup(expected) {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_tuple_element_type(index);
            }
        }

        // Handle TypeParameter - use its constraint
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_tuple_element_type(index);
        }

        // Handle Mapped, Conditional, and Lazy types by evaluating them first
        if let Some(TypeData::Mapped(_) | TypeData::Conditional(_) | TypeData::Lazy(_)) =
            self.interner.lookup(expected)
        {
            let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
            if evaluated != expected {
                let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                return ctx.get_tuple_element_type(index);
            }
        }

        let mut extractor = TupleElementExtractor::new(self.interner, index);
        extractor.extract(expected)
    }

    /// Get the contextual type for an object property.
    ///
    /// Example:
    /// ```typescript
    /// const obj: {x: number, y: string} = {x: 1, y: "hi"};
    /// ```
    pub fn get_property_type(&self, name: &str) -> Option<TypeId> {
        let expected = self.expected?;

        // Handle Union explicitly - collect property types from all members
        if let Some(TypeData::Union(members)) = self.interner.lookup(expected) {
            let members = self.interner.type_list(members);
            let prop_types: Vec<TypeId> = members
                .iter()
                .filter_map(|&m| {
                    let ctx = ContextualTypeContext::with_expected(self.interner, m);
                    ctx.get_property_type(name)
                })
                .collect();

            return if prop_types.is_empty() {
                None
            } else if prop_types.len() == 1 {
                Some(prop_types[0])
            } else {
                // CRITICAL: Use union_preserve_members to keep literal types intact
                // For discriminated unions like `{ success: false } | { success: true }`,
                // the property type should be `false | true`, NOT widened to `boolean`.
                // This preserves literal types for contextual typing.
                Some(self.interner.union_preserve_members(prop_types))
            };
        }

        // Handle Mapped, Conditional, and Application types.
        // These complex types need to be resolved to concrete object types before
        // property extraction can work.
        match self.interner.lookup(expected) {
            Some(TypeData::Mapped(mapped_id)) => {
                // First try evaluating the mapped type directly
                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type(name);
                }
                // If evaluation deferred (e.g. { [K in keyof T]: TakeString } where T is a type
                // parameter), use the mapped type's template as the contextual property type
                // IF the template doesn't reference the mapped type's bound parameter.
                // For example, { [P in keyof T]: TakeString } has template=TakeString which
                // is independent of P, so it's safe to use directly. But { [P in K]: T[P] }
                // has template=T[P] which depends on P, so we can't use it.
                let mapped = self.interner.mapped_type(mapped_id);
                if mapped.template != TypeId::ANY
                    && mapped.template != TypeId::ERROR
                    && mapped.template != TypeId::NEVER
                    && !crate::visitor::contains_type_matching(
                        self.interner,
                        mapped.template,
                        |key| matches!(key, TypeData::BoundParameter(_)),
                    )
                {
                    return Some(mapped.template);
                }
                // Fall back to the constraint of the mapped type's source.
                // For `keyof P` where `P extends Props`, use `Props` as the contextual type.
                if let Some(TypeData::KeyOf(operand)) = self.interner.lookup(mapped.constraint) {
                    // The operand may be a Lazy type wrapping a type parameter — resolve it
                    let resolved_operand =
                        crate::evaluation::evaluate::evaluate_type(self.interner, operand);
                    if let Some(constraint) = crate::type_queries::get_type_parameter_constraint(
                        self.interner,
                        resolved_operand,
                    ) {
                        let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
                        return ctx.get_property_type(name);
                    }
                    // Also try the original operand (may already be a TypeParameter)
                    if let Some(constraint) =
                        crate::type_queries::get_type_parameter_constraint(self.interner, operand)
                    {
                        let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
                        return ctx.get_property_type(name);
                    }
                }
            }
            Some(TypeData::Application(app_id)) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type(name);
                }
                // Fallback for unevaluated Application types (e.g. Readonly<T>, Partial<T>).
                // When evaluation fails (e.g. due to RefCell borrow conflicts during contextual
                // typing), try to extract the property from the type argument directly.
                // This is correct for homomorphic mapped types where property types are preserved.
                let app = self.interner.type_application(app_id);
                if !app.args.is_empty() {
                    let ctx = ContextualTypeContext::with_expected(self.interner, app.args[0]);
                    if let Some(prop) = ctx.get_property_type(name) {
                        return Some(prop);
                    }
                }
            }
            Some(TypeData::Conditional(_) | TypeData::Lazy(_)) => {
                let evaluated = crate::evaluation::evaluate::evaluate_type(self.interner, expected);
                if evaluated != expected {
                    let ctx = ContextualTypeContext::with_expected(self.interner, evaluated);
                    return ctx.get_property_type(name);
                }
            }
            _ => {}
        }

        // Handle TypeParameter - use its constraint for property extraction
        // Example: Actions extends ActionsObject<State>
        // When getting property from Actions, use ActionsObject<State> instead
        if let Some(constraint) =
            crate::type_queries::get_type_parameter_constraint(self.interner, expected)
        {
            let ctx = ContextualTypeContext::with_expected(self.interner, constraint);
            return ctx.get_property_type(name);
        }

        // Use visitor for Object types
        let mut extractor = PropertyExtractor::new(self.interner, name);
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

        // Generator<Y, R, N> — next type is arg 2
        let mut extractor = ApplicationArgExtractor::new(self.interner, 2);
        extractor.extract(expected)
    }

    /// Create a child context for a yield expression in a generator.
    pub fn for_yield(&self) -> Self {
        match self.get_generator_yield_type() {
            Some(ty) => ContextualTypeContext::with_expected(self.interner, ty),
            None => ContextualTypeContext::new(self.interner),
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

    // If contextual type is assignable to expr_type, use contextual type (it's more specific)
    // BUT: Skip for function/callable types — the solver's bivariant SubtypeChecker can
    // incorrectly say that a wider function type (e.g. (x: number|string) => void) is a
    // subtype of a narrower one (e.g. (x: number) => void), which would widen the property
    // type and suppress valid TS2322 errors under strict function types.
    let is_function_type = matches!(
        interner.lookup(expr_type),
        Some(TypeData::Function(_) | TypeData::Object(_))
    );
    if !is_function_type {
        checker.reset();
        if checker.is_subtype_of(ctx_type, expr_type) {
            return ctx_type;
        }
    }

    // Default: prefer the expression type (don't widen to contextual type)
    // This prevents incorrectly widening concrete types to generic type parameters
    expr_type
}

#[cfg(test)]
#[path = "../tests/contextual_tests.rs"]
mod tests;
