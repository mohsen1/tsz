//! Infer pattern matching for conditional types.
//!
//! Handles TypeScript's `infer` keyword in conditional types.
//! This module provides:
//! - Pattern matching for extracting types from complex type structures
//! - Binding inferred types to infer type parameters
//! - Substitution of infer bindings back into types
//!
//! Key functions:
//! - `match_infer_pattern`: Main entry point for pattern matching
//! - `substitute_infer`: Replace infer types with their bindings
//! - `bind_infer`: Bind a type to an infer parameter

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    LiteralValue, ParamInfo, TemplateSpan, TupleElement, TypeApplication, TypeData, TypeId,
    TypeParamInfo,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;
use super::infer_substitutor::InferSubstitutor;
use std::cell::Cell;

/// Selects how co-located `infer T` candidates are merged when the same name
/// gets distinct bindings from multiple structurally adjacent positions.
#[derive(Clone, Copy)]
enum CoLocatedMerge {
    /// Tuple elements, array elements, optional-tail elements — covariant.
    Union,
    /// Function/callable parameters — contravariant.
    Intersection,
}

thread_local! {
    /// Cross-evaluator nesting depth for infer-pattern matching that expands an
    /// `Application`/`Mapped` source or pattern in a *fresh* sub-evaluator.
    ///
    /// Infer matching cannot call `evaluate` on the current `&self` evaluator
    /// (those methods take `&self`), so it spins up a brand-new `TypeEvaluator`
    /// whose per-instance recursion guard, depth counter, and fuel all start at
    /// zero. A recursive generic-wrapper application — Zod's
    /// `ZodObject`/`ZodOptional`/`ZodArray` chains, `DeepPartial`-style helpers,
    /// etc. — makes that expansion re-enter conditional/infer evaluation at a
    /// deeper nesting through a *new* evaluator each level, so no per-evaluator
    /// guard ever fires and the compile hangs. This thread-global counter bounds
    /// that cross-evaluator nesting. See [`TypeEvaluator::evaluate_for_infer_match`].
    static INFER_MATCH_EXPANSION_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Maximum cross-evaluator nesting for infer-match sub-evaluator expansions.
///
/// Mirrors tsc's `instantiationDepth` cutoff (100): beyond this nesting, tsc
/// abandons the instantiation, so tsz stops expanding too rather than recurse
/// forever. Legitimate structural expansion of an application source during
/// infer matching never nests anywhere near this deep; only unbounded recursive
/// wrappers do.
const MAX_INFER_MATCH_EXPANSION_DEPTH: u32 = 100;

/// Per-walk state for [`TypeEvaluator::type_contains_infer`].
///
/// `visited` maps a node to whether its subtree walk *completed* (`true`) or
/// is still in progress (`false`). Re-entering an in-progress node is a cycle
/// and sets `tainted`, which blocks persisting provisional answers to the
/// project-wide `eval_contains_infer_cache`.
#[derive(Default)]
struct InferContainsWalk {
    visited: FxHashMap<TypeId, bool>,
    tainted: bool,
}

/// Logged visited set for one infer-pattern match operation.
///
/// The match algorithm needs branch-local rollback for speculative alias
/// recovery, but cloning the full visited set on every branch is a hot-path
/// multiplier for recursive conditional utilities. Logging only successful
/// inserts lets a branch checkpoint and roll back the entries it added while
/// preserving the parent walk's cycle guard.
#[derive(Default)]
pub(crate) struct InferPatternVisited {
    entries: FxHashSet<(TypeId, TypeId)>,
    insert_log: Vec<(TypeId, TypeId)>,
}

impl InferPatternVisited {
    #[inline]
    fn insert(&mut self, pair: (TypeId, TypeId)) -> bool {
        if self.entries.insert(pair) {
            self.insert_log.push(pair);
            true
        } else {
            false
        }
    }

    #[inline]
    fn contains(&self, pair: &(TypeId, TypeId)) -> bool {
        self.entries.contains(pair)
    }

    #[inline]
    const fn checkpoint(&self) -> usize {
        self.insert_log.len()
    }

    fn rollback_to(&mut self, checkpoint: usize) {
        while self.insert_log.len() > checkpoint {
            if let Some(pair) = self.insert_log.pop() {
                self.entries.remove(&pair);
            }
        }
    }

    #[inline]
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.insert_log.clear();
    }
}

/// RAII guard for [`INFER_MATCH_EXPANSION_DEPTH`].
///
/// `enter` returns `None` when the budget is exhausted (the caller must then
/// skip the expansion); otherwise it increments the counter and decrements it
/// on drop, so the bound is restored even if evaluation unwinds via panic.
struct InferMatchExpansionGuard;

impl InferMatchExpansionGuard {
    fn enter() -> Option<Self> {
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_INFER_MATCH_EXPANSION_DEPTH {
                None
            } else {
                depth.set(current + 1);
                Some(Self)
            }
        })
    }
}

impl Drop for InferMatchExpansionGuard {
    fn drop(&mut self) {
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

impl<'a, R: TypeResolver> TypeEvaluator<'a, R> {
    /// Substitute infer bindings into a type.
    ///
    /// Replaces all `infer X` references with their bound values from the bindings map.
    pub(crate) fn substitute_infer(
        &self,
        type_id: TypeId,
        bindings: &FxHashMap<Atom, TypeId>,
    ) -> TypeId {
        if bindings.is_empty() {
            return type_id;
        }
        let mut substitutor = InferSubstitutor::new(self.interner(), bindings);
        substitutor.substitute(type_id)
    }

    /// Check if a type contains any `infer` type parameters.
    #[inline]
    pub(crate) fn type_contains_infer(&self, type_id: TypeId) -> bool {
        // Fast path: intrinsic types never contain infer
        if type_id.is_intrinsic() {
            return false;
        }
        // Single TypeData lookup feeds both the direct-Infer check and the
        // terminal-kind fast path. Prevents the redundant lookup that would
        // happen if these checks ran independently before falling into the
        // recursive walker.
        let key = match self.interner().lookup(type_id) {
            Some(key) => key,
            None => return false,
        };
        // Direct Infer: short-circuit before allocating the visited set.
        if matches!(key, TypeData::Infer(_)) {
            return true;
        }
        // Terminal kinds that the recursive walker treats as not-containing-
        // infer (see `type_contains_infer_inner`'s leaf arm). Skipping the
        // visited-set allocation for these types eliminates the per-call
        // `FxHashMap::default()` for a large fraction of conditional-type
        // evaluation calls — `extends_type` is frequently `Lazy(DefId)` for
        // generic interface references like `Promise<T>` before evaluation.
        if matches!(
            key,
            TypeData::Literal(_)
                | TypeData::Lazy(_)
                | TypeData::Recursive(_)
                | TypeData::BoundParameter(_)
                | TypeData::TypeQuery(_)
                | TypeData::UniqueSymbol(_)
                | TypeData::ThisType
                | TypeData::ModuleNamespace(_)
                | TypeData::UnresolvedTypeName(_)
                | TypeData::Error
        ) {
            return false;
        }
        // Project-wide memo: the answer is immutable per `TypeId` within one
        // interner. Recursive conditional/application evaluation re-asks this
        // for the same (often very large) pattern and extends types.
        if let Some(cached) = self.interner().eval_contains_infer_cached(type_id) {
            return cached;
        }
        let mut walk = InferContainsWalk::default();
        let result = self.type_contains_infer_inner_with_key(type_id, key, &mut walk);
        if result {
            // A found `Infer` is a definite fact for the root regardless of
            // any in-flight cycle node; intermediate nodes were abandoned by
            // the short-circuit and stay uncached.
            self.interner().set_eval_contains_infer_cache(type_id, true);
        } else if !walk.tainted {
            // A fully-explored `false` walk finalizes every visited node:
            // each subtree was exhaustively walked and contains no `Infer`.
            for (&node, &done) in &walk.visited {
                if done {
                    self.interner().set_eval_contains_infer_cache(node, false);
                }
            }
        }
        result
    }

    fn type_contains_infer_inner(&self, type_id: TypeId, walk: &mut InferContainsWalk) -> bool {
        if type_id.is_intrinsic() {
            return false;
        }
        if let Some(cached) = self.interner().eval_contains_infer_cached(type_id) {
            return cached;
        }
        match walk.visited.entry(type_id) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                // Re-entering an in-progress node is a cycle: the provisional
                // `false` answer must not be persisted by any ancestor.
                if !entry.get() {
                    walk.tainted = true;
                }
                return false;
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(false);
            }
        }

        let Some(key) = self.interner().lookup(type_id) else {
            return false;
        };

        let result = self.match_contains_infer(type_id, key, walk);
        if !result {
            walk.visited.insert(type_id, true);
        }
        result
    }

    /// Walk one already-fetched `TypeData` for the contains-infer check.
    ///
    /// Splitting the walk from the `lookup`/`visited` bookkeeping
    /// lets `type_contains_infer` reuse a `TypeData` it already fetched
    /// for the entry-point fast paths, without performing a second
    /// interner lookup.
    fn type_contains_infer_inner_with_key(
        &self,
        type_id: TypeId,
        key: TypeData,
        walk: &mut InferContainsWalk,
    ) -> bool {
        match walk.visited.entry(type_id) {
            std::collections::hash_map::Entry::Occupied(entry) => {
                if !entry.get() {
                    walk.tainted = true;
                }
                return false;
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(false);
            }
        }
        let result = self.match_contains_infer(type_id, key, walk);
        if !result {
            walk.visited.insert(type_id, true);
        }
        result
    }

    fn match_contains_infer(
        &self,
        _type_id: TypeId,
        key: TypeData,
        visited: &mut InferContainsWalk,
    ) -> bool {
        match key {
            TypeData::Infer(_) => true,
            TypeData::Array(elem) => self.type_contains_infer_inner(elem, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner().tuple_list(elements);
                elements
                    .iter()
                    .any(|element| self.type_contains_infer_inner(element.type_id, visited))
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner().type_list(members);
                members
                    .iter()
                    .any(|&member| self.type_contains_infer_inner(member, visited))
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .any(|prop| self.type_contains_infer_inner(prop.type_id, visited))
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
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
            TypeData::Function(shape_id) => {
                let shape = self.interner().function_shape(shape_id);
                shape
                    .params
                    .iter()
                    .any(|param| self.type_contains_infer_inner(param.type_id, visited))
                    || shape
                        .this_type
                        .is_some_and(|this_type| self.type_contains_infer_inner(this_type, visited))
                    || self.type_contains_infer_inner(shape.return_type, visited)
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner().callable_shape(shape_id);
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
            TypeData::TypeParameter(info) => {
                info.constraint
                    .is_some_and(|constraint| self.type_contains_infer_inner(constraint, visited))
                    || info
                        .default
                        .is_some_and(|default| self.type_contains_infer_inner(default, visited))
            }
            TypeData::Application(app_id) => {
                let app = self.interner().type_application(app_id);
                self.type_contains_infer_inner(app.base, visited)
                    || app
                        .args
                        .iter()
                        .any(|&arg| self.type_contains_infer_inner(arg, visited))
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner().get_mapped(mapped_id);
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
            TypeData::IndexAccess(obj, idx) => {
                self.type_contains_infer_inner(obj, visited)
                    || self.type_contains_infer_inner(idx, visited)
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.type_contains_infer_inner(inner, visited)
            }
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                self.type_contains_infer_inner(base_type, visited)
                    || self.type_contains_infer_inner(constraint, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner().template_list(spans);
                spans.iter().any(|span| match span {
                    TemplateSpan::Text(_) => false,
                    TemplateSpan::Type(inner) => self.type_contains_infer_inner(*inner, visited),
                })
            }
            TypeData::StringIntrinsic { type_arg, .. } => {
                self.type_contains_infer_inner(type_arg, visited)
            }
            TypeData::Enum(_def_id, member_type) => {
                self.type_contains_infer_inner(member_type, visited)
            }
            // `Conditional`: an `infer X` declaration is scoped to the `extends`
            // clause of the conditional that introduces it, and `infer` can
            // syntactically only appear in an `extends` clause (references to the
            // inferred type are `TypeParameter`, not `Infer`). Every `Infer` node
            // inside a conditional is therefore *bound* by that conditional (or a
            // deeper nested one), so a nested conditional contributes no free infer
            // site to an enclosing conditional. Descending into it would make an
            // outer conditional whose `extends` clause merely *embeds* a complete
            // conditional — e.g. `[X] extends { p: unknown extends infer a ? a
            // : never }[] ? ...` — spuriously enter infer-matching mode and take the
            // false branch instead of the structural relation (issue #14238). The
            // remaining kinds are leaves with no `infer`-bearing sub-structure.
            TypeData::Conditional(_)
            | TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error => false,
        }
    }

    /// Validate an inferred candidate against a constrained `infer U extends C`.
    ///
    /// tsc treats the constraint as a whole-candidate check, not a per-member
    /// union filter: it infers a candidate `X` for `U`, and if `X` is not
    /// assignable to `C` it replaces `X` with `C` and re-checks the conditional
    /// structurally — a re-check that fails at the position that produced `X`,
    /// so the conditional resolves to its false branch. tsc never keeps a
    /// matching subset of a union candidate while dropping the rest.
    ///
    /// We therefore mirror that exactly: return `Some(inferred)` when the whole
    /// candidate satisfies the constraint, and `None` otherwise so the caller
    /// takes the false branch. Distributive conditionals have already split a
    /// union check type into individual members before reaching here, so each
    /// member is validated independently — the distributive `1 | 2 | "x"` case
    /// still yields `1 | 2 | <false-branch>` rather than a silently filtered
    /// `1 | 2`.
    pub(crate) fn filter_inferred_by_constraint(
        &self,
        inferred: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        if inferred == constraint {
            return Some(inferred);
        }

        checker
            .is_subtype_of(inferred, constraint)
            .then_some(inferred)
    }

    /// Validate a constrained `infer U extends C` candidate inferred from an
    /// *optional* source position (optional tuple element or optional property).
    ///
    /// An optional position contributes an extra `undefined` to the inferred
    /// candidate (an absent element/property reads as `undefined`). tsc strips
    /// that optionality-`undefined` before applying the constraint, and then
    /// performs the same whole-candidate check as a required position — it does
    /// *not* keep a matching subset of the remaining union. So:
    ///
    /// * `{ a?: string }` against `{ a?: infer R extends string }` strips the
    ///   `undefined` and yields `R = string`.
    /// * `{ a?: "x" | 1 }` strips the `undefined`, leaving `"x" | 1`, which is
    ///   not assignable to `string`, so the conditional takes its false branch
    ///   (tsc does not partially filter the candidate down to `"x"`).
    ///
    /// Returns the stripped candidate when it satisfies the constraint, or
    /// `None` so the caller selects the false branch.
    pub(crate) fn filter_optional_inferred_by_constraint(
        &self,
        inferred: TypeId,
        constraint: TypeId,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<TypeId> {
        let stripped = self.strip_optionality_undefined(inferred);
        self.filter_inferred_by_constraint(stripped, constraint, checker)
    }

    /// Remove the `undefined` member that an optional source position adds to an
    /// inferred candidate. Only strips a top-level `undefined` union member; a
    /// bare `undefined` candidate (no other members) is left untouched so the
    /// constraint check can still reject it.
    fn strip_optionality_undefined(&self, inferred: TypeId) -> TypeId {
        let Some(TypeData::Union(members)) = self.interner().lookup(inferred) else {
            return inferred;
        };
        let members = self.interner().type_list(members);
        // Fast path: no optionality-`undefined` to strip, so avoid rebuilding.
        if !members.contains(&TypeId::UNDEFINED) {
            return inferred;
        }
        let kept: SmallVec<[TypeId; 8]> = members
            .iter()
            .copied()
            .filter(|&m| m != TypeId::UNDEFINED)
            .collect();
        match kept.len() {
            // The candidate was only `undefined`; leave it so the constraint can
            // still reject it.
            0 => inferred,
            1 => kept[0],
            _ => self.interner().union_from_slice(&kept),
        }
    }

    /// Collect the names of `infer` type variables that appear in any
    /// contravariant position within `pattern`.
    ///
    /// A position is contravariant when it is reached through an odd number of
    /// function/callable parameter positions (parameters flip variance, return
    /// types and object/array/tuple members preserve it). When the same `infer`
    /// name receives candidates from structurally separate positions, names in
    /// this set merge their candidates via intersection; all others via union.
    /// This mirrors tsc's `inferTypes`, where a type variable with any
    /// contravariant occurrence produces an intersection of its candidates.
    pub(crate) fn collect_contravariant_infer_names(&self, pattern: TypeId) -> FxHashSet<Atom> {
        // An intrinsic pattern carries no `infer` names; skip the memo round-trip.
        if pattern.is_intrinsic() {
            return FxHashSet::default();
        }
        // Project-wide memo: the contravariant-`infer`-name set is a pure function
        // of the immutable interned pattern structure (the variance walk consults
        // neither the resolver nor the substitution environment), so the answer is
        // stable per `TypeId`. Recursive conditional/`infer` matching re-asks it
        // for the same pattern across many fresh evaluators (#14330).
        if let Some(cached) = self.interner().contravariant_infer_names_memo(pattern) {
            // The empty set is the dominant case (most patterns have no
            // contravariant `infer`); return it without iterating the slice.
            if cached.is_empty() {
                return FxHashSet::default();
            }
            return cached.iter().copied().collect();
        }
        let mut result = FxHashSet::default();
        let mut visited = FxHashSet::default();
        self.collect_variance_infer_names(pattern, false, &mut result, &mut visited);
        self.interner().set_contravariant_infer_names_memo(
            pattern,
            result.iter().copied().collect::<Vec<_>>().into(),
        );
        result
    }

    /// Walk `ty`, tracking whether the current position is contravariant, and
    /// record the names of `infer` variables found in a contravariant position.
    /// Walk `ty`, tracking whether the current position is contravariant, and
    /// record the names of `infer` variables found in a contravariant position.
    fn collect_variance_infer_names(
        &self,
        ty: TypeId,
        contravariant: bool,
        out: &mut FxHashSet<Atom>,
        visited: &mut FxHashSet<(TypeId, bool)>,
    ) {
        if ty.is_intrinsic() || !visited.insert((ty, contravariant)) {
            return;
        }
        match self.interner().lookup(ty) {
            Some(TypeData::Infer(info)) if contravariant => {
                out.insert(info.name);
            }
            Some(TypeData::Union(members) | TypeData::Intersection(members)) => {
                for &m in self.interner().type_list(members).iter() {
                    self.collect_variance_infer_names(m, contravariant, out, visited);
                }
            }
            Some(TypeData::Array(elem)) => {
                self.collect_variance_infer_names(elem, contravariant, out, visited);
            }
            Some(TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner)) => {
                self.collect_variance_infer_names(inner, contravariant, out, visited);
            }
            Some(TypeData::Tuple(elements)) => {
                for elem in self.interner().tuple_list(elements).iter() {
                    self.collect_variance_infer_names(elem.type_id, contravariant, out, visited);
                }
            }
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = self.interner().object_shape(shape_id);
                for prop in &shape.properties {
                    self.collect_variance_infer_names(prop.type_id, contravariant, out, visited);
                }
                for index in [shape.string_index.as_ref(), shape.number_index.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    self.collect_variance_infer_names(
                        index.value_type,
                        contravariant,
                        out,
                        visited,
                    );
                }
            }
            Some(TypeData::Function(fn_id)) => {
                let shape = self.interner().function_shape(fn_id);
                for param in &shape.params {
                    self.collect_variance_infer_names(param.type_id, !contravariant, out, visited);
                }
                self.collect_variance_infer_names(shape.return_type, contravariant, out, visited);
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = self.interner().callable_shape(callable_id);
                for sig in shape
                    .call_signatures
                    .iter()
                    .chain(shape.construct_signatures.iter())
                {
                    for param in &sig.params {
                        self.collect_variance_infer_names(
                            param.type_id,
                            !contravariant,
                            out,
                            visited,
                        );
                    }
                    self.collect_variance_infer_names(sig.return_type, contravariant, out, visited);
                }
                for prop in &shape.properties {
                    self.collect_variance_infer_names(prop.type_id, contravariant, out, visited);
                }
            }
            _ => {}
        }
    }

    /// Fill `default_ty` for every infer parameter in `pattern` that does not
    /// already have a candidate in `bindings`. Unlike `bind_infer_defaults`,
    /// this only fills gaps: it never overwrites or rejects an already-bound
    /// name. Used when a pattern position has no corresponding source position
    /// (e.g. the source callable supplies fewer parameters than the inference
    /// pattern requires), where tsc leaves the unmatched `infer` slots at their
    /// default of `unknown`.
    pub(crate) fn fill_unbound_infer_defaults(
        &self,
        pattern: TypeId,
        default_ty: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
    ) {
        // Reuse the comprehensive `for_each_infer` walk so coverage matches
        // `bind_infer_defaults` exactly (nested object/callable/function
        // members and deferred shells like `Application`/`Conditional`/
        // `Mapped`/`IndexAccess`/`KeyOf`/template/string-intrinsic/enum). Gap
        // semantics: fill `default_ty` only where a candidate is not already
        // bound, so a matched position always wins, and never short-circuit.
        let mut visited = FxHashSet::default();
        self.for_each_infer(
            pattern,
            &mut |info| {
                bindings.entry(info.name).or_insert(default_ty);
                true
            },
            &mut visited,
        );
    }

    /// Default each `infer` variable in `pattern` to its declared constraint, or
    /// `undefined` when unconstrained, for any name not already bound in `bindings`.
    ///
    /// Used when an *optional* property of the conditional extends-pattern is absent
    /// in the source type.  An absent optional property reads as `undefined`, so an
    /// unconstrained `infer R` gets `undefined`.  A constrained `infer R extends C`
    /// gets `C` directly (bypassing the constraint check), allowing the conditional
    /// to take its **true** branch without a spurious `undefined` failing the
    /// constraint.
    ///
    /// Note: the fast path (`eval_conditional_object_prop_infer`) uses `unknown`
    /// when ALL candidates across ALL slots are absent (a single-object source with
    /// no union). This function is called from the **general path** (tuple patterns,
    /// union sources iterated member-by-member), where each absent optional member
    /// should contribute `undefined` so the union of members is `<type> | undefined`.
    pub(crate) fn fill_absent_optional_infer_defaults(
        &self,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
    ) {
        let mut visited = FxHashSet::default();
        self.for_each_infer(
            pattern,
            &mut |info| {
                // A constrained `infer R extends C` defaults to C (true branch stays
                // valid). An unconstrained `infer R` defaults to `undefined`, matching
                // the value an absent optional property produces in a union member.
                let default_ty = info.constraint.unwrap_or(TypeId::UNDEFINED);
                bindings.entry(info.name).or_insert(default_ty);
                true
            },
            &mut visited,
        );
    }

    /// Bind every `infer` in `pattern` that has no candidate yet to its
    /// no-candidate default, mirroring tsc's `getInferredType` for an inference
    /// variable with zero candidates: the declared constraint when present,
    /// otherwise `fallback`. Callers pass `unknown` for a plain `infer`
    /// position and `unknown[]` for a rest `...infer T` position. Never
    /// overwrites a binding a matched position already produced.
    fn bind_unmatched_infer_defaults(
        &self,
        pattern: TypeId,
        fallback: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
    ) {
        let mut visited = FxHashSet::default();
        self.for_each_infer(
            pattern,
            &mut |info| {
                let default_ty = info.constraint.unwrap_or(fallback);
                bindings.entry(info.name).or_insert(default_ty);
                true
            },
            &mut visited,
        );
    }

    /// Bind an inferred type to an infer parameter.
    ///
    /// Handles constraint checking and merging with existing bindings.
    pub(crate) fn bind_infer(
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

    /// Bind default values for all infer parameters in a pattern.
    ///
    /// Used when the source type doesn't provide a value for an infer parameter.
    pub(crate) fn bind_infer_defaults(
        &self,
        pattern: TypeId,
        inferred: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let mut visited = FxHashSet::default();
        self.for_each_infer(
            pattern,
            &mut |info| self.bind_infer(info, inferred, bindings, checker),
            &mut visited,
        )
    }

    /// Comprehensive, variance-agnostic walk over every `infer` position in a
    /// pattern (object/callable/function members, deferred shells like
    /// `Application`/`Conditional`/`Mapped`/`IndexAccess`/`KeyOf`/template/
    /// string-intrinsic/enum, type-parameter constraints/defaults, etc.).
    /// `f` is invoked at each `infer` leaf; returning `false` short-circuits the
    /// walk. This is the single traversal shared by `bind_infer_defaults`
    /// (binds each leaf with constraint checking) and `fill_unbound_infer_defaults`
    /// (gap-fills `unknown` for leaves with no candidate yet), so their shape
    /// coverage can never drift apart.
    fn for_each_infer(
        &self,
        pattern: TypeId,
        f: &mut dyn FnMut(&TypeParamInfo) -> bool,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if !visited.insert(pattern) {
            return true;
        }

        let Some(key) = self.interner().lookup(pattern) else {
            return true;
        };

        match key {
            TypeData::Infer(info) => f(&info),
            TypeData::Array(elem) => self.for_each_infer(elem, f, visited),
            TypeData::Tuple(elements) => {
                let elements = self.interner().tuple_list(elements);
                for element in elements.iter() {
                    if !self.for_each_infer(element.type_id, f, visited) {
                        return false;
                    }
                }
                true
            }
            TypeData::Union(members) | TypeData::Intersection(members) => {
                let members = self.interner().type_list(members);
                for &member in members.iter() {
                    if !self.for_each_infer(member, f, visited) {
                        return false;
                    }
                }
                true
            }
            TypeData::Object(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                for prop in &shape.properties {
                    if !self.for_each_infer(prop.type_id, f, visited) {
                        return false;
                    }
                }
                true
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner().object_shape(shape_id);
                for prop in &shape.properties {
                    if !self.for_each_infer(prop.type_id, f, visited) {
                        return false;
                    }
                }
                if let Some(index) = &shape.string_index
                    && (!self.for_each_infer(index.key_type, f, visited)
                        || !self.for_each_infer(index.value_type, f, visited))
                {
                    return false;
                }
                if let Some(index) = &shape.number_index
                    && (!self.for_each_infer(index.key_type, f, visited)
                        || !self.for_each_infer(index.value_type, f, visited))
                {
                    return false;
                }
                true
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner().function_shape(shape_id);
                for param in &shape.params {
                    if !self.for_each_infer(param.type_id, f, visited) {
                        return false;
                    }
                }
                if let Some(this_type) = shape.this_type
                    && !self.for_each_infer(this_type, f, visited)
                {
                    return false;
                }
                self.for_each_infer(shape.return_type, f, visited)
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner().callable_shape(shape_id);
                for sig in &shape.call_signatures {
                    for param in &sig.params {
                        if !self.for_each_infer(param.type_id, f, visited) {
                            return false;
                        }
                    }
                    if let Some(this_type) = sig.this_type
                        && !self.for_each_infer(this_type, f, visited)
                    {
                        return false;
                    }
                    if !self.for_each_infer(sig.return_type, f, visited) {
                        return false;
                    }
                }
                for sig in &shape.construct_signatures {
                    for param in &sig.params {
                        if !self.for_each_infer(param.type_id, f, visited) {
                            return false;
                        }
                    }
                    if let Some(this_type) = sig.this_type
                        && !self.for_each_infer(this_type, f, visited)
                    {
                        return false;
                    }
                    if !self.for_each_infer(sig.return_type, f, visited) {
                        return false;
                    }
                }
                for prop in &shape.properties {
                    if !self.for_each_infer(prop.type_id, f, visited) {
                        return false;
                    }
                }
                true
            }
            TypeData::TypeParameter(info) => {
                if let Some(constraint) = info.constraint
                    && !self.for_each_infer(constraint, f, visited)
                {
                    return false;
                }
                if let Some(default) = info.default
                    && !self.for_each_infer(default, f, visited)
                {
                    return false;
                }
                true
            }
            TypeData::Application(app_id) => {
                let app = self.interner().type_application(app_id);
                if !self.for_each_infer(app.base, f, visited) {
                    return false;
                }
                for &arg in &app.args {
                    if !self.for_each_infer(arg, f, visited) {
                        return false;
                    }
                }
                true
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner().get_conditional(cond_id);
                self.for_each_infer(cond.check_type, f, visited)
                    && self.for_each_infer(cond.extends_type, f, visited)
                    && self.for_each_infer(cond.true_type, f, visited)
                    && self.for_each_infer(cond.false_type, f, visited)
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner().get_mapped(mapped_id);
                if let Some(constraint) = mapped.type_param.constraint
                    && !self.for_each_infer(constraint, f, visited)
                {
                    return false;
                }
                if let Some(default) = mapped.type_param.default
                    && !self.for_each_infer(default, f, visited)
                {
                    return false;
                }
                if !self.for_each_infer(mapped.constraint, f, visited) {
                    return false;
                }
                if let Some(name_type) = mapped.name_type
                    && !self.for_each_infer(name_type, f, visited)
                {
                    return false;
                }
                self.for_each_infer(mapped.template, f, visited)
            }
            TypeData::IndexAccess(obj, idx) => {
                self.for_each_infer(obj, f, visited) && self.for_each_infer(idx, f, visited)
            }
            TypeData::KeyOf(inner) | TypeData::ReadonlyType(inner) | TypeData::NoInfer(inner) => {
                self.for_each_infer(inner, f, visited)
            }
            TypeData::Substitution {
                base_type,
                constraint,
            } => {
                self.for_each_infer(base_type, f, visited)
                    && self.for_each_infer(constraint, f, visited)
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner().template_list(spans);
                for span in spans.iter() {
                    if let TemplateSpan::Type(inner) = span
                        && !self.for_each_infer(*inner, f, visited)
                    {
                        return false;
                    }
                }
                true
            }
            TypeData::StringIntrinsic { type_arg, .. } => self.for_each_infer(type_arg, f, visited),
            TypeData::Enum(_def_id, member_type) => self.for_each_infer(member_type, f, visited),
            TypeData::Intrinsic(_)
            | TypeData::Literal(_)
            | TypeData::Lazy(_)
            | TypeData::Recursive(_)
            | TypeData::BoundParameter(_)
            | TypeData::TypeQuery(_)
            | TypeData::UniqueSymbol(_)
            | TypeData::ThisType
            | TypeData::ModuleNamespace(_)
            | TypeData::UnresolvedTypeName(_)
            | TypeData::Error => true,
        }
    }

    /// Merge co-located `infer T` candidates extracted by independent
    /// sub-matches against the same outer pattern.
    ///
    /// For each `(source, pattern)` pair, the helper runs `match_infer_pattern`
    /// on a fresh clone of `bindings` so the first slot's binding does not
    /// poison the next slot's mutual-subtype check inside `bind_infer`. After
    /// every slot has produced its local bindings, names introduced by these
    /// slots are folded back into `bindings` using `merge_kind`: union for
    /// covariant container positions (tuple elements, array elements), or
    /// intersection for contravariant function/callable parameters. Bindings
    /// that already existed before the helper was called are preserved
    /// unchanged.
    pub(crate) fn match_co_located_intersect_pairs(
        &self,
        pairs: &[(TypeId, TypeId)],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        self.match_co_located_with_merge(
            pairs,
            bindings,
            visited,
            checker,
            CoLocatedMerge::Intersection,
        )
    }

    fn match_co_located_with_merge(
        &self,
        pairs: &[(TypeId, TypeId)],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
        merge_kind: CoLocatedMerge,
    ) -> bool {
        let base = bindings.clone();
        let mut merged = base.clone();
        for &(source, pattern) in pairs {
            let mut local = base.clone();
            if !self.match_infer_pattern(source, pattern, &mut local, visited, checker) {
                return false;
            }
            for (name, ty) in local {
                if base.contains_key(&name) {
                    continue;
                }
                if let Some(existing) = merged.get_mut(&name) {
                    if *existing != ty {
                        *existing = match merge_kind {
                            CoLocatedMerge::Union => self.interner().union2(*existing, ty),
                            CoLocatedMerge::Intersection => {
                                self.interner().intersection2(*existing, ty)
                            }
                        };
                    }
                } else {
                    merged.insert(name, ty);
                }
            }
        }
        *bindings = merged;
        true
    }

    /// Match the residual source slice of a tuple pattern against the pattern's
    /// rest slot. The residual is what remains after positional prefix/suffix
    /// matching, and may include source rest elements (variadic tuple tails).
    ///
    /// Dispatches on the pattern rest's shape:
    /// - `Array(P)` (and `ReadonlyType(Array(P))`): each residual element's
    ///   value type must satisfy `P`. Source spread elements contribute their
    ///   inner element type (e.g., `...number[]` contributes `number` against
    ///   `any`).
    /// - Anything else (typically `infer R`, a type-parameter spread, or a
    ///   structural tuple pattern): reify the residual as a tuple-or-array type
    ///   preserving `rest`/`optional` flags, then recurse via
    ///   `match_infer_pattern`.
    fn match_residual_against_pattern_rest(
        &self,
        residual: &[TupleElement],
        pattern_rest_type: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if let Some(array_elem_type) = self.unwrap_array_element_for_residual(pattern_rest_type) {
            return self.match_residual_against_array_element(
                residual,
                array_elem_type,
                bindings,
                visited,
                checker,
            );
        }

        // A residual of exactly one non-optional rest element is structurally
        // identical to its inner spread type: tsc treats `[...T[]]` as `T[]`,
        // `[...[A, B]]` as `[A, B]`, and `[...T]` (T extending an array) as
        // `T`. Returning the inner type directly keeps `infer R` bindings in
        // the canonical form tsc would produce, so identity probes such as
        // `Equal<RestOf<...>, T[]>` resolve to `true`.
        let residual_type = if residual.len() == 1 && residual[0].rest && !residual[0].optional {
            self.reify_application_over_tuple_index_residual(residual[0].type_id)
                .unwrap_or(residual[0].type_id)
        } else {
            self.interner().tuple(residual.to_vec())
        };
        self.match_infer_pattern(residual_type, pattern_rest_type, bindings, visited, checker)
    }

    /// Unwrap `Array(E)` (possibly under one `ReadonlyType` layer) and return
    /// the element type `E`. Returns `None` for non-array shapes such as
    /// `infer R` or a structural tuple pattern.
    fn unwrap_array_element_for_residual(&self, ty: TypeId) -> Option<TypeId> {
        match self.interner().lookup(ty)? {
            TypeData::Array(elem) => Some(elem),
            TypeData::ReadonlyType(inner) => {
                if let Some(TypeData::Array(elem)) = self.interner().lookup(inner) {
                    Some(elem)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Match each residual source element's value type against `target_elem`.
    ///
    /// Source spread elements (`rest: true`) contribute their inner element
    /// type — `Array(SE)` contributes `SE`; nested tuple spreads recurse so
    /// every flattened inner element is checked; other spread types
    /// (e.g., a type-parameter spread `...T`) fall back to matching the spread
    /// type itself against the array element pattern.
    fn match_residual_against_array_element(
        &self,
        residual: &[TupleElement],
        target_elem: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        for source_elem in residual {
            if source_elem.rest {
                // Flatten nested tuple spreads (`...[A, B]`) so each inner slot
                // is checked individually against `target_elem`.
                if let Some(TypeData::Tuple(inner_id)) = self.interner().lookup(source_elem.type_id)
                {
                    let inner = self.interner().tuple_list(inner_id);
                    if !self.match_residual_against_array_element(
                        &inner,
                        target_elem,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return false;
                    }
                    continue;
                }
                // Array-shaped spread (`...E[]` / `...readonly E[]`): match
                // the element type `E` against `target_elem`. Other shapes
                // (e.g., a type-parameter spread `...T`) fall through and
                // match the spread type itself.
                let spread_inner = self
                    .unwrap_array_element_for_residual(source_elem.type_id)
                    .unwrap_or(source_elem.type_id);
                if !self.match_infer_pattern(spread_inner, target_elem, bindings, visited, checker)
                {
                    return false;
                }
            } else {
                let source_type = if source_elem.optional {
                    self.interner()
                        .union2(source_elem.type_id, TypeId::UNDEFINED)
                } else {
                    source_elem.type_id
                };
                if !self.match_infer_pattern(source_type, target_elem, bindings, visited, checker) {
                    return false;
                }
            }
        }
        true
    }

    /// Merge the candidate bindings produced by one structural position
    /// (`local`) into the accumulator (`merged`), relative to the pre-existing
    /// bindings (`base`).
    ///
    /// Names already present in `base` are left untouched (they were resolved by
    /// an outer context). For a name that gains a second, distinct candidate, the
    /// merge intersects when the name occurs in any contravariant position of the
    /// surrounding pattern (`contravariant_infers`) and unions otherwise.
    pub(crate) fn merge_infer_candidates(
        &self,
        base: &FxHashMap<Atom, TypeId>,
        merged: &mut FxHashMap<Atom, TypeId>,
        local: FxHashMap<Atom, TypeId>,
        contravariant_infers: &FxHashSet<Atom>,
    ) {
        for (name, ty) in local {
            if base.contains_key(&name) {
                continue;
            }
            match merged.get_mut(&name) {
                Some(existing) => {
                    if *existing != ty {
                        *existing = if contravariant_infers.contains(&name) {
                            self.interner().intersection2(*existing, ty)
                        } else {
                            self.interner().union2(*existing, ty)
                        };
                    }
                }
                None => {
                    merged.insert(name, ty);
                }
            }
        }
    }

    /// Validate one fixed (non-rest) tuple slot pair and, when valid, push its
    /// `(source, pattern)` types onto `pairs` for co-located merge matching.
    ///
    /// Returns `false` when the source element cannot fill the pattern slot —
    /// either side carrying a `rest` flag (a fixed slot must align with a fixed
    /// slot), or an optional source against a required pattern slot. An optional
    /// source widens with `undefined` so the slot can still bind `T | undefined`.
    /// Shared by every fixed-slot collection loop (prefix, suffix, and the
    /// non-rest pairwise zip) so the shape rules stay in one place.
    fn push_fixed_tuple_pair(
        &self,
        source_elem: &TupleElement,
        pattern_elem: &TupleElement,
        pairs: &mut Vec<(TypeId, TypeId)>,
    ) -> bool {
        if source_elem.rest || pattern_elem.rest {
            return false;
        }
        if source_elem.optional && !pattern_elem.optional {
            return false;
        }
        let source_type = if source_elem.optional {
            self.interner()
                .union2(source_elem.type_id, TypeId::UNDEFINED)
        } else {
            source_elem.type_id
        };
        pairs.push((source_type, pattern_elem.type_id));
        true
    }

    /// Match tuple elements against a pattern, extracting infer bindings.
    pub(crate) fn match_tuple_elements(
        &self,
        source_elems: &[TupleElement],
        pattern_elems: &[TupleElement],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
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
            // Pattern has a rest element at `rest_index`.
            // Split pattern into: prefix (before rest), rest, suffix (after rest).
            // Match prefix from start of source, suffix from end of source,
            // collect remaining middle elements into the rest tuple.
            let prefix_len = rest_index;
            let suffix_len = pattern_len - rest_index - 1;

            // Optional leading (prefix) elements may be absent in the source:
            // tsc lets a short/empty source match `[(infer H)?, ...rest]` by
            // taking the optional prefix slot(s) as absent. Only the *required*
            // (non-optional) prefix elements plus the always-required suffix
            // elements impose a minimum source arity. (Once any prefix element
            // is optional a tuple type cannot place a fixed element after the
            // rest — TS1257 — so a partially-absent prefix implies an empty
            // suffix; the general arithmetic below stays correct regardless.)
            let required_prefix_len = pattern_elems[..prefix_len]
                .iter()
                .filter(|elem| !elem.optional)
                .count();
            if source_len < required_prefix_len + suffix_len {
                return false;
            }

            let rest_source_end = source_len - suffix_len;
            // Number of prefix slots the source actually fills. Any remaining
            // prefix slots are necessarily optional (required ones come first
            // and are covered by the arity check above) and read as absent.
            let present_prefix_len = std::cmp::min(prefix_len, rest_source_end);

            // Collect the fixed prefix and suffix `(source, pattern)` pairs.
            // These are all co-located *covariant* tuple positions: when the
            // same `infer T` name appears in more than one of them (whether two
            // prefix slots, two suffix slots, or one of each across the rest),
            // tsc unions the per-slot candidates instead of failing the second
            // slot's mutual-subtype check. Route them through the same
            // co-located union merge the non-rest path uses so, e.g.,
            // `[infer A, ...unknown[], infer A]` against `[1, 2, 3]` binds
            // `A = 1 | 3` (true branch) rather than collapsing to the false
            // branch. The fixed-slot validity checks (no nested rest, optional
            // source cannot fill a required slot) stay eager so an invalid
            // shape rejects before any inference work.
            let mut fixed_pairs: Vec<(TypeId, TypeId)> =
                Vec::with_capacity(present_prefix_len + suffix_len);
            for i in 0..present_prefix_len {
                if !self.push_fixed_tuple_pair(
                    &source_elems[i],
                    &pattern_elems[i],
                    &mut fixed_pairs,
                ) {
                    return false;
                }
            }
            for i in 0..suffix_len {
                if !self.push_fixed_tuple_pair(
                    &source_elems[rest_source_end + i],
                    &pattern_elems[rest_index + 1 + i],
                    &mut fixed_pairs,
                ) {
                    return false;
                }
            }
            // A purely `[...rest]` pattern has no fixed slots; skip the merge
            // (which would otherwise clone `bindings` twice for an empty pair
            // set) and fall straight through to the residual match.
            if !fixed_pairs.is_empty()
                && !self.match_co_located_with_merge(
                    &fixed_pairs,
                    bindings,
                    visited,
                    checker,
                    CoLocatedMerge::Union,
                )
            {
                return false;
            }

            let pattern_rest_type = pattern_elems[rest_index].type_id;

            if present_prefix_len < prefix_len {
                // The source did not fill the entire prefix, so the trailing
                // optional prefix slots are absent and the rest slot has no
                // source elements to match (the residual is empty). tsc gives an
                // inference variable with zero candidates its declared
                // constraint, or its position default otherwise: `unknown` for a
                // plain `infer`, `unknown[]` for a rest `...infer T`. Bind those
                // defaults explicitly so the conditional takes its true branch
                // with the bindings tsc produces (e.g.
                // `[] extends [(infer H)?, ...infer T]` -> `H = unknown`,
                // `T = unknown[]`) instead of being rejected on arity or
                // collapsing the empty residual into `T = []`.
                for pattern_elem in &pattern_elems[present_prefix_len..prefix_len] {
                    if !pattern_elem.optional {
                        return false;
                    }
                    self.bind_unmatched_infer_defaults(
                        pattern_elem.type_id,
                        TypeId::UNKNOWN,
                        bindings,
                    );
                }
                // An unconstrained top-level `...infer T` defaults to
                // `unknown[]`; a constrained `...infer T extends C` to `C`. Any
                // infers nested inside a structured rest pattern default to a
                // plain `unknown`.
                match self.interner().lookup(pattern_rest_type) {
                    Some(TypeData::Infer(info)) => {
                        let default_ty = info
                            .constraint
                            .unwrap_or_else(|| self.interner().array(TypeId::UNKNOWN));
                        bindings.entry(info.name).or_insert(default_ty);
                    }
                    _ => self.bind_unmatched_infer_defaults(
                        pattern_rest_type,
                        TypeId::UNKNOWN,
                        bindings,
                    ),
                }
                return true;
            }

            // Match the residual source slice against the pattern's rest slot.
            // This runs *after* the merged prefix/suffix bindings so a name
            // shared between a fixed position and the rest slot (e.g.
            // `[infer A, ...infer A]`) is still rejected: the residual's
            // `bind_infer` mutual-subtype check sees the element-level
            // candidate already bound and fails against the array/tuple-level
            // one, taking the false branch exactly as tsc does (tsc reaches the
            // same outcome via its post-inference structural re-check).
            // The residual may itself contain rest elements (when the source is
            // a variadic tuple like `[a, ...b[]]`), so the helpers preserve
            // each source element's `rest`/`optional` flags and structurally
            // simplify a single-rest-non-optional residual (`[...X[]]` -> `X[]`)
            // so that `infer R` binds to the array form tsc treats as identical.
            let residual = &source_elems[present_prefix_len..rest_source_end];
            return self.match_residual_against_pattern_rest(
                residual,
                pattern_rest_type,
                bindings,
                visited,
                checker,
            );
        }

        if source_len > pattern_len {
            return false;
        }

        // Collect (source, pattern) pairs for the matched prefix and any
        // optional-tail slots, then route them through the co-located merge
        // helper so that `[infer U, infer U]` against `[string, number]`
        // unions to `string | number` instead of failing on the second slot's
        // mutual-subtype check inside `bind_infer`.
        let shared = std::cmp::min(source_len, pattern_len);
        let mut pairs: Vec<(TypeId, TypeId)> = Vec::with_capacity(pattern_len);
        for i in 0..shared {
            if !self.push_fixed_tuple_pair(&source_elems[i], &pattern_elems[i], &mut pairs) {
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
                if self.type_contains_infer(pattern_elem.type_id) {
                    pairs.push((TypeId::UNDEFINED, pattern_elem.type_id));
                }
            }
        }

        self.match_co_located_with_merge(&pairs, bindings, visited, checker, CoLocatedMerge::Union)
    }

    /// Match function signature parameters against a pattern.
    pub(crate) fn match_signature_params(
        &self,
        source_params: &[ParamInfo],
        pattern_params: &[ParamInfo],
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if source_params.len() != pattern_params.len() {
            return false;
        }
        // Function/callable parameters are contravariant: when the same
        // `infer T` name appears in multiple parameter positions and the
        // source supplies distinct types, tsc intersects the candidates so
        // that `(a: infer U, b: infer U) => void` against
        // `(a: string, b: "hello") => void` infers `U = string & "hello"`
        // instead of failing the second slot via `bind_infer`'s mutual
        // subtype check.
        let mut pairs: Vec<(TypeId, TypeId)> = Vec::with_capacity(pattern_params.len());
        for (source_param, pattern_param) in source_params.iter().zip(pattern_params.iter()) {
            if source_param.optional != pattern_param.optional
                || source_param.rest != pattern_param.rest
            {
                return false;
            }
            // For optional params, add undefined to the source type for pattern matching.
            // This allows inferring T | undefined from optional params.
            let source_param_type = if source_param.optional {
                self.interner()
                    .union2(source_param.type_id, TypeId::UNDEFINED)
            } else {
                source_param.type_id
            };
            pairs.push((source_param_type, pattern_param.type_id));
        }
        self.match_co_located_with_merge(
            &pairs,
            bindings,
            visited,
            checker,
            CoLocatedMerge::Intersection,
        )
    }

    /// Maximum iterations for alias-application reduction loops.
    /// Bounds peel/reduce walks against pathological alias chains.
    pub(crate) const MAX_ALIAS_REDUCTION_STEPS: u32 = 8;

    /// Decode `Application(Lazy(DefId)/TypeQuery, args)` and substitute the
    /// alias's type-parameter args into its resolved body. Returns `None`
    /// when the base isn't a resolvable DefId, arities disagree, or the
    /// substitution is a no-op (so callers don't need their own fixed-point
    /// guard for the substituted form).
    ///
    /// Used by `peel_alias_application` (requires body to be `Application`)
    /// and by `reduce_alias_body_to_application_form` (also accepts
    /// `Conditional` body for one infer-match step).
    /// Resolve an application/reference base (`Lazy(DefId)` or
    /// `TypeQuery(SymbolRef)`) to its defining [`DefId`]. Returns `None` for any
    /// other base shape, or a `TypeQuery` whose symbol has no `DefId` yet.
    pub(crate) fn application_base_def_id(&self, base: TypeId) -> Option<crate::def::DefId> {
        match self.interner().lookup(base)? {
            TypeData::Lazy(def_id) => Some(def_id),
            TypeData::TypeQuery(sym_ref) => self.resolver().symbol_to_def_id(sym_ref),
            _ => None,
        }
    }

    pub(crate) fn alias_application_substituted_body(&self, ty: TypeId) -> Option<TypeId> {
        let Some(TypeData::Application(app_id)) = self.interner().lookup(ty) else {
            return None;
        };
        let app = self.interner().type_application(app_id);
        let def_id = self.application_base_def_id(app.base)?;
        let type_params = self.resolver().get_lazy_type_params(def_id)?;
        if type_params.len() != app.args.len() {
            return None;
        }
        let body = self.resolver().resolve_lazy(def_id, self.interner())?;
        let substituted = crate::instantiation::instantiate::instantiate_generic_cached(
            self.interner(),
            self.query_db(),
            body,
            &type_params,
            &app.args,
        );
        (substituted != ty).then_some(substituted)
    }

    /// Peel one alias layer off an `Application` whose body is itself an
    /// `Application(...)`. We do not gate on `get_def_kind`: zombie `DefId`s
    /// from `interner.reference` are not tagged with `DefKind` in the
    /// definition store, but the body shape (`Application` vs structural
    /// `Object`/`Callable`) is the reliable structural signal.
    pub(crate) fn peel_alias_application(&self, ty: TypeId) -> Option<TypeId> {
        let substituted = self.alias_application_substituted_body(ty)?;
        matches!(
            self.interner().lookup(substituted),
            Some(TypeData::Application(_))
        )
        .then_some(substituted)
    }

    /// Whether `base` names a generic *wrapper alias* — a `Lazy`/`TypeQuery`
    /// reference whose resolved body is itself an `Application` (e.g.
    /// `AB<T> = Promise<T[]>` or `Nest<T> = Promise<Promise<T[]>>`).
    ///
    /// Such a base does not participate in the covariant positional
    /// type-argument correspondence that a genuine interface/class hierarchy
    /// does (`Promise<T> <: PromiseLike<T>`): its single alias parameter maps
    /// through the alias body, not onto the wrapped interface's positional
    /// arguments. An infer-pattern whose base is a wrapper alias must therefore
    /// be *reduced* to its application form before structural matching, never
    /// matched positionally against a structurally-different concrete source
    /// base — doing the latter binds the `infer` one wrapper level early.
    pub(crate) fn is_wrapper_alias_base(&self, base: TypeId) -> bool {
        let Some(def_id) = self.application_base_def_id(base) else {
            return false;
        };
        self.resolver()
            .resolve_lazy(def_id, self.interner())
            .is_some_and(|body| {
                matches!(self.interner().lookup(body), Some(TypeData::Application(_)))
            })
    }

    /// Recover an `Application` form from a non-`Application` type via the
    /// global display-alias map. Used by infer-match reduction when the
    /// source has already been evaluated to its structural shape (e.g. an
    /// interface body substituted with concrete args) and
    /// `evaluate_application` recorded a back-reference to the original
    /// `Application` for this instantiation.
    pub(crate) fn try_recover_application_from_display_alias(&self, ty: TypeId) -> Option<TypeId> {
        if matches!(self.interner().lookup(ty), Some(TypeData::Application(_))) {
            return None;
        }
        let alias = self.interner().get_display_alias(ty)?;
        (alias != ty
            && matches!(
                self.interner().lookup(alias),
                Some(TypeData::Application(_))
            ))
        .then_some(alias)
    }

    /// Try to match a source Application's type args against a pattern Application's args.
    ///
    /// Returns `Some(true)` if all args matched, `Some(false)` if bases matched but an arg
    /// failed, `None` if the bases are incompatible (caller should try another candidate).
    ///
    /// One-directional subtyping (`source.base <: pattern.base`) is accepted because
    /// covariant interface hierarchies (e.g. `Promise<T> <: PromiseLike<T>`) preserve
    /// positional type-argument correspondence.
    fn try_match_application_args_to_pattern(
        &self,
        source: &TypeApplication,
        pattern: &TypeApplication,
        pattern_base_is_wrapper_alias: bool,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> Option<bool> {
        if source.args.len() != pattern.args.len() {
            return None;
        }
        if source.base != pattern.base {
            // A wrapper-alias pattern base (`Nest<T> = Promise<Promise<T[]>>`)
            // shares neither the identity nor the covariant positional
            // correspondence the base-subtype shortcut assumes for interface
            // hierarchies: matching its lone alias argument against a
            // structurally-different concrete source base binds the `infer` one
            // wrapper level early (e.g. `U = Promise<number[]>` instead of
            // `number`). Refuse the shortcut so the pattern is instead reduced to
            // its application form (caller's pattern-side alias-reduction step)
            // before the structural arguments are matched.
            if pattern_base_is_wrapper_alias {
                return None;
            }
            if !checker.is_subtype_of(source.base, pattern.base) {
                return None;
            }
        }
        for (source_arg, pattern_arg) in source.args.iter().zip(pattern.args.iter()) {
            if !self.match_infer_pattern(*source_arg, *pattern_arg, bindings, visited, checker) {
                return Some(false);
            }
        }
        Some(true)
    }

    /// Evaluate `type_id` in a fresh sub-evaluator during infer-pattern
    /// matching, bounded by a thread-global cross-evaluator recursion budget.
    ///
    /// Infer matching expands `Application`/`Mapped` sources and patterns by
    /// spinning up a new [`TypeEvaluator`] (the matching helpers only hold
    /// `&self`, so they cannot reuse the current evaluator's `&mut` evaluate
    /// path). Each fresh evaluator resets its own recursion guard, so a
    /// recursive generic wrapper can re-enter this expansion at ever-deeper
    /// nesting without any per-evaluator guard firing — an unbounded hang.
    ///
    /// This routes every such expansion through one place that participates in
    /// [`INFER_MATCH_EXPANSION_DEPTH`]. Once the budget is exhausted the input
    /// is returned **unchanged**: every caller already treats an unchanged
    /// result as "could not expand" (the infer match fails / the property is
    /// not found), which terminates the chain instead of looping. Mirrors tsc's
    /// global `instantiationDepth` cutoff.
    pub(crate) fn evaluate_for_infer_match(&self, type_id: TypeId) -> TypeId {
        let nuia = self.no_unchecked_indexed_access();
        // Per-query memo + cross-instance cycle break (#11586): a recursive
        // conditional/`infer` application fans out into the same root types across
        // fresh evaluators; serve a repeat within one query from the memo, and
        // return `type_id` unchanged on a cross-instance cycle (`None`) so the
        // in-flight ancestor expansion converges.
        crate::evaluation::cross_eval_guard::memoized_eval(type_id, nuia, || {
            let Some(_guard) = InferMatchExpansionGuard::enter() else {
                return (type_id, false);
            };
            let mut evaluator = TypeEvaluator::with_resolver(self.interner(), self.resolver());
            evaluator.set_no_unchecked_indexed_access(nuia);
            if let Some(query_db) = self.query_db() {
                evaluator = evaluator.with_query_db(query_db);
            }
            let result = evaluator.evaluate(type_id);
            // Memoize only stable results — a recursion/budget bail is a
            // stack-context artifact that must not be reused as the answer.
            (result, !evaluator.recursion_limit_hit())
        })
        .unwrap_or(type_id)
    }

    /// Match each member of a union source against `pattern`, merging the
    /// per-member infer bindings via [`Self::merge_infer_candidates`].
    ///
    /// Centralises the union-distribution policy used by `match_infer_pattern`
    /// so every entry path applies the same contravariance-aware merge: infer
    /// names that appear in contravariant positions (function/callable
    /// parameters) intersect their candidates, all others union them. Without
    /// this single source of truth, inner array/tuple branches could fall back
    /// to a naive `union2` merge, silently producing an over-constrained
    /// binding (e.g. `string & boolean` instead of `string | boolean`) for any
    /// pattern that reaches them.
    ///
    /// Returns `false` as soon as one member fails to match the pattern,
    /// matching the all-arms-must-match semantics of non-distributive
    /// conditionals (`[T] extends [Pattern]` with `T` a union).
    pub(crate) fn match_infer_pattern_union_members(
        &self,
        members: &[TypeId],
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        let base = bindings.clone();
        let mut merged = base.clone();

        // Determine which infer names appear in contravariant positions
        // (function/callable parameters) of the pattern. For those, multiple
        // candidates from union members should be intersected (not unioned).
        // This is essential for `UnionToIntersection<U>`:
        //   (U extends any ? (k: U) => void : never) extends ((k: infer I) => void) ? I : never
        // where `I` is in a contravariant (parameter) position, so candidates
        // from each union member are intersected to produce `A & B`.
        let contravariant_infers = self.collect_contravariant_infer_names(pattern);

        for &member in members.iter() {
            let mut local = base.clone();
            if !self.match_infer_pattern(member, pattern, &mut local, visited, checker) {
                return false;
            }
            self.merge_infer_candidates(&base, &mut merged, local, &contravariant_infers);
        }

        *bindings = merged;
        true
    }

    /// Main pattern matching function for infer types.
    ///
    /// Matches a source type against a pattern containing `infer` types,
    /// extracting the bound values into the bindings map.
    ///
    /// # Arguments
    /// * `source` - The concrete type to match against
    /// * `pattern` - The pattern type containing `infer` placeholders
    /// * `bindings` - Map to store extracted type bindings
    /// * `visited` - Set of already-visited type pairs (for cycle detection)
    /// * `checker` - Subtype checker for constraint validation
    ///
    /// # Returns
    /// `true` if the match succeeded and all bindings were extracted
    pub(crate) fn match_infer_pattern(
        &self,
        source: TypeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        // Cheap cycle guard first: re-entering the same `(source, pattern)` pair
        // is a converged cycle and returns before any further work or stack
        // growth.
        if !visited.insert((source, pattern)) {
            return true;
        }
        // Defensive stack growth for the structural infer-match recursion in
        // `match_infer_pattern_inner`. That recursion is already logically
        // bounded — the `visited` cycle guard above plus `INFER_MATCH_EXPANSION_
        // DEPTH` on alias expansion — so termination does not need the shared
        // solver frame budget; capping it with one could flip a legitimately
        // deep-but-terminating match to its bail default. But a deeply nested yet
        // acyclic source/pattern pair (e.g. a recursive conditional that extracts
        // `infer` through a chain of generic-alias wrappers) can still drive the
        // native stack past its limit. `grow_solver_stack` grows the stack on
        // demand without consuming a frame, so a pathological case degrades
        // through its own depth budget to a bounded TS2589 instead of a
        // process-aborting SIGABRT (issue #14123, fix direction 2: the
        // conditional/infer evaluation path must never crash the process). This
        // only grows the stack; it never changes a match result.
        crate::recursion::grow_solver_stack(|| {
            self.match_infer_pattern_inner(source, pattern, bindings, visited, checker)
        })
    }

    /// Structural body of [`Self::match_infer_pattern`]. Split out so the public
    /// entry can pair the `(source, pattern)` cycle guard with one
    /// `stacker::maybe_grow` segment-grow per recursion level; all recursive
    /// infer-matching re-enters through `match_infer_pattern`, never here.
    fn match_infer_pattern_inner(
        &self,
        source: TypeId,
        pattern: TypeId,
        bindings: &mut FxHashMap<Atom, TypeId>,
        visited: &mut InferPatternVisited,
        checker: &mut SubtypeChecker<'_, R>,
    ) -> bool {
        if source == TypeId::NEVER {
            return self.bind_infer_defaults(pattern, TypeId::NEVER, bindings, checker);
        }

        if source == pattern {
            return true;
        }

        if let Some(TypeData::Union(members)) = self.interner().lookup(source) {
            let members = self.interner().type_list(members);
            return self
                .match_infer_pattern_union_members(&members, pattern, bindings, visited, checker);
        }

        // Intersection sources match the pattern through whichever constituent
        // structurally matches it: e.g. `((g: () => T) => U) & { z?: 1 }` matched
        // against `(...args: any) => infer R` extracts `R = U` from the callable
        // member. Without this, `ReturnType<X>`/`Parameters<X>` over an
        // intersection-of-callable carrying a free type parameter fails to reduce
        // and the conditional stays deferred (a false `ReturnType<...>` residue).
        // Mirrors how `tsc` evaluates conditional `infer` extraction against an
        // intersection by inspecting each constituent. Try members in declaration
        // order and accept the first that binds the pattern; later members that do
        // not match the pattern shape (e.g. the `{ z?: 1 }` brand) are simply not
        // the constituent the pattern targets.
        if let Some(TypeData::Intersection(members)) = self.interner().lookup(source) {
            let members = self.interner().type_list(members);
            for &member in members.iter() {
                if member == source {
                    continue;
                }
                let mut local = bindings.clone();
                if self.match_infer_pattern(member, pattern, &mut local, visited, checker) {
                    *bindings = local;
                    return true;
                }
            }
            return false;
        }

        let Some(pattern_key) = self.interner().lookup(pattern) else {
            return false;
        };

        match pattern_key {
            TypeData::Infer(info) => self.bind_infer(&info, source, bindings, checker),
            TypeData::Function(pattern_fn_id) => self.match_infer_function_pattern(
                source,
                pattern_fn_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::Callable(pattern_shape_id) => self.match_infer_callable_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::Array(pattern_elem) => match self.interner().lookup(source) {
                Some(TypeData::Array(source_elem)) => {
                    self.match_infer_pattern(source_elem, pattern_elem, bindings, visited, checker)
                }
                Some(TypeData::Tuple(source_elems)) => {
                    // A tuple source matched against an array pattern `X[]` is
                    // a structural projection: every fixed element's type and
                    // every spread element's inner element type must satisfy
                    // `X`. Mirrors the residual matcher used by
                    // `match_tuple_elements`, so a tuple like
                    // `[boolean, ...number[]]` produced by residual reification
                    // can still pattern-match against `any[]`.
                    let source_elems = self.interner().tuple_list(source_elems);
                    self.match_residual_against_array_element(
                        &source_elems,
                        pattern_elem,
                        bindings,
                        visited,
                        checker,
                    )
                }
                // Union sources are caught by the top-level dispatch above and
                // routed through `match_infer_pattern_union_members`, so we
                // never reach here with `source = Union(...)`. Keep the match
                // arm explicit so a future change that loosens the top-level
                // catch still goes through the contravariance-aware helper
                // rather than a naive `union2` merge.
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    self.match_infer_pattern_union_members(
                        &members, pattern, bindings, visited, checker,
                    )
                }
                _ => false,
            },
            TypeData::Tuple(pattern_elems) => match self.interner().lookup(source) {
                Some(TypeData::Tuple(source_elems)) => {
                    let source_elems = self.interner().tuple_list(source_elems);
                    let pattern_elems = self.interner().tuple_list(pattern_elems);
                    self.match_tuple_elements(
                        &source_elems,
                        &pattern_elems,
                        bindings,
                        visited,
                        checker,
                    )
                }
                // See note above: union sources are routed via the top-level
                // helper to keep merge semantics uniform.
                Some(TypeData::Union(members)) => {
                    let members = self.interner().type_list(members);
                    self.match_infer_pattern_union_members(
                        &members, pattern, bindings, visited, checker,
                    )
                }
                _ => false,
            },
            TypeData::ReadonlyType(pattern_inner) => {
                let source_inner = match self.interner().lookup(source) {
                    Some(TypeData::ReadonlyType(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeData::NoInfer(pattern_inner) => {
                // NoInfer<T> matches if source matches T (strip wrapper)
                let source_inner = match self.interner().lookup(source) {
                    Some(TypeData::NoInfer(inner)) => inner,
                    _ => source,
                };
                self.match_infer_pattern(source_inner, pattern_inner, bindings, visited, checker)
            }
            TypeData::Object(pattern_shape_id) => self.match_infer_object_pattern(
                source,
                pattern_shape_id,
                pattern,
                bindings,
                visited,
                checker,
            ),
            TypeData::ObjectWithIndex(pattern_shape_id) => self
                .match_infer_object_with_index_pattern(
                    source,
                    pattern_shape_id,
                    pattern,
                    bindings,
                    visited,
                    checker,
                ),
            TypeData::Application(pattern_app_id) => {
                // Declaration-level match: walk `source` through one-step
                // alias-application peeling until its base aligns with the
                // pattern's base. Handles `Cond<RHS>` where `RHS = ToPromise<X>`
                // and `ToPromise<X> = Promise<X>` by reducing the source
                // `Application(ToPromise, [X])` to `Application(Promise, [X])`
                // before matching `Application(Promise, [infer Y])`.
                let pattern_app = self.interner().type_application(pattern_app_id);
                if pattern_app.args.len() == 1
                    && let Some(TypeData::Lazy(def_id)) = self.interner().lookup(pattern_app.base)
                    && self.resolver().is_builtin_readonly_array_def(def_id)
                    && let Some(source_elem) =
                        crate::type_queries::get_array_element_type(self.interner(), source)
                {
                    return self.match_infer_pattern(
                        source_elem,
                        pattern_app.args[0],
                        bindings,
                        visited,
                        checker,
                    );
                }
                // `pattern_app.base` is invariant across both the source-peeling
                // loop and the display-alias recovery below, so classify it once.
                // A wrapper-alias base (its resolved body is itself an
                // `Application`, e.g. `Nest<T> = Promise<Promise<T[]>>`) must be
                // reduced to its application form before matching: the positional
                // base-subtype shortcuts below would otherwise bind the `infer`
                // one wrapper level early.
                let pattern_base_is_wrapper_alias = self.is_wrapper_alias_base(pattern_app.base);
                let mut current_source = source;
                for _ in 0..Self::MAX_ALIAS_REDUCTION_STEPS {
                    if let Some(TypeData::Application(source_app_id)) =
                        self.interner().lookup(current_source)
                    {
                        let source_app = self.interner().type_application(source_app_id);
                        if let Some(result) = self.try_match_application_args_to_pattern(
                            &source_app,
                            &pattern_app,
                            pattern_base_is_wrapper_alias,
                            bindings,
                            visited,
                            checker,
                        ) {
                            return result;
                        }
                        // Rebuilding the wrapper alias over the *source's*
                        // arguments and accepting it by subtyping has the same
                        // early-binding hazard described above (the
                        // recursive-Promise relation accepts the rebuilt alias
                        // leniently), so skip this positional shortcut for wrapper
                        // bases and let the pattern-side reduction below peel them.
                        if source_app.args.len() == pattern_app.args.len()
                            && !pattern_base_is_wrapper_alias
                        {
                            let candidate_pattern = self
                                .interner()
                                .application(pattern_app.base, source_app.args.clone());
                            if checker.is_subtype_of(current_source, candidate_pattern) {
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
                                return true;
                            }
                        }
                    }
                    let Some(peeled) = self.peel_alias_application(current_source) else {
                        break;
                    };
                    current_source = peeled;
                }

                // Source may have been evaluated from Application(Promise,[T]) to Object before
                // reaching this point; display_alias records the original Application for recovery.
                if let Some(recovered) = self.try_recover_application_from_display_alias(source)
                    && let Some(TypeData::Application(recovered_app_id)) =
                        self.interner().lookup(recovered)
                {
                    let recovered_app = self.interner().type_application(recovered_app_id);
                    if let Some(result) = self.try_match_application_args_to_pattern(
                        &recovered_app,
                        &pattern_app,
                        pattern_base_is_wrapper_alias,
                        bindings,
                        visited,
                        checker,
                    ) {
                        return result;
                    }
                }

                // Wrapper-alias pattern reduction: when the pattern is a
                // generic *wrapper* alias carrying the `infer`
                // (`AB<infer U>` with `AB<T> = Promise<T[]>`) and the source is
                // the expanded structural form (`Promise<number[]>`, not
                // written via the alias), reduce the pattern head-only to its
                // body application form (`Promise<(infer U)[]>`), preserving the
                // `infer`, and match the source against that. The structural
                // `evaluate_for_infer_match` fallback below does not reduce an
                // infer-bearing application, so without this the alias pattern
                // never aligns with the expanded source and the conditional
                // wrongly collapses to its false branch (#14489). Gated to
                // aliases whose substituted body is itself an `Application`
                // (true wrappers) so conditional-/structural-body aliases stay
                // on the structural-expansion path.
                if let Some(reduced_pattern) = self.alias_application_substituted_body(pattern)
                    && reduced_pattern != pattern
                    && matches!(
                        self.interner().lookup(reduced_pattern),
                        Some(TypeData::Application(_))
                    )
                {
                    let mut reduced_bindings = bindings.clone();
                    let reduced_checkpoint = visited.checkpoint();
                    if self.match_infer_pattern(
                        source,
                        reduced_pattern,
                        &mut reduced_bindings,
                        visited,
                        checker,
                    ) && reduced_bindings.len() >= bindings.len()
                    {
                        *bindings = reduced_bindings;
                        return true;
                    }
                    visited.rollback_to(reduced_checkpoint);
                }

                // Fallback: Structural expansion
                // Expand the pattern Application to its structural form and recurse
                // This handles cases like: Reducer<infer S> matching a structural function type
                let expanded_pattern = self.evaluate_for_infer_match(pattern);

                // Only recurse if expansion actually changed the type
                if expanded_pattern != pattern {
                    if let Some(alias) = self.interner().get_display_alias(source)
                        && alias != source
                    {
                        if visited.contains(&(alias, expanded_pattern)) {
                            return true;
                        }
                        let mut alias_bindings = bindings.clone();
                        let alias_checkpoint = visited.checkpoint();
                        if self.match_infer_pattern(
                            alias,
                            expanded_pattern,
                            &mut alias_bindings,
                            visited,
                            checker,
                        ) {
                            visited.rollback_to(alias_checkpoint);
                            *bindings = alias_bindings;
                            return true;
                        }
                        visited.rollback_to(alias_checkpoint);
                    }
                    return self.match_infer_pattern(
                        source,
                        expanded_pattern,
                        bindings,
                        visited,
                        checker,
                    );
                }

                false
            }
            TypeData::TemplateLiteral(pattern_spans_id) => {
                let pattern_spans = self.interner().template_list(pattern_spans_id);
                match self.interner().lookup(source) {
                    Some(TypeData::Literal(LiteralValue::String(atom))) => {
                        let source_text = self.interner().resolve_atom_ref(atom);
                        self.match_template_literal_string(
                            source_text.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    Some(TypeData::TemplateLiteral(source_spans_id)) => {
                        let source_spans = self.interner().template_list(source_spans_id);
                        self.match_template_literal_spans(
                            source,
                            source_spans.as_ref(),
                            pattern_spans.as_ref(),
                            bindings,
                            checker,
                        )
                    }
                    // Primitive string does not match template literal patterns; tsc takes the false branch.
                    _ => false,
                }
            }
            // Handle union pattern containing infer types
            // Pattern: infer S | T | U where S is infer and T, U are not
            // Source: A | T | U or a single type A
            // Algorithm: Match source members against non-infer pattern members,
            // then bind the infer to the remaining source members
            TypeData::Union(pattern_members) => {
                let members = self.interner().type_list(pattern_members);
                if members.iter().any(|&member| {
                    !matches!(self.interner().lookup(member), Some(TypeData::Infer(_)))
                        && self.type_contains_infer(member)
                }) {
                    for &member in members.iter() {
                        let mut local_bindings = bindings.clone();
                        let mut local_visited = InferPatternVisited::default();
                        if self.match_infer_pattern(
                            source,
                            member,
                            &mut local_bindings,
                            &mut local_visited,
                            checker,
                        ) {
                            *bindings = local_bindings;
                            return true;
                        }
                    }
                    return false;
                }
                self.match_infer_union_pattern(source, pattern_members, pattern, bindings, checker)
            }
            _ => checker.is_subtype_of(source, pattern),
        }
    }
}

#[cfg(test)]
mod infer_match_expansion_guard_tests {
    use super::{
        INFER_MATCH_EXPANSION_DEPTH, InferMatchExpansionGuard, MAX_INFER_MATCH_EXPANSION_DEPTH,
    };

    /// The cross-evaluator infer-match expansion guard must (1) allow expansion
    /// up to the budget, (2) deny it beyond the budget so the caller skips the
    /// expansion (returns the source unchanged) instead of recursing through a
    /// fresh evaluator forever, and (3) restore capacity as guards unwind, so
    /// sequential (non-nested) expansions are never throttled. This is the
    /// primitive that turns the Zod non-termination (#10662) into a terminating
    /// compile.
    #[test]
    fn guard_bounds_cross_evaluator_expansion_depth() {
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.set(0));

        let mut held = Vec::new();
        for expected_prev in 0..MAX_INFER_MATCH_EXPANSION_DEPTH {
            let guard =
                InferMatchExpansionGuard::enter().expect("enter within budget must succeed");
            held.push(guard);
            assert_eq!(
                INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.get()),
                expected_prev + 1
            );
        }

        assert!(
            InferMatchExpansionGuard::enter().is_none(),
            "enter at the budget must be denied so the caller stops expanding"
        );

        held.clear();
        assert_eq!(INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.get()), 0);
        assert!(
            InferMatchExpansionGuard::enter().is_some(),
            "after unwinding, a fresh expansion must be allowed again"
        );
        INFER_MATCH_EXPANSION_DEPTH.with(|depth| depth.set(0));
    }
}
