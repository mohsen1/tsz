//! Canonicalization for structural type identity (Task #32: Graph Isomorphism)
//!
//! This module implements type canonicalization to achieve O(1) structural equality.
//! It transforms cyclic type definitions into trees using De Bruijn indices:
//!
//! - **Recursive(n)**: Self-reference N levels up the nesting path
//! - **BoundParameter(n)**: Type parameter using positional index for alpha-equivalence
//!
//! ## Key Concepts
//!
//! ### Structural vs Nominal Types
//!
//! - **`TypeAlias`**: Structural - `type A = { x: A }` and `type B = { x: B }`
//!   should canonicalize to the same type with `Recursive(0)`
//! - **Interface/Class/Enum**: Nominal - Must remain as `Lazy(DefId)` for nominal identity
//!
//! ### De Bruijn Indices
//!
//! - `Recursive(0)`: Immediate self-reference
//! - `Recursive(1)`: One level up (parent in nesting chain)
//! - `BoundParameter(0)`: Innermost type parameter
//! - `BoundParameter(n)`: (n+1)th-most-recently-bound type parameter
//!
//! ## Usage
//!
//! Canonicalization is for **comparison and hashing only**, not for display.
//! Use `canonicalize()` to check if two types are structurally identical:
//!
//! ```text
//! let canon_a = canonicalizer.canonicalize(type_a);
//! let canon_b = canonicalizer.canonicalize(type_b);
//! assert_eq!(canon_a, canon_b); // Same structure = same TypeId
//! ```

use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::def::DefKind;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::recursion::{RecursionGuard, RecursionProfile, RecursionResult};
use crate::relations::subtype::TypeResolver;
use crate::types::{
    ConditionalType, IndexSignature, ObjectShapeId, ParamInfo, TemplateSpan, TupleElement,
    TypeData, TypeId, TypePredicate, TypePredicateTarget,
};
use rustc_hash::FxHashMap;
use std::mem::size_of;
use tsz_common::interner::Atom;

/// Operation-local cache accounting for `Canonicalizer`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CanonicalizerCacheStatistics {
    /// Entries memoizing input `TypeId` to canonical `TypeId`.
    pub cache_entries: usize,
    /// Approximate heap and struct residency owned by the canonicalizer.
    pub estimated_size_bytes: usize,
}

/// Canonicalizer for structural type identity.
///
/// Transforms type aliases from cyclic graphs to trees using De Bruijn indices.
/// Only processes `DefKind::TypeAlias` (structural types), preserving nominal
/// types (Interface/Class/Enum) as `Lazy(DefId)`.
pub struct Canonicalizer<'a, R: TypeResolver> {
    /// Type interner for creating new `TypeIds`
    interner: &'a dyn TypeDatabase,
    /// Type resolver for looking up definitions
    resolver: &'a R,
    /// Stack of `DefIds` currently being expanded (for Recursive(n))
    def_stack: Vec<DefId>,
    /// Stack of type parameter scopes (for BoundParameter(n))
    /// Each scope is a list of parameter names in order
    param_stack: Vec<Vec<Atom>>,
    /// Cache to avoid re-canonicalizing the same type
    cache: FxHashMap<TypeId, TypeId>,
    /// Guard against unbounded canonicalization recursion for expanding aliases.
    guard: RecursionGuard<TypeId>,
}

impl<'a, R: TypeResolver> Canonicalizer<'a, R> {
    /// Create a new Canonicalizer.
    pub fn new(interner: &'a dyn TypeDatabase, resolver: &'a R) -> Self {
        Canonicalizer {
            interner,
            resolver,
            def_stack: Vec::new(),
            param_stack: Vec::new(),
            cache: FxHashMap::default(),
            guard: RecursionGuard::with_profile(RecursionProfile::SubtypeCheck),
        }
    }

    /// Return cache entry and residency accounting for this operation.
    pub fn cache_statistics(&self) -> CanonicalizerCacheStatistics {
        CanonicalizerCacheStatistics {
            cache_entries: self.cache.len(),
            estimated_size_bytes: self.estimated_size_bytes(),
        }
    }

    /// Estimate memory retained by this operation-local canonicalizer.
    pub fn estimated_size_bytes(&self) -> usize {
        let param_stack_bytes = self.param_stack.capacity() * size_of::<Vec<Atom>>()
            + self
                .param_stack
                .iter()
                .map(|scope| scope.capacity() * size_of::<Atom>())
                .sum::<usize>();

        size_of::<Self>()
            + self.def_stack.capacity() * size_of::<DefId>()
            + param_stack_bytes
            + self.cache.capacity() * size_of::<(TypeId, TypeId)>()
    }

    /// Canonicalize a type to its structural form.
    ///
    /// Returns a `TypeId` that represents the canonical structural form.
    /// Two types with the same structure will return the same `TypeId`.
    pub fn canonicalize(&mut self, type_id: TypeId) -> TypeId {
        // Fast path: intrinsic types (primitives, any, never, void, etc.)
        // are already canonical — the default arm of the inner match
        // returns `type_id` unchanged for them. Skip the cache lookup,
        // recursion-guard enter/leave, `TypeData` lookup, and match
        // dispatch entirely. `is_intrinsic()` is a free `TypeId`-range
        // check. Mirrors #2001 / #2005.
        if type_id.is_intrinsic() {
            return type_id;
        }

        // 1. Check cache
        if let Some(&cached) = self.cache.get(&type_id) {
            return cached;
        }

        // 2. Guard recursive expansion (e.g. infinitely expanding aliases).
        match self.guard.enter(type_id) {
            RecursionResult::Entered => {}
            RecursionResult::Cycle
            | RecursionResult::DepthExceeded
            | RecursionResult::IterationExceeded => {
                return type_id;
            }
        }

        // 3. Look up TypeData
        let result = if let Some(key) = self.interner.lookup(type_id) {
            match key {
                // Handle Type Alias Expansion (structural only)
                TypeData::Lazy(def_id) => {
                    match self.resolver.get_def_kind(def_id) {
                        Some(DefKind::TypeAlias) => {
                            // Structural type: canonicalize recursively
                            self.canonicalize_type_alias(def_id)
                        }
                        _ => {
                            // Nominal type (Interface/Class/Enum): preserve identity
                            // But canonicalize generic arguments if it's an Application
                            // For now, just return the Lazy as-is (nominal types keep their identity)
                            type_id
                        }
                    }
                }

                // Handle Type Parameters -> De Bruijn indices
                TypeData::TypeParameter(info) => {
                    if let Some(index) = self.find_param_index(info.name) {
                        self.interner.bound_parameter(index)
                    } else {
                        // Free type-parameter reference. Its identity is the
                        // parameter itself — its name and the *shape* of its
                        // constraint — never how the optional `default` was
                        // captured nor what *resolution state* the constraint
                        // snapshot happened to be in. Two references to the same
                        // parameter whose constraint differs only because one
                        // captured a resolved form (`keyof ResponseMap | "json"`)
                        // and the other its still-`Lazy` pre-resolution alias must
                        // canonicalize to one identity, or the relation's
                        // reflexive/identity fast path fragments (#13609). Reuse
                        // `canonical_type_param` so a free reference and a declared
                        // parameter (function/signature/mapped) reduce identically:
                        // canonicalize the constraint, drop the default. The
                        // interned parameter keeps its default for instantiation.
                        let normalized = self.canonical_type_param(info);
                        self.interner.type_param(normalized)
                    }
                }

                // `infer R` declarations carry the same `TypeParamInfo` as a
                // type parameter, so their identity follows the same rule as the
                // free `TypeParameter` branch above: the parameter is identified
                // by itself — its name and the *shape* of its constraint — never
                // by the optional `default` nor by the *resolution state* its
                // constraint snapshot was captured in. Leaving `Infer` in the
                // catch-all passthrough let two structurally-identical
                // conditionals whose `infer` parameter captured a resolved
                // constraint on one path and a still-`Lazy` cross-file alias on
                // the other (or merely differed in a captured default) fragment
                // into distinct identities, losing the relation's reflexive
                // short-circuit (#13609 — the `Infer` analogue of the free
                // `TypeParameter` fix). Reuse `canonical_type_param` so an
                // `infer` parameter and a declared parameter reduce identically.
                TypeData::Infer(info) => {
                    let normalized = self.canonical_type_param(info);
                    self.interner.infer(normalized)
                }

                // Recurse into composite types
                TypeData::Array(elem) => {
                    let c_elem = self.canonicalize(elem);
                    self.interner.array(c_elem)
                }

                TypeData::Tuple(list_id) => {
                    let elements = self.interner.tuple_list(list_id);
                    let c_elements: Vec<TupleElement> = elements
                        .iter()
                        .map(|e| TupleElement {
                            type_id: self.canonicalize(e.type_id),
                            // Identity-exempt: a tuple element's label is cosmetic.
                            // `[a: number]`, `[b: number]` and `[number]` are the
                            // same type — `tsc` never compares labels for tuple
                            // identity/assignability. `TupleElement` derives
                            // `Eq`/`Hash` over `name`, so keeping it here fragments
                            // alpha-equivalent tuples and misses the relation's
                            // reflexive fast path (#13609, value-name axis).
                            name: None,
                            optional: e.optional,
                            rest: e.rest,
                        })
                        .collect();
                    self.interner.tuple(c_elements)
                }

                TypeData::Union(members_id) => {
                    let members = self.interner.type_list(members_id);
                    let c_members: Vec<TypeId> =
                        members.iter().map(|&m| self.canonicalize(m)).collect();
                    // Sort and deduplicate (union is commutative)
                    // Sort by raw u32 value since TypeId doesn't implement Ord
                    let mut sorted = c_members;
                    sorted.sort_by_key(|t| t.0);
                    sorted.dedup();
                    self.interner.union(sorted)
                }

                TypeData::Intersection(members_id) => {
                    let members = self.interner.type_list(members_id);
                    // 1. Canonicalize all members
                    let c_members: Vec<TypeId> =
                        members.iter().map(|&m| self.canonicalize(m)).collect();

                    // 2. Separate callables (preserve order) from structural types (sort)
                    let mut structural = Vec::with_capacity(c_members.len());
                    let mut callables = Vec::new();
                    for m in c_members {
                        if crate::type_queries::is_callable_type(self.interner, m) {
                            callables.push(m);
                        } else {
                            structural.push(m);
                        }
                    }

                    // 3. Sort structural members by canonical TypeId (commutative)
                    structural.sort_by_key(|t| t.0);
                    structural.dedup();

                    // 4. Combine: structural first (sorted), then callables (preserved order)
                    let mut final_members = structural;
                    final_members.extend(callables);
                    self.interner.intersection(final_members)
                }

                // Generic type application (e.g., Box<string>)
                TypeData::Application(app_id) => {
                    let app = self.interner.type_application(app_id);
                    if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base)
                        && self.resolver.get_def_kind(def_id) == Some(DefKind::TypeAlias)
                    {
                        self.canonicalize_type_alias_application(def_id, &app.args)
                    } else {
                        // Canonicalize base type
                        let c_base = self.canonicalize(app.base);
                        // Canonicalize all generic arguments
                        let c_args: Vec<TypeId> =
                            app.args.iter().map(|&arg| self.canonicalize(arg)).collect();
                        self.interner.application(c_base, c_args)
                    }
                }

                TypeData::Function(shape_id) => {
                    let shape = self.interner.function_shape(shape_id);

                    // Enter new scope if this function has type parameters (alpha-equivalence)
                    let pushed_scope = if !shape.type_params.is_empty() {
                        let param_names: Vec<Atom> =
                            shape.type_params.iter().map(|p| p.name).collect();
                        self.param_stack.push(param_names);
                        true
                    } else {
                        false
                    };

                    // Canonicalize this_type if present
                    let c_this_type = shape.this_type.map(|t| self.canonicalize(t));
                    // Canonicalize return type
                    let c_return_type = self.canonicalize(shape.return_type);
                    // Canonicalize parameter types (names dropped — see
                    // `canonical_params`).
                    let c_params = self.canonical_params(&shape.params);

                    // Canonicalize type parameter constraints. Names and the
                    // identity-irrelevant modifiers are dropped for
                    // alpha-equivalence — references are already positional
                    // (see `canonical_bound_type_param`).
                    let c_type_params: Vec<crate::types::TypeParamInfo> = shape
                        .type_params
                        .iter()
                        .map(|&tp| self.canonical_bound_type_param(tp))
                        .collect();

                    // Canonicalize type predicate (identifier target normalized —
                    // see `canonical_type_predicate`).
                    let c_type_predicate = shape
                        .type_predicate
                        .as_ref()
                        .map(|pred| self.canonical_type_predicate(pred));

                    // Pop scope
                    if pushed_scope {
                        self.param_stack.pop();
                    }

                    let new_shape = crate::types::FunctionShape {
                        type_params: c_type_params,
                        params: c_params,
                        this_type: c_this_type,
                        return_type: c_return_type,
                        type_predicate: c_type_predicate,
                        is_constructor: shape.is_constructor,
                        is_method: shape.is_method,
                    };

                    self.interner.function(new_shape)
                }

                TypeData::Callable(shape_id) => self.canonicalize_callable(shape_id),

                // Task #39: Mapped type canonicalization for alpha-equivalence
                // When comparing mapped types over type parameters (deferred), we need
                // to canonicalize the constraint, template, and name_type to achieve
                // structural identity. The type_param name is handled via param_stack.
                TypeData::Mapped(mapped_id) => {
                    let mapped = self.interner.mapped_type(mapped_id);

                    // 1. Canonicalize the constraint FIRST (Outside scope)
                    // The iteration variable K is NOT visible in its own constraint
                    let c_constraint = self.canonicalize(mapped.constraint);

                    // 2. Enter new scope for the iteration variable (alpha-equivalence)
                    self.param_stack.push(vec![mapped.type_param.name]);

                    // 3. Canonicalize the template type (Inside scope - K is visible here)
                    let c_template = self.canonicalize(mapped.template);

                    // 4. Canonicalize name_type if present (Inside scope - as clause sees K)
                    let c_name_type = mapped.name_type.map(|t| self.canonicalize(t));

                    // 5. Pop scope
                    self.param_stack.pop();

                    // 6. Normalize the TypeParamInfo name for alpha-equivalence
                    // so that { [K in T]: K } and { [P in T]: P } hash to the
                    // same value. Since we use De Bruijn indices (BoundParameter)
                    // in the body, this name is never looked up, only used for
                    // hashing identity (see `canonical_bound_type_param`).
                    let c_type_param = self.canonical_bound_type_param(mapped.type_param);

                    let c_mapped = crate::types::MappedType {
                        type_param: c_type_param,
                        constraint: c_constraint,
                        template: c_template,
                        name_type: c_name_type,
                        readonly_modifier: mapped.readonly_modifier,
                        optional_modifier: mapped.optional_modifier,
                    };

                    self.interner.mapped(c_mapped)
                }

                // Object types: canonicalize property types while preserving metadata
                TypeData::Object(shape_id) => self.canonicalize_object(shape_id, false),

                TypeData::ObjectWithIndex(shape_id) => self.canonicalize_object(shape_id, true),

                // Task #47: Template Literal canonicalization for alpha-equivalence
                // Uppercase<T> and Uppercase<U> should be identical when T and U are identical
                TypeData::TemplateLiteral(id) => {
                    let spans = self.interner.template_list(id);
                    let c_spans: Vec<TemplateSpan> = spans
                        .iter()
                        .map(|span| match span {
                            TemplateSpan::Text(atom) => TemplateSpan::Text(*atom),
                            TemplateSpan::Type(t) => TemplateSpan::Type(self.canonicalize(*t)),
                        })
                        .collect();
                    self.interner.template_literal(c_spans)
                }

                // Task #47: String Intrinsic canonicalization for alpha-equivalence
                // Uppercase<T>, Lowercase<T>, etc. should canonicalize nested type parameters
                TypeData::StringIntrinsic { kind, type_arg } => {
                    let c_arg = self.canonicalize(type_arg);
                    self.interner.string_intrinsic(kind, c_arg)
                }

                // Index access type (T[K]) - canonicalize both object and key
                TypeData::IndexAccess(object_type, key_type) => {
                    let c_obj = self.canonicalize(object_type);
                    let c_key = self.canonicalize(key_type);
                    self.interner.index_access(c_obj, c_key)
                }

                // KeyOf type (keyof T) - canonicalize the inner type
                TypeData::KeyOf(inner) => {
                    let c_inner = self.canonicalize(inner);
                    self.interner.keyof(c_inner)
                }

                // Readonly type (readonly T[]) - canonicalize the inner type
                TypeData::ReadonlyType(inner) => {
                    let c_inner = self.canonicalize(inner);
                    self.interner.readonly_type(c_inner)
                }

                // `NoInfer<T>` - single-nested structural wrapper: canonicalize the
                // inner like its `child_policy` siblings `Array`/`ReadonlyType`, then
                // re-wrap to preserve the wrapper's distinct identity from `T`.
                // Omitting it left `NoInfer<…>` in the catch-all `_ => type_id` arm,
                // fragmenting the canonical identity of any type containing it when
                // the inner was structurally identical but differently interned
                // (alpha-equivalent generics; pre-resolution `Lazy` vs expanded body)
                // — the #13609 identity-fragmentation family on the `NoInfer` axis.
                TypeData::NoInfer(inner) => {
                    let c_inner = self.canonicalize(inner);
                    self.interner.no_infer(c_inner)
                }

                // Substitution type: canonicalize the base variable and the
                // implied constraint, then re-derive through the simplifying
                // constructor so its identity tracks the canonical base/constraint.
                TypeData::Substitution {
                    base_type,
                    constraint,
                } => {
                    let c_base = self.canonicalize(base_type);
                    let c_constraint = self.canonicalize(constraint);
                    self.interner.substitution(c_base, c_constraint)
                }

                // Conditional type (T extends U ? X : Y)
                TypeData::Conditional(cond_id) => {
                    let cond = self.interner.conditional_type(cond_id);
                    let c_check = self.canonicalize(cond.check_type);
                    let c_extends = self.canonicalize(cond.extends_type);
                    let c_true = self.canonicalize(cond.true_type);
                    let c_false = self.canonicalize(cond.false_type);
                    self.interner.conditional(ConditionalType {
                        check_type: c_check,
                        extends_type: c_extends,
                        true_type: c_true,
                        false_type: c_false,
                        is_distributive: cond.is_distributive,
                    })
                }

                // Other types: preserve as-is (will be handled as needed)
                _ => type_id,
            }
        } else {
            // Error/None - preserve as-is
            type_id
        };

        self.guard.leave(type_id);
        self.cache.insert(type_id, result);
        result
    }

    /// Canonicalize a type alias definition.
    ///
    /// This handles:
    /// - Cycle detection via `def_stack`
    /// - Generic parameter scope management
    /// - Recursive self-references -> Recursive(n)
    fn canonicalize_type_alias(&mut self, def_id: DefId) -> TypeId {
        // Check for cycles (mutual recursion or self-reference)
        if let Some(depth) = self.get_recursion_depth(def_id) {
            return self.interner.recursive(depth);
        }

        // Push to stack for cycle detection
        self.def_stack.push(def_id);

        // Enter new scope if generic
        let params = self.resolver.get_lazy_type_params(def_id);
        let pushed_scope = if let Some(ps) = params.as_ref() {
            let param_names: Vec<Atom> = ps.iter().map(|p| p.name).collect();
            self.param_stack.push(param_names);
            true
        } else {
            false
        };

        // Resolve the alias body and canonicalize recursively
        let body = self
            .resolver
            .resolve_lazy(def_id, self.interner)
            .unwrap_or(TypeId::ERROR);
        let canonical_body = self.canonicalize(body);

        // Pop scope and def_stack
        if pushed_scope {
            self.param_stack.pop();
        }
        self.def_stack.pop();

        canonical_body
    }

    fn canonicalize_type_alias_application(&mut self, def_id: DefId, args: &[TypeId]) -> TypeId {
        if let Some(depth) = self.get_recursion_depth(def_id) {
            let recursive = self.interner.recursive(depth);
            if args.is_empty() {
                return recursive;
            }
            let c_args = args.iter().map(|&arg| self.canonicalize(arg)).collect();
            return self.interner.application(recursive, c_args);
        }

        self.def_stack.push(def_id);

        let params = self.resolver.get_lazy_type_params(def_id);
        let pushed_scope = if let Some(ps) = params.as_ref() {
            let param_names: Vec<Atom> = ps.iter().map(|p| p.name).collect();
            self.param_stack.push(param_names);
            true
        } else {
            false
        };

        let body = self
            .resolver
            .resolve_lazy(def_id, self.interner)
            .unwrap_or(TypeId::ERROR);
        let instantiated = if let Some(ps) = params {
            let subst = TypeSubstitution::from_args(self.interner, &ps, args);
            instantiate_type(self.interner, body, &subst)
        } else {
            body
        };
        let canonical_body = self.canonicalize(instantiated);

        if pushed_scope {
            self.param_stack.pop();
        }
        self.def_stack.pop();

        canonical_body
    }

    /// Get the recursion depth for a `DefId` if it's in the `def_stack`.
    ///
    /// Returns Some(depth) if the `DefId` is being expanded, where:
    /// - 0 = immediate self-reference (current `DefId`)
    /// - n = n levels up the nesting chain
    fn get_recursion_depth(&self, def_id: DefId) -> Option<u32> {
        self.def_stack
            .iter()
            .rev()
            .position(|&d| d == def_id)
            .map(|pos| pos as u32)
    }

    /// Canonicalize `type_id` with an extra outer scope of type-parameter
    /// names visible on top of the stack.
    ///
    /// Callers compare types whose type parameters are "free" relative to
    /// the supplied input — for example, two interface declarations'
    /// constraints, where each declaration's `T` is bound by its enclosing
    /// declaration list rather than by any wrapper in the constraint
    /// expression itself. Pushing the shared scope on entry rewrites those
    /// otherwise-free `TypeParameter(name)` occurrences to
    /// `BoundParameter(n)` so two declarations whose constraints reference
    /// positionally-equivalent parameters canonicalize to the same form.
    ///
    /// The scope is pushed before the recursive walk and popped after, so
    /// subsequent calls on the same `Canonicalizer` are unaffected.
    pub fn canonicalize_with_param_scope(
        &mut self,
        type_id: TypeId,
        param_names: &[Atom],
    ) -> TypeId {
        self.param_stack.push(param_names.to_vec());
        let result = self.canonicalize(type_id);
        self.param_stack.pop();
        result
    }

    /// Find the De Bruijn index for a type parameter by name.
    ///
    /// Searches from the top of the stack (innermost scope) downward.
    /// Returns Some(index) if found, where:
    /// - 0 = innermost parameter
    /// - n = (n+1)th-most-recently-bound parameter
    fn find_param_index(&self, name: Atom) -> Option<u32> {
        let mut flattened_index = 0u32;

        // Search from top of stack (innermost scope) to bottom
        for scope in self.param_stack.iter().rev() {
            for (idx, &param_name) in scope.iter().enumerate() {
                if param_name == name {
                    // Calculate flattened index from innermost
                    let innermost_offset = scope.len() - idx - 1;
                    return Some(flattened_index + innermost_offset as u32);
                }
            }
            flattened_index += scope.len() as u32;
        }

        None
    }

    /// Canonicalize an index signature by recursively canonicalizing its key and value types.
    fn canonicalize_index_signature(
        &mut self,
        idx: &Option<IndexSignature>,
    ) -> Option<IndexSignature> {
        idx.as_ref().map(|idx| IndexSignature {
            key_type: self.canonicalize(idx.key_type),
            value_type: self.canonicalize(idx.value_type),
            readonly: idx.readonly,
            // Identity-exempt: the source key name (`[k: string]` vs
            // `[key: string]`) is cosmetic. `IndexSignature` itself excludes it
            // from `Eq`/`Hash`, but `ObjectShape`/`CallableShape` re-add it to
            // interned identity for display, so two structurally-identical
            // index signatures fragment on it. The comparison-only canonical
            // form drops it so they reduce identically (#13609, value-name axis).
            param_name: None,
        })
    }

    /// Canonicalize an object type by recursively canonicalizing property types.
    ///
    /// Preserves all metadata (names, optional, readonly, visibility, `parent_id`)
    /// and nominal symbols. Only transforms the `TypeIds` within properties.
    fn canonicalize_object(&mut self, shape_id: ObjectShapeId, _with_index: bool) -> TypeId {
        let shape = self.interner.object_shape(shape_id);

        // Canonicalize all properties
        let mut new_props = Vec::with_capacity(shape.properties.len());
        for prop in &shape.properties {
            let mut new_prop = prop.clone();
            // Canonicalize read type (getter/lookup)
            new_prop.type_id = self.canonicalize(prop.type_id);
            // Canonicalize write type (setter/assignment)
            new_prop.write_type = self.canonicalize(prop.write_type);
            // Preserve all other metadata as-is
            // - name (Atom): Property names are NOT remapped
            // - optional (bool): Part of type identity
            // - readonly (bool): Part of type identity
            // - is_method (bool): Part of type identity
            // - visibility (Visibility): Part of type identity (nominal subtyping)
            // - parent_id (Option<SymbolId>): Brand for private/protected members
            new_props.push(new_prop);
        }

        // Canonicalize index signatures if present
        let new_string_index = self.canonicalize_index_signature(&shape.string_index);
        let new_number_index = self.canonicalize_index_signature(&shape.number_index);
        let new_symbol_index = self.canonicalize_index_signature(&shape.symbol_index);

        // Preserve the symbol field for nominal types (class instances)
        // This ensures that class A and class B with same properties remain distinct
        let symbol = shape.symbol;

        // Create new object shape with canonicalized types but preserved metadata
        let new_shape = crate::types::ObjectShape {
            base_types: Vec::new(),
            flags: shape.flags,
            properties: new_props,
            string_index: new_string_index,
            number_index: new_number_index,
            symbol_index: new_symbol_index,
            symbol,
        };

        // Intern using the appropriate method
        // Note: object_with_index takes ObjectShape by value and sorts properties
        self.interner.object_with_index(new_shape)
    }

    /// Canonical (identity) form of a declared type parameter.
    ///
    /// The `constraint` is canonicalized recursively, but the identity-irrelevant
    /// declaration modifiers `default` and `is_const` are dropped: `tsc` never
    /// distinguishes type parameters by either in relation/identity. The `default`
    /// is consumed only at instantiation when no argument is supplied, and the
    /// `const` modifier (`<const R>`) is an inference-site modifier that preserves
    /// literal types at call sites — `compareTypeParametersIdentical` compares
    /// constraints only. Keeping either would let two otherwise-identical generic
    /// signatures — `<R extends X = "json">` / `<const R extends X>` vs
    /// `<R extends X>` — canonicalize to distinct identities and miss the
    /// relation's reflexive short-circuit (#13609). Both modifiers are still read
    /// where they matter (instantiation / inference) off the *interned* parameter,
    /// which is unchanged; only this comparison/hashing-only canonical form drops
    /// them. Callers that manage an alpha-equivalence scope (function/signature/
    /// mapped) must push the parameter name before calling so the constraint's
    /// self/sibling references resolve to bound parameters.
    fn canonical_type_param(
        &mut self,
        tp: crate::types::TypeParamInfo,
    ) -> crate::types::TypeParamInfo {
        crate::types::TypeParamInfo {
            name: tp.name,
            constraint: tp.constraint.map(|c| self.canonicalize(c)),
            default: None,
            is_const: false,
            origin: tp.origin,
        }
    }

    /// Canonical (identity) form of a *bound* declared type parameter — one whose
    /// references inside the surrounding type have already been rewritten to
    /// positional [`TypeData::BoundParameter`](crate::types::TypeData::BoundParameter)
    /// indices under a pushed alpha-equivalence scope (function / call-signature /
    /// mapped binding sites).
    ///
    /// On top of [`Self::canonical_type_param`] this also erases the declared
    /// `name` and the internal `origin` discriminant. Once every reference to the
    /// parameter is positional, its name is purely cosmetic: `tsc`'s
    /// `compareTypeParametersIdentical` maps parameters positionally and compares
    /// constraints only — never names. Keeping the name in the canonical shape let
    /// two alpha-equivalent generic callables (`<T extends X>() => T` vs
    /// `<U extends X>() => U`) hash to distinct `TypeId`s and miss the relation's
    /// reflexive/identity fast path — the same fragmentation family as #13609, here
    /// on the parameter *name* axis rather than a declaration modifier. The
    /// mapped-type binder already erased the name for exactly this reason
    /// (`{ [K in T]: K }` vs `{ [P in T]: P }`); this generalizes it to every
    /// binding site.
    ///
    /// The [`TypeParamOrigin`](crate::types::TypeParamOrigin) discriminant is
    /// erased to the [`User`](crate::types::TypeParamOrigin::User) default for the
    /// same reason. `origin` is a purely internal tsz classification (whether a
    /// parameter is a source-written generic or an inference placeholder) and is
    /// never part of TypeScript's notion of type identity. Its inference variants
    /// carry program-unique `id`s (`InferPlaceholder { id }` / `InferSource { id,
    /// .. }`), and `TypeParamInfo` derives `Eq`/`Hash` over `origin`, so two
    /// otherwise-identical *bound* signatures whose parameters are
    /// higher-order-inference placeholders minted at different ids (the
    /// re-generalized return-type form) canonicalized to distinct `TypeId`s and
    /// missed the reflexive short-circuit — the `origin` axis of the #13609
    /// fragmentation family. Because a bound parameter's references are already
    /// positional, the id is never load-bearing here. (A *free* reference is
    /// different: an inference placeholder's `id` is its identity, so
    /// [`Self::canonical_type_param`] keeps `origin` — exactly as it keeps the
    /// name.) The discriminant is still read where it matters (inference) off the
    /// *interned* parameter, which is unchanged; only this comparison/hashing-only
    /// canonical form drops it.
    fn canonical_bound_type_param(
        &mut self,
        tp: crate::types::TypeParamInfo,
    ) -> crate::types::TypeParamInfo {
        crate::types::TypeParamInfo {
            name: Atom::NONE,
            origin: crate::types::TypeParamOrigin::User,
            ..self.canonical_type_param(tp)
        }
    }

    /// Canonical (identity) forms of a parameter list, with the cosmetic
    /// parameter `name` dropped.
    ///
    /// A function/method parameter's name is **not** part of TypeScript
    /// structural identity: `(a: string) => void` and `(b: string) => void` are
    /// the same type, and `compareSignaturesIdentical` matches parameters
    /// positionally and compares their types only — never their names. But
    /// [`ParamInfo`] derives `Eq`/`Hash` over `name`, so keeping it would let two
    /// alpha-equivalent signatures intern to distinct canonical `TypeId`s and
    /// miss the relation's reflexive/identity fast path (#13609, the value-name
    /// analogue of the type-parameter name fix #14096). The optional/rest flags
    /// and the parameter *type* are identity-bearing and preserved; only the
    /// comparison/hashing-only canonical form drops the name — the *interned*
    /// signature keeps it where it is rendered.
    fn canonical_params(&mut self, params: &[ParamInfo]) -> Vec<ParamInfo> {
        params
            .iter()
            .map(|p| ParamInfo {
                name: None,
                type_id: self.canonicalize(p.type_id),
                optional: p.optional,
                rest: p.rest,
            })
            .collect()
    }

    /// Canonical (identity) form of a type predicate.
    ///
    /// The asserted `type_id` is canonicalized recursively. An `Identifier`
    /// target's atom is dropped (normalized to the empty atom) on the same
    /// rationale as [`Self::canonical_params`]: the predicate names a parameter
    /// that `parameter_index` already anchors positionally, so the identifier
    /// text is cosmetic and `(x: unknown): x is string` and
    /// `(y: unknown): y is string` are the same type. The `This`/`Identifier`
    /// discriminant, `asserts`, and `parameter_index` are identity-bearing and
    /// preserved.
    fn canonical_type_predicate(&mut self, pred: &TypePredicate) -> TypePredicate {
        let target = match pred.target {
            // `parameter_index` already anchors the referenced parameter; the
            // identifier atom is cosmetic, so erase it to the empty sentinel.
            // `This` is identity-bearing and passes through unchanged.
            TypePredicateTarget::Identifier(_) => TypePredicateTarget::Identifier(Atom::NONE),
            TypePredicateTarget::This => TypePredicateTarget::This,
        };
        TypePredicate {
            asserts: pred.asserts,
            target,
            type_id: pred.type_id.map(|t| self.canonicalize(t)),
            parameter_index: pred.parameter_index,
        }
    }

    /// Canonicalize a single call signature with type parameter scope management.
    fn canonicalize_signature(
        &mut self,
        sig: &crate::types::CallSignature,
    ) -> crate::types::CallSignature {
        // Enter new scope if this signature has type parameters (alpha-equivalence)
        let pushed_scope = if !sig.type_params.is_empty() {
            let param_names: Vec<Atom> = sig.type_params.iter().map(|p| p.name).collect();
            self.param_stack.push(param_names);
            true
        } else {
            false
        };

        // Canonicalize this_type if present
        let c_this_type = sig.this_type.map(|t| self.canonicalize(t));

        // Canonicalize return type
        let c_return_type = self.canonicalize(sig.return_type);

        // Canonicalize parameter types (names dropped — see `canonical_params`).
        let c_params = self.canonical_params(&sig.params);

        // Canonicalize type parameter constraints. Names and the
        // identity-irrelevant modifiers are dropped for alpha-equivalence —
        // references are already positional (see `canonical_bound_type_param`).
        let c_type_params: Vec<crate::types::TypeParamInfo> = sig
            .type_params
            .iter()
            .map(|&tp| self.canonical_bound_type_param(tp))
            .collect();

        // Canonicalize type predicate (identifier target normalized — see
        // `canonical_type_predicate`).
        let c_type_predicate = sig
            .type_predicate
            .as_ref()
            .map(|pred| self.canonical_type_predicate(pred));

        // Pop scope
        if pushed_scope {
            self.param_stack.pop();
        }

        crate::types::CallSignature {
            type_params: c_type_params,
            params: c_params,
            this_type: c_this_type,
            return_type: c_return_type,
            type_predicate: c_type_predicate,
            is_method: sig.is_method,
        }
    }

    /// Canonicalize a callable type (overloaded functions).
    fn canonicalize_callable(&mut self, shape_id: crate::types::CallableShapeId) -> TypeId {
        let shape = self.interner.callable_shape(shape_id);

        // Canonicalize all call signatures (order matters for overload resolution)
        let c_call_signatures: Vec<crate::types::CallSignature> = shape
            .call_signatures
            .iter()
            .map(|sig| self.canonicalize_signature(sig))
            .collect();

        // Canonicalize all construct signatures
        let c_construct_signatures: Vec<crate::types::CallSignature> = shape
            .construct_signatures
            .iter()
            .map(|sig| self.canonicalize_signature(sig))
            .collect();

        // Canonicalize properties
        let mut new_props = Vec::with_capacity(shape.properties.len());
        for prop in &shape.properties {
            let mut new_prop = prop.clone();
            new_prop.type_id = self.canonicalize(prop.type_id);
            new_prop.write_type = self.canonicalize(prop.write_type);
            new_props.push(new_prop);
        }

        // Canonicalize index signatures
        let new_string_index = self.canonicalize_index_signature(&shape.string_index);
        let new_number_index = self.canonicalize_index_signature(&shape.number_index);

        let new_shape = crate::types::CallableShape {
            call_signatures: c_call_signatures,
            construct_signatures: c_construct_signatures,
            properties: new_props,
            string_index: new_string_index,
            number_index: new_number_index,
            symbol: shape.symbol,
            is_abstract: shape.is_abstract,
        };

        self.interner.callable(new_shape)
    }
}

#[cfg(test)]
#[path = "../../tests/canonicalize_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/canonicalize_origin_axis_tests.rs"]
mod origin_axis_tests;
