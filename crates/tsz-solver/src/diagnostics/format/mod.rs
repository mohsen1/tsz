//! Type formatting for the solver.
//! Centralizes logic for converting `TypeIds` and `TypeDatas` to human-readable strings.

mod alias_underlying;
mod application_reduction;
mod array;
mod cache_accounting;
mod compound;
mod display_simplification;
mod intrinsic;
mod key;
mod property_names;
// `test_tracing` exercises `debug!` / `debug_span!` / `trace_span!`. The
// workspace `tracing` dep filters those macros out at compile time when
// `debug_assertions` is off (via the `release_max_level_warn` feature), so
// the capture-based assertions can never observe events under
// `cargo test --release`. Gate the module on `debug_assertions` so the
// release-mode test build doesn't try to run (or compile) tests that have
// nothing to capture.
#[cfg(test)]
mod keyof_alias_display_tests;
#[cfg(all(test, debug_assertions))]
pub mod test_tracing;
#[cfg(test)]
mod tests;
pub mod tracing_helpers;

pub use alias_underlying::{
    application_reduces_to_displayable_shape, type_alias_displayed_as_underlying,
};
pub use property_names::format_excess_property_name;

/// Reorder union members for display so the nullish intrinsics render at the
/// tail: every non-nullish member keeps its relative order, then `null`, then
/// `undefined`.
///
/// This is `tsc`'s `formatUnionTypes` rule: the printer filters
/// `TypeFlags.Nullable` constituents out of the member walk and appends
/// `nullType` then `undefinedType` after it, so a rendered union always shows
/// `... | null | undefined` regardless of the union's internal (type-id) or
/// as-written member order. [`TypeFormatter::format_union`] applies the same
/// rule internally; this shared helper is for checker-side diagnostic
/// reconstructions that join a member list themselves instead of going
/// through `format_union`.
pub fn reorder_union_members_nullish_last(members: &[TypeId]) -> Vec<TypeId> {
    let mut ordered: Vec<TypeId> = members
        .iter()
        .copied()
        .filter(|&member| member != TypeId::NULL && member != TypeId::UNDEFINED)
        .collect();
    if members.contains(&TypeId::NULL) {
        ordered.push(TypeId::NULL);
    }
    if members.contains(&TypeId::UNDEFINED) {
        ordered.push(TypeId::UNDEFINED);
    }
    ordered
}
pub(crate) use property_names::needs_property_name_quotes;

use crate::construction::TypeDatabase;
use crate::def::{DefId, DefinitionStore};
use crate::diagnostics::{
    DiagnosticArg, PendingDiagnostic, RelatedInformation, SourceSpan, TypeDiagnostic,
    get_message_template,
};
use crate::types::{MappedModifier, ObjectShape, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::sync::Arc;
use tsz_common::interner::Atom;

/// Operation-local cache accounting for `TypeFormatter`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypeFormatterCacheStatistics {
    /// Cached atom-to-string display entries.
    pub atom_cache_entries: usize,
    /// Cached generic application display-reduction decisions.
    pub application_reduction_cache_entries: usize,
    /// Cached recursive-alias base predicate decisions.
    pub recursive_alias_base_cache_entries: usize,
    /// Approximate heap and struct residency owned by the formatter.
    pub estimated_size_bytes: usize,
}

/// Context for generating type strings.
pub struct TypeFormatter<'a> {
    interner: &'a dyn TypeDatabase,
    /// Symbol arena for looking up symbol names (optional)
    symbol_arena: Option<&'a tsz_binder::SymbolArena>,
    /// Definition store for looking up `DefId` names (optional)
    def_store: Option<&'a DefinitionStore>,
    /// Maps `file_id` -> module specifier for import-qualified type display.
    module_specifiers: Option<&'a FxHashMap<u32, String>>,
    /// Maps `file_id` -> full project-relative stripped path for cross-module
    /// diagnostic disambiguation (e.g. `src/library-a/index`). When this is
    /// set it overrides `module_specifiers` for
    /// `import_qualified_name_for_type` so the `import("<path>")` qualifier
    /// distinguishes two files that share the same basename.
    module_path_specifiers: Option<&'a FxHashMap<u32, String>>,
    /// Maps object `TypeId` -> module name for namespace types that were
    /// created as plain objects but should display as `typeof import("module")`.
    namespace_module_names: Option<&'a FxHashMap<TypeId, String>>,
    /// The `file_id` of the file currently being checked.
    current_file_id: Option<u32>,
    /// Maximum depth for nested type printing
    max_depth: u32,
    /// Maximum number of union members to display before truncating
    max_union_members: usize,
    /// Current depth
    current_depth: u32,
    atom_cache: FxHashMap<Atom, Arc<str>>,
    /// When true, skip adding synthetic `?: undefined` members to object unions.
    /// This should be set for error-message formatting (tsc doesn't optionalize
    /// union members in diagnostics, only in quickinfo/hover).
    skip_union_optionalize: bool,
    /// When true, format types using tsc's diagnostic display surface.
    diagnostic_mode: bool,
    /// When true, preserve the declared surface syntax of optional properties
    /// instead of appending synthetic `| undefined`.
    preserve_optional_property_surface_syntax: bool,
    /// When true, preserve the declared surface syntax of optional parameters
    /// instead of appending synthetic `| undefined`.
    preserve_optional_parameter_surface_syntax: bool,
    /// When true, use display properties (pre-widened literal types) for fresh
    /// object literals. This implements tsc's freshness model where error messages
    /// show literal types like `{ x: "hello" }` even when the type system uses
    /// widened types like `{ x: string }`.
    use_display_properties: bool,
    /// Nesting depth while formatting generic application arguments.
    ///
    /// tsc preserves the inferred literal surface for object arguments written
    /// inside `Alias<{ tag: "x" }>` even after the canonical object type has
    /// been widened. Outside application arguments, copied display properties
    /// should only affect fresh object literals.
    application_arg_display_depth: u32,
    /// Set of Application `TypeIds` currently being formatted via `display_alias`.
    /// Prevents infinite recursion when a `display_alias` chain forms a cycle.
    display_alias_visiting: FxHashSet<TypeId>,
    /// Set of `TypeId`s currently on the formatter's recursion stack. Used to
    /// elide self-referential composite types with `...`, mirroring tsc's
    /// `canPossiblyExpandType` cycle detection.
    format_visiting: FxHashSet<TypeId>,
    /// When true, preserve `Array<T>` generic syntax instead of `T[]` shorthand.
    /// tsc preserves the declared form in type-parameter constraints.
    pub(crate) preserve_array_generic_form: bool,
    /// When true, skip using type alias names for aliases whose body is a generic
    /// Application (e.g., `type Foo = Id<{...}>`). In assignability error messages,
    /// tsc shows the Application form `Id<{...}>` rather than the outer alias `Foo`.
    skip_application_alias_names: bool,
    /// Internal guard used while formatting helper application arguments that
    /// should show structural inputs instead of chasing nested application
    /// display aliases.
    skip_application_display_alias_chase: bool,
    /// Internal guard used while formatting generic application arguments.
    /// In that context, tsc preserves indexed-access alias spelling such as
    /// `Partial<T>[keyof T]` instead of simplifying the nested access to
    /// `T[keyof T] | undefined`.
    preserve_application_arg_index_alias_surface: bool,
    /// Internal guard used while rendering the object operand of an indexed
    /// access that could **not** be reduced. Reference materialization is
    /// suppressed underneath it, so an access whose outer link stays deferred
    /// prints the whole chain as written (`A["p"]["q"]`) rather than a hybrid
    /// of a resolved inner object and the remaining written keys.
    render_index_access_object_as_written: bool,
    /// Specific non-generic type aliases whose name should not be used for
    /// diagnostic display. This is used for `typeof` aliases in assignability
    /// messages where tsc prints the target's structural type rather than the
    /// alias name.
    skip_type_alias_def_ids: FxHashSet<DefId>,
    /// Type aliases currently being expanded through `skip_type_alias_def_ids`.
    /// This lets a recursive alias expand one structural layer before nested
    /// self-references elide as `...`.
    skipped_type_alias_expansion_visiting: FxHashSet<DefId>,
    /// Optional compiler-controlled display replacement for the lib-only
    /// `BuiltinIteratorReturn` alias.
    builtin_iterator_return_type: Option<TypeId>,
    /// When true, don't follow `display_alias` when it points to an Intersection
    /// type and the current type is an Object. Used for TS2741 messages where
    /// tsc shows the merged object form instead of the intersection form.
    skip_intersection_display_alias: bool,
    /// When true, don't follow `display_alias` when it points to an Application
    /// type and the current type is an Intersection. Used for TS2739 messages
    /// where tsc shows the structural `Number & { __brand: T }` form instead of
    /// the branded alias `Brand<T>`.
    skip_application_alias_for_intersections: bool,
    /// When true, format the primitive members of an intersection type using their
    /// apparent/boxed names: `Number` instead of `number`, `String` instead of
    /// `string`, `Boolean` instead of `boolean`. tsc always uses the capitalized
    /// forms for primitive members in intersection type display.
    capitalize_primitive_intersection_members: bool,
    /// When true, do not follow `display_alias` when the current type is an
    /// `Object` / `ObjectWithIndex`. Used for diagnostics like the JS
    /// prototype "property does not exist on type `{...}`" message where tsc
    /// shows the literal's structural shape regardless of any
    /// constructor-prototype symbol aliasing recorded by the type system.
    skip_object_display_alias: bool,
    /// When true, do not name a *composite* `Object` / `ObjectWithIndex` /
    /// `Union` / `Intersection` type by a reverse lookup to a **non-generic
    /// type-alias** definition (`find_def_for_type` / `find_def_by_shape`).
    ///
    /// Structural interning collapses an inline annotation (`{ a: number }`) and
    /// a coincidentally-shaped alias body (`type A = { a: number }`) onto one
    /// `TypeId`, so the reverse lookup cannot tell whether the source actually
    /// referenced the alias. tsc spells the alias name only when the reference's
    /// `aliasSymbol` is set; an inline / anonymous annotation carries none and is
    /// rendered structurally. Callers that know the operand came from an inline
    /// (non-reference) composite annotation set this flag so the shape renders
    /// structurally instead of being repainted with the unrelated alias name.
    ///
    /// Nominal shapes are unaffected: interfaces / classes resolve through their
    /// shape symbol stamp, and generic applications keep their `Name<Args>`
    /// surface — only the unsound non-generic-type-alias redirect is suppressed.
    anonymous_composite_structural: bool,
    /// When true, preserve a longer generic alias prefix while eliding nested
    /// structural object branches. Used for long property receiver diagnostics.
    long_property_receiver_display: bool,
    long_property_receiver_object_elision_end_depth: u32,
    /// When true, generic mapped type aliases that evaluate to scalar types are
    /// displayed as their evaluated result. Used for assignability diagnostics.
    expand_scalar_mapped_alias_applications: bool,
    /// When true, the canonical primitive key union (`string | number | symbol`,
    /// shared by `keyof any` and the lib.d.ts alias `PropertyKey`) is rendered
    /// in its structural form even in diagnostic mode. tsc strips the
    /// `aliasSymbol` from the constraint type before formatting TS2344 messages
    /// (`Type 'X' does not satisfy the constraint 'string | number | symbol'`)
    /// while still keeping `PropertyKey` in other diagnostics. The default is
    /// false to preserve the existing behavior across every other surface.
    expand_primitive_key_union: bool,
    /// When true, render union members in canonical interner order even when a
    /// source/display origin was recorded. This is used by narrow diagnostic
    /// surfaces where tsc does not preserve source-written union order.
    ignore_union_origins: bool,
    /// Per-formatter memo for the `Application` display-reduction verdict,
    /// keyed on the `Application` `TypeId`. See
    /// [`Self::application_display_reduction`] for the strategy cascade and
    /// the DAG-walk rationale (#13480).
    application_reduction_cache: std::cell::RefCell<
        FxHashMap<TypeId, Option<application_reduction::ApplicationDisplayReduction>>,
    >,
    /// Per-formatter memo for `is_recursive_type_alias_application_base`, keyed
    /// on the application *base* `TypeId`. The predicate runs an uncached
    /// recursive `type_reaches_alias_def` walk over the alias body; the same
    /// base recurs across the shared `Application` DAG, so memoizing the
    /// boolean verdict removes the repeated graph walk. Same purity and
    /// lifetime contract as `application_reduction_cache`.
    recursive_alias_base_cache: std::cell::RefCell<FxHashMap<TypeId, bool>>,
    /// Remaining `format` node budget for bounded display walks.
    ///
    /// `format` walks the type as a *tree*; deeply nested generic receiver
    /// types (e.g. drizzle-orm's relational builders behind a TS2339/TS2322
    /// receiver) are DAGs whose shared subtrees re-expand combinatorially under
    /// the relaxed `max_depth` used by the long-property-receiver display path,
    /// so a single diagnostic can build a multi-megabyte string (#13480). The
    /// rendered message is truncated for display anyway, so producing the full
    /// string is wasted work.
    ///
    /// When `Some(n)`, each `format` call spends one unit; once the budget
    /// reaches zero the walk emits the same `...` / `{ ...; }` elision it
    /// already uses at `max_depth` and stops descending. The budget is a
    /// deterministic global bound on total walk size (independent of depth and
    /// fan-out), set only for the long-receiver display path so normal
    /// diagnostics — which spend orders of magnitude fewer nodes — render
    /// identically. `None` (the default) leaves formatting unbounded for every
    /// other surface. tsc likewise caps diagnostic type display length.
    format_node_budget: Option<std::cell::Cell<u32>>,
}

/// Default total-`format`-node budget for the long-property-receiver display
/// path. Calibrated well above the node count any conformance receiver type
/// needs to render fully (deep mapped/conditional chains, wide unions) yet far
/// below the millions of redundant nodes a shared-DAG receiver re-expands into.
/// Retune only with a witness on both the perf row and the conformance corpus.
pub(crate) const LONG_RECEIVER_FORMAT_NODE_BUDGET: u32 = 200_000;

impl<'a> TypeFormatter<'a> {
    pub(super) fn is_recursive_type_alias_application_base(&self, base: TypeId) -> bool {
        if let Some(&cached) = self.recursive_alias_base_cache.borrow().get(&base) {
            return cached;
        }
        let result = self.compute_is_recursive_type_alias_application_base(base);
        self.recursive_alias_base_cache
            .borrow_mut()
            .insert(base, result);
        result
    }

    fn compute_is_recursive_type_alias_application_base(&self, base: TypeId) -> bool {
        let Some(TypeData::Lazy(def_id)) = self.interner.lookup(base) else {
            return false;
        };
        let Some(def_store) = self.def_store else {
            return false;
        };
        let Some(def) = def_store.get(def_id) else {
            return false;
        };
        if def.kind != crate::def::DefKind::TypeAlias {
            return false;
        }
        let Some(body) = def.body else {
            return false;
        };
        let mut visited = FxHashSet::default();
        self.type_reaches_alias_def(body, def_id, &mut visited)
    }

    fn type_reaches_alias_def(
        &self,
        type_id: TypeId,
        target_def_id: DefId,
        visited: &mut FxHashSet<TypeId>,
    ) -> bool {
        if type_id.is_intrinsic() || !visited.insert(type_id) {
            return false;
        }
        if matches!(self.interner.lookup(type_id), Some(TypeData::Lazy(def_id)) if def_id == target_def_id)
        {
            return true;
        }
        if let Some(TypeData::Application(app_id)) = self.interner.lookup(type_id) {
            let app = self.interner.type_application(app_id);
            if matches!(self.interner.lookup(app.base), Some(TypeData::Lazy(def_id)) if def_id == target_def_id)
            {
                return true;
            }
        }
        let mut found = false;
        crate::visitor::for_each_child_by_id(self.interner, type_id, |child| {
            if !found {
                found = self.type_reaches_alias_def(child, target_def_id, visited);
            }
        });
        found
    }

    fn is_primitive_key_union_data(&self, key: &TypeData) -> bool {
        let TypeData::Union(list_id) = key else {
            return false;
        };
        let members = self.interner.type_list(*list_id);
        members.len() == 3
            && members.contains(&TypeId::STRING)
            && members.contains(&TypeId::NUMBER)
            && members.contains(&TypeId::SYMBOL)
    }

    /// True when `key` is a `Union` whose members are all unit types: literal
    /// values, enum members, or unique symbols. Such a union is exactly what a
    /// user can spell directly as an annotation (`"a" | "b"`, `0 | 1`), so it
    /// must be rendered by its members rather than redirected to a shared
    /// `keyof X` display alias that would repaint unrelated annotations.
    fn union_is_all_unit_literals(&self, key: &TypeData) -> bool {
        let TypeData::Union(list_id) = key else {
            return false;
        };
        let members = self.interner.type_list(*list_id);
        !members.is_empty()
            && members.iter().all(|&m| {
                matches!(
                    self.interner.lookup(m),
                    Some(TypeData::Literal(_) | TypeData::Enum(_, _) | TypeData::UniqueSymbol(_))
                )
            })
    }

    /// For Application-arg display: when the arg is an `IndexAccess(obj, idx)`
    /// whose `obj` is fully concrete (no type parameters, no infer
    /// placeholders) and `idx` is a literal, resolve the indexed access for
    /// display. tsc unfolds these — `View<TypeA["bar"]>` is shown as
    /// `View<TypeB>` — because the concrete index is just an indirection
    /// over the resolved property type.
    ///
    /// Returns the original `arg` for any other shape (deferred `IndexAccess`
    /// over a type parameter, non-literal index, etc.) so generic and
    /// deferred types continue to print verbatim.
    fn resolve_concrete_index_access_for_display(&self, arg: TypeId) -> TypeId {
        let Some(TypeData::IndexAccess(obj, idx)) = self.interner.lookup(arg) else {
            return arg;
        };
        if crate::type_queries::contains_type_parameters_db(self.interner, obj)
            || crate::type_queries::contains_type_parameters_db(self.interner, idx)
        {
            return arg;
        }
        // Idx must be a display-reducible key shape for tsc's unfold — a
        // generic key would also be deferred even when the obj is concrete.
        if !crate::type_queries::extended::is_display_reducible_index_key(self.interner, idx) {
            return arg;
        }
        // A chained access (`A["p"]["q"]`) nests one indexed access inside the
        // next, and the inner link's own object may be a reference. Reduce the
        // object operand to a fixed point first: either the whole chain
        // resolves, or this returns `arg` and the render path below prints it
        // as written. A partially reduced chain is never produced — it would
        // render an internal intermediate that corresponds to nothing the user
        // wrote and grows with nesting depth.
        let object_for_eval = self
            .materialize_reference_for_display(self.resolve_concrete_index_access_for_display(obj));
        let resolved =
            crate::evaluation::evaluate::evaluate_index_access(self.interner, object_for_eval, idx);
        if resolved == arg || resolved == TypeId::ERROR {
            return arg;
        }
        resolved
    }

    /// A concrete (type-parameter-free) mapped type whose key constraint is a
    /// finite set of literal keys — `{ [K in Color]: number }`,
    /// `{ [K in "a" | "b"]: number }` — is resolved by `tsc` to its member
    /// object (`{ green: number; red: number; }`) for display, exactly as if
    /// the members had been written out. tsz keeps the `Mapped` node live for
    /// semantic identity (#15392), so the printer resolves it here instead.
    ///
    /// Returns the resolved object only when evaluation produces a plain
    /// named-property object carrying no free type parameters and no index
    /// signature. A generic mapped (`{ [K in keyof T]: T[K] }`) stays deferred
    /// and a `string`/`number`/`symbol`-constrained mapped is an index
    /// signature (`{ [x: string]: T }`, owned by `format_mapped`); both keep
    /// their `{ [K in ...]: ... }` source form by returning `type_id`.
    fn resolve_concrete_mapped_for_display(&self, type_id: TypeId) -> TypeId {
        let Some(TypeData::Mapped(mapped_id)) = self.interner.lookup(type_id) else {
            return type_id;
        };
        let mapped = self.interner.mapped_type(mapped_id);
        // A generic key constraint (`keyof T`, `AB[S]`) can never reduce to a
        // concrete member object, so it prints as written.
        if crate::type_queries::contains_type_parameters_db(self.interner, mapped.constraint) {
            return type_id;
        }
        // An enum / aliased-union constraint hides its keys behind a
        // `Lazy(DefId)`; resolve one hop through the store — a direct body
        // lookup, NOT an `evaluate`, so it registers no `store_display_alias`
        // side effect the way a whole-mapped evaluation would.
        let mut eff_constraint = mapped.constraint;
        if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(mapped.constraint)
            && let Some(def_store) = self.def_store
        {
            use crate::def::resolver::TypeResolver;
            let resolver = crate::caches::query_cache_evaluation::StoreOnlyResolver::new(def_store);
            if let Some(body) = resolver.resolve_lazy(def_id, self.interner) {
                eff_constraint = body;
            }
        }
        // Only a finite literal-key constraint (a literal/enum/unique-symbol
        // union, or a single such key) both reduces to a member object AND is
        // safe to hand to the key collector. A `string`/`number`/`symbol`
        // constraint is an index signature owned by `format_mapped`; an
        // index-access / application / conditional constraint would trigger a
        // side-effectful `evaluate` inside the collector. Either keeps the
        // source / index-signature form via the fall-through below.
        if !self.is_finite_literal_key_constraint(eff_constraint) {
            return type_id;
        }
        // Build the member object from the finite key set, mirroring
        // `DiagnosticBuilder::materialize_finite_mapped_type_for_display` — a
        // key query plus per-property template resolution, over a mapped
        // carrying the resolved constraint.
        let eff_mapped_id = if eff_constraint == mapped.constraint {
            mapped_id
        } else {
            let rebuilt = self.interner.mapped(crate::types::MappedType {
                constraint: eff_constraint,
                ..*mapped
            });
            match self.interner.lookup(rebuilt) {
                Some(TypeData::Mapped(id)) => id,
                _ => return type_id,
            }
        };
        let Some(names) =
            crate::type_queries::collect_finite_mapped_property_names(self.interner, eff_mapped_id)
        else {
            return type_id;
        };
        if names.is_empty() {
            return type_id;
        }
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort_by(|a, b| {
            self.interner
                .resolve_atom_ref(*a)
                .cmp(&self.interner.resolve_atom_ref(*b))
        });
        let mut properties = Vec::with_capacity(names.len());
        for name in names {
            let property_name = self.interner.resolve_atom_ref(name).to_string();
            let Some(value_type) = crate::type_queries::get_finite_mapped_property_type(
                self.interner,
                eff_mapped_id,
                &property_name,
            ) else {
                return type_id;
            };
            // A free type parameter in the template (`{ [K in "a"]: T }`) keeps
            // the mapped generic; print it as written.
            if crate::type_queries::contains_type_parameters_db(self.interner, value_type) {
                return type_id;
            }
            let mut property = crate::PropertyInfo::new(name, value_type);
            property.optional = mapped.optional_modifier == Some(MappedModifier::Add);
            property.readonly = mapped.readonly_modifier == Some(MappedModifier::Add);
            properties.push(property);
        }
        self.interner.object(properties)
    }

    /// True when `constraint` is a finite set of literal keys — a single
    /// literal / enum member / unique symbol, or a union of them. Such a
    /// constraint is exactly what `resolve_concrete_mapped_for_display` can
    /// expand to a member object, and it is safe to hand to the finite-key
    /// collector without triggering a side-effectful `evaluate`.
    fn is_finite_literal_key_constraint(&self, constraint: TypeId) -> bool {
        match self.interner.lookup(constraint) {
            Some(TypeData::Literal(_) | TypeData::Enum(_, _) | TypeData::UniqueSymbol(_)) => true,
            Some(key @ TypeData::Union(_)) => self.union_is_all_unit_literals(&key),
            _ => false,
        }
    }

    /// A semantic reference (`Lazy(DefId)`, or an `Application` over one)
    /// carries no members of its own, so `evaluate_index_access` cannot reduce
    /// `Iface["m"]` while the object operand is still that reference. Swap in
    /// the definition's own body — instantiated with the written arguments when
    /// the reference is an application — so the evaluation has members to index.
    ///
    /// Display-only: the returned `TypeId` is never handed back to the caller,
    /// only used as the evaluation's object operand. A free type parameter
    /// anywhere in the access is rejected before this runs, so an instantiation
    /// here is always fully concrete.
    fn materialize_reference_for_display(&self, obj: TypeId) -> TypeId {
        if self.render_index_access_object_as_written {
            return obj;
        }
        let Some(def_store) = self.def_store else {
            return obj;
        };
        let materialized = match self.interner.lookup(obj) {
            Some(TypeData::Lazy(def_id)) => def_store.get(def_id).and_then(|def| {
                // A bare reference to a generic definition has no arguments to
                // substitute, so its body still mentions the type parameters
                // and the access stays deferred — as tsc renders it.
                def.type_params.is_empty().then_some(def.body).flatten()
            }),
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base) else {
                    return obj;
                };
                def_store.get(def_id).and_then(|def| {
                    def.body.map(|body| {
                        crate::computation::instantiate_generic(
                            self.interner,
                            body,
                            &def.type_params,
                            &app.args,
                        )
                    })
                })
            }
            _ => return obj,
        };
        match materialized {
            Some(body) if body != obj => body,
            _ => obj,
        }
    }

    /// If `obj` is a homomorphic identity mapped type
    /// (`{ [P in keyof X]: X[P] }`, with optional/readonly modifier variants)
    /// then `obj[idx]` displays as `X[idx]`, plus `| undefined` when the
    /// mapped's optional modifier is `+`. Returns `None` for any other
    /// mapped shape so non-homomorphic mapped types continue to print
    /// with their full structural form.
    ///
    /// This mirrors tsc's display: `Partial<U>[K]` shows as `U[K] | undefined`,
    /// `Readonly<U>[K]` shows as `U[K]`, regardless of the user-chosen
    /// iteration variable name in the alias body.
    fn try_format_homomorphic_mapped_index_access(
        &mut self,
        obj: TypeId,
        idx: TypeId,
    ) -> Option<String> {
        let mapped_id = match self.interner.lookup(obj) {
            Some(TypeData::Mapped(id)) => id,
            Some(TypeData::Application(app_id)) => {
                let app = self.interner.type_application(app_id);
                let Some(TypeData::Lazy(def_id)) = self.interner.lookup(app.base) else {
                    return None;
                };
                let def_store = self.def_store?;
                let def = def_store.get(def_id)?;
                let body = def.body?;
                let instantiated = crate::computation::instantiate_generic(
                    self.interner,
                    body,
                    &def.type_params,
                    &app.args,
                );
                match self.interner.lookup(instantiated) {
                    Some(TypeData::Mapped(id)) => id,
                    _ => return None,
                }
            }
            _ => return None,
        };
        let mapped = self.interner.mapped_type(mapped_id);

        // `as` clauses (name remapping) change the key relationship; bail.
        if mapped.name_type.is_some() {
            return None;
        }

        // Constraint must be `keyof <source>`.
        let source = match self.interner.lookup(mapped.constraint) {
            Some(TypeData::KeyOf(operand)) => operand,
            _ => return None,
        };

        // Template body must be `IndexAccess(source, P)` where P is the
        // mapped's own iteration parameter — i.e. the homomorphic-identity
        // shape that tsc treats as `Partial<source>` / `Readonly<source>`.
        let (template_obj, template_idx) = match self.interner.lookup(mapped.template) {
            Some(TypeData::IndexAccess(o, i)) => (o, i),
            _ => return None,
        };
        if template_obj != source {
            return None;
        }
        match self.interner.lookup(template_idx) {
            Some(TypeData::TypeParameter(tp)) if tp.name == mapped.type_param.name => {}
            _ => return None,
        }

        // Format `source[idx]`, parenthesizing source only when needed for
        // unions / intersections that actually render with operators.
        let source_str = self.format(source);
        let needs_parens = matches!(
            self.interner.lookup(source),
            Some(TypeData::Union(_) | TypeData::Intersection(_))
        ) && (source_str.contains(" & ") || source_str.contains(" | "));
        let idx_for_display = match self.interner.lookup(idx) {
            Some(TypeData::TypeParameter(tp)) if tp.name == mapped.type_param.name => {
                mapped.constraint
            }
            _ => idx,
        };
        let idx_str = self.format(idx_for_display);
        let core = if needs_parens {
            format!("({source_str})[{idx_str}]")
        } else {
            format!("{source_str}[{idx_str}]")
        };

        // Optional + adds `| undefined`; readonly is a property-level
        // modifier and does not change the value type at an index access.
        if mapped.optional_modifier == Some(MappedModifier::Add) {
            Some(format!("{core} | undefined"))
        } else {
            Some(core)
        }
    }

    pub fn new(interner: &'a dyn TypeDatabase) -> Self {
        TypeFormatter {
            interner,
            symbol_arena: None,
            def_store: None,
            module_specifiers: None,
            module_path_specifiers: None,
            namespace_module_names: None,
            current_file_id: None,
            max_depth: 8,
            max_union_members: 10,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
            skip_union_optionalize: false,
            diagnostic_mode: false,
            preserve_optional_property_surface_syntax: false,
            preserve_optional_parameter_surface_syntax: true,
            use_display_properties: false,
            application_arg_display_depth: 0,
            display_alias_visiting: FxHashSet::default(),
            format_visiting: FxHashSet::default(),
            preserve_array_generic_form: false,
            skip_application_alias_names: false,
            skip_application_display_alias_chase: false,
            preserve_application_arg_index_alias_surface: false,
            render_index_access_object_as_written: false,
            skip_type_alias_def_ids: FxHashSet::default(),
            skipped_type_alias_expansion_visiting: FxHashSet::default(),
            builtin_iterator_return_type: None,
            skip_intersection_display_alias: false,
            skip_application_alias_for_intersections: false,
            capitalize_primitive_intersection_members: false,
            skip_object_display_alias: false,
            anonymous_composite_structural: false,
            long_property_receiver_display: false,
            long_property_receiver_object_elision_end_depth: 26,
            expand_scalar_mapped_alias_applications: false,
            expand_primitive_key_union: false,
            ignore_union_origins: false,
            application_reduction_cache: std::cell::RefCell::new(FxHashMap::default()),
            recursive_alias_base_cache: std::cell::RefCell::new(FxHashMap::default()),
            format_node_budget: None,
        }
    }

    /// Create a formatter with access to symbol names.
    pub fn with_symbols(
        interner: &'a dyn TypeDatabase,
        symbol_arena: &'a tsz_binder::SymbolArena,
    ) -> Self {
        TypeFormatter {
            interner,
            symbol_arena: Some(symbol_arena),
            def_store: None,
            module_specifiers: None,
            module_path_specifiers: None,
            namespace_module_names: None,
            current_file_id: None,
            max_depth: 8,
            max_union_members: 10,
            current_depth: 0,
            atom_cache: FxHashMap::default(),
            skip_union_optionalize: false,
            diagnostic_mode: false,
            preserve_optional_property_surface_syntax: false,
            preserve_optional_parameter_surface_syntax: true,
            use_display_properties: false,
            application_arg_display_depth: 0,
            display_alias_visiting: FxHashSet::default(),
            format_visiting: FxHashSet::default(),
            preserve_array_generic_form: false,
            skip_application_alias_names: false,
            skip_application_display_alias_chase: false,
            preserve_application_arg_index_alias_surface: false,
            render_index_access_object_as_written: false,
            skip_type_alias_def_ids: FxHashSet::default(),
            skipped_type_alias_expansion_visiting: FxHashSet::default(),
            builtin_iterator_return_type: None,
            skip_intersection_display_alias: false,
            skip_application_alias_for_intersections: false,
            capitalize_primitive_intersection_members: false,
            skip_object_display_alias: false,
            anonymous_composite_structural: false,
            long_property_receiver_display: false,
            long_property_receiver_object_elision_end_depth: 26,
            expand_scalar_mapped_alias_applications: false,
            expand_primitive_key_union: false,
            ignore_union_origins: false,
            application_reduction_cache: std::cell::RefCell::new(FxHashMap::default()),
            recursive_alias_base_cache: std::cell::RefCell::new(FxHashMap::default()),
            format_node_budget: None,
        }
    }

    /// Add access to definition store for `DefId` name resolution.
    pub const fn with_def_store(mut self, def_store: &'a DefinitionStore) -> Self {
        self.def_store = Some(def_store);
        self
    }

    /// Add module specifier map for import-qualified type display.
    pub const fn with_module_specifiers(
        mut self,
        module_specifiers: &'a FxHashMap<u32, String>,
    ) -> Self {
        self.module_specifiers = Some(module_specifiers);
        self
    }

    /// Add full-path module specifier map used by diagnostic cross-module
    /// disambiguation. Separate from `with_module_specifiers` because the
    /// existing map preserves the basename shape expected by declaration
    /// emit / JS export tracking.
    pub const fn with_module_path_specifiers(
        mut self,
        module_path_specifiers: &'a FxHashMap<u32, String>,
    ) -> Self {
        self.module_path_specifiers = Some(module_path_specifiers);
        self
    }

    /// Add namespace module name mapping for displaying module namespace types
    /// as `typeof import("module")` instead of their object shape.
    pub const fn with_namespace_module_names(
        mut self,
        names: &'a FxHashMap<TypeId, String>,
    ) -> Self {
        self.namespace_module_names = Some(names);
        self
    }

    /// Set the `file_id` of the currently-checked file.
    pub const fn with_current_file_id(mut self, file_id: u32) -> Self {
        self.current_file_id = Some(file_id);
        self
    }

    /// Skip synthetic `?: undefined` member optionalization in union display.
    /// Should be set when formatting types for error messages (not hover/quickinfo).
    pub const fn with_diagnostic_mode(mut self) -> Self {
        self.skip_union_optionalize = true;
        self.diagnostic_mode = true;
        self
    }

    /// Render the canonical primitive key union (`string | number | symbol`)
    /// in its structural form rather than collapsing it to `PropertyKey`. tsc
    /// strips the `aliasSymbol` from the constraint type before formatting
    /// the TS2344 message; opt-in callers (the constraint-not-satisfied
    /// emitter) use this to mirror that surface without affecting any other
    /// diagnostic.
    pub const fn with_expanded_primitive_key_union(mut self) -> Self {
        self.expand_primitive_key_union = true;
        self
    }

    /// Render unions in canonical formatter order, ignoring any stored
    /// source/display origin for this formatter instance.
    pub const fn with_ignore_union_origins(mut self) -> Self {
        self.ignore_union_origins = true;
        self
    }

    /// Preserve optional parameter surface syntax when formatting type output.
    /// When false, optional params append `| undefined` unless already present.
    pub const fn with_preserve_optional_parameter_surface_syntax(mut self, preserve: bool) -> Self {
        self.preserve_optional_parameter_surface_syntax = preserve;
        self
    }

    /// Preserve optional property surface syntax when formatting type output.
    /// When false, optional properties append `| undefined` unless already present.
    pub const fn with_preserve_optional_property_surface_syntax(mut self, preserve: bool) -> Self {
        self.preserve_optional_property_surface_syntax = preserve;
        self
    }

    /// Preserve enough generic alias context for very long TS2339 receiver types
    /// while still eliding nested structural object branches.
    pub const fn with_long_property_receiver_display(mut self) -> Self {
        self.max_depth = 192;
        self.long_property_receiver_display = true;
        // Bound the total walk: the relaxed `max_depth` lets a shared-DAG
        // receiver re-expand combinatorially, so cap the number of `format`
        // nodes this display path may spend (#13480).
        self.format_node_budget = Some(std::cell::Cell::new(LONG_RECEIVER_FORMAT_NODE_BUDGET));
        self
    }

    pub const fn with_long_property_receiver_object_elision_end_depth(
        mut self,
        end_depth: u32,
    ) -> Self {
        self.long_property_receiver_object_elision_end_depth = end_depth;
        self
    }

    fn display_alias_application_base_is_type_alias(&self, alias_origin: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(alias_origin) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(def_store) = self.def_store else {
            return false;
        };

        let def_id = match self.interner.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            _ => def_store.find_def_for_type(app.base),
        };

        def_id
            .and_then(|def_id| def_store.get(def_id))
            .is_some_and(|def| def.kind == crate::def::DefKind::TypeAlias)
    }

    /// Whether `alias_origin` is an application whose base resolves to an
    /// interface or class definition. Combined with the empty-object guard at
    /// the read site, this recognizes a marker instantiation: an application
    /// only records a display alias on the shared empty object `{}` when
    /// instantiating its base produced `{}`, so at this read the base
    /// contributed no members for these arguments — a pure marker such as the
    /// lib's `interface ThisType<T> {}`. `tsc` treats `ThisType` as a
    /// contextual-`this` marker and never renders a value as `ThisType<...>`;
    /// more generally it prints the shared empty object structurally as `{}`.
    /// Following the alias here would repaint every empty object in the file
    /// with the marker's name — the `Object.defineProperty`
    /// `PropertyDescriptor & ThisType<any>` witness. Genuinely named generic
    /// interfaces/classes whose instantiation is non-empty never intern to
    /// `{}` and never reach this read; those carrying a def registered against
    /// the `{}` `TypeId` are resolved earlier by the def-name path, which keeps
    /// their application display (e.g. `AsyncGenerator<number, void, unknown>`).
    fn display_alias_application_base_is_marker_interface(&self, alias_origin: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(alias_origin) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(def_store) = self.def_store else {
            return false;
        };

        let def_id = match self.interner.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            _ => def_store.find_def_for_type(app.base),
        };

        def_id
            .and_then(|def_id| def_store.get(def_id))
            .is_some_and(|def| {
                matches!(
                    def.kind,
                    crate::def::DefKind::Interface | crate::def::DefKind::Class
                )
            })
    }

    fn display_alias_application_base_has_conditional_body(&self, alias_origin: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(alias_origin) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(def_store) = self.def_store else {
            return false;
        };

        let def_id = match self.interner.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => Some(def_id),
            _ => def_store.find_def_for_type(app.base),
        };

        def_id
            .and_then(|def_id| def_store.get(def_id))
            .and_then(|def| def.body)
            .is_some_and(|body| {
                matches!(self.interner.lookup(body), Some(TypeData::Conditional(_)))
            })
    }

    /// Skip type alias names for aliases whose body is a generic Application.
    /// Used in assignability messages where tsc shows the Application form.
    pub const fn with_skip_application_alias_names(mut self) -> Self {
        self.skip_application_alias_names = true;
        self
    }

    /// Do not follow display aliases whose origin is an Application.
    /// Used when a diagnostic has already selected the application spelling it
    /// wants to show and formatter-level provenance would repaint it as a
    /// wrapper alias.
    pub const fn with_skip_application_display_alias_chase(mut self) -> Self {
        self.skip_application_display_alias_chase = true;
        self
    }

    /// Skip one specific type alias name and display its evaluated body instead.
    pub fn with_skip_type_alias_def_id(mut self, def_id: DefId) -> Self {
        self.skip_type_alias_def_ids.insert(def_id);
        self
    }

    /// Don't follow `display_alias` when it points to an Intersection type
    /// and the current type is an Object. tsc shows the merged object form
    /// in TS2741 messages, not the intersection form.
    pub const fn with_skip_intersection_display_alias(mut self) -> Self {
        self.skip_intersection_display_alias = true;
        self
    }

    /// Don't follow `display_alias` when the current type is an Intersection
    /// and the alias points to an Application. tsc shows the structural
    /// `Number & { __brand: T }` form instead of the branded alias `Brand<T>`.
    pub const fn with_skip_application_alias_for_intersections(mut self) -> Self {
        self.skip_application_alias_for_intersections = true;
        self
    }

    /// Show capitalized primitive names (`Number`, `String`, `Boolean`) for
    /// primitive members of intersection types, matching tsc's apparent-type
    /// display for branded primitives in error messages.
    pub const fn with_capitalize_primitive_intersection_members(mut self) -> Self {
        self.capitalize_primitive_intersection_members = true;
        self
    }

    /// Don't follow `display_alias` when the current type is an `Object` or
    /// `ObjectWithIndex`. Used for diagnostics where the structural literal
    /// shape is the desired display, even if the type system recorded an
    /// alias to a named symbol (e.g. a JS constructor's `prototype` property).
    pub const fn with_skip_object_display_alias(mut self) -> Self {
        self.skip_object_display_alias = true;
        self
    }

    /// Render composites structurally instead of repainting them with a
    /// coincidental non-generic type-alias name reached through reverse def/shape
    /// lookup. See [`Self::anonymous_composite_structural`].
    pub const fn with_anonymous_composite_structural(mut self) -> Self {
        self.anonymous_composite_structural = true;
        self
    }

    /// Configure strict null checks mode.
    /// When strictNullChecks is off, optional properties should not display
    /// `| undefined` since undefined is implicit in all types.
    pub const fn with_strict_null_checks(mut self, strict: bool) -> Self {
        if !strict {
            self.preserve_optional_property_surface_syntax = true;
            self.preserve_optional_parameter_surface_syntax = true;
        }
        self
    }

    /// Replace diagnostic display of the compiler-internal lib alias
    /// `BuiltinIteratorReturn` with the option-selected concrete type.
    pub const fn with_builtin_iterator_return_type(mut self, ty: TypeId) -> Self {
        self.builtin_iterator_return_type = Some(ty);
        self
    }

    /// Configure exactOptionalPropertyTypes mode.
    /// When enabled, optional properties (`foo?: T`) do NOT implicitly include
    /// `undefined` in their declared type, so diagnostic messages must display
    /// them as `foo?: T` rather than `foo?: T | undefined`.
    pub const fn with_exact_optional_property_types(mut self, exact: bool) -> Self {
        if exact {
            self.preserve_optional_property_surface_syntax = true;
        }
        self
    }

    /// Enable display properties for fresh object literal types.
    /// When enabled, the formatter uses pre-widened literal types from the
    /// freshness model side table for error messages.
    pub const fn with_display_properties(mut self) -> Self {
        self.use_display_properties = true;
        self
    }

    /// Expand mapped type aliases like `{ [K in keyof T]: T[K] }` when a
    /// concrete instantiation reduces to a scalar type. This is intentionally
    /// opt-in for error-message contexts that need tsc's assignability surface.
    pub const fn with_expand_scalar_mapped_alias_applications(mut self) -> Self {
        self.expand_scalar_mapped_alias_applications = true;
        self
    }

    fn format_skipped_type_alias_body(&mut self, def_id: DefId, body: TypeId) -> Cow<'static, str> {
        if !self.skipped_type_alias_expansion_visiting.insert(def_id) {
            return Cow::Borrowed("...");
        }

        let body_was_visiting = self.format_visiting.remove(&body);
        let formatted = self.format(body);
        if body_was_visiting {
            self.format_visiting.insert(body);
        }

        self.skipped_type_alias_expansion_visiting.remove(&def_id);
        formatted
    }

    fn skipped_type_alias_body_by_name(&self, name: Atom) -> Option<(DefId, TypeId)> {
        let def_store = self.def_store?;
        def_store
            .find_defs_by_name(name)?
            .into_iter()
            .find_map(|def_id| {
                if !self.skip_type_alias_def_ids.contains(&def_id) {
                    return None;
                }
                let def = def_store.get(def_id)?;
                if def.kind != crate::def::DefKind::TypeAlias {
                    return None;
                }
                Some((def_id, def.body?))
            })
    }

    fn atom(&mut self, atom: Atom) -> Arc<str> {
        if let Some(value) = self.atom_cache.get(&atom) {
            return std::sync::Arc::clone(value);
        }
        let resolved = self.interner.resolve_atom_ref(atom);
        self.atom_cache
            .insert(atom, std::sync::Arc::clone(&resolved));
        resolved
    }

    /// Render a pending diagnostic to a complete diagnostic with formatted message.
    ///
    /// This is where the lazy evaluation happens - we format types to strings
    /// only when the diagnostic is actually going to be displayed.
    pub fn render(&mut self, pending: &PendingDiagnostic) -> TypeDiagnostic {
        let template = get_message_template(pending.code);
        let message = self.render_template(template, &pending.args);

        let mut diag = TypeDiagnostic {
            message,
            code: pending.code,
            severity: pending.severity,
            span: pending.span.clone(),
            related: Vec::new(),
        };

        // Render related diagnostics, falling back to the primary span.
        // Recursively walk the elaboration tree so nested chains built via
        // `with_related` (e.g. `PropertyTypeMismatch { nested_reason: ... }`)
        // surface every level instead of being silently truncated after the
        // first. Depth carries the nesting level so the reporter can render
        // tsc-style progressive 2-space indentation.
        if !pending.related.is_empty() {
            let fallback_span = pending
                .span
                .clone()
                .unwrap_or_else(|| SourceSpan::new("<unknown>", 0, 0));
            for related in &pending.related {
                self.render_related_chain(related, 0, &fallback_span, &mut diag.related);
            }
        }

        diag
    }

    /// Recursively flatten a nested `PendingDiagnostic` elaboration chain into
    /// `RelatedInformation` entries with monotonically increasing depth so the
    /// reporter can render tsc-style progressive indentation.
    fn render_related_chain(
        &mut self,
        pending: &PendingDiagnostic,
        depth: u8,
        fallback_span: &SourceSpan,
        out: &mut Vec<RelatedInformation>,
    ) {
        let message = self.render_template(get_message_template(pending.code), &pending.args);
        let span = pending.span.as_ref().unwrap_or(fallback_span).clone();
        out.push(RelatedInformation {
            span,
            message,
            depth,
        });
        let next_depth = depth.saturating_add(1);
        for child in &pending.related {
            self.render_related_chain(child, next_depth, fallback_span, out);
        }
    }

    /// Render a message template with arguments.
    fn render_template(&mut self, template: &str, args: &[DiagnosticArg]) -> String {
        let mut result = template.to_string();

        for (i, arg) in args.iter().enumerate() {
            let placeholder = format!("{{{i}}}");
            if !template.contains(&placeholder) {
                continue;
            }
            let replacement: Cow<'_, str> = match arg {
                DiagnosticArg::Type(type_id) => self.format(*type_id),
                DiagnosticArg::Symbol(sym_id) => {
                    if let Some(name) = self.format_symbol_name(*sym_id) {
                        Cow::Owned(name)
                    } else {
                        Cow::Owned(format!("Symbol({})", sym_id.0))
                    }
                }
                DiagnosticArg::Atom(atom) => Cow::Owned(self.atom(*atom).to_string()),
                DiagnosticArg::String(s) => Cow::Owned(s.to_string()),
                DiagnosticArg::Number(n) => Cow::Owned(n.to_string()),
            };
            result = result.replace(&placeholder, &replacement);
        }

        result
    }

    /// Total-walk budget for the long-property-receiver display path. Once
    /// exhausted, elide the remaining subtree exactly as the `max_depth`
    /// limit does (objects as `{ ...; }`, everything else as `...`). This
    /// bounds a shared-DAG receiver's combinatorial re-expansion to O(budget)
    /// nodes regardless of depth or fan-out (#13480). The budget is refilled
    /// at every top-level entry (`current_depth == 0`) so each independently
    /// formatted type display — e.g. each diagnostic argument — gets the full
    /// allowance and one large type cannot starve the next. Decrement before
    /// descending; returns `None` (continue formatting) when no budget is set.
    fn spend_format_node_budget(&self, type_key: Option<&TypeData>) -> Option<Cow<'static, str>> {
        let budget = self.format_node_budget.as_ref()?;
        if self.current_depth == 0 {
            budget.set(LONG_RECEIVER_FORMAT_NODE_BUDGET);
        }
        let remaining = budget.get();
        if remaining == 0 {
            if matches!(
                type_key,
                Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
            ) {
                return Some(Cow::Borrowed("{ ...; }"));
            }
            return Some(Cow::Borrowed("..."));
        }
        budget.set(remaining - 1);
        None
    }

    /// Format a type as a human-readable string.
    ///
    /// Returns `Cow::Borrowed` for static type names (e.g., `"never"`, `"any"`)
    /// and `Cow::Owned` for dynamically formatted types.
    pub fn format(&mut self, type_id: TypeId) -> Cow<'static, str> {
        if self.format_visiting.contains(&type_id) {
            return Cow::Borrowed("...");
        }
        let type_key = self.interner.lookup(type_id);
        if let Some(elided) = self.spend_format_node_budget(type_key.as_ref()) {
            return elided;
        }
        if self.long_property_receiver_display
            && (8..=self.long_property_receiver_object_elision_end_depth)
                .contains(&self.current_depth)
            && matches!(
                type_key,
                Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
            )
            && self.interner.get_display_alias(type_id).is_none()
        {
            return Cow::Borrowed("{ ...; }");
        }
        if self.current_depth >= self.max_depth {
            // tsc elides deep object branches as `{ ...; }` rather than raw `...`.
            if matches!(
                type_key,
                Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
            ) {
                return Cow::Borrowed("{ ...; }");
            }
            return Cow::Borrowed("...");
        }

        // Handle intrinsic types
        match type_id {
            TypeId::NEVER => return Cow::Borrowed("never"),
            TypeId::UNKNOWN => return Cow::Borrowed("unknown"),
            // The error-type sentinel keeps its distinct `TypeId::ERROR`
            // identity (any-poisoning prevention), but tsc's printer always
            // renders `errorType` as `any` — never the internal `error`
            // spelling. Render it identically to `any` so a failed-resolution
            // type never leaks the token `error` into a user-facing diagnostic.
            TypeId::ANY | TypeId::ERROR => return Cow::Borrowed("any"),
            TypeId::VOID => return Cow::Borrowed("void"),
            TypeId::UNDEFINED => return Cow::Borrowed("undefined"),
            TypeId::NULL => return Cow::Borrowed("null"),
            TypeId::BOOLEAN => return Cow::Borrowed("boolean"),
            TypeId::NUMBER => return Cow::Borrowed("number"),
            TypeId::STRING => return Cow::Borrowed("string"),
            TypeId::BIGINT => return Cow::Borrowed("bigint"),
            TypeId::SYMBOL => return Cow::Borrowed("symbol"),
            TypeId::OBJECT => return Cow::Borrowed("object"),
            TypeId::FUNCTION => return Cow::Borrowed("Function"),
            _ => {}
        }

        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return format!("Type({})", type_id.0).into(),
        };

        // Detect the empty object shape `{}`. It is a universally-shared
        // interning target: many generic reductions (e.g., `T50<unknown>`
        // where `T50<T> = { [P in keyof T]: number }` reduces to `{}`
        // because `keyof unknown = never`) evaluate to the same TypeId as a
        // literal `{}` annotation. For such types, we must not follow a
        // type-alias def-name redirect, because tsc shows the literal `{}`
        // (not the alias name) when the alias body reduces to `{}`. This
        // flag is consumed by the `skip_alias` heuristic below.
        let is_empty_object = matches!(
            &key,
            TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)
                if {
                    let shape = self.interner.object_shape(*shape_id);
                    shape.properties.is_empty()
                        && shape.string_index.is_none()
                        && shape.number_index.is_none()
                }
        );
        // A truly anonymous empty object (a user-written `{}` annotation)
        // has no symbol stamp on its shape. Class instance types whose
        // bodies happen to be empty (e.g., `class B<T> { constructor() {} }`)
        // keep their shape symbol and remain distinguishable, so they may
        // still use the def-name path with type params (`B<T>`). The
        // generic-interface/class skip below gates on this distinction to
        // avoid repainting bare `{}` annotations as unrelated def names
        // without losing class identity for empty-body classes.
        let is_empty_anonymous_object = is_empty_object
            && matches!(
                &key,
                TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)
                    if self.interner.object_shape(*shape_id).symbol.is_none()
            );

        // Named composite types can display by definition when the TypeId itself
        // carries enough provenance (interfaces, classes, referenced aliases).
        // Do not use body-shape lookup: tsc only shows aliases that were directly
        // referenced, while `display_alias` handles evaluated-type provenance.
        // Restricted to composite shapes to avoid false positives where a primitive
        // or literal type coincidentally matches an alias body (e.g. `type U = 1`).
        // Object types are content-interned, so a hand-written literal annotation
        // (`{ a: number }`) shares its `TypeId` with any utility/type-alias body of
        // the same shape; `find_def_for_type` (and the `display_alias` chase below)
        // would repaint it with that unrelated name (`P`, `Pick<...>`). `tsc` never
        // stamps an alias symbol on a literal annotation or a `skip_object_display_alias`
        // receiver (e.g. a JS constructor `prototype`), so force the structural shape.
        let key_is_object = matches!(&key, TypeData::Object(_) | TypeData::ObjectWithIndex(_));
        let is_literal_object_annotation =
            key_is_object && self.interner.is_literal_object_annotation(type_id);
        let skip_object_def_lookup =
            (self.skip_object_display_alias || is_literal_object_annotation) && key_is_object;
        let key_is_composite_for_def_lookup = matches!(
            &key,
            TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Union(_)
                | TypeData::Intersection(_)
                | TypeData::Tuple(_)
                | TypeData::Callable(_)
                | TypeData::Function(_)
                | TypeData::Mapped(_)
                | TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _)
        );
        if !skip_object_def_lookup
            && key_is_composite_for_def_lookup
            && let Some(def_store) = self.def_store
            && let Some(def_id) = def_store.find_def_for_type(type_id)
            && let Some(def) = def_store.get(def_id)
        {
            // Skip type aliases whose body was computed by intersection
            // reduction or conditional evaluation. tsc shows the expanded
            // form for these types, not the alias name.
            use crate::def::DefKind;
            let shape_is_non_empty = |shape: &ObjectShape| -> bool {
                !shape.properties.is_empty()
                    || shape.string_index.is_some()
                    || shape.number_index.is_some()
            };
            let def_represents_non_empty_object = def
                .instance_shape
                .as_ref()
                .is_some_and(|s| shape_is_non_empty(s.as_ref()))
                || def
                    .static_shape
                    .as_ref()
                    .is_some_and(|s| shape_is_non_empty(s.as_ref()));
            let def_kind_matches_type_shape = def.kind == DefKind::TypeAlias
                || matches!(
                    (&key, def.kind),
                    (
                        TypeData::Object(_) | TypeData::ObjectWithIndex(_),
                        DefKind::Interface
                            | DefKind::Class
                            | DefKind::Namespace
                            | DefKind::Enum
                            | DefKind::ClassConstructor
                            | DefKind::Function
                            | DefKind::Variable
                    ) | (
                        TypeData::Callable(_) | TypeData::Function(_),
                        DefKind::Interface
                            | DefKind::ClassConstructor
                            | DefKind::Function
                            | DefKind::Variable
                    ) | (TypeData::Enum(_, _), DefKind::Enum)
                );
            let unproven_primitive_key_union_alias =
                def.kind == DefKind::TypeAlias && self.is_primitive_key_union_data(&key);
            // An inline / anonymous composite annotation shares its interned
            // `TypeId` with a coincidentally-shaped non-generic type-alias body,
            // so the reverse `find_def_for_type` lookup cannot prove the source
            // referenced the alias. When the caller knows the operand came from an
            // anonymous composite annotation, render the structural shape instead
            // of the unrelated alias name (matching tsc's `aliasSymbol` policy).
            let anonymous_composite_structural_skip = self.anonymous_composite_structural
                && matches!(
                    &key,
                    TypeData::Object(_)
                        | TypeData::ObjectWithIndex(_)
                        | TypeData::Tuple(_)
                        | TypeData::Union(_)
                        | TypeData::Intersection(_)
                        // An inline function / constructor annotation
                        // (`(a: number) => void`, `new () => T`) likewise carries
                        // no `aliasSymbol`, so a coincidentally-shaped alias body
                        // must not repaint it (#17119).
                        | TypeData::Callable(_)
                        | TypeData::Function(_)
                );
            let skip_alias = if !def_kind_matches_type_shape {
                true
            } else if def.kind == DefKind::TypeAlias {
                anonymous_composite_structural_skip
                        || self.skip_type_alias_def_ids.contains(&def_id)
                        || def.body.is_some_and(|b| def_store.is_computed_body(b))
                        // A non-generic alias whose tuple body was built by
                        // flattening a fixed-tuple spread (`type T = [...[a, b], c]`)
                        // carries no `aliasSymbol` in tsc, so render the structural
                        // tuple (`[a, b, c]`), not `T`. Keyed per def: the flattened
                        // tuple interns to the same shape as a directly-written
                        // `type T = [a, b, c]`, which keeps its name.
                        || def_store.is_tuple_spread_flattened_alias(def_id)
                        || (!def.type_params.is_empty()
                            && def.body.is_some_and(|b| {
                                matches!(
                                    self.interner.lookup(b),
                                    Some(TypeData::IndexAccess(_, _) | TypeData::Conditional(_))
                                )
                            }))
                        || (self.skip_application_alias_names
                            && def.type_params.is_empty()
                            && self.interner.get_display_alias(type_id).is_some())
                        // A type alias whose body reduces to the empty object
                        // `{}` shares its TypeId with every literal `{}` in the
                        // program (`{}` is the universal empty-shape target of
                        // interning). Following the alias name here would
                        // repaint user-written `{}` annotations; tsc shows `{}`
                        // structurally in that case, so we do too.
                        || is_empty_object
                        // The canonical property-key union (`keyof any`) is a shared
                        // structural TypeId. Ambient or local aliases with the same
                        // body must not repaint constraint diagnostics.
                        || unproven_primitive_key_union_alias
            } else {
                // Interfaces and classes are also subject to the universal
                // empty-shape interning: a non-empty interface/class def
                // (e.g. `interface Promise<T> { then; catch; ... }`) may
                // have been registered against the canonical empty `{}`
                // TypeId during lib resolution. When the type we're
                // rendering is the truly anonymous empty `{}` (no shape
                // symbol stamp), do not repaint it as the unrelated def
                // name.
                //
                // Two skip cases (both gated on `is_empty_anonymous_object`):
                //   1. The def's recorded shape is itself non-empty.
                //   2. The def is generic (has type params) and has no
                //      `display_alias` provenance for this TypeId. The
                //      fall-through path would render `Promise<T>` from
                //      the bare type-param names, which is wrong: there
                //      is no concrete instantiation, just the universal
                //      `{}` shape that happens to share the TypeId.
                //
                // Empty interfaces (`interface I {}`) keep their name:
                // `def_represents_non_empty_object` is false for them and
                // they have no type params.
                //
                // Class instance types with empty bodies but a shape
                // symbol stamp (e.g., `class B<T> { constructor() {} }`)
                // keep `B<T>`: `is_empty_anonymous_object` is false
                // because the shape carries the class's symbol.
                is_empty_anonymous_object
                    && (def_represents_non_empty_object
                        || (!def.type_params.is_empty()
                            && self.interner.get_display_alias(type_id).is_none()))
            };
            if skip_alias {
                // Fall through to format the structural type
            } else {
                let name = self.format_def_name(&def);
                // Enum and namespace value types are displayed as `typeof Name` by tsc.
                // Class instance types and interfaces use just the name.
                // Exception: qualified enum member names like `W.a` are NOT prefixed
                // with `typeof` — only the enum container itself gets `typeof W`.
                // The `format_def_name` method qualifies names only with enum parents,
                // so a dot in the name reliably indicates an enum member reference.
                if matches!(
                    def.kind,
                    DefKind::Enum | DefKind::Namespace | DefKind::ClassConstructor
                ) {
                    if name.contains('.') {
                        return name.into();
                    }
                    return format!("typeof {name}").into();
                }
                // For generic types, prefer the display_alias (which has the actual
                // instantiated type arguments like `A<number>`) over appending raw
                // type parameter names from the definition (like `A<T>`).
                // The display_alias is set when an Application type is evaluated,
                // and preserves the concrete type arguments from the instantiation.
                let prefer_array_shorthand = name == "Array" && matches!(&key, TypeData::Array(_));
                if !def.type_params.is_empty() && !prefer_array_shorthand {
                    if let Some(alias_origin) = self.interner.get_display_alias(type_id)
                        && self.display_alias_visiting.insert(alias_origin)
                    {
                        let result = self.format(alias_origin);
                        self.display_alias_visiting.remove(&alias_origin);
                        return result;
                    }
                    // For Mapped types with generic params (e.g., Partial<T>,
                    // Record<K, V>), fall through to structural formatting.
                    // tsc shows the expanded mapped type form in error messages
                    // for these, not the alias name. The display_alias mechanism
                    // handles concrete instantiations (e.g., Partial<{a: string}>)
                    // via the check above.
                    if !matches!(&key, TypeData::Mapped(_)) {
                        let params: Vec<String> = def
                            .type_params
                            .iter()
                            .map(|tp| self.atom(tp.name).to_string())
                            .collect();
                        return format!("{}<{}>", name, params.join(", ")).into();
                    }
                    // Mapped type with generic params — fall through to structural display
                } else {
                    // For non-generic type aliases, check if the display_alias
                    // is a generic Application whose base type has a mapped type
                    // body. tsc shows `Id<{...}>` for `type Foo1 = Id<{...}>`
                    // (where Id is a mapped type), but preserves `Bar` for
                    // `type Bar = Omit<Foo, "c">` (where Omit is a type alias).
                    if def.kind == DefKind::TypeAlias
                        && let Some(alias_origin) = self.interner.get_display_alias(type_id)
                        && let Some(TypeData::Application(app_id)) =
                            self.interner.lookup(alias_origin)
                    {
                        let app = self.interner.type_application(app_id);
                        let base_has_mapped_body = if let Some(TypeData::Lazy(base_def_id)) =
                            self.interner.lookup(app.base)
                            && let Some(ds) = self.def_store
                            && let Some(base_def) = ds.get(base_def_id)
                            && let Some(body) = base_def.body
                        {
                            crate::visitors::visitor_predicates::is_mapped_type(self.interner, body)
                        } else {
                            false
                        };
                        if base_has_mapped_body && self.display_alias_visiting.insert(alias_origin)
                        {
                            let result = self.format(alias_origin);
                            self.display_alias_visiting.remove(&alias_origin);
                            return result;
                        }
                    }
                    // When a type resolves to a named definition (interface,
                    // class, or type alias), show that name. tsc preserves alias
                    // symbols: `type Bar = Omit<Foo, "c">` displays as "Bar".
                    return name.into();
                }
            }
        }

        // Check if this type was produced by evaluating an Application (e.g.,
        // `Dictionary<string>` evaluated to `{ [index: string]: string }`).
        // If so, format the original Application type instead of the expanded form.
        // Guard against cycles: if we're already inside a display_alias Application's
        // args, skip further display_alias redirects to prevent `Wrap<Wrap<...>>`.
        //
        // Skip for simple/resolved types: tsc shows the resolved form directly
        // (e.g., `"b"` not `KeysExtendedBy<M, number>`, or `"a" | "b"` not
        // `ValueOf<Obj>`), so we should not redirect these back to the
        // Application form.
        //
        // Exception: Union types that came from `keyof NamedType` should be
        // redirected to the KeyOf display alias.  tsc preserves the `keyof`
        // form for named operands (interfaces, classes, aliases) while showing
        // the expanded union for Application-sourced aliases.
        let is_simple_type = matches!(
            &key,
            TypeData::Literal(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Union(_)
                | TypeData::Function(_)
                | TypeData::TemplateLiteral(_)
                | TypeData::StringIntrinsic { .. }
                | TypeData::Enum(_, _)
        );
        if let Some(alias_origin) = self.interner.get_display_alias(type_id) {
            // KeyOf aliases: for Union types that came from `keyof NamedType`,
            // redirect to the `keyof` display form. Only do this when the keyof
            // operand has a named definition (interface/class/alias) so that
            // anonymous keyof (`keyof { a: string }`) still shows the expanded
            // union form, matching tsc behavior.
            let use_keyof_alias = if self.union_is_all_unit_literals(&key) {
                // A bare union of unit literals (`"a" | "b"`, `0 | 1`, enum
                // members, unique symbols) is indistinguishable from a
                // user-written union annotation: the same structural type is
                // interned once and shared. tsc only spells `keyof X` for an
                // index type — which tsz preserves as a `KeyOf` node (handled
                // by the `KeyOf` arm above) — and always renders a bare union
                // by its members. Following a global `union -> keyof X`
                // display alias here would repaint unrelated user-written
                // literal-union annotations, so never do it for this shape.
                false
            } else if let Some(TypeData::KeyOf(keyof_operand)) = self.interner.lookup(alias_origin)
            {
                self.def_store.is_some_and(|ds| {
                    ds.find_def_for_type(keyof_operand).is_some()
                        || matches!(
                            self.interner.lookup(keyof_operand),
                            Some(TypeData::Lazy(def_id)) if ds.get(def_id).is_some()
                        )
                        || self.interner.get_display_alias(keyof_operand).is_some_and(
                            |operand_alias| {
                                ds.find_def_for_type(operand_alias).is_some()
                                    || matches!(
                                        self.interner.lookup(operand_alias),
                                        Some(TypeData::Lazy(def_id)) if ds.get(def_id).is_some()
                                    )
                            },
                        )
                })
            } else {
                false
            };

            // Application aliases: for Union types that expanded from a generic type alias
            // (e.g., `IteratorResult<T>` → `IteratorYieldResult<T> | IteratorReturnResult<TReturn>`),
            // redirect to the application form. tsc preserves the generic name in error messages.
            //
            // Only do this when the union has at least one non-literal, non-intrinsic member.
            // Purely-literal unions from generic aliases (e.g., `1 | 2` from `ValueOf<Obj>`)
            // should still show in expanded form, matching tsc behavior.
            let use_application_alias = is_simple_type
                && matches!(&key, TypeData::Union(..))
                && matches!(
                    self.interner.lookup(alias_origin),
                    Some(TypeData::Application(_))
                )
                && !if let TypeData::Union(member_list_id) = &key {
                    let members = self.interner.type_list(*member_list_id);
                    !members.is_empty()
                        && members.iter().all(|&m| {
                            matches!(
                                self.interner.lookup(m),
                                Some(
                                    TypeData::TemplateLiteral(_)
                                        | TypeData::StringIntrinsic { .. }
                                        | TypeData::Literal(_)
                                        | TypeData::Intrinsic(_)
                                )
                            )
                        })
                } else {
                    false
                }
                && if let TypeData::Union(member_list_id) = &key {
                    let members = self.interner.type_list(*member_list_id);
                    members.iter().any(|&m| {
                        !matches!(
                            self.interner.lookup(m),
                            Some(TypeData::Literal(_) | TypeData::Intrinsic(_) | TypeData::Error)
                                | None
                        )
                    })
                } else {
                    false
                };
            let use_lazy_display_alias =
                if let Some(TypeData::Lazy(def_id)) = self.interner.lookup(alias_origin) {
                    self.def_store
                        .and_then(|ds| ds.get(def_id).map(|def| (ds, def)))
                        .is_some_and(|(ds, def)| {
                            matches!(
                                def.kind,
                                crate::def::DefKind::TypeAlias
                                    | crate::def::DefKind::Interface
                                    | crate::def::DefKind::Class
                            ) && def.type_params.is_empty()
                                && !def.body.is_some_and(|body| ds.is_computed_body(body))
                        })
                } else {
                    false
                };

            let skip_intersection_alias = (self.skip_intersection_display_alias
                && matches!(
                    self.interner.lookup(alias_origin),
                    Some(TypeData::Intersection(_))
                )
                && matches!(&key, TypeData::Object(_) | TypeData::ObjectWithIndex(_)))
                || (self.skip_application_alias_for_intersections
                    && matches!(
                        self.interner.lookup(alias_origin),
                        Some(TypeData::Application(_))
                    )
                    && matches!(&key, TypeData::Intersection(_)));

            // Skip the alias chase when the alias points to a distributive
            // conditional Application that will distribute (boolean or union
            // check arg). Following the alias would land in the Application
            // formatter, distribute back to the same evaluated form, and trip
            // `format_visiting` cycle detection (printing `...`). tsc shows the
            // expanded distributed form for these aliases anyway.
            let skip_distributive_alias = self.application_alias_distributes(alias_origin);

            // For empty `{}`, do not follow applications of type aliases: the
            // empty object is a universally-shared shape and mapped/conditional
            // reductions can point many unrelated annotations at the same TypeId.
            // Named generic interfaces/classes with empty bodies still need their
            // application display (e.g. `AsyncGenerator<number, void, unknown>`).
            let skip_object_alias = self.skip_object_display_alias
                && matches!(&key, TypeData::Object(_) | TypeData::ObjectWithIndex(_));
            // A hand-written object literal annotation never carries a direct
            // mapped-utility (`Application`) display alias in `tsc`; render it
            // structurally even when a same-shape utility result recorded one on
            // the shared id. Keep wrapper aliases such as `Value<"dup">` whose
            // body is not itself the mapped type, matching `tsc`'s source-facing
            // aggregate diagnostics.
            let skip_literal_annotation_application_alias = is_literal_object_annotation
                && self.application_alias_base_has_mapped_body(alias_origin);
            let skip_primitive_key_union_type_alias = self.is_primitive_key_union_data(&key)
                && matches!(
                    self.interner.lookup(alias_origin),
                    Some(TypeData::Lazy(def_id))
                        if self
                            .def_store
                            .and_then(|ds| ds.get(def_id))
                            .is_some_and(|def| def.kind == crate::def::DefKind::TypeAlias)
                );
            let skip_alias_chase = skip_intersection_alias
                || skip_distributive_alias
                || skip_object_alias
                || skip_literal_annotation_application_alias
                || skip_primitive_key_union_type_alias
                || (self.skip_application_display_alias_chase
                    && matches!(
                        self.interner.lookup(alias_origin),
                        Some(TypeData::Application(_))
                    ))
                || (self.skip_application_alias_names
                    && self.display_alias_application_base_has_conditional_body(alias_origin))
                // A conditional-bodied type alias loses its name in `tsc`
                // diagnostics once the conditional reduces to a concrete type:
                // `Tail<Src>` displays as the resolved `[number, string]`, not
                // `Tail<Src>`, independent of whether the argument was spelled
                // inline or via a named alias. A still-deferred result keeps the
                // alias (e.g. an unreduced generic `Tail<T>` whose `key` is a
                // `Conditional`/`IndexAccess`/`Mapped`). The provenance record
                // is left intact so the conditional evaluator can still recover
                // the application form (the `Equal<X, Y>` `any`-distinction
                // trick depends on it); only this display read is gated.
                || (self.display_alias_application_base_has_conditional_body(alias_origin)
                    && !matches!(
                        &key,
                        TypeData::Conditional(_)
                            | TypeData::IndexAccess(_, _)
                            | TypeData::Mapped(_)
                    ))
                || (is_empty_object
                    && (self.display_alias_application_base_is_type_alias(alias_origin)
                        || self.display_alias_application_base_is_marker_interface(alias_origin)));
            if (!is_simple_type
                || use_keyof_alias
                || use_application_alias
                || use_lazy_display_alias)
                && !skip_alias_chase
                && self.display_alias_visiting.insert(alias_origin)
            {
                let result = self.format(alias_origin);
                self.display_alias_visiting.remove(&alias_origin);
                return result;
                // Otherwise: cycle detected — fall through to format the expanded type directly
            }
        }

        // Check if this type is a module namespace object that should display
        // as `typeof import("module")` instead of its expanded object shape.
        if matches!(&key, TypeData::Object(_) | TypeData::ObjectWithIndex(_))
            && let Some(ns_names) = self.namespace_module_names
            && let Some(module_name) = ns_names.get(&type_id)
        {
            let display_name =
                Self::strip_module_extension(module_name.strip_prefix("./").unwrap_or(module_name));
            return format!("typeof import(\"{display_name}\")").into();
        }

        self.current_depth += 1;
        let result = self.format_key_guarded(type_id, &key);
        self.current_depth -= 1;
        result
    }

    pub fn format_union_members_in_order(&mut self, members: &[TypeId]) -> Cow<'static, str> {
        self.current_depth += 1;
        let result = self.format_union_preserving_member_order(members).into();
        self.current_depth -= 1;
        result
    }

    /// Strip TypeScript/JavaScript file extensions from module specifier
    /// display names. TSC omits extensions in `typeof import("mod")` output.
    fn strip_module_extension(module_name: &str) -> &str {
        tsz_common::file_extensions::strip_known_extension(module_name)
    }
}

/// Standalone form of the marker-instantiation rule for non-formatter
/// consumers (e.g. the checker's property-receiver display): an EMPTY-object
/// `evaluated` whose display alias is an application of an interface/class
/// base is a marker render — tsc prints the shared `{}` structurally, never
/// the marker's name (`ThisType<any>` from `Object.defineProperty`).
pub fn empty_object_display_alias_is_marker_render(
    interner: &dyn crate::construction::TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    evaluated: TypeId,
    alias_origin: TypeId,
) -> bool {
    let empty = match interner.lookup(evaluated) {
        Some(TypeData::Object(shape_id)) => interner.object_shape(shape_id).properties.is_empty(),
        Some(TypeData::ObjectWithIndex(shape_id)) => {
            let shape = interner.object_shape(shape_id);
            shape.properties.is_empty()
                && shape.string_index.is_none()
                && shape.number_index.is_none()
        }
        _ => false,
    };
    if !empty {
        return false;
    }
    let Some(TypeData::Application(app_id)) = interner.lookup(alias_origin) else {
        return false;
    };
    let app = interner.type_application(app_id);
    let def_id = match interner.lookup(app.base) {
        Some(TypeData::Lazy(def_id)) => Some(def_id),
        _ => def_store.find_def_for_type(app.base),
    };
    def_id.and_then(|d| def_store.get(d)).is_some_and(|def| {
        matches!(
            def.kind,
            crate::def::DefKind::Interface | crate::def::DefKind::Class
        )
    })
}
