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

use crate::interner::Atom;
use crate::solver::infer::InferenceContext;
use crate::solver::instantiate::{
    TypeSubstitution, instantiate_generic, instantiate_type, instantiate_type_with_infer,
};
use crate::solver::subtype::{NoopResolver, SubtypeChecker, TypeResolver};
use crate::solver::types::*;
use crate::solver::{ApparentMemberKind, TypeDatabase, apparent_primitive_members};
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

/// Type evaluator for meta-types.
pub struct TypeEvaluator<'a, R: TypeResolver = NoopResolver> {
    interner: &'a dyn TypeDatabase,
    resolver: &'a R,
    no_unchecked_indexed_access: bool,
    cache: RefCell<FxHashMap<TypeId, TypeId>>,
    visiting: RefCell<FxHashSet<TypeId>>,
    depth: RefCell<u32>,
}

struct MappedKeys {
    string_literals: Vec<Atom>,
    has_string: bool,
    has_number: bool,
}

struct KeyofKeySet {
    string_literals: FxHashSet<Atom>,
    has_string: bool,
    has_number: bool,
    has_symbol: bool,
}

impl KeyofKeySet {
    fn new() -> Self {
        KeyofKeySet {
            string_literals: FxHashSet::default(),
            has_string: false,
            has_number: false,
            has_symbol: false,
        }
    }

    fn insert_type(&mut self, interner: &dyn TypeDatabase, type_id: TypeId) -> bool {
        let Some(key) = interner.lookup(type_id) else {
            return false;
        };

        match key {
            TypeKey::Union(members) => {
                let members = interner.type_list(members);
                members
                    .iter()
                    .all(|&member| self.insert_type(interner, member))
            }
            TypeKey::Intrinsic(kind) => match kind {
                IntrinsicKind::String => {
                    self.has_string = true;
                    true
                }
                IntrinsicKind::Number => {
                    self.has_number = true;
                    true
                }
                IntrinsicKind::Symbol => {
                    self.has_symbol = true;
                    true
                }
                IntrinsicKind::Never => true,
                _ => false,
            },
            TypeKey::Literal(LiteralValue::String(atom)) => {
                self.string_literals.insert(atom);
                true
            }
            _ => false,
        }
    }
}

const ARRAY_METHODS_RETURN_ANY: &[&str] = &[
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
const ARRAY_METHODS_RETURN_BOOLEAN: &[&str] = &["every", "includes", "some"];
const ARRAY_METHODS_RETURN_NUMBER: &[&str] = &[
    "findIndex",
    "findLastIndex",
    "indexOf",
    "lastIndexOf",
    "push",
    "unshift",
];
const ARRAY_METHODS_RETURN_VOID: &[&str] = &["forEach", "copyWithin", "fill"];
const ARRAY_METHODS_RETURN_STRING: &[&str] = &["join", "toLocaleString", "toString"];

fn is_member(name: &str, list: &[&str]) -> bool {
    list.contains(&name)
}

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
        }
    }

    pub fn set_no_unchecked_indexed_access(&mut self, enabled: bool) {
        self.no_unchecked_indexed_access = enabled;
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

        if let Some(&cached) = self.cache.borrow().get(&type_id) {
            return cached;
        }

        // Depth guard to prevent stack overflow from deeply recursive mapped types
        const MAX_DEPTH: u32 = 50;
        {
            let mut depth = self.depth.borrow_mut();
            *depth += 1;
            if *depth > MAX_DEPTH {
                *depth -= 1;
                drop(depth);
                // Return the type unevaluated to prevent deep recursion
                self.cache.borrow_mut().insert(type_id, type_id);
                return type_id;
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
        seen: &mut std::collections::HashSet<Atom>,
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
            TypeKey::Application(app_id) => {
                // Recursively evaluate the nested Application
                self.evaluate_application(app_id)
            }
            TypeKey::Ref(sym_ref) => {
                // Also try to resolve Ref types in type arguments
                // This helps with generic instantiation accuracy
                self.resolver
                    .resolve_ref(sym_ref, self.interner)
                    .unwrap_or(arg)
            }
            TypeKey::Conditional(cond_id) => {
                // Evaluate conditional types in type arguments
                let cond = self.interner.conditional_type(cond_id);
                self.evaluate_conditional(cond.as_ref())
            }
            TypeKey::Mapped(mapped_id) => {
                // Evaluate mapped types in type arguments
                let mapped = self.interner.mapped_type(mapped_id);
                self.evaluate_mapped(mapped.as_ref())
            }
            TypeKey::TemplateLiteral(spans) => {
                // Evaluate template literal types in type arguments
                self.evaluate_template_literal(spans)
            }
            _ => arg,
        }
    }

    /// Evaluate a conditional type: T extends U ? X : Y
    ///
    /// Algorithm:
    /// 1. If check_type is a union and the conditional is distributive, distribute
    /// 2. Otherwise, check if check_type <: extends_type
    /// 3. If true -> return true_type
    /// 4. If false (disjoint) -> return false_type
    /// 5. If ambiguous (unresolved type param) -> return deferred conditional
    pub fn evaluate_conditional(&self, cond: &ConditionalType) -> TypeId {
        let check_type = self.evaluate(cond.check_type);
        let extends_type = self.evaluate(cond.extends_type);

        if cond.is_distributive && check_type == TypeId::NEVER {
            return TypeId::NEVER;
        }

        if check_type == TypeId::ANY {
            // For distributive `any extends X ? T : F`:
            // - Distributive: return union of both branches (any distributes over the conditional)
            // - Non-distributive: return union of both branches (any poisons the result)
            // In both cases, we evaluate and union the branches to handle infer types correctly
            let true_eval = self.evaluate(cond.true_type);
            let false_eval = self.evaluate(cond.false_type);
            return self.interner.union2(true_eval, false_eval);
        }

        // Step 1: Check for distributivity
        // Only distribute for naked type parameters (recorded at lowering time).
        if cond.is_distributive
            && let Some(TypeKey::Union(members)) = self.interner.lookup(check_type)
        {
            let members = self.interner.type_list(members);
            return self.distribute_conditional(
                members.as_ref(),
                check_type, // Pass original check_type for substitution
                extends_type,
                cond.true_type,
                cond.false_type,
            );
        }

        if let Some(TypeKey::Infer(info)) = self.interner.lookup(extends_type) {
            if matches!(
                self.interner.lookup(check_type),
                Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
            ) {
                return self.interner.conditional(cond.clone());
            }

            if check_type == TypeId::ANY {
                let mut subst = TypeSubstitution::new();
                subst.insert(info.name, check_type);
                let true_eval = self.evaluate(instantiate_type_with_infer(
                    self.interner,
                    cond.true_type,
                    &subst,
                ));
                let false_eval = self.evaluate(instantiate_type_with_infer(
                    self.interner,
                    cond.false_type,
                    &subst,
                ));
                return self.interner.union2(true_eval, false_eval);
            }

            let mut subst = TypeSubstitution::new();
            subst.insert(info.name, check_type);
            let mut inferred = check_type;
            if let Some(constraint) = info.constraint {
                let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
                checker.enforce_weak_types = true;
                let Some(filtered) =
                    self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                else {
                    let false_inst =
                        instantiate_type_with_infer(self.interner, cond.false_type, &subst);
                    return self.evaluate(false_inst);
                };
                inferred = filtered;
            }

            subst.insert(info.name, inferred);

            let true_inst = instantiate_type_with_infer(self.interner, cond.true_type, &subst);
            return self.evaluate(true_inst);
        }

        let extends_unwrapped = match self.interner.lookup(extends_type) {
            Some(TypeKey::ReadonlyType(inner)) => inner,
            _ => extends_type,
        };
        let check_unwrapped = match self.interner.lookup(check_type) {
            Some(TypeKey::ReadonlyType(inner)) => inner,
            _ => check_type,
        };

        if let Some(TypeKey::Array(ext_elem)) = self.interner.lookup(extends_unwrapped)
            && let Some(TypeKey::Infer(info)) = self.interner.lookup(ext_elem)
        {
            if matches!(
                self.interner.lookup(check_unwrapped),
                Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
            ) {
                return self.interner.conditional(cond.clone());
            }

            let inferred = match self.interner.lookup(check_unwrapped) {
                Some(TypeKey::Array(elem)) => Some(elem),
                Some(TypeKey::Tuple(elements)) => {
                    let elements = self.interner.tuple_list(elements);
                    let mut parts = Vec::new();
                    for element in elements.iter() {
                        if element.rest {
                            let rest_type = self.rest_element_type(element.type_id);
                            parts.push(rest_type);
                        } else {
                            let elem_type = if element.optional {
                                self.interner.union2(element.type_id, TypeId::UNDEFINED)
                            } else {
                                element.type_id
                            };
                            parts.push(elem_type);
                        }
                    }
                    if parts.is_empty() {
                        None
                    } else {
                        Some(self.interner.union(parts))
                    }
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner.type_list(members);
                    let mut parts = Vec::new();
                    for &member in members.iter() {
                        match self.interner.lookup(member) {
                            Some(TypeKey::Array(elem)) => parts.push(elem),
                            Some(TypeKey::ReadonlyType(inner)) => {
                                let Some(TypeKey::Array(elem)) = self.interner.lookup(inner) else {
                                    return self.evaluate(cond.false_type);
                                };
                                parts.push(elem);
                            }
                            _ => return self.evaluate(cond.false_type),
                        }
                    }
                    if parts.is_empty() {
                        None
                    } else if parts.len() == 1 {
                        Some(parts[0])
                    } else {
                        Some(self.interner.union(parts))
                    }
                }
                _ => None,
            };

            let Some(mut inferred) = inferred else {
                return self.evaluate(cond.false_type);
            };

            let mut subst = TypeSubstitution::new();
            subst.insert(info.name, inferred);

            if let Some(constraint) = info.constraint {
                let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
                let is_union = matches!(self.interner.lookup(inferred), Some(TypeKey::Union(_)));
                if is_union && !cond.is_distributive {
                    // For unions in non-distributive conditionals, use filter that adds undefined
                    inferred = self.filter_inferred_by_constraint_or_undefined(
                        inferred,
                        constraint,
                        &mut checker,
                    );
                } else {
                    // For single values or distributive conditionals, fail if constraint doesn't match
                    if !checker.is_subtype_of(inferred, constraint) {
                        return self.evaluate(cond.false_type);
                    }
                }
                subst.insert(info.name, inferred);
            }

            let true_inst = instantiate_type_with_infer(self.interner, cond.true_type, &subst);
            return self.evaluate(true_inst);
        }

        if let Some(TypeKey::Tuple(extends_elements)) = self.interner.lookup(extends_unwrapped) {
            let extends_elements = self.interner.tuple_list(extends_elements);
            if extends_elements.len() == 1
                && !extends_elements[0].rest
                && let Some(TypeKey::Infer(info)) =
                    self.interner.lookup(extends_elements[0].type_id)
            {
                if matches!(
                    self.interner.lookup(check_unwrapped),
                    Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
                ) {
                    return self.interner.conditional(cond.clone());
                }

                let inferred = match self.interner.lookup(check_unwrapped) {
                    Some(TypeKey::Tuple(check_elements)) => {
                        let check_elements = self.interner.tuple_list(check_elements);
                        if check_elements.is_empty() {
                            extends_elements[0].optional.then_some(TypeId::UNDEFINED)
                        } else if check_elements.len() == 1 && !check_elements[0].rest {
                            let elem = &check_elements[0];
                            Some(if elem.optional {
                                self.interner.union2(elem.type_id, TypeId::UNDEFINED)
                            } else {
                                elem.type_id
                            })
                        } else {
                            None
                        }
                    }
                    Some(TypeKey::Union(members)) => {
                        let members = self.interner.type_list(members);
                        let mut inferred_members = Vec::new();
                        for &member in members.iter() {
                            let member_type = match self.interner.lookup(member) {
                                Some(TypeKey::ReadonlyType(inner)) => inner,
                                _ => member,
                            };
                            match self.interner.lookup(member_type) {
                                Some(TypeKey::Tuple(check_elements)) => {
                                    let check_elements = self.interner.tuple_list(check_elements);
                                    if check_elements.is_empty() {
                                        if extends_elements[0].optional {
                                            inferred_members.push(TypeId::UNDEFINED);
                                            continue;
                                        }
                                        return self.evaluate(cond.false_type);
                                    }
                                    if check_elements.len() == 1 && !check_elements[0].rest {
                                        let elem = &check_elements[0];
                                        let elem_type = if elem.optional {
                                            self.interner.union2(elem.type_id, TypeId::UNDEFINED)
                                        } else {
                                            elem.type_id
                                        };
                                        inferred_members.push(elem_type);
                                    } else {
                                        return self.evaluate(cond.false_type);
                                    }
                                }
                                _ => return self.evaluate(cond.false_type),
                            }
                        }
                        if inferred_members.is_empty() {
                            None
                        } else if inferred_members.len() == 1 {
                            Some(inferred_members[0])
                        } else {
                            Some(self.interner.union(inferred_members))
                        }
                    }
                    _ => None,
                };

                let Some(mut inferred) = inferred else {
                    return self.evaluate(cond.false_type);
                };

                let mut subst = TypeSubstitution::new();
                subst.insert(info.name, inferred);

                if let Some(constraint) = info.constraint {
                    let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
                    let Some(filtered) =
                        self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                    else {
                        let false_inst =
                            instantiate_type_with_infer(self.interner, cond.false_type, &subst);
                        return self.evaluate(false_inst);
                    };
                    inferred = filtered;
                    subst.insert(info.name, inferred);
                }

                let true_inst = instantiate_type_with_infer(self.interner, cond.true_type, &subst);
                return self.evaluate(true_inst);
            }
        }

        if let Some(extends_shape_id) = match self.interner.lookup(extends_unwrapped) {
            Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => Some(shape_id),
            _ => None,
        } {
            let extends_shape = self.interner.object_shape(extends_shape_id);
            let mut infer_prop = None;
            let mut infer_nested = None;

            for prop in extends_shape.properties.iter() {
                if let Some(TypeKey::Infer(info)) = self.interner.lookup(prop.type_id) {
                    if infer_prop.is_some() || infer_nested.is_some() {
                        infer_prop = None;
                        infer_nested = None;
                        break;
                    }
                    infer_prop = Some((prop.name, info, prop.optional));
                    continue;
                }

                let nested_type = match self.interner.lookup(prop.type_id) {
                    Some(TypeKey::ReadonlyType(inner)) => inner,
                    _ => prop.type_id,
                };
                if let Some(nested_shape_id) = match self.interner.lookup(nested_type) {
                    Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                        Some(shape_id)
                    }
                    _ => None,
                } {
                    let nested_shape = self.interner.object_shape(nested_shape_id);
                    let mut nested_infer = None;
                    for nested_prop in nested_shape.properties.iter() {
                        if let Some(TypeKey::Infer(info)) =
                            self.interner.lookup(nested_prop.type_id)
                        {
                            if nested_infer.is_some() {
                                nested_infer = None;
                                break;
                            }
                            nested_infer = Some((nested_prop.name, info));
                        }
                    }
                    if let Some((nested_name, info)) = nested_infer {
                        if infer_prop.is_some() || infer_nested.is_some() {
                            infer_prop = None;
                            infer_nested = None;
                            break;
                        }
                        infer_nested = Some((prop.name, nested_name, info));
                    }
                }
            }

            if let Some((prop_name, info, prop_optional)) = infer_prop {
                if matches!(
                    self.interner.lookup(check_unwrapped),
                    Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
                ) {
                    return self.interner.conditional(cond.clone());
                }

                let inferred = match self.interner.lookup(check_unwrapped) {
                    Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                        let shape = self.interner.object_shape(shape_id);
                        shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == prop_name)
                            .map(|prop| {
                                if prop_optional {
                                    self.optional_property_type(prop)
                                } else {
                                    prop.type_id
                                }
                            })
                            .or_else(|| prop_optional.then_some(TypeId::UNDEFINED))
                    }
                    Some(TypeKey::Union(members)) => {
                        let members = self.interner.type_list(members);
                        let mut inferred_members = Vec::new();
                        for &member in members.iter() {
                            let member_unwrapped = match self.interner.lookup(member) {
                                Some(TypeKey::ReadonlyType(inner)) => inner,
                                _ => member,
                            };
                            match self.interner.lookup(member_unwrapped) {
                                Some(
                                    TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id),
                                ) => {
                                    let shape = self.interner.object_shape(shape_id);
                                    if let Some(prop) =
                                        shape.properties.iter().find(|prop| prop.name == prop_name)
                                    {
                                        inferred_members.push(if prop_optional {
                                            self.optional_property_type(prop)
                                        } else {
                                            prop.type_id
                                        });
                                    } else if prop_optional {
                                        inferred_members.push(TypeId::UNDEFINED);
                                    } else {
                                        return self.evaluate(cond.false_type);
                                    }
                                }
                                _ => return self.evaluate(cond.false_type),
                            }
                        }
                        if inferred_members.is_empty() {
                            None
                        } else if inferred_members.len() == 1 {
                            Some(inferred_members[0])
                        } else {
                            Some(self.interner.union(inferred_members))
                        }
                    }
                    _ => None,
                };

                let Some(mut inferred) = inferred else {
                    return self.evaluate(cond.false_type);
                };

                let mut subst = TypeSubstitution::new();
                subst.insert(info.name, inferred);

                if let Some(constraint) = info.constraint {
                    let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
                    let is_union =
                        matches!(self.interner.lookup(inferred), Some(TypeKey::Union(_)));
                    if prop_optional {
                        let Some(filtered) =
                            self.filter_inferred_by_constraint(inferred, constraint, &mut checker)
                        else {
                            let false_inst =
                                instantiate_type_with_infer(self.interner, cond.false_type, &subst);
                            return self.evaluate(false_inst);
                        };
                        inferred = filtered;
                    } else if is_union || cond.is_distributive {
                        // For unions or distributive conditionals, use filter that adds undefined
                        inferred = self.filter_inferred_by_constraint_or_undefined(
                            inferred,
                            constraint,
                            &mut checker,
                        );
                    } else {
                        // For non-distributive single values, fail if constraint doesn't match
                        if !checker.is_subtype_of(inferred, constraint) {
                            return self.evaluate(cond.false_type);
                        }
                    }
                    subst.insert(info.name, inferred);
                }

                let true_inst = instantiate_type_with_infer(self.interner, cond.true_type, &subst);
                return self.evaluate(true_inst);
            } else if let Some((outer_name, inner_name, info)) = infer_nested {
                if matches!(
                    self.interner.lookup(check_unwrapped),
                    Some(TypeKey::TypeParameter(_)) | Some(TypeKey::Infer(_))
                ) {
                    return self.interner.conditional(cond.clone());
                }

                let inferred = match self.interner.lookup(check_unwrapped) {
                    Some(TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id)) => {
                        let shape = self.interner.object_shape(shape_id);
                        shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == outer_name)
                            .and_then(|prop| {
                                let inner_type = match self.interner.lookup(prop.type_id) {
                                    Some(TypeKey::ReadonlyType(inner)) => inner,
                                    _ => prop.type_id,
                                };
                                match self.interner.lookup(inner_type) {
                                    Some(
                                        TypeKey::Object(inner_shape_id)
                                        | TypeKey::ObjectWithIndex(inner_shape_id),
                                    ) => {
                                        let inner_shape =
                                            self.interner.object_shape(inner_shape_id);
                                        inner_shape
                                            .properties
                                            .iter()
                                            .find(|prop| prop.name == inner_name)
                                            .map(|prop| prop.type_id)
                                    }
                                    _ => None,
                                }
                            })
                    }
                    Some(TypeKey::Union(members)) => {
                        let members = self.interner.type_list(members);
                        let mut inferred_members = Vec::new();
                        for &member in members.iter() {
                            let member_unwrapped = match self.interner.lookup(member) {
                                Some(TypeKey::ReadonlyType(inner)) => inner,
                                _ => member,
                            };
                            let Some(
                                TypeKey::Object(shape_id) | TypeKey::ObjectWithIndex(shape_id),
                            ) = self.interner.lookup(member_unwrapped)
                            else {
                                return self.evaluate(cond.false_type);
                            };
                            let shape = self.interner.object_shape(shape_id);
                            let Some(prop) =
                                shape.properties.iter().find(|prop| prop.name == outer_name)
                            else {
                                return self.evaluate(cond.false_type);
                            };
                            let inner_type = match self.interner.lookup(prop.type_id) {
                                Some(TypeKey::ReadonlyType(inner)) => inner,
                                _ => prop.type_id,
                            };
                            let Some(
                                TypeKey::Object(inner_shape_id)
                                | TypeKey::ObjectWithIndex(inner_shape_id),
                            ) = self.interner.lookup(inner_type)
                            else {
                                return self.evaluate(cond.false_type);
                            };
                            let inner_shape = self.interner.object_shape(inner_shape_id);
                            let Some(inner_prop) = inner_shape
                                .properties
                                .iter()
                                .find(|prop| prop.name == inner_name)
                            else {
                                return self.evaluate(cond.false_type);
                            };
                            inferred_members.push(inner_prop.type_id);
                        }
                        if inferred_members.is_empty() {
                            None
                        } else if inferred_members.len() == 1 {
                            Some(inferred_members[0])
                        } else {
                            Some(self.interner.union(inferred_members))
                        }
                    }
                    _ => None,
                };

                let Some(mut inferred) = inferred else {
                    return self.evaluate(cond.false_type);
                };

                let mut subst = TypeSubstitution::new();
                subst.insert(info.name, inferred);

                if let Some(constraint) = info.constraint {
                    let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
                    let is_union =
                        matches!(self.interner.lookup(inferred), Some(TypeKey::Union(_)));
                    if is_union || cond.is_distributive {
                        // For unions or distributive conditionals, use filter that adds undefined
                        inferred = self.filter_inferred_by_constraint_or_undefined(
                            inferred,
                            constraint,
                            &mut checker,
                        );
                    } else {
                        // For non-distributive single values, fail if constraint doesn't match
                        if !checker.is_subtype_of(inferred, constraint) {
                            return self.evaluate(cond.false_type);
                        }
                    }
                    subst.insert(info.name, inferred);
                }

                let true_inst = instantiate_type_with_infer(self.interner, cond.true_type, &subst);
                return self.evaluate(true_inst);
            }
        }

        // Step 2: Check for naked type parameter
        if let Some(TypeKey::TypeParameter(param)) = self.interner.lookup(check_type) {
            // If extends_type contains infer patterns and the type parameter has a constraint,
            // try to infer from the constraint. This handles cases like:
            // R extends Reducer<infer S, any> ? S : never
            // where R is constrained to Reducer<any, any>
            if self.type_contains_infer(extends_type)
                && let Some(constraint) = param.constraint
            {
                let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
                checker.enforce_weak_types = true;
                let mut bindings = FxHashMap::default();
                let mut visited = FxHashSet::default();
                if self.match_infer_pattern(
                    constraint,
                    extends_type,
                    &mut bindings,
                    &mut visited,
                    &mut checker,
                ) {
                    let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                    return self.evaluate(substituted_true);
                }
            }
            // Type parameter hasn't been substituted - defer evaluation
            return self.interner.conditional(cond.clone());
        }

        // Step 3: Perform subtype check or infer pattern matching
        let mut checker = SubtypeChecker::with_resolver(self.interner, self.resolver);
        checker.enforce_weak_types = true;

        if self.type_contains_infer(extends_type) {
            let mut bindings = FxHashMap::default();
            let mut visited = FxHashSet::default();
            if self.match_infer_pattern(
                check_type,
                extends_type,
                &mut bindings,
                &mut visited,
                &mut checker,
            ) {
                let substituted_true = self.substitute_infer(cond.true_type, &bindings);
                return self.evaluate(substituted_true);
            }

            return self.evaluate(cond.false_type);
        }

        if checker.is_subtype_of(check_type, extends_type) {
            // T <: U -> true branch
            self.evaluate(cond.true_type)
        } else {
            // Check if types are definitely disjoint
            // For now, we use a simple heuristic: if not subtype, assume disjoint
            // More sophisticated: check if intersection is never
            self.evaluate(cond.false_type)
        }
    }

    /// Distribute a conditional type over a union.
    /// (A | B) extends U ? X : Y -> (A extends U ? X : Y) | (B extends U ? X : Y)
    fn distribute_conditional(
        &self,
        members: &[TypeId],
        original_check_type: TypeId,
        extends_type: TypeId,
        true_type: TypeId,
        false_type: TypeId,
    ) -> TypeId {
        let mut results: Vec<TypeId> = Vec::with_capacity(members.len());

        for &member in members {
            // Substitute the specific member if true_type or false_type references the original check_type
            // This handles cases like: NonNullable<T> = T extends null ? never : T
            // When T = A | B, we need (A extends null ? never : A) | (B extends null ? never : B)
            let substituted_true_type = if true_type == original_check_type {
                member
            } else {
                true_type
            };
            let substituted_false_type = if false_type == original_check_type {
                member
            } else {
                false_type
            };

            // Create conditional for this union member
            let member_cond = ConditionalType {
                check_type: member,
                extends_type,
                true_type: substituted_true_type,
                false_type: substituted_false_type,
                is_distributive: false,
            };

            // Recursively evaluate (handles nested unions, type parameters)
            let result = self.evaluate_conditional(&member_cond);
            results.push(result);
        }

        // Combine results into a union
        self.interner.union(results)
    }

    /// Evaluate an index access type: T[K]
    ///
    /// This resolves property access on object types.
    pub fn evaluate_index_access(&self, object_type: TypeId, index_type: TypeId) -> TypeId {
        let evaluated_object = self.evaluate(object_type);
        let evaluated_index = self.evaluate(index_type);
        if evaluated_object != object_type || evaluated_index != index_type {
            return self.evaluate_index_access(evaluated_object, evaluated_index);
        }
        // Be more strict: don't fall back to 'any' for index access
        // This improves type safety by requiring proper types
        // Returning ERROR instead of ANY makes the solver stricter
        if evaluated_object == TypeId::ANY || evaluated_index == TypeId::ANY {
            return TypeId::ERROR;
        }

        // Get the object structure
        let obj_key = match self.interner.lookup(object_type) {
            Some(k) => k,
            None => return TypeId::ERROR,
        };

        if let Some(shape) = self.apparent_primitive_shape_for_key(&obj_key) {
            return self.evaluate_object_with_index(&shape, index_type);
        }

        match obj_key {
            TypeKey::ReadonlyType(inner) => self.evaluate_index_access(inner, index_type),
            TypeKey::Ref(sym) => {
                if let Some(resolved) = self.resolver.resolve_ref(sym, self.interner) {
                    if resolved == object_type {
                        self.interner
                            .intern(TypeKey::IndexAccess(object_type, index_type))
                    } else {
                        self.evaluate_index_access(resolved, index_type)
                    }
                } else {
                    TypeId::ERROR
                }
            }
            TypeKey::TypeParameter(param) | TypeKey::Infer(param) => {
                if let Some(constraint) = param.constraint {
                    if constraint == object_type {
                        self.interner
                            .intern(TypeKey::IndexAccess(object_type, index_type))
                    } else {
                        self.evaluate_index_access(constraint, index_type)
                    }
                } else {
                    self.interner
                        .intern(TypeKey::IndexAccess(object_type, index_type))
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                self.evaluate_object_index(&shape.properties, index_type)
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                self.evaluate_object_with_index(&shape, index_type)
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let mut results = Vec::new();
                for &member in members.iter() {
                    let result = self.evaluate_index_access(member, index_type);
                    if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access {
                        results.push(result);
                    }
                }
                if results.is_empty() {
                    return TypeId::UNDEFINED;
                }
                self.interner.union(results)
            }
            TypeKey::Array(elem) => self.evaluate_array_index(elem, index_type),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                self.evaluate_tuple_index(&elements, index_type)
            }
            // For other types, keep as IndexAccess (deferred)
            _ => self
                .interner
                .intern(TypeKey::IndexAccess(object_type, index_type)),
        }
    }

    /// Evaluate property access on an object type
    fn evaluate_object_index(&self, props: &[PropertyInfo], index_type: TypeId) -> TypeId {
        // If index is a literal string, look up the property directly
        if let Some(TypeKey::Literal(LiteralValue::String(name))) = self.interner.lookup(index_type)
        {
            for prop in props {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            // Property not found
            return TypeId::UNDEFINED;
        }

        // If index is a union of literals, return union of property types
        if let Some(TypeKey::Union(members)) = self.interner.lookup(index_type) {
            let members = self.interner.type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_object_index(props, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner.union(results);
        }

        // If index is string, return union of all property types (index signature behavior)
        if index_type == TypeId::STRING {
            let union = self.union_property_types(props);
            return self.add_undefined_if_unchecked(union);
        }

        TypeId::UNDEFINED
    }

    /// Evaluate property access on an object type with index signatures.
    fn evaluate_object_with_index(&self, shape: &ObjectShape, index_type: TypeId) -> TypeId {
        // If index is a union, evaluate each member
        if let Some(TypeKey::Union(members)) = self.interner.lookup(index_type) {
            let members = self.interner.type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_object_with_index(shape, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner.union(results);
        }

        // If index is a literal string, look up the property first, then fallback to string index.
        if let Some(TypeKey::Literal(LiteralValue::String(name))) = self.interner.lookup(index_type)
        {
            for prop in &shape.properties {
                if prop.name == name {
                    return self.optional_property_type(prop);
                }
            }
            if self.is_numeric_property_name(name)
                && let Some(number_index) = shape.number_index.as_ref()
            {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = shape.string_index.as_ref() {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        // If index is a literal number, prefer number index, then string index.
        if let Some(TypeKey::Literal(LiteralValue::Number(_))) = self.interner.lookup(index_type) {
            if let Some(number_index) = shape.number_index.as_ref() {
                return self.add_undefined_if_unchecked(number_index.value_type);
            }
            if let Some(string_index) = shape.string_index.as_ref() {
                return self.add_undefined_if_unchecked(string_index.value_type);
            }
            return TypeId::UNDEFINED;
        }

        if index_type == TypeId::STRING {
            let result = if let Some(string_index) = shape.string_index.as_ref() {
                string_index.value_type
            } else {
                self.union_property_types(&shape.properties)
            };
            return self.add_undefined_if_unchecked(result);
        }

        if index_type == TypeId::NUMBER {
            let result = if let Some(number_index) = shape.number_index.as_ref() {
                number_index.value_type
            } else if let Some(string_index) = shape.string_index.as_ref() {
                string_index.value_type
            } else {
                self.union_property_types(&shape.properties)
            };
            return self.add_undefined_if_unchecked(result);
        }

        TypeId::UNDEFINED
    }

    fn union_property_types(&self, props: &[PropertyInfo]) -> TypeId {
        let all_types: Vec<TypeId> = props
            .iter()
            .map(|prop| self.optional_property_type(prop))
            .collect();
        if all_types.is_empty() {
            TypeId::UNDEFINED
        } else {
            self.interner.union(all_types)
        }
    }

    fn optional_property_type(&self, prop: &PropertyInfo) -> TypeId {
        if prop.optional {
            self.interner.union2(prop.type_id, TypeId::UNDEFINED)
        } else {
            prop.type_id
        }
    }

    fn array_keyof_keys(&self) -> Vec<TypeId> {
        let mut keys = Vec::new();
        keys.push(TypeId::NUMBER);
        keys.push(self.interner.literal_string("length"));
        for &name in ARRAY_METHODS_RETURN_ANY {
            keys.push(self.interner.literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_BOOLEAN {
            keys.push(self.interner.literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_NUMBER {
            keys.push(self.interner.literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_VOID {
            keys.push(self.interner.literal_string(name));
        }
        for &name in ARRAY_METHODS_RETURN_STRING {
            keys.push(self.interner.literal_string(name));
        }
        keys
    }

    fn append_tuple_indices(
        &self,
        elements: &[TupleElement],
        base: usize,
        out: &mut Vec<TypeId>,
    ) -> Option<usize> {
        let mut index = base;

        for element in elements {
            if element.rest {
                match self.interner.lookup(element.type_id) {
                    Some(TypeKey::Tuple(rest_elements)) => {
                        let rest_elements = self.interner.tuple_list(rest_elements);
                        match self.append_tuple_indices(&rest_elements, index, out) {
                            Some(next) => {
                                index = next;
                                continue;
                            }
                            None => return None,
                        }
                    }
                    Some(TypeKey::Array(_)) => return None,
                    _ => return None,
                }
            } else {
                out.push(self.interner.literal_string(&index.to_string()));
                index += 1;
            }
        }

        Some(index)
    }

    fn intersect_keyof_sets(&self, key_sets: &[TypeId]) -> Option<TypeId> {
        let mut parsed_sets = Vec::with_capacity(key_sets.len());
        for &key_set in key_sets {
            let mut parsed = KeyofKeySet::new();
            if !parsed.insert_type(self.interner, key_set) {
                return None;
            }
            parsed_sets.push(parsed);
        }

        let mut all_string = true;
        let mut string_possible = true;
        let mut common_literals: Option<FxHashSet<Atom>> = None;
        let mut all_number = true;
        let mut all_symbol = true;

        for set in &parsed_sets {
            if set.has_string {
                // string index signatures don't restrict literal key overlap
            } else {
                all_string = false;
                if set.string_literals.is_empty() {
                    string_possible = false;
                } else {
                    common_literals = Some(match common_literals {
                        Some(mut existing) => {
                            existing.retain(|atom| set.string_literals.contains(atom));
                            existing
                        }
                        None => set.string_literals.clone(),
                    });
                }
            }

            if !set.has_number {
                all_number = false;
            }
            if !set.has_symbol {
                all_symbol = false;
            }
        }

        let mut result_keys = Vec::new();
        if string_possible {
            if all_string {
                result_keys.push(TypeId::STRING);
            } else if let Some(common) = common_literals {
                for atom in common {
                    result_keys.push(
                        self.interner
                            .intern(TypeKey::Literal(LiteralValue::String(atom))),
                    );
                }
            }
        }
        if all_number {
            result_keys.push(TypeId::NUMBER);
        }
        if all_symbol {
            result_keys.push(TypeId::SYMBOL);
        }

        if result_keys.is_empty() {
            Some(TypeId::NEVER)
        } else if result_keys.len() == 1 {
            Some(result_keys[0])
        } else {
            Some(self.interner.union(result_keys))
        }
    }

    fn array_member_types(&self) -> Vec<TypeId> {
        vec![
            TypeId::NUMBER,
            self.apparent_method_type(TypeId::ANY),
            self.apparent_method_type(TypeId::BOOLEAN),
            self.apparent_method_type(TypeId::NUMBER),
            self.apparent_method_type(TypeId::UNDEFINED),
            self.apparent_method_type(TypeId::STRING),
        ]
    }

    fn array_member_kind(&self, name: &str) -> Option<ApparentMemberKind> {
        if name == "length" {
            return Some(ApparentMemberKind::Value(TypeId::NUMBER));
        }
        if is_member(name, ARRAY_METHODS_RETURN_ANY) {
            return Some(ApparentMemberKind::Method(TypeId::ANY));
        }
        if is_member(name, ARRAY_METHODS_RETURN_BOOLEAN) {
            return Some(ApparentMemberKind::Method(TypeId::BOOLEAN));
        }
        if is_member(name, ARRAY_METHODS_RETURN_NUMBER) {
            return Some(ApparentMemberKind::Method(TypeId::NUMBER));
        }
        if is_member(name, ARRAY_METHODS_RETURN_VOID) {
            return Some(ApparentMemberKind::Method(TypeId::VOID));
        }
        if is_member(name, ARRAY_METHODS_RETURN_STRING) {
            return Some(ApparentMemberKind::Method(TypeId::STRING));
        }
        None
    }

    fn evaluate_array_index(&self, elem: TypeId, index_type: TypeId) -> TypeId {
        if let Some(TypeKey::Union(members)) = self.interner.lookup(index_type) {
            let members = self.interner.type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_array_index(elem, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner.union(results);
        }

        if self.is_number_like(index_type) {
            return self.add_undefined_if_unchecked(elem);
        }

        if index_type == TypeId::STRING {
            let union = self.interner.union(self.array_member_types());
            return self.add_undefined_if_unchecked(union);
        }

        if let Some(TypeKey::Literal(LiteralValue::String(name))) = self.interner.lookup(index_type)
        {
            if self.is_numeric_property_name(name) {
                return self.add_undefined_if_unchecked(elem);
            }
            let name_str = self.interner.resolve_atom_ref(name);
            if let Some(member) = self.array_member_kind(name_str.as_ref()) {
                return match member {
                    ApparentMemberKind::Value(type_id) => type_id,
                    ApparentMemberKind::Method(return_type) => {
                        self.apparent_method_type(return_type)
                    }
                };
            }
            return TypeId::UNDEFINED;
        }

        elem
    }

    fn add_undefined_if_unchecked(&self, type_id: TypeId) -> TypeId {
        if !self.no_unchecked_indexed_access || type_id == TypeId::UNDEFINED {
            return type_id;
        }
        self.interner.union2(type_id, TypeId::UNDEFINED)
    }

    fn rest_element_type(&self, type_id: TypeId) -> TypeId {
        match self.interner.lookup(type_id) {
            Some(TypeKey::Array(elem)) => elem,
            Some(TypeKey::Tuple(elements)) => {
                let elements = self.interner.tuple_list(elements);
                let types: Vec<TypeId> = elements
                    .iter()
                    .map(|e| self.tuple_element_type(e))
                    .collect();
                if types.is_empty() {
                    TypeId::NEVER
                } else {
                    self.interner.union(types)
                }
            }
            _ => type_id,
        }
    }

    fn tuple_element_type(&self, element: &TupleElement) -> TypeId {
        let mut type_id = if element.rest {
            self.rest_element_type(element.type_id)
        } else {
            element.type_id
        };

        if element.optional {
            type_id = self.interner.union2(type_id, TypeId::UNDEFINED);
        }

        type_id
    }

    fn tuple_index_literal(&self, elements: &[TupleElement], idx: usize) -> Option<TypeId> {
        for (logical_idx, element) in elements.iter().enumerate() {
            if element.rest {
                match self.interner.lookup(element.type_id) {
                    Some(TypeKey::Tuple(rest_elements)) => {
                        let rest_elements = self.interner.tuple_list(rest_elements);
                        let inner_idx = idx.saturating_sub(logical_idx);
                        return self.tuple_index_literal(&rest_elements, inner_idx);
                    }
                    _ => {
                        return Some(self.tuple_element_type(element));
                    }
                }
            }

            if logical_idx == idx {
                return Some(self.tuple_element_type(element));
            }
        }

        None
    }

    /// Evaluate index access on a tuple type
    fn evaluate_tuple_index(&self, elements: &[TupleElement], index_type: TypeId) -> TypeId {
        if let Some(TypeKey::Union(members)) = self.interner.lookup(index_type) {
            let members = self.interner.type_list(members);
            let mut results = Vec::new();
            for &member in members.iter() {
                let result = self.evaluate_tuple_index(elements, member);
                if result != TypeId::UNDEFINED || self.no_unchecked_indexed_access {
                    results.push(result);
                }
            }
            if results.is_empty() {
                return TypeId::UNDEFINED;
            }
            return self.interner.union(results);
        }

        // If index is a literal number, return the specific element
        if let Some(TypeKey::Literal(LiteralValue::Number(n))) = self.interner.lookup(index_type) {
            let value = n.0;
            if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                return TypeId::UNDEFINED;
            }
            let idx = value as usize;
            return self
                .tuple_index_literal(elements, idx)
                .unwrap_or(TypeId::UNDEFINED);
        }

        if index_type == TypeId::STRING {
            let mut types: Vec<TypeId> = elements
                .iter()
                .map(|e| self.tuple_element_type(e))
                .collect();
            types.extend(self.array_member_types());
            if types.is_empty() {
                return TypeId::NEVER;
            }
            let union = self.interner.union(types);
            return self.add_undefined_if_unchecked(union);
        }

        if let Some(TypeKey::Literal(LiteralValue::String(name))) = self.interner.lookup(index_type)
        {
            if self.is_numeric_property_name(name) {
                let name_str = self.interner.resolve_atom_ref(name);
                if let Ok(idx) = name_str.as_ref().parse::<i64>()
                    && let Ok(idx) = usize::try_from(idx)
                {
                    return self
                        .tuple_index_literal(elements, idx)
                        .unwrap_or(TypeId::UNDEFINED);
                }
                return TypeId::UNDEFINED;
            }

            let name_str = self.interner.resolve_atom_ref(name);
            if let Some(member) = self.array_member_kind(name_str.as_ref()) {
                return match member {
                    ApparentMemberKind::Value(type_id) => type_id,
                    ApparentMemberKind::Method(return_type) => {
                        self.apparent_method_type(return_type)
                    }
                };
            }

            return TypeId::UNDEFINED;
        }

        // If index is number, return union of all element types
        if index_type == TypeId::NUMBER {
            let all_types: Vec<TypeId> = elements
                .iter()
                .map(|e| self.tuple_element_type(e))
                .collect();
            if all_types.is_empty() {
                return TypeId::NEVER;
            }
            let union = self.interner.union(all_types);
            return self.add_undefined_if_unchecked(union);
        }

        TypeId::UNDEFINED
    }

    /// Check if a type is number-like (number or numeric literal)
    fn is_number_like(&self, type_id: TypeId) -> bool {
        if type_id == TypeId::NUMBER {
            return true;
        }
        if let Some(TypeKey::Literal(LiteralValue::Number(_))) = self.interner.lookup(type_id) {
            return true;
        }
        false
    }

    /// Evaluate a mapped type: { [K in Keys]: Template }
    ///
    /// Algorithm:
    /// 1. Extract the constraint (Keys) - this defines what keys to iterate over
    /// 2. For each key K in the constraint:
    ///    - Substitute K into the template type
    ///    - Apply readonly/optional modifiers
    /// 3. Construct a new object type with the resulting properties
    pub fn evaluate_mapped(&self, mapped: &MappedType) -> TypeId {
        // Get the constraint - this tells us what keys to iterate over
        let constraint = mapped.constraint;

        // Evaluate the constraint to get concrete keys
        let keys = self.evaluate_keyof_or_constraint(constraint);

        // If we can't determine concrete keys, keep it as a mapped type (deferred)
        let key_set = match self.extract_mapped_keys(keys) {
            Some(keys) => keys,
            None => return self.interner.mapped(mapped.clone()),
        };

        // Check if this is a homomorphic mapped type (template is T[K] indexed access)
        // In this case, we should preserve the original property modifiers
        let is_homomorphic = self.is_homomorphic_mapped_type(mapped);

        // Extract source object type if this is homomorphic
        // For { [K in keyof T]: T[K] }, the constraint is keyof T and template is T[K]
        let source_object = if is_homomorphic {
            self.extract_source_from_homomorphic(mapped)
        } else {
            None
        };

        let remap_key_type = |key_type: TypeId| -> Result<Option<TypeId>, ()> {
            let Some(name_type) = mapped.name_type else {
                return Ok(Some(key_type));
            };

            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_type);
            let remapped = instantiate_type(self.interner, name_type, &subst);
            let remapped = self.evaluate(remapped);
            if remapped == TypeId::NEVER {
                return Ok(None);
            }
            Ok(Some(remapped))
        };

        // Helper to get property modifiers for a given key
        let get_property_modifiers = |key_name: Atom| -> (bool, bool) {
            if let Some(source_obj) = source_object {
                if let Some(TypeKey::Object(shape_id)) = self.interner.lookup(source_obj) {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in &shape.properties {
                        if prop.name == key_name {
                            return (prop.optional, prop.readonly);
                        }
                    }
                } else if let Some(TypeKey::ObjectWithIndex(shape_id)) = self.interner.lookup(source_obj) {
                    let shape = self.interner.object_shape(shape_id);
                    for prop in &shape.properties {
                        if prop.name == key_name {
                            return (prop.optional, prop.readonly);
                        }
                    }
                }
            }
            // Default modifiers when we can't determine
            (false, false)
        };

        let get_modifiers_for_key = |_key_type: TypeId, key_name: Atom| -> (bool, bool) {
            let mut optional = match mapped.optional_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    // For homomorphic types with no explicit modifier, preserve original
                    if is_homomorphic {
                        get_property_modifiers(key_name).0
                    } else {
                        false
                    }
                }
            };

            let readonly = match mapped.readonly_modifier {
                Some(MappedModifier::Add) => true,
                Some(MappedModifier::Remove) => false,
                None => {
                    // For homomorphic types with no explicit modifier, preserve original
                    if is_homomorphic {
                        get_property_modifiers(key_name).1
                    } else {
                        false
                    }
                }
            };

            (optional, readonly)
        };

        // Build the resulting object properties
        let mut properties = Vec::new();

        for key_name in key_set.string_literals {
            // Create substitution: type_param.name -> literal key type
            // First intern the Atom as a literal string type
            let key_literal = self
                .interner
                .intern(TypeKey::Literal(LiteralValue::String(key_name)));
            let remapped = match remap_key_type(key_literal) {
                Ok(Some(remapped)) => remapped,
                Ok(None) => continue,
                Err(()) => return self.interner.mapped(mapped.clone()),
            };
            let remapped_name = match self.interner.lookup(remapped) {
                Some(TypeKey::Literal(LiteralValue::String(name))) => name,
                _ => return self.interner.mapped(mapped.clone()),
            };

            let mut subst = TypeSubstitution::new();
            subst.insert(mapped.type_param.name, key_literal);

            // Substitute into the template
            let property_type =
                self.evaluate(instantiate_type(self.interner, mapped.template, &subst));

            // Get modifiers for this specific key (preserves homomorphic behavior)
            let (optional, readonly) = get_modifiers_for_key(key_literal, key_name);

            properties.push(PropertyInfo {
                name: remapped_name,
                type_id: property_type,
                write_type: property_type,
                optional,
                readonly,
                is_method: false,
            });
        }

        let string_index = if key_set.has_string {
            match remap_key_type(TypeId::STRING) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::STRING {
                        return self.interner.mapped(mapped.clone());
                    }
                    let key_type = TypeId::STRING;
                    let mut subst = TypeSubstitution::new();
                    subst.insert(mapped.type_param.name, key_type);
                    let mut value_type =
                        self.evaluate(instantiate_type(self.interner, mapped.template, &subst));

                    // Get modifiers for string index
                    let (idx_optional, idx_readonly) = get_modifiers_for_key(key_type, self.interner.intern_string(""));
                    if idx_optional {
                        value_type = self.interner.union2(value_type, TypeId::UNDEFINED);
                    }
                    Some(IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx_readonly,
                    })
                }
                Ok(None) => None,
                Err(()) => return self.interner.mapped(mapped.clone()),
            }
        } else {
            None
        };

        let number_index = if key_set.has_number {
            match remap_key_type(TypeId::NUMBER) {
                Ok(Some(remapped)) => {
                    if remapped != TypeId::NUMBER {
                        return self.interner.mapped(mapped.clone());
                    }
                    let key_type = TypeId::NUMBER;
                    let mut subst = TypeSubstitution::new();
                    subst.insert(mapped.type_param.name, key_type);
                    let mut value_type =
                        self.evaluate(instantiate_type(self.interner, mapped.template, &subst));

                    // Get modifiers for number index
                    let (idx_optional, idx_readonly) = get_modifiers_for_key(key_type, self.interner.intern_string(""));
                    if idx_optional {
                        value_type = self.interner.union2(value_type, TypeId::UNDEFINED);
                    }
                    Some(IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx_readonly,
                    })
                }
                Ok(None) => None,
                Err(()) => return self.interner.mapped(mapped.clone()),
            }
        } else {
            None
        };

        if string_index.is_some() || number_index.is_some() {
            self.interner.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
            })
        } else {
            self.interner.object(properties)
        }
    }

    /// Check if a mapped type is homomorphic (template is T[K] indexed access)
    /// Homomorphic mapped types preserve modifiers from the source type
    fn is_homomorphic_mapped_type(&self, mapped: &MappedType) -> bool {
        // Check if template is an IndexAccess type
        match self.interner.lookup(mapped.template) {
            Some(TypeKey::IndexAccess(_obj, idx)) => {
                // Check if the index is our type parameter
                match self.interner.lookup(idx) {
                    Some(TypeKey::TypeParameter(param)) => param.name == mapped.type_param.name,
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Extract the source object type from a homomorphic mapped type
    /// For { [K in keyof T]: T[K] }, extract T
    fn extract_source_from_homomorphic(&self, mapped: &MappedType) -> Option<TypeId> {
        match self.interner.lookup(mapped.template) {
            Some(TypeKey::IndexAccess(obj, _idx)) => {
                // The object part of T[K] is the source type
                Some(obj)
            }
            _ => None,
        }
    }

    /// Evaluate a template literal type: `hello${T}world`
    ///
    /// Template literals evaluate to a union of all possible literal string combinations.
    /// For example: `get${K}` where K = "a" | "b" evaluates to "geta" | "getb"
    fn evaluate_template_literal(&self, spans: TemplateLiteralId) -> TypeId {
        let span_list = self.interner.template_list(spans);

        // Check if all spans are just text (no interpolation)
        let all_text = span_list.iter().all(|span| matches!(span, TemplateSpan::Text(_)));

        if all_text {
            // Concatenate all text spans into a single string literal
            let mut result = String::new();
            for span in span_list.iter() {
                if let TemplateSpan::Text(atom) = span {
                    result.push_str(self.interner.resolve_atom_ref(*atom).as_ref());
                }
            }
            return self.interner.literal_string(&result);
        }

        // Check if we can fully evaluate to a union of literals
        let mut combinations = vec![String::new()];

        for span in span_list.iter() {
            match span {
                TemplateSpan::Text(atom) => {
                    let text = self.interner.resolve_atom_ref(*atom).to_string();
                    for combo in &mut combinations {
                        combo.push_str(&text);
                    }
                }
                TemplateSpan::Type(type_id) => {
                    let evaluated = self.evaluate(*type_id);

                    // Check if this is a union of literal strings
                    if let Some(TypeKey::Union(members)) = self.interner.lookup(evaluated) {
                        let members = self.interner.type_list(members);
                        let mut new_combinations = Vec::new();

                        for combo in &combinations {
                            for &member in members.iter() {
                                if let Some(TypeKey::Literal(LiteralValue::String(atom))) =
                                    self.interner.lookup(member)
                                {
                                    let text = self.interner.resolve_atom_ref(atom).to_string();
                                    new_combinations.push(format!("{}{}", combo, text));
                                } else {
                                    // Can't fully evaluate - return template literal as-is
                                    return self.interner.template_literal(span_list.to_vec());
                                }
                            }
                        }
                        combinations = new_combinations;
                    } else if let Some(TypeKey::Literal(LiteralValue::String(atom))) =
                        self.interner.lookup(evaluated)
                    {
                        let text = self.interner.resolve_atom_ref(atom).to_string();
                        for combo in &mut combinations {
                            combo.push_str(&text);
                        }
                    } else if evaluated == TypeId::STRING {
                        // String type in template means any string - can't fully evaluate
                        return self.interner.template_literal(span_list.to_vec());
                    } else {
                        // Can't evaluate this type - return template literal as-is
                        return self.interner.template_literal(span_list.to_vec());
                    }
                }
            }
        }

        // Convert combinations to union of literal strings
        if combinations.is_empty() {
            return TypeId::NEVER;
        }

        let literal_types: Vec<TypeId> = combinations
            .iter()
            .map(|s| self.interner.literal_string(s))
            .collect();

        if literal_types.len() == 1 {
            literal_types[0]
        } else {
            self.interner.union(literal_types)
        }
    }

    /// Evaluate keyof T - extract the keys of an object type
    pub fn evaluate_keyof(&self, operand: TypeId) -> TypeId {
        // First evaluate the operand in case it's a meta-type
        let evaluated_operand = self.evaluate(operand);

        let key = match self.interner.lookup(evaluated_operand) {
            Some(k) => k,
            None => return TypeId::NEVER,
        };

        match key {
            TypeKey::ReadonlyType(inner) => self.evaluate_keyof(inner),
            TypeKey::Ref(sym) => {
                if let Some(resolved) = self.resolver.resolve_ref(sym, self.interner) {
                    if resolved == evaluated_operand {
                        self.interner.intern(TypeKey::KeyOf(operand))
                    } else {
                        self.evaluate_keyof(resolved)
                    }
                } else {
                    TypeId::ERROR
                }
            }
            TypeKey::TypeParameter(param) | TypeKey::Infer(param) => {
                if let Some(constraint) = param.constraint {
                    if constraint == evaluated_operand {
                        self.interner.intern(TypeKey::KeyOf(operand))
                    } else {
                        self.evaluate_keyof(constraint)
                    }
                } else {
                    self.interner.intern(TypeKey::KeyOf(operand))
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                if shape.properties.is_empty() {
                    return TypeId::NEVER;
                }
                let key_types: Vec<TypeId> = shape
                    .properties
                    .iter()
                    .map(|p| {
                        self.interner
                            .intern(TypeKey::Literal(LiteralValue::String(p.name)))
                    })
                    .collect();
                self.interner.union(key_types)
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let mut key_types: Vec<TypeId> = shape
                    .properties
                    .iter()
                    .map(|p| {
                        self.interner
                            .intern(TypeKey::Literal(LiteralValue::String(p.name)))
                    })
                    .collect();

                if shape.string_index.is_some() {
                    key_types.push(TypeId::STRING);
                    key_types.push(TypeId::NUMBER);
                } else if shape.number_index.is_some() {
                    key_types.push(TypeId::NUMBER);
                }

                if key_types.is_empty() {
                    TypeId::NEVER
                } else {
                    self.interner.union(key_types)
                }
            }
            TypeKey::Array(_) => self.interner.union(self.array_keyof_keys()),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                let mut key_types: Vec<TypeId> = Vec::new();
                self.append_tuple_indices(&elements, 0, &mut key_types);
                let mut array_keys = self.array_keyof_keys();
                key_types.append(&mut array_keys);
                if key_types.is_empty() {
                    return TypeId::NEVER;
                }
                self.interner.union(key_types)
            }
            TypeKey::Intrinsic(kind) => match kind {
                IntrinsicKind::Any => {
                    // keyof any = string | number | symbol
                    self.interner
                        .union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL)
                }
                IntrinsicKind::Unknown => {
                    // keyof unknown = never
                    TypeId::NEVER
                }
                IntrinsicKind::Never
                | IntrinsicKind::Void
                | IntrinsicKind::Null
                | IntrinsicKind::Undefined
                | IntrinsicKind::Object => TypeId::NEVER,
                IntrinsicKind::String
                | IntrinsicKind::Number
                | IntrinsicKind::Boolean
                | IntrinsicKind::Bigint
                | IntrinsicKind::Symbol => self.apparent_primitive_keyof(kind),
            },
            TypeKey::Literal(literal) => {
                if let Some(kind) = self.apparent_literal_kind(&literal) {
                    self.apparent_primitive_keyof(kind)
                } else {
                    self.interner.intern(TypeKey::KeyOf(operand))
                }
            }
            TypeKey::TemplateLiteral(_) => self.apparent_primitive_keyof(IntrinsicKind::String),
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                // keyof (A | B) = keyof A & keyof B
                let key_sets: Vec<TypeId> =
                    members.iter().map(|&m| self.evaluate_keyof(m)).collect();
                // Prefer explicit key-set intersection to avoid opaque literal intersections.
                if let Some(intersection) = self.intersect_keyof_sets(&key_sets) {
                    intersection
                } else {
                    self.interner.intersection(key_sets)
                }
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                // keyof (A & B) = keyof A | keyof B
                let key_sets: Vec<TypeId> =
                    members.iter().map(|&m| self.evaluate_keyof(m)).collect();
                self.interner.union(key_sets)
            }
            // For other types (type parameters, etc.), keep as KeyOf (deferred)
            _ => self.interner.intern(TypeKey::KeyOf(operand)),
        }
    }

    /// Helper to evaluate keyof or pass through union constraint
    fn evaluate_keyof_or_constraint(&self, constraint: TypeId) -> TypeId {
        if let Some(TypeKey::Conditional(cond_id)) = self.interner.lookup(constraint) {
            let cond = self.interner.conditional_type(cond_id);
            return self.evaluate_conditional(cond.as_ref());
        }

        // If constraint is already a union of literals, return it
        if let Some(TypeKey::Union(_)) = self.interner.lookup(constraint) {
            return constraint;
        }

        // If constraint is a literal, return it
        if let Some(TypeKey::Literal(LiteralValue::String(_))) = self.interner.lookup(constraint) {
            return constraint;
        }

        // If constraint is KeyOf, evaluate it
        if let Some(TypeKey::KeyOf(operand)) = self.interner.lookup(constraint) {
            return self.evaluate_keyof(operand);
        }

        // Otherwise return as-is
        constraint
    }

    /// Extract mapped keys from a type (for mapped type iteration)
    fn extract_mapped_keys(&self, type_id: TypeId) -> Option<MappedKeys> {
        let key = self.interner.lookup(type_id)?;

        let mut keys = MappedKeys {
            string_literals: Vec::new(),
            has_string: false,
            has_number: false,
        };

        match key {
            TypeKey::Literal(LiteralValue::String(s)) => {
                keys.string_literals.push(s);
                Some(keys)
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    if member == TypeId::STRING {
                        keys.has_string = true;
                        continue;
                    }
                    if member == TypeId::NUMBER {
                        keys.has_number = true;
                        continue;
                    }
                    if member == TypeId::SYMBOL {
                        // We don't model symbol index signatures yet; ignore symbol keys.
                        continue;
                    }
                    if let Some(TypeKey::Literal(LiteralValue::String(s))) =
                        self.interner.lookup(member)
                    {
                        keys.string_literals.push(s);
                    } else {
                        // Non-literal in union - can't fully evaluate
                        return None;
                    }
                }
                if !keys.has_string && !keys.has_number && keys.string_literals.is_empty() {
                    // Only symbol keys (or nothing) - defer until we support symbol indices.
                    return None;
                }
                Some(keys)
            }
            TypeKey::Intrinsic(IntrinsicKind::String) => {
                keys.has_string = true;
                Some(keys)
            }
            TypeKey::Intrinsic(IntrinsicKind::Number) => {
                keys.has_number = true;
                Some(keys)
            }
            TypeKey::Intrinsic(IntrinsicKind::Never) => {
                // Mapped over `never` yields an empty object.
                Some(keys)
            }
            // Can't extract literals from other types
            _ => None,
        }
    }

    fn apparent_literal_kind(&self, literal: &LiteralValue) -> Option<IntrinsicKind> {
        match literal {
            LiteralValue::String(_) => Some(IntrinsicKind::String),
            LiteralValue::Number(_) => Some(IntrinsicKind::Number),
            LiteralValue::BigInt(_) => Some(IntrinsicKind::Bigint),
            LiteralValue::Boolean(_) => Some(IntrinsicKind::Boolean),
        }
    }

    fn apparent_primitive_shape_for_key(&self, key: &TypeKey) -> Option<ObjectShape> {
        let kind = self.apparent_primitive_kind(key)?;
        Some(self.apparent_primitive_shape(kind))
    }

    fn apparent_primitive_kind(&self, key: &TypeKey) -> Option<IntrinsicKind> {
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

    fn apparent_primitive_shape(&self, kind: IntrinsicKind) -> ObjectShape {
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

    fn apparent_method_type(&self, return_type: TypeId) -> TypeId {
        let rest_array = self.interner.array(TypeId::ANY);
        let rest_param = ParamInfo {
            name: None,
            type_id: rest_array,
            optional: false,
            rest: true,
        };
        self.interner.function(FunctionShape {
            params: vec![rest_param],
            this_type: None,
            return_type,
            type_params: Vec::new(),
            type_predicate: None,
            is_constructor: false,
            is_method: false,
        })
    }

    fn apparent_primitive_keyof(&self, kind: IntrinsicKind) -> TypeId {
        let members = apparent_primitive_members(self.interner, kind);
        let mut key_types = Vec::with_capacity(members.len());
        for member in members {
            key_types.push(self.interner.literal_string(member.name));
        }
        if kind == IntrinsicKind::String {
            key_types.push(TypeId::NUMBER);
        }
        if key_types.is_empty() {
            TypeId::NEVER
        } else {
            self.interner.union(key_types)
        }
    }

    fn substitute_infer(&self, type_id: TypeId, bindings: &FxHashMap<Atom, TypeId>) -> TypeId {
        if bindings.is_empty() {
            return type_id;
        }
        let mut substitutor = InferSubstitutor::new(self.interner, bindings);
        substitutor.substitute(type_id)
    }

    fn type_contains_infer(&self, type_id: TypeId) -> bool {
        let mut visited = FxHashSet::default();
        self.type_contains_infer_inner(type_id, &mut visited)
    }

    fn type_contains_infer_inner(&self, type_id: TypeId, visited: &mut FxHashSet<TypeId>) -> bool {
        if !visited.insert(type_id) {
            return false;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return false;
        };

        match key {
            TypeKey::Infer(_) => true,
            TypeKey::Array(elem) => self.type_contains_infer_inner(elem, visited),
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                elements
                    .iter()
                    .any(|element| self.type_contains_infer_inner(element.type_id, visited))
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_infer_inner(member, visited))
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                if shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.string_index
                    && (self.type_contains_infer_inner(index.key_type, visited)
                        || self.type_contains_infer_inner(index.value_type, visited))
                {
                    return true;
                }
                if let Some(index) = &shape.number_index
                    && (self.type_contains_infer_inner(index.key_type, visited)
                        || self.type_contains_infer_inner(index.value_type, visited))
                {
                    return true;
                }
                false
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_infer_inner(param.type_id, visited))
                    || shape
                        .this_type
                        .is_some_and(|this_type| self.type_contains_infer_inner(this_type, visited))
                    || self.type_contains_infer_inner(shape.return_type, visited)
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                shape.call_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_infer_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_infer_inner(this_type, visited)
                        })
                        || self.type_contains_infer_inner(sig.return_type, visited)
                }) || shape.construct_signatures.iter().any(|sig| {
                    sig.params
                        .iter()
                        .any(|param| self.type_contains_infer_inner(param.type_id, visited))
                        || sig.this_type.is_some_and(|this_type| {
                            self.type_contains_infer_inner(this_type, visited)
                        })
                        || self.type_contains_infer_inner(sig.return_type, visited)
                }) || shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
            }
            TypeKey::TypeParameter(info) => {
                info.constraint
                    .is_some_and(|constraint| self.type_contains_infer_inner(constraint, visited))
                    || info
                        .default
                        .is_some_and(|default| self.type_contains_infer_inner(default, visited))
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                self.type_contains_infer_inner(app.base, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_infer_inner(arg, visited))
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.type_contains_infer_inner(cond.check_type, visited)
                    || self.type_contains_infer_inner(cond.extends_type, visited)
                    || self.type_contains_infer_inner(cond.true_type, visited)
                    || self.type_contains_infer_inner(cond.false_type, visited)
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                mapped
                    .type_param
                    .constraint
                    .is_some_and(|constraint| self.type_contains_infer_inner(constraint, visited))
                    || mapped
                        .type_param
                        .default
                        .is_some_and(|default| self.type_contains_infer_inner(default, visited))
                    || self.type_contains_infer_inner(mapped.constraint, visited)
                    || mapped
                        .name_type
                        .is_some_and(|name_type| self.type_contains_infer_inner(name_type, visited))
                    || self.type_contains_infer_inner(mapped.template, visited)
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.type_contains_infer_inner(obj, visited)
                    || self.type_contains_infer_inner(idx, visited)
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.type_contains_infer_inner(inner, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.type_contains_infer_inner(*inner, visited),
                })
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::Error => false,
        }
    }

    fn filter_inferred_by_constraint(
        &self,
        inferred: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        if inferred == constraint {
            return Some(inferred);
        }

        if let Some(TypeKey::Union(members)) = self.interner.lookup(inferred) {
            let members = self.interner.type_list(members);
            let mut filtered = Vec::new();
            for &member in members.iter() {
                if checker.is_subtype_of(member, constraint) {
                    filtered.push(member);
                }
            }
            return match filtered.len() {
                0 => None,
                1 => Some(filtered[0]),
                _ => Some(self.interner.union(filtered)),
            };
        }

        if checker.is_subtype_of(inferred, constraint) {
            Some(inferred)
        } else {
            None
        }
    }

    fn filter_inferred_by_constraint_or_undefined(
        &self,
        inferred: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> TypeId {
        if inferred == constraint {
            return inferred;
        }

        if let Some(TypeKey::Union(members)) = self.interner.lookup(inferred) {
            let members = self.interner.type_list(members);
            let mut filtered = Vec::new();
            let mut had_non_matching = false;
            for &member in members.iter() {
                if checker.is_subtype_of(member, constraint) {
                    filtered.push(member);
                } else {
                    had_non_matching = true;
                }
            }

            if had_non_matching {
                filtered.push(TypeId::UNDEFINED);
            }

            return match filtered.len() {
                0 => TypeId::UNDEFINED,
                1 => filtered[0],
                _ => self.interner.union(filtered),
            };
        }

        if checker.is_subtype_of(inferred, constraint) {
            inferred
        } else {
            TypeId::UNDEFINED
        }
    }

    fn bind_infer(
        &self,
        info: &TypeParamInfo,
        inferred: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut inferred = inferred;
        if let Some(constraint) = info.constraint {
            let Some(filtered) = self.filter_inferred_by_constraint(inferred, constraint, checker)
            else {
                return false;
            };
            inferred = filtered;
        }

        if let Some(existing) = bindings.get(&info.name) {
            return checker.is_subtype_of(inferred, *existing)
                && checker.is_subtype_of(*existing, inferred);
        }

        bindings.insert(info.name, inferred);
        true
    }

    fn bind_infer_defaults(
        &self,
        pattern: TypeId,
        inferred: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.bind_infer_defaults_inner(pattern, inferred, bindings, checker, &mut visited)
    }

    fn bind_infer_defaults_inner(
        &self,
        pattern: TypeId,
        inferred: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(pattern) {
            return true;
        }

        let Some(key) = self.interner.lookup(pattern) else {
            return true;
        };

        match key {
            TypeKey::Infer(info) => self.bind_infer(&info, inferred, bindings, checker),
            TypeKey::Array(elem) => {
                self.bind_infer_defaults_inner(elem, inferred, bindings, checker, visited)
            }
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                for element in elements.iter() {
                    if !self.bind_infer_defaults_inner(
                        element.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                true
            }
            TypeKey::Union(members) | TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                for &member in members.iter() {
                    if !self.bind_infer_defaults_inner(member, inferred, bindings, checker, visited)
                    {
                        return false;
                    }
                }
                true
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    if !self.bind_infer_defaults_inner(
                        prop.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                true
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                for prop in shape.properties.iter() {
                    if !self.bind_infer_defaults_inner(
                        prop.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                if let Some(index) = &shape.string_index
                    && (!self.bind_infer_defaults_inner(
                        index.key_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) || !self.bind_infer_defaults_inner(
                        index.value_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ))
                {
                    return false;
                }
                if let Some(index) = &shape.number_index
                    && (!self.bind_infer_defaults_inner(
                        index.key_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) || !self.bind_infer_defaults_inner(
                        index.value_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ))
                {
                    return false;
                }
                true
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                for param in shape.params.iter() {
                    if !self.bind_infer_defaults_inner(
                        param.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                if let Some(this_type) = shape.this_type
                    && !self
                        .bind_infer_defaults_inner(this_type, inferred, bindings, checker, visited)
                {
                    return false;
                }
                self.bind_infer_defaults_inner(
                    shape.return_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                )
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                for sig in shape.call_signatures.iter() {
                    for param in sig.params.iter() {
                        if !self.bind_infer_defaults_inner(
                            param.type_id,
                            inferred,
                            bindings,
                            checker,
                            visited,
                        ) {
                            return false;
                        }
                    }
                    if let Some(this_type) = sig.this_type
                        && !self.bind_infer_defaults_inner(
                            this_type, inferred, bindings, checker, visited,
                        )
                    {
                        return false;
                    }
                    if !self.bind_infer_defaults_inner(
                        sig.return_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                for sig in shape.construct_signatures.iter() {
                    for param in sig.params.iter() {
                        if !self.bind_infer_defaults_inner(
                            param.type_id,
                            inferred,
                            bindings,
                            checker,
                            visited,
                        ) {
                            return false;
                        }
                    }
                    if let Some(this_type) = sig.this_type
                        && !self.bind_infer_defaults_inner(
                            this_type, inferred, bindings, checker, visited,
                        )
                    {
                        return false;
                    }
                    if !self.bind_infer_defaults_inner(
                        sig.return_type,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                for prop in shape.properties.iter() {
                    if !self.bind_infer_defaults_inner(
                        prop.type_id,
                        inferred,
                        bindings,
                        checker,
                        visited,
                    ) {
                        return false;
                    }
                }
                true
            }
            TypeKey::TypeParameter(info) => {
                if let Some(constraint) = info.constraint
                    && !self
                        .bind_infer_defaults_inner(constraint, inferred, bindings, checker, visited)
                {
                    return false;
                }
                if let Some(default) = info.default
                    && !self
                        .bind_infer_defaults_inner(default, inferred, bindings, checker, visited)
                {
                    return false;
                }
                true
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                if !self.bind_infer_defaults_inner(app.base, inferred, bindings, checker, visited) {
                    return false;
                }
                for &arg in app.args.iter() {
                    if !self.bind_infer_defaults_inner(arg, inferred, bindings, checker, visited) {
                        return false;
                    }
                }
                true
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                self.bind_infer_defaults_inner(
                    cond.check_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) && self.bind_infer_defaults_inner(
                    cond.extends_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) && self.bind_infer_defaults_inner(
                    cond.true_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) && self.bind_infer_defaults_inner(
                    cond.false_type,
                    inferred,
                    bindings,
                    checker,
                    visited,
                )
            }
            TypeKey::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(mapped_id);
                if let Some(constraint) = mapped.type_param.constraint
                    && !self
                        .bind_infer_defaults_inner(constraint, inferred, bindings, checker, visited)
                {
                    return false;
                }
                if let Some(default) = mapped.type_param.default
                    && !self
                        .bind_infer_defaults_inner(default, inferred, bindings, checker, visited)
                {
                    return false;
                }
                if !self.bind_infer_defaults_inner(
                    mapped.constraint,
                    inferred,
                    bindings,
                    checker,
                    visited,
                ) {
                    return false;
                }
                if let Some(name_type) = mapped.name_type
                    && !self
                        .bind_infer_defaults_inner(name_type, inferred, bindings, checker, visited)
                {
                    return false;
                }
                self.bind_infer_defaults_inner(
                    mapped.template,
                    inferred,
                    bindings,
                    checker,
                    visited,
                )
            }
            TypeKey::IndexAccess(obj, idx) => {
                self.bind_infer_defaults_inner(obj, inferred, bindings, checker, visited)
                    && self.bind_infer_defaults_inner(idx, inferred, bindings, checker, visited)
            }
            TypeKey::KeyOf(inner) | TypeKey::ReadonlyType(inner) => {
                self.bind_infer_defaults_inner(inner, inferred, bindings, checker, visited)
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span
                        && !self
                            .bind_infer_defaults_inner(*inner, inferred, bindings, checker, visited)
                    {
                        return false;
                    }
                }
                true
            }
            TypeKey::Intrinsic(_)
            | TypeKey::Literal(_)
            | TypeKey::Ref(_)
            | TypeKey::TypeQuery(_)
            | TypeKey::UniqueSymbol(_)
            | TypeKey::ThisType
            | TypeKey::Error => true,
        }
    }

    fn match_tuple_elements(
        &self,
        source_elems: &[TupleElement],
        pattern_elems: &[TupleElement],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let source_len = source_elems.len();
        let pattern_len = pattern_elems.len();

        let mut rest_index = None;
        for (idx, elem) in pattern_elems.iter().enumerate() {
            if elem.rest {
                if rest_index.is_some() {
                    return false;
                }
                rest_index = Some(idx);
            }
        }

        if let Some(rest_index) = rest_index {
            if rest_index + 1 != pattern_len {
                return false;
            }
            if source_len < rest_index {
                return false;
            }

            for i in 0..rest_index {
                let source_elem = &source_elems[i];
                let pattern_elem = &pattern_elems[i];
                if source_elem.rest || pattern_elem.rest {
                    return false;
                }
                let source_type = if source_elem.optional {
                    self.interner.union2(source_elem.type_id, TypeId::UNDEFINED)
                } else {
                    source_elem.type_id
                };
                if !self.match_infer_pattern(
                    source_type,
                    pattern_elem.type_id,
                    bindings,
                    visited,
                    checker,
                ) {
                    return false;
                }
            }

            let mut rest_elems = Vec::new();
            for source_elem in &source_elems[rest_index..] {
                if source_elem.rest {
                    return false;
                }
                rest_elems.push(TupleElement {
                    type_id: source_elem.type_id,
                    name: source_elem.name,
                    optional: source_elem.optional,
                    rest: false,
                });
            }

            let rest_tuple = self.interner.tuple(rest_elems);
            return self.match_infer_pattern(
                rest_tuple,
                pattern_elems[rest_index].type_id,
                bindings,
                visited,
                checker,
            );
        }

        if source_len > pattern_len {
            return false;
        }

        let shared = std::cmp::min(source_len, pattern_len);
        for i in 0..shared {
            let source_elem = &source_elems[i];
            let pattern_elem = &pattern_elems[i];
            if source_elem.rest || pattern_elem.rest {
                return false;
            }
            let source_type = if source_elem.optional {
                self.interner.union2(source_elem.type_id, TypeId::UNDEFINED)
            } else {
                source_elem.type_id
            };
            if !self.match_infer_pattern(
                source_type,
                pattern_elem.type_id,
                bindings,
                visited,
                checker,
            ) {
                return false;
            }
        }

        if source_len < pattern_len {
            for pattern_elem in &pattern_elems[source_len..] {
                if pattern_elem.rest {
                    return false;
                }
                if !pattern_elem.optional {
                    return false;
                }
                if self.type_contains_infer(pattern_elem.type_id)
                    && !self.match_infer_pattern(
                        TypeId::UNDEFINED,
                        pattern_elem.type_id,
                        bindings,
                        visited,
                        checker,
                    )
                {
                    return false;
                }
            }
        }

        true
    }

    fn match_signature_params(
        &self,
        source_params: &[ParamInfo],
        pattern_params: &[ParamInfo],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if source_params.len() != pattern_params.len() {
            return false;
        }
        for (source_param, pattern_param) in source_params.iter().zip(pattern_params.iter()) {
            if source_param.optional != pattern_param.optional
                || source_param.rest != pattern_param.rest
            {
                return false;
            }
            // For optional params, add undefined to the source type for pattern matching.
            // This allows inferring T | undefined from optional params.
            let source_param_type = if source_param.optional {
                self.interner
                    .union2(source_param.type_id, TypeId::UNDEFINED)
            } else {
                source_param.type_id
            };
            if !self.match_infer_pattern(
                source_param_type,
                pattern_param.type_id,
                bindings,
                visited,
                checker,
            ) {
                return false;
            }
        }
        true
    }

    fn match_infer_pattern(
        &self,
        source: TypeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut FxHashSet<(TypeId, TypeId)>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if !visited.insert((source, pattern)) {
            return true;
        }

        if source == TypeId::NEVER {
            return self.bind_infer_defaults(pattern, TypeId::NEVER, bindings, checker);
        }

        if source == pattern {
            return true;
        }

        if let Some(TypeKey::Union(members)) = self.interner.lookup(source) {
            let members = self.interner.type_list(members);
            let base = bindings.clone();
            let mut merged = base.clone();

            for &member in members.iter() {
                let mut local = base.clone();
                if !self.match_infer_pattern(member, pattern, &mut local, visited, checker) {
                    return false;
                }

                for (name, ty) in local {
                    if base.contains_key(&name) {
                        continue;
                    }

                    if let Some(existing) = merged.get_mut(&name) {
                        if *existing != ty {
                            *existing = self.interner.union2(*existing, ty);
                        }
                    } else {
                        merged.insert(name, ty);
                    }
                }
            }

            *bindings = merged;
            return true;
        }

        let Some(pattern_key) = self.interner.lookup(pattern) else {
            return false;
        };

        match pattern_key {
            TypeKey::Infer(info) => self.bind_infer(&info, source, bindings, checker),
            TypeKey::Function(pattern_fn_id) => {
                let pattern_fn = self.interner.function_shape(pattern_fn_id);
                let has_param_infer = pattern_fn
                    .params
                    .iter()
                    .any(|param| self.type_contains_infer(param.type_id));
                let has_return_infer = self.type_contains_infer(pattern_fn.return_type);

                if pattern_fn.this_type.is_none() && has_param_infer && has_return_infer {
                    let mut match_function_params_and_return =
                        |source_type: TypeId,
                         source_fn_id: FunctionShapeId,
                         bindings: &mut FxHashMap<Atom, TypeId>|
                         -> bool {
                            let source_fn = self.interner.function_shape(source_fn_id);
                            if source_fn.params.len() != pattern_fn.params.len() {
                                return false;
                            }
                            let mut local_visited = FxHashSet::default();
                            for (source_param, pattern_param) in
                                source_fn.params.iter().zip(pattern_fn.params.iter())
                            {
                                if source_param.optional != pattern_param.optional
                                    || source_param.rest != pattern_param.rest
                                {
                                    return false;
                                }
                                let source_param_type = if source_param.optional {
                                    self.interner
                                        .union2(source_param.type_id, TypeId::UNDEFINED)
                                } else {
                                    source_param.type_id
                                };
                                if !self.match_infer_pattern(
                                    source_param_type,
                                    pattern_param.type_id,
                                    bindings,
                                    &mut local_visited,
                                    checker,
                                ) {
                                    return false;
                                }
                            }
                            if !self.match_infer_pattern(
                                source_fn.return_type,
                                pattern_fn.return_type,
                                bindings,
                                &mut local_visited,
                                checker,
                            ) {
                                return false;
                            }
                            let substituted = self.substitute_infer(pattern, bindings);
                            checker.is_subtype_of(source_type, substituted)
                        };

                    return match self.interner.lookup(source) {
                        Some(TypeKey::Function(source_fn_id)) => {
                            match_function_params_and_return(source, source_fn_id, bindings)
                        }
                        Some(TypeKey::Union(members)) => {
                            let members = self.interner.type_list(members);
                            let mut combined = FxHashMap::default();
                            for &member in members.iter() {
                                let Some(TypeKey::Function(source_fn_id)) =
                                    self.interner.lookup(member)
                                else {
                                    return false;
                                };
                                let mut member_bindings = FxHashMap::default();
                                if !match_function_params_and_return(
                                    member,
                                    source_fn_id,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                                for (name, ty) in member_bindings {
                                    combined
                                        .entry(name)
                                        .and_modify(|existing| {
                                            *existing = self.interner.union2(*existing, ty);
                                        })
                                        .or_insert(ty);
                                }
                            }
                            bindings.extend(combined);
                            true
                        }
                        _ => false,
                    };
                }

                if pattern_fn.this_type.is_none() && has_param_infer && !has_return_infer {
                    let mut match_function_params = |_source_type: TypeId,
                                                     source_fn_id: FunctionShapeId,
                                                     bindings: &mut FxHashMap<Atom, TypeId>|
                     -> bool {
                        let source_fn = self.interner.function_shape(source_fn_id);
                        if source_fn.params.len() != pattern_fn.params.len() {
                            return false;
                        }
                        let mut local_visited = FxHashSet::default();
                        for (source_param, pattern_param) in
                            source_fn.params.iter().zip(pattern_fn.params.iter())
                        {
                            if source_param.optional != pattern_param.optional
                                || source_param.rest != pattern_param.rest
                            {
                                return false;
                            }
                            let source_param_type = if source_param.optional {
                                self.interner
                                    .union2(source_param.type_id, TypeId::UNDEFINED)
                            } else {
                                source_param.type_id
                            };
                            if !self.match_infer_pattern(
                                source_param_type,
                                pattern_param.type_id,
                                bindings,
                                &mut local_visited,
                                checker,
                            ) {
                                return false;
                            }
                        }
                        // For param-only inference, parameter matching is sufficient.
                        // Skipping the final subtype check avoids issues with optional
                        // param widening (undefined added twice).
                        true
                    };

                    return match self.interner.lookup(source) {
                        Some(TypeKey::Function(source_fn_id)) => {
                            match_function_params(source, source_fn_id, bindings)
                        }
                        Some(TypeKey::Union(members)) => {
                            let members = self.interner.type_list(members);
                            let mut combined = FxHashMap::default();
                            for &member in members.iter() {
                                let Some(TypeKey::Function(source_fn_id)) =
                                    self.interner.lookup(member)
                                else {
                                    return false;
                                };
                                let mut member_bindings = FxHashMap::default();
                                if !match_function_params(
                                    member,
                                    source_fn_id,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                                for (name, ty) in member_bindings {
                                    combined
                                        .entry(name)
                                        .and_modify(|existing| {
                                            *existing = self.interner.union2(*existing, ty);
                                        })
                                        .or_insert(ty);
                                }
                            }
                            bindings.extend(combined);
                            true
                        }
                        _ => false,
                    };
                }
                if pattern_fn.this_type.is_none() && !has_param_infer && has_return_infer {
                    let mut match_function_return = |source_type: TypeId,
                                                     source_fn_id: FunctionShapeId,
                                                     bindings: &mut FxHashMap<Atom, TypeId>|
                     -> bool {
                        let source_fn = self.interner.function_shape(source_fn_id);
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            source_fn.return_type,
                            pattern_fn.return_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        let substituted = self.substitute_infer(pattern, bindings);
                        checker.is_subtype_of(source_type, substituted)
                    };

                    return match self.interner.lookup(source) {
                        Some(TypeKey::Function(source_fn_id)) => {
                            match_function_return(source, source_fn_id, bindings)
                        }
                        Some(TypeKey::Union(members)) => {
                            let members = self.interner.type_list(members);
                            let mut combined = FxHashMap::default();
                            for &member in members.iter() {
                                let Some(TypeKey::Function(source_fn_id)) =
                                    self.interner.lookup(member)
                                else {
                                    return false;
                                };
                                let mut member_bindings = FxHashMap::default();
                                if !match_function_return(
                                    member,
                                    source_fn_id,
                                    &mut member_bindings,
                                ) {
                                    return false;
                                }
                                for (name, ty) in member_bindings {
                                    combined
                                        .entry(name)
                                        .and_modify(|existing| {
                                            *existing = self.interner.union2(*existing, ty);
                                        })
                                        .or_insert(ty);
                                }
                            }
                            bindings.extend(combined);
                            true
                        }
                        _ => false,
                    };
                }

                let Some(pattern_this) = pattern_fn.this_type else {
                    return checker.is_subtype_of(source, pattern);
                };
                if !self.type_contains_infer(pattern_this) {
                    return checker.is_subtype_of(source, pattern);
                }

                if has_param_infer || has_return_infer {
                    return false;
                }

                let mut match_function_this = |source_type: TypeId,
                                               source_fn_id: FunctionShapeId,
                                               bindings: &mut FxHashMap<Atom, TypeId>|
                 -> bool {
                    let source_fn = self.interner.function_shape(source_fn_id);
                    // Use Unknown instead of Any for stricter type checking
                    // When this parameter type is not specified, use Unknown
                    let source_this = source_fn.this_type.unwrap_or(TypeId::UNKNOWN);
                    let mut local_visited = FxHashSet::default();
                    if !self.match_infer_pattern(
                        source_this,
                        pattern_this,
                        bindings,
                        &mut local_visited,
                        checker,
                    ) {
                        return false;
                    }
                    let substituted = self.substitute_infer(pattern, bindings);
                    checker.is_subtype_of(source_type, substituted)
                };

                match self.interner.lookup(source) {
                    Some(TypeKey::Function(source_fn_id)) => {
                        match_function_this(source, source_fn_id, bindings)
                    }
                    Some(TypeKey::Union(members)) => {
                        let members = self.interner.type_list(members);
                        let mut combined = FxHashMap::default();
                        for &member in members.iter() {
                            let Some(TypeKey::Function(source_fn_id)) =
                                self.interner.lookup(member)
                            else {
                                return false;
                            };
                            let mut member_bindings = FxHashMap::default();
                            if !match_function_this(member, source_fn_id, &mut member_bindings) {
                                return false;
                            }
                            for (name, ty) in member_bindings {
                                combined
                                    .entry(name)
                                    .and_modify(|existing| {
                                        *existing = self.interner.union2(*existing, ty);
                                    })
                                    .or_insert(ty);
                            }
                        }
                        bindings.extend(combined);
                        true
                    }
                    _ => false,
                }
            }
            TypeKey::Callable(pattern_shape_id) => {
                let pattern_shape = self.interner.callable_shape(pattern_shape_id);
                if pattern_shape.call_signatures.len() != 1
                    || !pattern_shape.construct_signatures.is_empty()
                    || !pattern_shape.properties.is_empty()
                {
                    return checker.is_subtype_of(source, pattern);
                }
                let pattern_sig = &pattern_shape.call_signatures[0];
                let has_param_infer = pattern_sig
                    .params
                    .iter()
                    .any(|param| self.type_contains_infer(param.type_id));
                let has_return_infer = self.type_contains_infer(pattern_sig.return_type);
                if pattern_sig.this_type.is_none() && has_param_infer && has_return_infer {
                    let mut match_params_and_return = |source_type: TypeId,
                                                       source_params: &[ParamInfo],
                                                       source_return: TypeId,
                                                       bindings: &mut FxHashMap<Atom, TypeId>|
                     -> bool {
                        let mut local_visited = FxHashSet::default();
                        if !self.match_signature_params(
                            source_params,
                            &pattern_sig.params,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        if !self.match_infer_pattern(
                            source_return,
                            pattern_sig.return_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        let substituted = self.substitute_infer(pattern, bindings);
                        checker.is_subtype_of(source_type, substituted)
                    };

                    return match self.interner.lookup(source) {
                        Some(TypeKey::Callable(source_shape_id)) => {
                            let source_shape = self.interner.callable_shape(source_shape_id);
                            if source_shape.call_signatures.len() != 1
                                || !source_shape.construct_signatures.is_empty()
                                || !source_shape.properties.is_empty()
                            {
                                return false;
                            }
                            let source_sig = &source_shape.call_signatures[0];
                            match_params_and_return(
                                source,
                                &source_sig.params,
                                source_sig.return_type,
                                bindings,
                            )
                        }
                        Some(TypeKey::Function(source_fn_id)) => {
                            let source_fn = self.interner.function_shape(source_fn_id);
                            match_params_and_return(
                                source,
                                &source_fn.params,
                                source_fn.return_type,
                                bindings,
                            )
                        }
                        Some(TypeKey::Union(members)) => {
                            let members = self.interner.type_list(members);
                            let mut combined = FxHashMap::default();
                            for &member in members.iter() {
                                let mut member_bindings = FxHashMap::default();
                                match self.interner.lookup(member) {
                                    Some(TypeKey::Callable(source_shape_id)) => {
                                        let source_shape =
                                            self.interner.callable_shape(source_shape_id);
                                        if source_shape.call_signatures.len() != 1
                                            || !source_shape.construct_signatures.is_empty()
                                            || !source_shape.properties.is_empty()
                                        {
                                            return false;
                                        }
                                        let source_sig = &source_shape.call_signatures[0];
                                        if !match_params_and_return(
                                            member,
                                            &source_sig.params,
                                            source_sig.return_type,
                                            &mut member_bindings,
                                        ) {
                                            return false;
                                        }
                                    }
                                    Some(TypeKey::Function(source_fn_id)) => {
                                        let source_fn = self.interner.function_shape(source_fn_id);
                                        if !match_params_and_return(
                                            member,
                                            &source_fn.params,
                                            source_fn.return_type,
                                            &mut member_bindings,
                                        ) {
                                            return false;
                                        }
                                    }
                                    _ => return false,
                                }
                                for (name, ty) in member_bindings {
                                    combined
                                        .entry(name)
                                        .and_modify(|existing| {
                                            *existing = self.interner.union2(*existing, ty);
                                        })
                                        .or_insert(ty);
                                }
                            }
                            bindings.extend(combined);
                            true
                        }
                        _ => false,
                    };
                }
                if pattern_sig.this_type.is_none() && has_param_infer && !has_return_infer {
                    let mut match_params = |source_params: &[ParamInfo],
                                            bindings: &mut FxHashMap<Atom, TypeId>|
                     -> bool {
                        let mut local_visited = FxHashSet::default();
                        // Match params and infer types. Skip subtype check since pattern matching
                        // success implies compatibility. The subtype check can fail for optional
                        // params due to contravariance issues with undefined.
                        self.match_signature_params(
                            source_params,
                            &pattern_sig.params,
                            bindings,
                            &mut local_visited,
                            checker,
                        )
                    };

                    return match self.interner.lookup(source) {
                        Some(TypeKey::Callable(source_shape_id)) => {
                            let source_shape = self.interner.callable_shape(source_shape_id);
                            if source_shape.call_signatures.len() != 1
                                || !source_shape.construct_signatures.is_empty()
                                || !source_shape.properties.is_empty()
                            {
                                return false;
                            }
                            let source_sig = &source_shape.call_signatures[0];
                            match_params(&source_sig.params, bindings)
                        }
                        Some(TypeKey::Function(source_fn_id)) => {
                            let source_fn = self.interner.function_shape(source_fn_id);
                            match_params(&source_fn.params, bindings)
                        }
                        Some(TypeKey::Union(members)) => {
                            let members = self.interner.type_list(members);
                            let mut combined = FxHashMap::default();
                            for &member in members.iter() {
                                let mut member_bindings = FxHashMap::default();
                                match self.interner.lookup(member) {
                                    Some(TypeKey::Callable(source_shape_id)) => {
                                        let source_shape =
                                            self.interner.callable_shape(source_shape_id);
                                        if source_shape.call_signatures.len() != 1
                                            || !source_shape.construct_signatures.is_empty()
                                            || !source_shape.properties.is_empty()
                                        {
                                            return false;
                                        }
                                        let source_sig = &source_shape.call_signatures[0];
                                        if !match_params(&source_sig.params, &mut member_bindings) {
                                            return false;
                                        }
                                    }
                                    Some(TypeKey::Function(source_fn_id)) => {
                                        let source_fn = self.interner.function_shape(source_fn_id);
                                        if !match_params(&source_fn.params, &mut member_bindings) {
                                            return false;
                                        }
                                    }
                                    _ => return false,
                                }
                                for (name, ty) in member_bindings {
                                    combined
                                        .entry(name)
                                        .and_modify(|existing| {
                                            *existing = self.interner.union2(*existing, ty);
                                        })
                                        .or_insert(ty);
                                }
                            }
                            bindings.extend(combined);
                            true
                        }
                        _ => false,
                    };
                }

                if pattern_sig.this_type.is_none() && !has_param_infer && has_return_infer {
                    let mut match_return = |source_type: TypeId,
                                            source_return: TypeId,
                                            bindings: &mut FxHashMap<Atom, TypeId>|
                     -> bool {
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            source_return,
                            pattern_sig.return_type,
                            bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        let substituted = self.substitute_infer(pattern, bindings);
                        checker.is_subtype_of(source_type, substituted)
                    };

                    return match self.interner.lookup(source) {
                        Some(TypeKey::Callable(source_shape_id)) => {
                            let source_shape = self.interner.callable_shape(source_shape_id);
                            if source_shape.call_signatures.len() != 1
                                || !source_shape.construct_signatures.is_empty()
                                || !source_shape.properties.is_empty()
                            {
                                return false;
                            }
                            let source_sig = &source_shape.call_signatures[0];
                            match_return(source, source_sig.return_type, bindings)
                        }
                        Some(TypeKey::Function(source_fn_id)) => {
                            let source_fn = self.interner.function_shape(source_fn_id);
                            match_return(source, source_fn.return_type, bindings)
                        }
                        Some(TypeKey::Union(members)) => {
                            let members = self.interner.type_list(members);
                            let mut combined = FxHashMap::default();
                            for &member in members.iter() {
                                let mut member_bindings = FxHashMap::default();
                                match self.interner.lookup(member) {
                                    Some(TypeKey::Callable(source_shape_id)) => {
                                        let source_shape =
                                            self.interner.callable_shape(source_shape_id);
                                        if source_shape.call_signatures.len() != 1
                                            || !source_shape.construct_signatures.is_empty()
                                            || !source_shape.properties.is_empty()
                                        {
                                            return false;
                                        }
                                        let source_sig = &source_shape.call_signatures[0];
                                        if !match_return(
                                            member,
                                            source_sig.return_type,
                                            &mut member_bindings,
                                        ) {
                                            return false;
                                        }
                                    }
                                    Some(TypeKey::Function(source_fn_id)) => {
                                        let source_fn = self.interner.function_shape(source_fn_id);
                                        if !match_return(
                                            member,
                                            source_fn.return_type,
                                            &mut member_bindings,
                                        ) {
                                            return false;
                                        }
                                    }
                                    _ => return false,
                                }
                                for (name, ty) in member_bindings {
                                    combined
                                        .entry(name)
                                        .and_modify(|existing| {
                                            *existing = self.interner.union2(*existing, ty);
                                        })
                                        .or_insert(ty);
                                }
                            }
                            bindings.extend(combined);
                            true
                        }
                        _ => false,
                    };
                }

                checker.is_subtype_of(source, pattern)
            }
            TypeKey::Array(pattern_elem) => match self.interner.lookup(source) {
                Some(TypeKey::Array(source_elem)) => {
                    self.match_infer_pattern(source_elem, pattern_elem, bindings, visited, checker)
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner.type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeKey::Array(source_elem)) = self.interner.lookup(member) else {
                            return false;
                        };
                        let mut member_bindings = FxHashMap::default();
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            source_elem,
                            pattern_elem,
                            &mut member_bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner.union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            },
            TypeKey::Tuple(pattern_elems) => match self.interner.lookup(source) {
                Some(TypeKey::Tuple(source_elems)) => {
                    let source_elems = self.interner.tuple_list(source_elems);
                    let pattern_elems = self.interner.tuple_list(pattern_elems);
                    self.match_tuple_elements(
                        &source_elems,
                        &pattern_elems,
                        bindings,
                        visited,
                        checker,
                    )
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner.type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let Some(TypeKey::Tuple(source_elems)) = self.interner.lookup(member)
                        else {
                            return false;
                        };
                        let source_elems = self.interner.tuple_list(source_elems);
                        let pattern_elems = self.interner.tuple_list(pattern_elems);
                        let mut member_bindings = FxHashMap::default();
                        let mut local_visited = FxHashSet::default();
                        if !self.match_tuple_elements(
                            &source_elems,
                            &pattern_elems,
                            &mut member_bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner.union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            },
            TypeKey::ReadonlyType(pattern_inner) => {
                let source_inner = match self.interner.lookup(source) {
                    Some(TypeKey::ReadonlyType(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeKey::Object(pattern_shape_id) => match self.interner.lookup(source) {
                Some(TypeKey::Object(source_shape_id))
                | Some(TypeKey::ObjectWithIndex(source_shape_id)) => {
                    let source_shape = self.interner.object_shape(source_shape_id);
                    let pattern_shape = self.interner.object_shape(pattern_shape_id);
                    for pattern_prop in &pattern_shape.properties {
                        let source_prop = source_shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == pattern_prop.name);
                        let Some(source_prop) = source_prop else {
                            if pattern_prop.optional {
                                if self.type_contains_infer(pattern_prop.type_id)
                                    && !self.match_infer_pattern(
                                        TypeId::UNDEFINED,
                                        pattern_prop.type_id,
                                        bindings,
                                        visited,
                                        checker,
                                    )
                                {
                                    return false;
                                }
                                continue;
                            }
                            return false;
                        };
                        let source_type = self.optional_property_type(source_prop);
                        if !self.match_infer_pattern(
                            source_type,
                            pattern_prop.type_id,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                    true
                }
                Some(TypeKey::Intersection(members)) => {
                    let members = self.interner.type_list(members);
                    let pattern_shape = self.interner.object_shape(pattern_shape_id);
                    for pattern_prop in &pattern_shape.properties {
                        let mut merged_type = None;
                        for &member in members.iter() {
                            let shape_id = match self.interner.lookup(member) {
                                Some(TypeKey::Object(shape_id))
                                | Some(TypeKey::ObjectWithIndex(shape_id)) => shape_id,
                                _ => return false,
                            };
                            let shape = self.interner.object_shape(shape_id);
                            if let Some(source_prop) = shape
                                .properties
                                .iter()
                                .find(|prop| prop.name == pattern_prop.name)
                            {
                                let source_type = self.optional_property_type(source_prop);
                                merged_type = Some(match merged_type {
                                    Some(existing) => {
                                        self.interner.intersection2(existing, source_type)
                                    }
                                    None => source_type,
                                });
                            }
                        }

                        let Some(source_type) = merged_type else {
                            if pattern_prop.optional {
                                if self.type_contains_infer(pattern_prop.type_id)
                                    && !self.match_infer_pattern(
                                        TypeId::UNDEFINED,
                                        pattern_prop.type_id,
                                        bindings,
                                        visited,
                                        checker,
                                    )
                                {
                                    return false;
                                }
                                continue;
                            }
                            return false;
                        };

                        if !self.match_infer_pattern(
                            source_type,
                            pattern_prop.type_id,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                    true
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner.type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            member,
                            pattern,
                            &mut member_bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner.union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            },
            TypeKey::ObjectWithIndex(pattern_shape_id) => match self.interner.lookup(source) {
                Some(TypeKey::Object(source_shape_id))
                | Some(TypeKey::ObjectWithIndex(source_shape_id)) => {
                    let source_shape = self.interner.object_shape(source_shape_id);
                    let pattern_shape = self.interner.object_shape(pattern_shape_id);
                    for pattern_prop in &pattern_shape.properties {
                        let source_prop = source_shape
                            .properties
                            .iter()
                            .find(|prop| prop.name == pattern_prop.name);
                        let Some(source_prop) = source_prop else {
                            if pattern_prop.optional {
                                if self.type_contains_infer(pattern_prop.type_id)
                                    && !self.match_infer_pattern(
                                        TypeId::UNDEFINED,
                                        pattern_prop.type_id,
                                        bindings,
                                        visited,
                                        checker,
                                    )
                                {
                                    return false;
                                }
                                continue;
                            }
                            return false;
                        };
                        let source_type = self.optional_property_type(source_prop);
                        if !self.match_infer_pattern(
                            source_type,
                            pattern_prop.type_id,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    }

                    if let Some(pattern_index) = &pattern_shape.string_index {
                        if let Some(source_index) = &source_shape.string_index {
                            if !self.match_infer_pattern(
                                source_index.key_type,
                                pattern_index.key_type,
                                bindings,
                                visited,
                                checker,
                            ) {
                                return false;
                            }
                            if !self.match_infer_pattern(
                                source_index.value_type,
                                pattern_index.value_type,
                                bindings,
                                visited,
                                checker,
                            ) {
                                return false;
                            }
                        } else {
                            let mut local_visited = FxHashSet::default();
                            if !self.match_infer_pattern(
                                TypeId::STRING,
                                pattern_index.key_type,
                                bindings,
                                &mut local_visited,
                                checker,
                            ) {
                                return false;
                            }
                            let values: Vec<TypeId> = source_shape
                                .properties
                                .iter()
                                .map(|prop| self.optional_property_type(prop))
                                .collect();
                            let value_type = if values.is_empty() {
                                TypeId::NEVER
                            } else if values.len() == 1 {
                                values[0]
                            } else {
                                self.interner.union(values)
                            };
                            let mut local_visited = FxHashSet::default();
                            if !self.match_infer_pattern(
                                value_type,
                                pattern_index.value_type,
                                bindings,
                                &mut local_visited,
                                checker,
                            ) {
                                return false;
                            }
                        }
                    }

                    if let Some(pattern_index) = &pattern_shape.number_index {
                        if let Some(source_index) = &source_shape.number_index {
                            if !self.match_infer_pattern(
                                source_index.key_type,
                                pattern_index.key_type,
                                bindings,
                                visited,
                                checker,
                            ) {
                                return false;
                            }
                            if !self.match_infer_pattern(
                                source_index.value_type,
                                pattern_index.value_type,
                                bindings,
                                visited,
                                checker,
                            ) {
                                return false;
                            }
                        } else {
                            let mut local_visited = FxHashSet::default();
                            if !self.match_infer_pattern(
                                TypeId::NUMBER,
                                pattern_index.key_type,
                                bindings,
                                &mut local_visited,
                                checker,
                            ) {
                                return false;
                            }
                            let values: Vec<TypeId> = source_shape
                                .properties
                                .iter()
                                .filter(|prop| self.is_numeric_property_name(prop.name))
                                .map(|prop| self.optional_property_type(prop))
                                .collect();
                            let value_type = if values.is_empty() {
                                TypeId::NEVER
                            } else if values.len() == 1 {
                                values[0]
                            } else {
                                self.interner.union(values)
                            };
                            let mut local_visited = FxHashSet::default();
                            if !self.match_infer_pattern(
                                value_type,
                                pattern_index.value_type,
                                bindings,
                                &mut local_visited,
                                checker,
                            ) {
                                return false;
                            }
                        }
                    }

                    true
                }
                Some(TypeKey::Union(members)) => {
                    let members = self.interner.type_list(members);
                    let mut combined = FxHashMap::default();
                    for &member in members.iter() {
                        let mut member_bindings = FxHashMap::default();
                        let mut local_visited = FxHashSet::default();
                        if !self.match_infer_pattern(
                            member,
                            pattern,
                            &mut member_bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            return false;
                        }
                        for (name, ty) in member_bindings {
                            combined
                                .entry(name)
                                .and_modify(|existing| {
                                    *existing = self.interner.union2(*existing, ty);
                                })
                                .or_insert(ty);
                        }
                    }
                    bindings.extend(combined);
                    true
                }
                _ => false,
            },
            TypeKey::Application(pattern_app_id) => match self.interner.lookup(source) {
                Some(TypeKey::Application(source_app_id)) => {
                    let source_app = self.interner.type_application(source_app_id);
                    let pattern_app = self.interner.type_application(pattern_app_id);
                    if source_app.args.len() != pattern_app.args.len() {
                        return false;
                    }
                    if !checker.is_subtype_of(source_app.base, pattern_app.base)
                        || !checker.is_subtype_of(pattern_app.base, source_app.base)
                    {
                        return false;
                    }
                    for (source_arg, pattern_arg) in
                        source_app.args.iter().zip(pattern_app.args.iter())
                    {
                        if !self.match_infer_pattern(
                            *source_arg,
                            *pattern_arg,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return false;
                        }
                    }
                    true
                }
                _ => false,
            },
            TypeKey::TemplateLiteral(pattern_spans_id) => {
                let pattern_spans = self.interner.template_list(pattern_spans_id);
                match self.interner.lookup(source) {
                    Some(TypeKey::Literal(LiteralValue::String(atom))) => {
                        let source_text = self.interner.resolve_atom_ref(atom);
                        self.match_template_literal_string(
                            source_text.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeKey::TemplateLiteral(source_spans_id)) => {
                        let source_spans = self.interner.template_list(source_spans_id);
                        self.match_template_literal_spans(
                            source,
                            source_spans.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeKey::Intrinsic(IntrinsicKind::String)) => self
                        .match_template_literal_string_type(
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        ),
                    _ => false,
                }
            }
            // Handle union pattern containing infer types
            // Pattern: infer S | T | U where S is infer and T, U are not
            // Source: A | T | U or a single type A
            // Algorithm: Match source members against non-infer pattern members,
            // then bind the infer to the remaining source members
            TypeKey::Union(pattern_members) => {
                let pattern_members = self.interner.type_list(pattern_members);

                // Find infer members and non-infer members in the pattern
                let mut infer_members: Vec<(Atom, Option<TypeId>)> = Vec::new();
                let mut non_infer_pattern_members: Vec<TypeId> = Vec::new();

                for &pattern_member in pattern_members.iter() {
                    if let Some(TypeKey::Infer(info)) = self.interner.lookup(pattern_member) {
                        infer_members.push((info.name, info.constraint));
                    } else {
                        non_infer_pattern_members.push(pattern_member);
                    }
                }

                // If no infer members, just do subtype check
                if infer_members.is_empty() {
                    return checker.is_subtype_of(source, pattern);
                }

                // Currently only handle single infer in union pattern
                if infer_members.len() != 1 {
                    return checker.is_subtype_of(source, pattern);
                }

                let (infer_name, infer_constraint) = infer_members[0];

                // Handle both union and non-union sources
                match self.interner.lookup(source) {
                    Some(TypeKey::Union(source_members)) => {
                        let source_members = self.interner.type_list(source_members);

                        // Find source members that DON'T match non-infer pattern members
                        let mut remaining_source_members: Vec<TypeId> = Vec::new();

                        for &source_member in source_members.iter() {
                            let mut matched = false;
                            for &non_infer in &non_infer_pattern_members {
                                if checker.is_subtype_of(source_member, non_infer)
                                    && checker.is_subtype_of(non_infer, source_member)
                                {
                                    matched = true;
                                    break;
                                }
                            }
                            if !matched {
                                remaining_source_members.push(source_member);
                            }
                        }

                        // Bind infer to the remaining source members
                        let inferred_type = if remaining_source_members.is_empty() {
                            TypeId::NEVER
                        } else if remaining_source_members.len() == 1 {
                            remaining_source_members[0]
                        } else {
                            self.interner.union(remaining_source_members)
                        };

                        self.bind_infer(
                            &TypeParamInfo {
                                name: infer_name,
                                constraint: infer_constraint,
                                default: None,
                            },
                            inferred_type,
                            bindings,
                            checker,
                        )
                    }
                    _ => {
                        // Source is not a union - check if source matches any non-infer pattern member
                        for &non_infer in &non_infer_pattern_members {
                            if checker.is_subtype_of(source, non_infer)
                                && checker.is_subtype_of(non_infer, source)
                            {
                                // Source is exactly a non-infer member, so infer gets never
                                return self.bind_infer(
                                    &TypeParamInfo {
                                        name: infer_name,
                                        constraint: infer_constraint,
                                        default: None,
                                    },
                                    TypeId::NEVER,
                                    bindings,
                                    checker,
                                );
                            }
                        }
                        // Source doesn't match non-infer members, so infer = source
                        self.bind_infer(
                            &TypeParamInfo {
                                name: infer_name,
                                constraint: infer_constraint,
                                default: None,
                            },
                            source,
                            bindings,
                            checker,
                        )
                    }
                }
            }
            _ => checker.is_subtype_of(source, pattern),
        }
    }

    fn match_template_literal_string(
        &self,
        source: &str,
        pattern: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut pos = 0;
        let mut index = 0;

        while index < pattern.len() {
            match pattern[index] {
                TemplateSpan::Text(text) => {
                    let text_value = self.interner.resolve_atom_ref(text);
                    let text_value = text_value.as_ref();
                    if !source[pos..].starts_with(text_value) {
                        return false;
                    }
                    pos += text_value.len();
                    index += 1;
                }
                TemplateSpan::Type(type_id) => {
                    let next_text = pattern[index + 1..].iter().find_map(|span| match span {
                        TemplateSpan::Text(text) => Some(*text),
                        TemplateSpan::Type(_) => None,
                    });
                    let end = if let Some(next_text) = next_text {
                        let next_value = self.interner.resolve_atom_ref(next_text);
                        match source[pos..].find(next_value.as_ref()) {
                            Some(offset) => pos + offset,
                            None => return false,
                        }
                    } else {
                        source.len()
                    };

                    let captured = &source[pos..end];
                    pos = end;
                    let captured_type = self.interner.literal_string(captured);

                    if let Some(TypeKey::Infer(info)) = self.interner.lookup(type_id) {
                        if !self.bind_infer(&info, captured_type, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(captured_type, type_id) {
                        return false;
                    }
                    index += 1;
                }
            }
        }

        pos == source.len()
    }

    fn match_template_literal_spans(
        &self,
        source: TypeId,
        source_spans: &[TemplateSpan],
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if pattern_spans.len() == 1
            && let TemplateSpan::Type(type_id) = pattern_spans[0]
        {
            if let Some(TypeKey::Infer(info)) = self.interner.lookup(type_id) {
                let inferred = if source_spans
                    .iter()
                    .all(|span| matches!(span, TemplateSpan::Type(_)))
                {
                    TypeId::STRING
                } else {
                    source
                };
                return self.bind_infer(&info, inferred, bindings, checker);
            }
            return checker.is_subtype_of(source, type_id);
        }

        if source_spans.len() != pattern_spans.len() {
            return false;
        }

        for (source_span, pattern_span) in source_spans.iter().zip(pattern_spans.iter()) {
            match pattern_span {
                TemplateSpan::Text(text) => match source_span {
                    TemplateSpan::Text(source_text) if source_text == text => {}
                    _ => return false,
                },
                TemplateSpan::Type(type_id) => {
                    let inferred = match source_span {
                        TemplateSpan::Text(text) => {
                            let text_value = self.interner.resolve_atom_ref(*text);
                            self.interner.literal_string(text_value.as_ref())
                        }
                        TemplateSpan::Type(source_type) => *source_type,
                    };
                    if let Some(TypeKey::Infer(info)) = self.interner.lookup(*type_id) {
                        if !self.bind_infer(&info, inferred, bindings, checker) {
                            return false;
                        }
                    } else if !checker.is_subtype_of(inferred, *type_id) {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn match_template_literal_string_type(
        &self,
        pattern_spans: &[TemplateSpan],
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if pattern_spans
            .iter()
            .any(|span| matches!(span, TemplateSpan::Text(_)))
        {
            return false;
        }

        for span in pattern_spans {
            if let TemplateSpan::Type(type_id) = span {
                if let Some(TypeKey::Infer(info)) = self.interner.lookup(*type_id) {
                    if !self.bind_infer(&info, TypeId::STRING, bindings, checker) {
                        return false;
                    }
                } else if !checker.is_subtype_of(TypeId::STRING, *type_id) {
                    return false;
                }
            }
        }

        true
    }

    fn is_numeric_property_name(&self, name: Atom) -> bool {
        let prop_name = self.interner.resolve_atom_ref(name);
        InferenceContext::is_numeric_literal_name(prop_name.as_ref())
    }
}

struct InferSubstitutor<'a> {
    interner: &'a dyn TypeDatabase,
    bindings: &'a FxHashMap<Atom, TypeId>,
    visiting: FxHashMap<TypeId, TypeId>,
}

impl<'a> InferSubstitutor<'a> {
    fn new(interner: &'a dyn TypeDatabase, bindings: &'a FxHashMap<Atom, TypeId>) -> Self {
        InferSubstitutor {
            interner,
            bindings,
            visiting: FxHashMap::default(),
        }
    }

    fn substitute(&mut self, type_id: TypeId) -> TypeId {
        if let Some(&cached) = self.visiting.get(&type_id) {
            return cached;
        }

        let Some(key) = self.interner.lookup(type_id) else {
            return type_id;
        };

        self.visiting.insert(type_id, type_id);

        let result = match key {
            TypeKey::Infer(info) => self.bindings.get(&info.name).copied().unwrap_or(type_id),
            TypeKey::Array(elem) => {
                let substituted = self.substitute(elem);
                if substituted == elem {
                    type_id
                } else {
                    self.interner.array(substituted)
                }
            }
            TypeKey::Tuple(elements) => {
                let elements = self.interner.tuple_list(elements);
                let mut changed = false;
                let mut new_elements = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let substituted = self.substitute(element.type_id);
                    if substituted != element.type_id {
                        changed = true;
                    }
                    new_elements.push(TupleElement {
                        type_id: substituted,
                        name: element.name,
                        optional: element.optional,
                        rest: element.rest,
                    });
                }
                if changed {
                    self.interner.tuple(new_elements)
                } else {
                    type_id
                }
            }
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let mut changed = false;
                let mut new_members = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    let substituted = self.substitute(member);
                    if substituted != member {
                        changed = true;
                    }
                    new_members.push(substituted);
                }
                if changed {
                    self.interner.union(new_members)
                } else {
                    type_id
                }
            }
            TypeKey::Intersection(members) => {
                let members = self.interner.type_list(members);
                let mut changed = false;
                let mut new_members = Vec::with_capacity(members.len());
                for &member in members.iter() {
                    let substituted = self.substitute(member);
                    if substituted != member {
                        changed = true;
                    }
                    new_members.push(substituted);
                }
                if changed {
                    self.interner.intersection(new_members)
                } else {
                    type_id
                }
            }
            TypeKey::Object(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let mut changed = false;
                let mut properties = Vec::with_capacity(shape.properties.len());
                for prop in shape.properties.iter() {
                    let type_id = self.substitute(prop.type_id);
                    let write_type = self.substitute(prop.write_type);
                    if type_id != prop.type_id || write_type != prop.write_type {
                        changed = true;
                    }
                    properties.push(PropertyInfo {
                        name: prop.name,
                        type_id,
                        write_type,
                        optional: prop.optional,
                        readonly: prop.readonly,
                        is_method: prop.is_method,
                    });
                }
                if changed {
                    self.interner.object(properties)
                } else {
                    type_id
                }
            }
            TypeKey::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(shape_id);
                let mut changed = false;
                let mut properties = Vec::with_capacity(shape.properties.len());
                for prop in shape.properties.iter() {
                    let type_id = self.substitute(prop.type_id);
                    let write_type = self.substitute(prop.write_type);
                    if type_id != prop.type_id || write_type != prop.write_type {
                        changed = true;
                    }
                    properties.push(PropertyInfo {
                        name: prop.name,
                        type_id,
                        write_type,
                        optional: prop.optional,
                        readonly: prop.readonly,
                        is_method: prop.is_method,
                    });
                }
                let string_index = shape.string_index.as_ref().map(|index| {
                    let key_type = self.substitute(index.key_type);
                    let value_type = self.substitute(index.value_type);
                    if key_type != index.key_type || value_type != index.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: index.readonly,
                    }
                });
                let number_index = shape.number_index.as_ref().map(|index| {
                    let key_type = self.substitute(index.key_type);
                    let value_type = self.substitute(index.value_type);
                    if key_type != index.key_type || value_type != index.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: index.readonly,
                    }
                });
                if changed {
                    self.interner.object_with_index(ObjectShape {
                        properties,
                        string_index,
                        number_index,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(cond_id);
                let check_type = self.substitute(cond.check_type);
                let extends_type = self.substitute(cond.extends_type);
                let true_type = self.substitute(cond.true_type);
                let false_type = self.substitute(cond.false_type);
                if check_type == cond.check_type
                    && extends_type == cond.extends_type
                    && true_type == cond.true_type
                    && false_type == cond.false_type
                {
                    type_id
                } else {
                    self.interner.conditional(ConditionalType {
                        check_type,
                        extends_type,
                        true_type,
                        false_type,
                        is_distributive: cond.is_distributive,
                    })
                }
            }
            TypeKey::IndexAccess(obj, idx) => {
                let new_obj = self.substitute(obj);
                let new_idx = self.substitute(idx);
                if new_obj == obj && new_idx == idx {
                    type_id
                } else {
                    self.interner.intern(TypeKey::IndexAccess(new_obj, new_idx))
                }
            }
            TypeKey::KeyOf(inner) => {
                let new_inner = self.substitute(inner);
                if new_inner == inner {
                    type_id
                } else {
                    self.interner.intern(TypeKey::KeyOf(new_inner))
                }
            }
            TypeKey::ReadonlyType(inner) => {
                let new_inner = self.substitute(inner);
                if new_inner == inner {
                    type_id
                } else {
                    self.interner.intern(TypeKey::ReadonlyType(new_inner))
                }
            }
            TypeKey::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(spans);
                let mut changed = false;
                let mut new_spans = Vec::with_capacity(spans.len());
                for span in spans.iter() {
                    let new_span = match span {
                        TemplateSpan::Text(text) => TemplateSpan::Text(*text),
                        TemplateSpan::Type(inner) => {
                            let substituted = self.substitute(*inner);
                            if substituted != *inner {
                                changed = true;
                            }
                            TemplateSpan::Type(substituted)
                        }
                    };
                    new_spans.push(new_span);
                }
                if changed {
                    self.interner.template_literal(new_spans)
                } else {
                    type_id
                }
            }
            TypeKey::Application(app_id) => {
                let app = self.interner.type_application(app_id);
                let base = self.substitute(app.base);
                let mut changed = base != app.base;
                let mut new_args = Vec::with_capacity(app.args.len());
                for &arg in &app.args {
                    let substituted = self.substitute(arg);
                    if substituted != arg {
                        changed = true;
                    }
                    new_args.push(substituted);
                }
                if changed {
                    self.interner.application(base, new_args)
                } else {
                    type_id
                }
            }
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                let mut changed = false;
                let mut new_params = Vec::with_capacity(shape.params.len());
                for param in shape.params.iter() {
                    let param_type = self.substitute(param.type_id);
                    if param_type != param.type_id {
                        changed = true;
                    }
                    new_params.push(ParamInfo {
                        name: param.name,
                        type_id: param_type,
                        optional: param.optional,
                        rest: param.rest,
                    });
                }
                let return_type = self.substitute(shape.return_type);
                if return_type != shape.return_type {
                    changed = true;
                }
                let this_type = shape.this_type.map(|t| {
                    let substituted = self.substitute(t);
                    if substituted != t {
                        changed = true;
                    }
                    substituted
                });
                if changed {
                    self.interner.function(FunctionShape {
                        params: new_params,
                        this_type,
                        return_type,
                        type_params: shape.type_params.clone(),
                        type_predicate: shape.type_predicate.clone(),
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    })
                } else {
                    type_id
                }
            }
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let mut changed = false;

                let call_signatures: Vec<CallSignature> = shape
                    .call_signatures
                    .iter()
                    .map(|sig| {
                        let mut new_params = Vec::with_capacity(sig.params.len());
                        for param in sig.params.iter() {
                            let param_type = self.substitute(param.type_id);
                            if param_type != param.type_id {
                                changed = true;
                            }
                            new_params.push(ParamInfo {
                                name: param.name,
                                type_id: param_type,
                                optional: param.optional,
                                rest: param.rest,
                            });
                        }
                        let return_type = self.substitute(sig.return_type);
                        if return_type != sig.return_type {
                            changed = true;
                        }
                        let this_type = sig.this_type.map(|t| {
                            let substituted = self.substitute(t);
                            if substituted != t {
                                changed = true;
                            }
                            substituted
                        });
                        CallSignature {
                            params: new_params,
                            this_type,
                            return_type,
                            type_params: sig.type_params.clone(),
                            type_predicate: sig.type_predicate.clone(),
                        }
                    })
                    .collect();

                let construct_signatures: Vec<CallSignature> = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| {
                        let mut new_params = Vec::with_capacity(sig.params.len());
                        for param in sig.params.iter() {
                            let param_type = self.substitute(param.type_id);
                            if param_type != param.type_id {
                                changed = true;
                            }
                            new_params.push(ParamInfo {
                                name: param.name,
                                type_id: param_type,
                                optional: param.optional,
                                rest: param.rest,
                            });
                        }
                        let return_type = self.substitute(sig.return_type);
                        if return_type != sig.return_type {
                            changed = true;
                        }
                        let this_type = sig.this_type.map(|t| {
                            let substituted = self.substitute(t);
                            if substituted != t {
                                changed = true;
                            }
                            substituted
                        });
                        CallSignature {
                            params: new_params,
                            this_type,
                            return_type,
                            type_params: sig.type_params.clone(),
                            type_predicate: sig.type_predicate.clone(),
                        }
                    })
                    .collect();

                let properties: Vec<PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let prop_type = self.substitute(prop.type_id);
                        let write_type = self.substitute(prop.write_type);
                        if prop_type != prop.type_id || write_type != prop.write_type {
                            changed = true;
                        }
                        PropertyInfo {
                            name: prop.name,
                            type_id: prop_type,
                            write_type,
                            optional: prop.optional,
                            readonly: prop.readonly,
                            is_method: prop.is_method,
                        }
                    })
                    .collect();

                let string_index = shape.string_index.as_ref().map(|idx| {
                    let key_type = self.substitute(idx.key_type);
                    let value_type = self.substitute(idx.value_type);
                    if key_type != idx.key_type || value_type != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx.readonly,
                    }
                });

                let number_index = shape.number_index.as_ref().map(|idx| {
                    let key_type = self.substitute(idx.key_type);
                    let value_type = self.substitute(idx.value_type);
                    if key_type != idx.key_type || value_type != idx.value_type {
                        changed = true;
                    }
                    IndexSignature {
                        key_type,
                        value_type,
                        readonly: idx.readonly,
                    }
                });

                if changed {
                    self.interner.callable(CallableShape {
                        call_signatures,
                        construct_signatures,
                        properties,
                        string_index,
                        number_index,
                    })
                } else {
                    type_id
                }
            }
            _ => type_id,
        };

        self.visiting.insert(type_id, result);
        result
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
