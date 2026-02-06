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
use crate::solver::db::QueryDatabase;
use crate::solver::def::DefId;
use crate::solver::instantiate::instantiate_generic;
use crate::solver::subtype::{NoopResolver, TypeResolver};
use crate::solver::types::*;
use rustc_hash::{FxHashMap, FxHashSet};

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
///
/// # Salsa Preparation
/// This struct uses `&mut self` methods instead of `RefCell` + `&self`.
/// This makes the evaluator thread-safe (Send) and prepares for future
/// Salsa integration where state is managed by the database runtime.
pub struct TypeEvaluator<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    /// Optional query database for Salsa-backed memoization.
    query_db: Option<&'a dyn QueryDatabase>,
    resolver: &'a R,
    no_unchecked_indexed_access: bool,
    cache: FxHashMap<TypeId, TypeId>,
    visiting: FxHashSet<TypeId>,
    /// DefId-level cycle detection for expansive recursion
    /// Prevents infinite expansion of recursive type aliases like `type T<X> = T<Box<X>>`
    visiting_defs: FxHashSet<DefId>,
    depth: u32,
    /// Total number of evaluate calls (iteration limit)
    total_evaluations: u32,
    /// Whether the recursion depth limit was exceeded
    depth_exceeded: bool,
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
            query_db: None,
            resolver: &NOOP,
            no_unchecked_indexed_access: false,
            cache: FxHashMap::default(),
            visiting: FxHashSet::default(),
            visiting_defs: FxHashSet::default(),
            depth: 0,
            total_evaluations: 0,
            depth_exceeded: false,
        }
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Create a new evaluator with a custom resolver.
    pub fn with_resolver(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        TypeEvaluator {
            interner,
            query_db: None,
            resolver,
            no_unchecked_indexed_access: false,
            cache: FxHashMap::default(),
            visiting: FxHashSet::default(),
            visiting_defs: FxHashSet::default(),
            depth: 0,
            total_evaluations: 0,
            depth_exceeded: false,
        }
    }

    /// Set the query database for Salsa-backed memoization.
    pub fn with_query_db(mut self, db: &'a dyn QueryDatabase) -> Self {
        self.query_db = Some(db);
        self
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

    /// Get the query database, if available.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn query_db(&self) -> Option<&'a dyn QueryDatabase> {
        self.query_db
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
        self.depth_exceeded
    }

    /// Set the depth exceeded flag.
    #[inline]
    pub(crate) fn set_depth_exceeded(&mut self, value: bool) {
        self.depth_exceeded = value;
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
    pub fn evaluate(&mut self, type_id: TypeId) -> TypeId {
        // Fast path for intrinsics
        if type_id.is_intrinsic() {
            return type_id;
        }

        // Check if depth was already exceeded in a previous call
        if self.depth_exceeded {
            return TypeId::ERROR;
        }

        if let Some(&cached) = self.cache.get(&type_id) {
            return cached;
        }

        // Total evaluations limit to prevent infinite loops
        self.total_evaluations += 1;
        if self.total_evaluations > MAX_TOTAL_EVALUATIONS {
            // Too many evaluations - return unevaluated to break out
            self.cache.insert(type_id, type_id);
            return type_id;
        }

        // Depth guard to prevent OOM from infinitely expanding types
        // Examples: interface AA<T extends AA<T>>, type SelfReference<T = SelfReference>
        self.depth += 1;
        if self.depth > MAX_EVALUATE_DEPTH {
            self.depth -= 1;
            // Mark depth as exceeded and return ERROR to stop expansion
            self.depth_exceeded = true;
            self.cache.insert(type_id, TypeId::ERROR);
            return TypeId::ERROR;
        }

        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => {
                self.depth -= 1;
                return type_id;
            }
        };

        // Memory safety: limit the visiting set size
        if self.visiting.len() >= MAX_VISITING_SET_SIZE {
            self.depth -= 1;
            self.cache.insert(type_id, type_id);
            return type_id;
        }
        if !self.visiting.insert(type_id) {
            // Recursion guard for self-referential mapped/application types.
            // Per TypeScript behavior, recursive mapped types evaluate to empty objects.
            if matches!(key, TypeKey::Mapped(_)) {
                self.depth -= 1;
                let empty = self.interner.object(vec![]);
                self.cache.insert(type_id, empty);
                return empty;
            }
            self.depth -= 1;
            return type_id;
        }

        // Visitor pattern: dispatch to appropriate visit_* method
        let result = self.visit_type_key(type_id, &key);

        // Symmetric cleanup: remove from visiting set and cache result
        self.visiting.remove(&type_id);
        self.cache.insert(type_id, result);

        self.depth -= 1;
        result
    }

    /// Evaluate a generic type application: Base<Args>
    ///
    /// Algorithm:
    /// 1. Look up the base type - if it's a Ref, resolve it
    /// 2. Get the type parameters for the base symbol
    /// 3. If we have type params, instantiate the resolved type with args
    /// 4. Recursively evaluate the result
    fn evaluate_application(&mut self, app_id: TypeApplicationId) -> TypeId {
        let app = self.interner.type_application(app_id);

        // Look up the base type
        let base_key = match self.interner.lookup(app.base) {
            Some(k) => k,
            None => return self.interner.application(app.base, app.args.clone()),
        };

        // If the base is a Lazy(DefId), try to resolve and instantiate (Phase 4.3)
        if let TypeKey::Lazy(def_id) = base_key {
            // =======================================================================
            // DEFD-LEVEL CYCLE DETECTION (before resolution!)
            // =======================================================================
            // This catches expansive recursion in type aliases like `type T<X> = T<Box<X>>`
            // that produce new TypeIds on each evaluation, bypassing the `visiting` set.
            //
            // We check if we're already visiting this DefId. If so, we return ERROR
            // to prevent infinite expansion. This matches TypeScript behavior for
            // "Type instantiation is excessively deep and possibly infinite".
            // =======================================================================
            if self.visiting_defs.contains(&def_id) {
                // CRITICAL: Do NOT return the application.
                // Return ERROR to stop the solver from trying to expand it forever.
                // This prevents infinite loops where:
                // 1. evaluate returns App (unevaluated)
                // 2. check_subtype sees no change, calls check_subtype_inner
                // 3. check_subtype_inner tries to evaluate again -> infinite loop
                self.depth_exceeded = true;
                return TypeId::ERROR;
            }

            // Mark this DefId as being visited
            self.visiting_defs.insert(def_id);

            // Try to get the type parameters for this DefId
            let type_params = self.resolver.get_lazy_type_params(def_id);
            let resolved = self.resolver.resolve_lazy(def_id, self.interner);

            let result = if let Some(type_params) = type_params {
                // Resolve the base type to get the body
                if let Some(resolved) = resolved {
                    // Pre-expand type arguments that are TypeQuery or Application
                    let expanded_args = self.expand_type_args(&app.args);

                    // Instantiate the resolved type with the type arguments
                    let instantiated =
                        instantiate_generic(self.interner, resolved, &type_params, &expanded_args);
                    // Recursively evaluate the result
                    self.evaluate(instantiated)
                } else {
                    self.interner.application(app.base, app.args.clone())
                }
            } else if let Some(resolved) = resolved {
                // Fallback: try to extract type params from the resolved type's properties
                let extracted_params = self.extract_type_params_from_type(resolved);
                if !extracted_params.is_empty() && extracted_params.len() == app.args.len() {
                    // Pre-expand type arguments
                    let expanded_args = self.expand_type_args(&app.args);

                    let instantiated = instantiate_generic(
                        self.interner,
                        resolved,
                        &extracted_params,
                        &expanded_args,
                    );
                    self.evaluate(instantiated)
                } else {
                    self.interner.application(app.base, app.args.clone())
                }
            } else {
                self.interner.application(app.base, app.args.clone())
            };

            // Remove from visiting_defs after evaluation
            self.visiting_defs.remove(&def_id);

            result
        } else {
            // If we can't expand, return the original application
            self.interner.application(app.base, app.args.clone())
        }
    }

    /// Expand type arguments by evaluating any that are TypeQuery or Application.
    /// Uses a loop instead of closure to allow mutable self access.
    fn expand_type_args(&mut self, args: &[TypeId]) -> Vec<TypeId> {
        let mut expanded = Vec::with_capacity(args.len());
        for &arg in args {
            expanded.push(self.try_expand_type_arg(arg));
        }
        expanded
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
    fn try_expand_type_arg(&mut self, arg: TypeId) -> TypeId {
        let Some(key) = self.interner.lookup(arg) else {
            return arg;
        };
        match key {
            TypeKey::TypeQuery(sym_ref) => {
                // Resolve the TypeQuery to get the actual type, or pass through if unresolved
                if let Some(def_id) = self.resolver.symbol_to_def_id(sym_ref) {
                    match self.resolver.resolve_lazy(def_id, self.interner) {
                        Some(resolved) => resolved,
                        None => arg,
                    }
                } else {
                    #[allow(deprecated)]
                    match self.resolver.resolve_ref(sym_ref, self.interner) {
                        Some(resolved) => resolved,
                        None => arg,
                    }
                }
            }
            TypeKey::Application(_) => {
                // Use evaluate() to ensure depth limits are enforced
                self.evaluate(arg)
            }
            TypeKey::Lazy(def_id) => {
                // Resolve Lazy types in type arguments (Phase 4.3)
                // This helps with generic instantiation accuracy
                self.resolver
                    .resolve_lazy(def_id, self.interner)
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

    /// Check if a type is "complex" and requires full evaluation for identity.
    ///
    /// Complex types are those whose structural identity depends on evaluation context:
    /// - TypeParameter: Opaque until instantiation
    /// - Lazy: Requires resolution
    /// - Conditional: Requires evaluation of extends clause
    /// - Mapped: Requires evaluation of mapped type
    /// - IndexAccess: Requires evaluation of T[K]
    /// - KeyOf: Requires evaluation of keyof
    /// - Application: Requires expansion of Base<Args>
    /// - TypeQuery: Requires resolution of typeof
    /// - TemplateLiteral: Requires evaluation of template parts
    /// - ReadonlyType: Wraps another type
    /// - StringIntrinsic: Uppercase, Lowercase, Capitalize, Uncapitalize
    ///
    /// These types are NOT safe for simplification because bypassing evaluation
    /// would produce incorrect results (e.g., treating T[K] as a distinct type from
    /// the value it evaluates to).
    ///
    /// ## Task #37: Deep Structural Simplification
    ///
    /// After implementing the Canonicalizer (Task #32), we can now safely handle
    /// `Lazy` (type aliases) and `Application` (generics) structurally. These types
    /// are now "unlocked" for simplification because:
    /// - `Lazy` types are canonicalized using De Bruijn indices
    /// - `Application` types are recursively canonicalized
    /// - The SubtypeChecker's fast-path (Task #36) uses O(1) structural identity
    ///
    /// Types that remain "complex" are those that are **inherently deferred**:
    /// - `TypeParameter`, `Infer`: Waiting for generic substitution
    /// - `Conditional`, `Mapped`, `IndexAccess`, `KeyOf`: Require type-level computation
    /// - These cannot be compared structurally until they are fully evaluated
    fn is_complex_type(&self, type_id: TypeId) -> bool {
        let Some(key) = self.interner.lookup(type_id) else {
            return false;
        };

        matches!(
            key,
            TypeKey::TypeParameter(_)
                | TypeKey::Infer(_) // Type parameter for conditional types
                | TypeKey::Conditional(_)
                | TypeKey::Mapped(_)
                | TypeKey::IndexAccess(_, _)
                | TypeKey::KeyOf(_)
                | TypeKey::TypeQuery(_)
                | TypeKey::TemplateLiteral(_)
                | TypeKey::ReadonlyType(_)
                | TypeKey::StringIntrinsic { .. }
                | TypeKey::ThisType // Context-dependent polymorphic type
                                    // Note: Lazy and Application are REMOVED (Task #37)
                                    // They are now handled by the Canonicalizer (Task #32)
        )
    }

    /// Evaluate an intersection type by recursively evaluating members and re-interning.
    /// This enables "deferred reduction" where intersections containing meta-types
    /// (e.g., `string & T[K]`) are reduced after the meta-types are evaluated.
    ///
    /// Example: `string & T[K]` where `T[K]` evaluates to `number` will become
    /// `string & number`, which then reduces to `never` via the interner's normalization.
    fn evaluate_intersection(&mut self, list_id: TypeListId) -> TypeId {
        let members = self.interner.type_list(list_id);
        let mut evaluated_members = Vec::with_capacity(members.len());

        for &member in members.iter() {
            evaluated_members.push(self.evaluate(member));
        }

        // Deep structural simplification using SubtypeChecker
        self.simplify_intersection_members(&mut evaluated_members);

        self.interner.intersection(evaluated_members)
    }

    /// Evaluate a union type by recursively evaluating members and re-interning.
    /// This enables "deferred reduction" where unions containing meta-types
    /// (e.g., `string | T[K]`) are reduced after the meta-types are evaluated.
    ///
    /// Example: `string | T[K]` where `T[K]` evaluates to `string` will become
    /// `string | string`, which then reduces to `string` via the interner's normalization.
    fn evaluate_union(&mut self, list_id: TypeListId) -> TypeId {
        let members = self.interner.type_list(list_id);
        let mut evaluated_members = Vec::with_capacity(members.len());

        for &member in members.iter() {
            evaluated_members.push(self.evaluate(member));
        }

        // Deep structural simplification using SubtypeChecker
        self.simplify_union_members(&mut evaluated_members);

        self.interner.union(evaluated_members)
    }

    /// Simplify union members by removing redundant types using deep subtype checks.
    /// If A <: B, then A | B = B (A is redundant in the union).
    ///
    /// This uses SubtypeChecker with bypass_evaluation=true to prevent infinite
    /// recursion, since TypeEvaluator has already evaluated all members.
    ///
    /// Performance: O(N²) where N is the number of members. We skip simplification
    /// if the union has more than 25 members to avoid excessive computation.
    ///
    /// ## Strategy
    ///
    /// 1. **Early exit for large unions** (>25 members) to avoid O(N²) explosion
    /// 2. **Skip complex types** that require full resolution:
    ///    - TypeParameter, Infer, Conditional, Mapped, IndexAccess, KeyOf, TypeQuery
    ///    - TemplateLiteral, ReadonlyType, String manipulation types
    ///    - Note: Lazy and Application are NOW safe (Task #37: handled by Canonicalizer)
    /// 3. **Fast-path for any/unknown**: If any member is any, entire union becomes any
    /// 4. **Identity check**: O(1) structural identity via SubtypeChecker (Task #36 fast-path)
    /// 5. **Depth limit**: MAX_SUBTYPE_DEPTH enables deep recursive type simplification (Task #37)
    ///
    /// ## Example Reductions
    ///
    /// - `"a" | string` → `string` (literal absorbed by primitive)
    /// - `number | 1 | 2` → `number` (literals absorbed by primitive)
    /// - `{ a: string } | { a: string; b: number }` → `{ a: string; b: number }`
    fn simplify_union_members(&mut self, members: &mut Vec<TypeId>) {
        // Performance guard: skip large unions
        if members.len() < 2 || members.len() > 25 {
            return;
        }

        // Fast-path: if any member is any, entire union becomes any
        // (But we don't modify members, just skip simplification - the interner will handle it)
        if members.iter().any(|&id| id.is_any()) {
            return;
        }

        // Fast-path: if any member is unknown, entire union becomes unknown
        // (Interner handles the collapse, we just stop simplification)
        if members.iter().any(|&id| id.is_unknown()) {
            return;
        }

        // OPTIMIZATION: Skip deep simplification if all members are unit types.
        // Unit types are disjoint - no member can be a subtype of another, so the
        // O(n²) SubtypeChecker loop would find nothing. The interner's
        // reduce_union_subtypes already handles shallow cases (literal<:primitive).
        if members.iter().all(|&id| self.interner.is_unit_type(id)) {
            return;
        }

        // Skip simplification if union contains types that require full resolution
        // These types are "complex" because their identity depends on evaluation context
        if members.iter().any(|&id| self.is_complex_type(id)) {
            return;
        }

        // Use SubtypeChecker with bypass_evaluation=true to prevent infinite recursion
        // Task #37: Use MAX_SUBTYPE_DEPTH for deep structural simplification
        use crate::solver::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker};
        let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
        checker.bypass_evaluation = true;
        checker.max_depth = MAX_SUBTYPE_DEPTH; // Deep simplification for recursive types
        checker.no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        let mut i = 0;
        while i < members.len() {
            let mut redundant = false;
            for j in 0..members.len() {
                if i == j {
                    continue;
                }

                // Fast-path: identity check is O(1)
                if members[i] == members[j] {
                    continue;
                }

                // If members[i] <: members[j], members[i] is redundant in the union
                // Example: "a" | string => "a" is redundant (result: string)
                if checker.is_subtype_of(members[i], members[j]) {
                    redundant = true;
                    break;
                }
            }
            if redundant {
                members.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// Simplify intersection members by removing redundant types using deep subtype checks.
    /// If A <: B, then A & B = A (B is redundant in the intersection).
    ///
    /// This uses SubtypeChecker with bypass_evaluation=true to prevent infinite
    /// recursion, since TypeEvaluator has already evaluated all members.
    ///
    /// Performance: O(N²) where N is the number of members. We skip simplification
    /// if the intersection has more than 25 members to avoid excessive computation.
    ///
    /// ## Strategy
    ///
    /// 1. **Early exit for large intersections** (>25 members) to avoid O(N²) explosion
    /// 2. **Skip complex types** that require full resolution:
    ///    - TypeParameter, Infer, Conditional, Mapped, IndexAccess, KeyOf, TypeQuery
    ///    - TemplateLiteral, ReadonlyType, String manipulation types
    ///    - Note: Lazy and Application are NOW safe (Task #37: handled by Canonicalizer)
    /// 3. **Fast-path for any/unknown**: If any member is any, entire intersection becomes any
    /// 4. **Identity check**: O(1) structural identity via SubtypeChecker (Task #36 fast-path)
    /// 5. **Depth limit**: MAX_SUBTYPE_DEPTH enables deep recursive type simplification (Task #37)
    ///
    /// ## Example Reductions
    ///
    /// - `{ a: string } & { a: string; b: number }` → `{ a: string; b: number }`
    /// - `{ readonly a: string } & { a: string }` → `{ readonly a: string }`
    /// - `number & 1` → `1` (literal is more specific)
    fn simplify_intersection_members(&mut self, members: &mut Vec<TypeId>) {
        // Performance guard: skip large intersections
        if members.len() < 2 || members.len() > 25 {
            return;
        }

        // Fast-path: if any member is any, entire intersection becomes any
        if members.iter().any(|&id| id.is_any()) {
            return;
        }

        // Skip simplification if intersection contains types that require full resolution
        if members.iter().any(|&id| self.is_complex_type(id)) {
            return;
        }

        // Use SubtypeChecker with bypass_evaluation=true to prevent infinite recursion
        // Task #37: Use MAX_SUBTYPE_DEPTH for deep structural simplification
        use crate::solver::subtype::{MAX_SUBTYPE_DEPTH, SubtypeChecker};
        let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
        checker.bypass_evaluation = true;
        checker.max_depth = MAX_SUBTYPE_DEPTH; // Deep simplification for recursive types
        checker.no_unchecked_indexed_access = self.no_unchecked_indexed_access;

        let mut i = 0;
        while i < members.len() {
            let mut redundant = false;
            for j in 0..members.len() {
                if i == j {
                    continue;
                }

                // Fast-path: identity check is O(1)
                if members[i] == members[j] {
                    continue;
                }

                // If members[j] <: members[i], members[i] is redundant in the intersection
                // Example: { a: string } & { a: string; b: number } => { a: string; b: number }
                // The supertype is redundant, we keep the more specific type
                if checker.is_subtype_of(members[j], members[i]) {
                    redundant = true;
                    break;
                }
            }
            if redundant {
                members.remove(i);
            } else {
                i += 1;
            }
        }
    }

    // =========================================================================
    // Visitor Pattern Implementation (North Star Rule 2)
    // =========================================================================

    /// Visit a TypeKey and return its evaluated form.
    ///
    /// This is the visitor dispatch method that routes to specific visit_* methods.
    /// The visiting.remove() and cache.insert() are handled in evaluate() for symmetry.
    fn visit_type_key(&mut self, type_id: TypeId, key: &TypeKey) -> TypeId {
        match key {
            TypeKey::Conditional(cond_id) => self.visit_conditional(*cond_id),
            TypeKey::IndexAccess(obj, idx) => self.visit_index_access(*obj, *idx),
            TypeKey::Mapped(mapped_id) => self.visit_mapped(*mapped_id),
            TypeKey::KeyOf(operand) => self.visit_keyof(*operand),
            TypeKey::TypeQuery(symbol) => self.visit_type_query(symbol.0, type_id),
            TypeKey::Application(app_id) => self.visit_application(*app_id),
            TypeKey::TemplateLiteral(spans) => self.visit_template_literal(*spans),
            TypeKey::Lazy(def_id) => self.visit_lazy(*def_id, type_id),
            TypeKey::StringIntrinsic { kind, type_arg } => {
                self.visit_string_intrinsic(*kind, *type_arg)
            }
            TypeKey::Intersection(list_id) => self.visit_intersection(*list_id),
            TypeKey::Union(list_id) => self.visit_union(*list_id),
            // All other types pass through unchanged (default behavior)
            _ => type_id,
        }
    }

    /// Visit a conditional type: T extends U ? X : Y
    fn visit_conditional(&mut self, cond_id: ConditionalTypeId) -> TypeId {
        let cond = self.interner.conditional_type(cond_id);
        self.evaluate_conditional(cond.as_ref())
    }

    /// Visit an index access type: T[K]
    fn visit_index_access(&mut self, object_type: TypeId, index_type: TypeId) -> TypeId {
        self.evaluate_index_access(object_type, index_type)
    }

    /// Visit a mapped type: { [K in Keys]: V }
    fn visit_mapped(&mut self, mapped_id: MappedTypeId) -> TypeId {
        let mapped = self.interner.mapped_type(mapped_id);
        self.evaluate_mapped(mapped.as_ref())
    }

    /// Visit a keyof type: keyof T
    fn visit_keyof(&mut self, operand: TypeId) -> TypeId {
        self.evaluate_keyof(operand)
    }

    /// Visit a type query: typeof expr
    fn visit_type_query(&mut self, symbol_ref: u32, original_type_id: TypeId) -> TypeId {
        use crate::solver::types::SymbolRef;
        let symbol = SymbolRef(symbol_ref);

        // Try to resolve via DefId (type alias, interface, class)
        if let Some(def_id) = self.resolver.symbol_to_def_id(symbol) {
            if let Some(resolved) = self.resolver.resolve_lazy(def_id, self.interner) {
                return resolved;
            }
        }

        // Fallback to legacy Ref resolution
        #[allow(deprecated)]
        if let Some(resolved) = self.resolver.resolve_ref(symbol, self.interner) {
            return resolved;
        }

        original_type_id
    }

    /// Visit a generic type application: Base<Args>
    fn visit_application(&mut self, app_id: TypeApplicationId) -> TypeId {
        self.evaluate_application(app_id)
    }

    /// Visit a template literal type: `hello${T}world`
    fn visit_template_literal(&mut self, spans: TemplateLiteralId) -> TypeId {
        self.evaluate_template_literal(spans)
    }

    /// Visit a lazy type reference: Lazy(DefId)
    fn visit_lazy(&mut self, def_id: DefId, original_type_id: TypeId) -> TypeId {
        if let Some(resolved) = self.resolver.resolve_lazy(def_id, self.interner) {
            // Re-evaluate the resolved type in case it needs further evaluation
            self.evaluate(resolved)
        } else {
            original_type_id
        }
    }

    /// Visit a string manipulation intrinsic type: Uppercase<T>, Lowercase<T>, etc.
    fn visit_string_intrinsic(&mut self, kind: StringIntrinsicKind, type_arg: TypeId) -> TypeId {
        self.evaluate_string_intrinsic(kind, type_arg)
    }

    /// Visit an intersection type: A & B & C
    fn visit_intersection(&mut self, list_id: TypeListId) -> TypeId {
        self.evaluate_intersection(list_id)
    }

    /// Visit a union type: A | B | C
    fn visit_union(&mut self, list_id: TypeListId) -> TypeId {
        self.evaluate_union(list_id)
    }
}

/// Convenience function for evaluating conditional types
pub fn evaluate_conditional(interner: &dyn TypeDatabase, cond: &ConditionalType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_conditional(cond)
}

/// Convenience function for evaluating index access types
pub fn evaluate_index_access(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for evaluating index access types with options.
pub fn evaluate_index_access_with_options(
    interner: &dyn TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
    no_unchecked_indexed_access: bool,
) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.set_no_unchecked_indexed_access(no_unchecked_indexed_access);
    evaluator.evaluate_index_access(object_type, index_type)
}

/// Convenience function for full type evaluation
pub fn evaluate_type(interner: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate(type_id)
}

/// Convenience function for evaluating mapped types
pub fn evaluate_mapped(interner: &dyn TypeDatabase, mapped: &MappedType) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_mapped(mapped)
}

/// Convenience function for evaluating keyof types
pub fn evaluate_keyof(interner: &dyn TypeDatabase, operand: TypeId) -> TypeId {
    let mut evaluator = TypeEvaluator::new(interner);
    evaluator.evaluate_keyof(operand)
}

// Re-enabled evaluate tests - verifying API compatibility
#[cfg(test)]
#[path = "tests/evaluate_tests.rs"]
mod tests;
