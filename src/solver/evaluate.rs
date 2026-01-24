//! Type evaluation for meta-types (conditional, mapped, index access).
//!
//! Meta-types are "type-level functions" that compute output types from input types.
//! This module provides evaluation logic for:
//! - Conditional types: T extends U ? X : Y
//! - Distributive conditional types: (A | B) extends U ? X : Y
//! - Index access types: T[K]
//!
//! Key design:
//! - Lazy evaluation: only evaluate when needed for subtype checking
//! - Handles deferred evaluation when type parameters are unknown
//! - Supports distributivity for naked type parameters in unions

use crate::solver::TypeDatabase;
use crate::solver::instantiate::instantiate_generic;
use crate::solver::subtype::{NoopResolver, TypeResolver};
use crate::solver::types::*;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

#[cfg(test)]
use crate::solver::TypeInterner;

/// Result of conditional type evaluation
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConditionalResult {
    /// The condition was resolved to a definite type
    Resolved(TypeId),
    /// The condition could not be resolved (deferred)
    /// This happens when check_type is a type parameter that hasn't been substituted
    Deferred(TypeId),
}

/// Maximum recursion depth for type evaluation.
/// Prevents stack overflow on deeply recursive mapped/conditional types.
/// Also prevents OOM from infinitely expanding types like:
/// - `interface AA<T extends AA<T>>`
/// - `interface SelfReference<T = SelfReference>`
pub const MAX_EVALUATE_DEPTH: u32 = 50;

/// Maximum number of unique types to track in the visiting set.
/// Prevents unbounded memory growth in pathological cases.
pub const MAX_VISITING_SET_SIZE: usize = 10_000;

/// Maximum total evaluations allowed per TypeEvaluator instance.
/// Prevents infinite loops in pathological type evaluation scenarios.
pub const MAX_TOTAL_EVALUATIONS: u32 = 100_000;

/// Type evaluator for meta-types.
pub struct TypeEvaluator<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    no_unchecked_indexed_access: bool,
    cache: RefCell<FxHashMap<TypeId, TypeId>>,
    visiting: RefCell<FxHashSet<TypeId>>,
    depth: RefCell<u32>,
    /// Total number of evaluate calls (iteration limit)
    total_evaluations: RefCell<u32>,
    /// Whether the recursion depth limit was exceeded
    depth_exceeded: RefCell<bool>,
}

/// Array methods that return any (used for apparent type computation).
pub const ARRAY_METHODS_RETURN_ANY: &[&str] = &[
    "concat",
    "filter",
    "flat",
    "flatMap",
    "map",
    "reverse",
    "slice",
    "sort",
    "splice",
    "toReversed",
    "toSorted",
    "toSpliced",
    "with",
    "at",
    "find",
    "findLast",
    "pop",
    "shift",
    "entries",
    "keys",
    "values",
    "reduce",
    "reduceRight",
];
/// Array methods that return boolean.
pub const ARRAY_METHODS_RETURN_BOOLEAN: &[&str] = &["every", "includes", "some"];
/// Array methods that return number.
pub const ARRAY_METHODS_RETURN_NUMBER: &[&str] = &[
    "findIndex",
    "findLastIndex",
    "indexOf",
    "lastIndexOf",
    "push",
    "unshift",
];
/// Array methods that return void.
pub const ARRAY_METHODS_RETURN_VOID: &[&str] = &["forEach", "copyWithin", "fill"];
/// Array methods that return string.
pub const ARRAY_METHODS_RETURN_STRING: &[&str] = &["join", "toLocaleString", "toString"];

impl<'a> TypeEvaluator<'a, NoopResolver> {
    /// Create a new evaluator without a resolver.
    pub fn new(interner: &'a dyn TypeDatabase) -> TypeEvaluator<'a, NoopResolver> {
        static NOOP: NoopResolver = NoopResolver;
        TypeEvaluator {
            interner,
            resolver: &NOOP,
            no_unchecked_indexed_access: false,
            cache: RefCell::new(FxHashMap::default()),
            visiting: RefCell::new(FxHashSet::default()),
            depth: RefCell::new(0),
            total_evaluations: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
        }
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Create a new evaluator with a custom resolver.
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        TypeEvaluator {
            interner,
            resolver,
            no_unchecked_indexed_access: false,
            cache: RefCell::new(FxHashMap::default()),
            visiting: RefCell::new(FxHashSet::default()),
            depth: RefCell::new(0),
            total_evaluations: RefCell::new(0),
            depth_exceeded: RefCell::new(false),
        }
    }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
    }

    // =========================================================================
    // Accessor methods for evaluate_rules modules
    // =========================================================================

    /// Get the type interner.
    #[inline]
    pub(crate) fn interner(&self) -> &'a dyn TypeDatabase {
        self.interner
    }

    /// Get the type resolver.
    #[inline]
    pub(crate) fn resolver(&self) -> &'a R {
        self.resolver
    }

    /// Check if no_unchecked_indexed_access is enabled.
    #[inline]
    pub(crate) fn no_unchecked_indexed_access(&self) -> bool {
        self.no_unchecked_indexed_access
    }

    /// Check if depth limit was exceeded.
    #[inline]
    pub(crate) fn is_depth_exceeded(&self) -> bool {
        *self.depth_exceeded.borrow()
    }

    /// Set the depth exceeded flag.
    #[inline]
    pub(crate) fn set_depth_exceeded(&self, value: bool) {
        *self.depth_exceeded.borrow_mut() = value;
    }

    /// Evaluate a type, resolving any meta-types if possible.
    /// Returns the evaluated type (may be the same if no evaluation needed).
    ///
    /// # TODO: Application Type Expansion (Worker 2 - Redux test fix)
    ///
    /// **Problem**: `Application(Ref(sym), args)` types (like `Reducer<S, A>`) are not
    /// being expanded to their instantiated form. This causes diagnostics to show
    /// `Ref(5)<error>` instead of the actual type.
    ///
    /// **Current Behavior**: Application types pass through unchanged at line ~202.
    /// This means when comparing a function type against `Reducer<S, A>`, the
    /// Application type is not expanded to its underlying function type.
    ///
    /// **Observed Diagnostics in redux test**:
    /// - `Type '(state: undefined | Ref(5)<error>, action: Ref(6)<error>) => any'
    ///    is not assignable to type 'Ref(1)<Ref(5)<error>, Ref(6)<error>>'`
    /// - `Ref(5)`, `Ref(6)`, `Ref(7)` etc. should be expanded to actual types
    ///
    /// **Fix Approach**: Add a case for `TypeKey::Application(app_id)`:
    /// 1. Get the base type from the Application
    /// 2. If base is a `Ref(sym)`, resolve it using `self.resolver.resolve_ref(sym, ...)`
    /// 3. Get the type parameters from the resolved type (type alias or interface)
    /// 4. Create a substitution map: type_params[i] -> args[i]
    /// 5. Instantiate the resolved type body with the substitution
    /// 6. Return the instantiated type
    ///
    /// **Example**:
    /// ```text
    /// // Given: type Reducer<S, A> = (state: S | undefined, action: A) => S
    /// // And: Application(Ref(Reducer), [number, AnyAction])
    /// // Should expand to: (state: number | undefined, action: AnyAction) => number
    /// ```
    ///
    /// **Related Files**:
    /// - `instantiate.rs` - Has substitution logic for type parameters
    /// - `checker/state.rs:2900-2918` - Type alias resolution with type params
    /// - `lower.rs:856-868` - `lower_type_alias_declaration` with params
    pub fn evaluate(&self, type_id: TypeId) -> TypeId {
        // Fast path for intrinsics
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Check if depth was already exceeded in a previous call
        if *self.depth_exceeded.borrow() {
            return TypeId::ERROR;
        }

        if let Some(&cached) = self.cache.borrow().get(&type_id) {
            return cached;
        }

        // Total evaluations limit to prevent infinite loops
        {
            let mut total = self.total_evaluations.borrow_mut();
            *total += 1;
            if *total > MAX_TOTAL_EVALUATIONS {
                // Too many evaluations - return unevaluated to break out
                self.cache.borrow_mut().insert(type_id, type_id);
                return type_id;
            }
        }

        // Depth guard to prevent OOM from infinitely expanding types
        // Examples: interface AA<T extends AA<T>>, type SelfReference<T = SelfReference>
        {
            let mut depth = self.depth.borrow_mut();
            *depth += 1;
            if *depth > MAX_EVALUATE_DEPTH {
                *depth -= 1;
                drop(depth);
                // Mark depth as exceeded and return ERROR to stop expansion
                *self.depth_exceeded.borrow_mut() = true;
                self.cache.borrow_mut().insert(type_id, TypeId::ERROR);
                return TypeId::ERROR;
            }
        }

        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => {
                *self.depth.borrow_mut() -= 1;
                return type_id;
            }
        };

        {
            let mut visiting = self.visiting.borrow_mut();
            // Memory safety: limit the visiting set size
            if visiting.len() >= MAX_VISITING_SET_SIZE {
                *self.depth.borrow_mut() -= 1;
                self.cache.borrow_mut().insert(type_id, type_id);
                return type_id;
            }
            if !visiting.insert(type_id) {
                // Recursion guard for self-referential mapped/application types.
                // Per TypeScript behavior, recursive mapped types evaluate to empty objects.
                if matches!(key, TypeKey::Mapped(_)) {
                    drop(visiting);
                    *self.depth.borrow_mut() -= 1;
                    let empty = self.interner.object(vec![]);
                    self.cache.borrow_mut().insert(type_id, empty);
                    return empty;
                }
                *self.depth.borrow_mut() -= 1;
                return type_id;
            }
        }

        let result = match &key {
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(*cond_id);
                let result = self.evaluate_conditional(cond.as_ref());
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            TypeKey::IndexAccess(obj, idx) => {
                let result = self.evaluate_index_access(*obj, *idx);
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(*mapped_id);
                let result = self.evaluate_mapped(mapped.as_ref());
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            TypeKey::KeyOf(operand) => {
                let result = self.evaluate_keyof(*operand);
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            TypeKey::TypeQuery(symbol) => {
                let result =
                    if let Some(resolved) = self.resolver.resolve_ref(*symbol, self.interner) {
                        resolved
                    } else {
                        // Pass through unchanged if not resolved
                        type_id
                    };
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            TypeKey::Application(app_id) => {
                let result = self.evaluate_application(*app_id);
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            TypeKey::TemplateLiteral(spans) => {
                let result = self.evaluate_template_literal(*spans);
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            // Resolve Ref types to their structural form
            TypeKey::Ref(symbol) => {
                let result =
                    if let Some(resolved) = self.resolver.resolve_ref(*symbol, self.interner) {
                        resolved
                    } else {
                        TypeId::ERROR
                    };
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            TypeKey::StringIntrinsic { kind, type_arg } => {
                let result = self.evaluate_string_intrinsic(*kind, *type_arg);
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, result);
                result
            }
            // Other types pass through unchanged
            _ => {
                self.visiting.borrow_mut().remove(&type_id);
                self.cache.borrow_mut().insert(type_id, type_id);
                type_id
            }
        };

        *self.depth.borrow_mut() -= 1;
        result
    }

    /// Evaluate a generic type application: Base<Args>
    ///
    /// Algorithm:
    /// 1. Look up the base type - if it's a Ref, resolve it
    /// 2. Get the type parameters for the base symbol
    /// 3. If we have type params, instantiate the resolved type with args
    /// 4. Recursively evaluate the result
    fn evaluate_application(&self, app_id: TypeApplicationId) -> TypeId {
        let app = self.interner.type_application(app_id);

        // Look up the base type
        let base_key = match self.interner.lookup(app.base) {
            Some(k) => k,
            None => return self.interner.application(app.base, app.args.clone()),
        };

        // If the base is a Ref, try to resolve and instantiate
        if let TypeKey::Ref(symbol) = base_key {
            // Try to get the type parameters for this symbol
            let type_params = self.resolver.get_type_params(symbol);
            let resolved = self.resolver.resolve_ref(symbol, self.interner);

            if let Some(type_params) = type_params {
                // Resolve the base type to get the body
                if let Some(resolved) = resolved {
                    // Pre-expand type arguments that are TypeQuery or Application
                    let expanded_args: Vec<TypeId> = app
                        .args
                        .iter()
                        .map(|&arg| self.try_expand_type_arg(arg))
                        .collect();

                    // Instantiate the resolved type with the type arguments
                    let instantiated =
                        instantiate_generic(self.interner, resolved, &type_params, &expanded_args);
                    // Recursively evaluate the result
                    return self.evaluate(instantiated);
                }
            } else if let Some(resolved) = resolved {
                // Fallback: try to extract type params from the resolved type's properties
                let extracted_params = self.extract_type_params_from_type(resolved);
                if !extracted_params.is_empty() && extracted_params.len() == app.args.len() {
                    // Pre-expand type arguments
                    let expanded_args: Vec<TypeId> = app
                        .args
                        .iter()
                        .map(|&arg| self.try_expand_type_arg(arg))
                        .collect();

                    let instantiated = instantiate_generic(
                        self.interner,
                        resolved,
                        &extracted_params,
                        &expanded_args,
                    );
                    return self.evaluate(instantiated);
                }
            }
        }

        // If we can't expand, return the original application
        self.interner.application(app.base, app.args.clone())
    }

    /// Extract type parameter infos from a type by scanning for TypeParameter types.
    fn extract_type_params_from_type(&self, type_id: TypeId) -> Vec<TypeParamInfo> {
        let mut seen = std::collections::HashSet::new();
        let mut params = Vec::new();
        self.collect_type_params(type_id, &mut seen, &mut params);
        params
    }

    /// Recursively collect TypeParameter types from a type.
    fn collect_type_params(
        &self,
        type_id: TypeId,
        seen: &mut std::collections::HashSet<crate::interner::Atom>,
        params: &mut Vec<TypeParamInfo>,
    ) {
        if type_id.is_intrinsic() {
            return;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return;
        };

        match key {
            TypeKey::TypeParameter(ref info) => {
                if !seen.contains(&info.name) {
                    seen.insert(info.name);
                    params.push(info.clone());
                }
            }
            TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_type_params(prop.type_id, seen, params);
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in &shape.params {
                    self.collect_type_params(param.type_id, seen, params);
                }
                self.collect_type_params(shape.return_type, seen, params);
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    self.collect_type_params(member, seen, params);
                }
            }
            TypeKey::Array(elem) => {
                self.collect_type_params(elem, seen, params);
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.collect_type_params(cond.check_type, seen, params);
                self.collect_type_params(cond.extends_type, seen, params);
                self.collect_type_params(cond.true_type, seen, params);
                self.collect_type_params(cond.false_type, seen, params);
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.collect_type_params(app.base, seen, params);
                for &arg in &app.args {
                    self.collect_type_params(arg, seen, params);
                }
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                // Note: mapped.type_param is the iteration variable (e.g., K in "K in keyof T")
                // We should NOT add it directly - the outer type param (T) is found in the constraint.
                // For DeepPartial<T> = { [K in keyof T]?: DeepPartial<T[K]> }:
                //   - type_param is K (iteration var, NOT the outer param)
                //   - constraint is "keyof T" (contains T, the actual param to extract)
                //   - template is DeepPartial<T[K]> (also contains T)
                self.collect_type_params(mapped.constraint, seen, params);
                self.collect_type_params(mapped.template, seen, params);
                if let Some(name_type) = mapped.name_type {
                    self.collect_type_params(name_type, seen, params);
                }
            }
            TypeKey::KeyOf(operand) => {
                // Extract type params from the operand of keyof
                // e.g., keyof T -> extract T
                self.collect_type_params(operand, seen, params);
            }
            TypeKey::IndexAccess(obj, idx) => {
                // Extract type params from both object and index
                // e.g., T[K] -> extract T and K
                self.collect_type_params(obj, seen, params);
                self.collect_type_params(idx, seen, params);
            }
            TypeKey::TemplateLiteral(spans) => {
                // Extract type params from template literal interpolations
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span {
                        self.collect_type_params(*inner, seen, params);
                    }
                }
            }
            _ => {}
        }
    }

    /// Try to expand a type argument that may be a TypeQuery or Application.
    /// Returns the expanded type, or the original if it can't be expanded.
    /// This ensures type arguments are resolved before instantiation.
    ///
    /// NOTE: This method uses self.evaluate() for Application, Conditional, Mapped,
    /// and TemplateLiteral types to ensure recursion depth limits are enforced.
    fn try_expand_type_arg(&self, arg: TypeId) -> TypeId {
        let Some(key) = self.interner.lookup(arg) else {
            return arg;
        };
        match key {
            TypeKey::TypeQuery(sym_ref) => {
                // Resolve the TypeQuery to get the actual type, or pass through if unresolved
                self.resolver
                    .resolve_ref(sym_ref, self.interner)
                    .unwrap_or(arg)
            }
            TypeKey::Application(_) => {
                // Use evaluate() to ensure depth limits are enforced
                self.evaluate(arg)
            }
            TypeKey::Ref(sym_ref) => {
                // Also try to resolve Ref types in type arguments
                // This helps with generic instantiation accuracy
                self.resolver
                    .resolve_ref(sym_ref, self.interner)
                    .unwrap_or(arg)
            }
            TypeKey::Conditional(_) => {
                // Use evaluate() to ensure depth limits are enforced
                self.evaluate(arg)
            }
            TypeKey::Mapped(_) => {
                // Use evaluate() to ensure depth limits are enforced
                self.evaluate(arg)
            }
            TypeKey::TemplateLiteral(_) => {
                // Use evaluate() to ensure depth limits are enforced
                self.evaluate(arg)
            }
            _ => arg,
        }
    }
}

/// Convenience function for evaluating conditional types
pub fn evaluate_conditional(interner: &dyn TypeDatabase, cond: &ConditionalType) -> TypeId {
    let evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_conditional(cond)
}

/// Convenience function for evaluating index access types
pub fn evaluate_index_access(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    let evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for full type evaluation
pub fn evaluate_type(interner: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate(type_id)
}

/// Convenience function for evaluating mapped types
pub fn evaluate_mapped(interner: &dyn TypeDatabase, mapped: &MappedType) -> TypeId {
    let evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_mapped(mapped)
}

/// Convenience function for evaluating keyof types
pub fn evaluate_keyof(interner: &dyn TypeDatabase, operand: TypeId) -> TypeId {
    let evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_keyof(operand)
}

// Re-enabled evaluate tests - verifying API compatibility
#[cfg(test)]
#[path = "evaluate_tests.rs"]
mod tests;
