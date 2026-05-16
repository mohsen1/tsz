//! Type formatting for the solver.
//! Centralizes logic for converting `TypeIds` and `TypeDatas` to human-readable strings.

mod compound;
mod display_simplification;
mod intrinsic;
// `test_tracing` exercises `debug!` / `debug_span!` / `trace_span!`. The
// workspace `tracing` dep filters those macros out at compile time when
// `debug_assertions` is off (via the `release_max_level_warn` feature), so
// the capture-based assertions can never observe events under
// `cargo test --release`. Gate the module on `debug_assertions` so the
// release-mode test build doesn't try to run (or compile) tests that have
// nothing to capture.
#[cfg(all(test, debug_assertions))]
pub mod test_tracing;
#[cfg(test)]
mod tests;
pub mod tracing_helpers;

use crate::TypeDatabase;
use crate::def::{DefId, DefinitionStore};
use crate::diagnostics::{
    DiagnosticArg, PendingDiagnostic, RelatedInformation, SourceSpan, TypeDiagnostic,
    get_message_template,
};
use crate::types::{
    MappedModifier, ObjectFlags, ObjectShape, StringIntrinsicKind, SymbolRef, TypeData, TypeId,
    TypeListId, TypeParamInfo,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::sync::Arc;
use tracing::trace;
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;

/// Returns `true` if a property name needs to be quoted in type display
/// (i.e. it is not a valid JS identifier or numeric literal).
fn needs_property_name_quotes(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Computed property names wrapped in brackets (e.g. [Symbol.asyncIterator])
    // are displayed as-is without quotes, matching tsc behavior.
    if name.starts_with('[') && name.ends_with(']') {
        return false;
    }
    // Numeric property names don't need quotes. This includes integer-only
    // forms (`19230`) as well as canonical JS-numeric forms with decimals
    // (`3.14`), exponents (`5.462437423415177e+244`), or signs (`-1`), all
    // of which match `Number.prototype.toString()` round-trip and are
    // displayed unquoted by tsc in object type literals.
    if crate::utils::is_numeric_literal_name(name) {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' || first == '$' => {
            !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        }
        _ => true,
    }
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
}

impl<'a> TypeFormatter<'a> {
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

    fn scalar_mapped_alias_application_display(
        &self,
        type_id: TypeId,
        base: TypeId,
        args: &[TypeId],
    ) -> Option<TypeId> {
        if !self.expand_scalar_mapped_alias_applications {
            return None;
        }

        let def_store = self.def_store?;
        let def_id = match self.interner.lookup(base) {
            Some(TypeData::Lazy(def_id)) => def_id,
            _ => def_store.find_def_for_type(base)?,
        };
        let def = def_store.get(def_id)?;
        if def.kind != crate::def::DefKind::TypeAlias {
            return None;
        }
        let body = crate::evaluation::evaluate::evaluate_type(self.interner, def.body?);
        let mapped_id = match self.interner.lookup(body) {
            Some(TypeData::Mapped(mapped_id)) => mapped_id,
            _ => return None,
        };
        let identity_info =
            crate::type_queries::mapped::classify_identity_mapped(self.interner, mapped_id)?;
        let source_arg_index = def
            .type_params
            .iter()
            .position(|param| param.name == identity_info.source_param_name)?;
        let evaluated = crate::type_queries::mapped::evaluate_identity_mapped_passthrough(
            self.interner,
            mapped_id,
            *args.get(source_arg_index)?,
        )?;
        if evaluated == type_id
            || evaluated == TypeId::ERROR
            || crate::type_queries::contains_type_parameters_db(self.interner, evaluated)
        {
            return None;
        }

        (crate::visitor::is_primitive_type(self.interner, evaluated)
            || matches!(self.interner.lookup(evaluated), Some(TypeData::Literal(_)))
            || evaluated == TypeId::NEVER)
            .then_some(evaluated)
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
        // Idx must be a literal (or union of literals) for tsc's unfold —
        // a generic key would also be deferred even when the obj is concrete.
        if !matches!(
            self.interner.lookup(idx),
            Some(
                TypeData::Literal(_)
                    | TypeData::Union(_)
                    | TypeData::UniqueSymbol(_)
                    | TypeData::TypeQuery(_)
            )
        ) {
            return arg;
        }
        let resolved = crate::evaluation::evaluate::evaluate_index_access(self.interner, obj, idx);
        if resolved == arg || resolved == TypeId::ERROR {
            return arg;
        }
        resolved
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
            display_alias_visiting: FxHashSet::default(),
            format_visiting: FxHashSet::default(),
            preserve_array_generic_form: false,
            skip_application_alias_names: false,
            skip_application_display_alias_chase: false,
            skip_type_alias_def_ids: FxHashSet::default(),
            skipped_type_alias_expansion_visiting: FxHashSet::default(),
            builtin_iterator_return_type: None,
            skip_intersection_display_alias: false,
            skip_application_alias_for_intersections: false,
            capitalize_primitive_intersection_members: false,
            skip_object_display_alias: false,
            long_property_receiver_display: false,
            long_property_receiver_object_elision_end_depth: 26,
            expand_scalar_mapped_alias_applications: false,
            expand_primitive_key_union: false,
            ignore_union_origins: false,
        }
    }

    fn distributed_conditional_application_display(
        &self,
        base: TypeId,
        args: &[TypeId],
    ) -> Option<TypeId> {
        let def_store = self.def_store?;
        let def_id = match self.interner.lookup(base) {
            Some(TypeData::Lazy(def_id)) => def_id,
            _ => def_store.find_def_for_type(base)?,
        };
        let def = def_store.get(def_id)?;
        if def.kind != crate::def::DefKind::TypeAlias {
            return None;
        }
        let body = def.body?;
        let TypeData::Conditional(cond_id) = self.interner.lookup(body)? else {
            return None;
        };
        let cond = self.interner.conditional_type(cond_id);
        if !cond.is_distributive {
            return None;
        }
        let TypeData::TypeParameter(check_tp) = self.interner.lookup(cond.check_type)? else {
            return None;
        };
        let check_index = def
            .type_params
            .iter()
            .position(|param| param.name == check_tp.name)?;
        let check_arg = *args.get(check_index)?;

        // For distributive conditionals, `boolean` distributes as `true | false`.
        // Mirrors the instantiation policy in instantiate.rs that expands
        // `BOOLEAN` to `[BOOLEAN_TRUE, BOOLEAN_FALSE]` before substitution.
        let mut members: Vec<TypeId> = if check_arg == TypeId::BOOLEAN {
            vec![TypeId::BOOLEAN_FALSE, TypeId::BOOLEAN_TRUE]
        } else if let Some(TypeData::Union(member_list_id)) = self.interner.lookup(check_arg) {
            if let Some(origin) = self.interner.get_union_origin(check_arg) {
                if origin.len() < 2 {
                    return None;
                }
                origin.to_vec()
            } else {
                let list = self.interner.type_list(member_list_id);
                if list.len() < 2 {
                    return None;
                }
                list.to_vec()
            }
        } else {
            return None;
        };

        if let Some(def_store) = self.def_store {
            let positions: Vec<_> = members
                .iter()
                .map(|&member| self.get_source_position_for_type(member, def_store))
                .collect();
            if positions.iter().all(|&(tier, _, _)| tier < 2) {
                let mut pairs: Vec<_> = members.iter().copied().zip(positions).collect();
                pairs.sort_by_key(|&(_, pos)| pos);
                members = pairs.into_iter().map(|(member, _)| member).collect();
            }
        }

        // Only evaluate the distributed branches when the *other* type args are
        // fully concrete. If any non-check arg carries free type parameters
        // (e.g. `ChannelOfType<T, Channel>` where `T` is bound in an outer
        // scope), the conditional inside the body cannot be reliably resolved,
        // and tsc preserves the alias-application form
        // (`ChannelOfType<T, TextChannel> | ChannelOfType<T, EmailChannel>`).
        let other_args_concrete = args.iter().enumerate().all(|(i, &arg)| {
            i == check_index
                || !crate::visitors::visitor_predicates::contains_type_parameters(
                    self.interner,
                    arg,
                )
        });

        // Distribute into per-member branches. When other args are concrete we
        // can safely evaluate the conditional body and render the resolved
        // branch (`{ kind: "b" }`). Otherwise we keep each branch as an
        // Application so the formatter renders `Foo<member>` rather than a
        // misleading evaluation (which can collapse to `never` when relations
        // involve free type parameters).
        let distributed: Vec<TypeId> = members
            .iter()
            .map(|&member| {
                let mut branch_args = args.to_vec();
                branch_args[check_index] = member;
                if other_args_concrete {
                    let mut subst = crate::instantiation::instantiate::TypeSubstitution::new();
                    for (i, param) in def.type_params.iter().enumerate() {
                        let Some(&arg) = branch_args.get(i) else {
                            return TypeId::ERROR;
                        };
                        subst.insert(param.name, arg);
                    }
                    let substituted = crate::instantiation::instantiate::instantiate_type(
                        self.interner,
                        body,
                        &subst,
                    );
                    crate::evaluation::evaluate::evaluate_type(self.interner, substituted)
                } else {
                    self.interner.application(base, branch_args)
                }
            })
            .collect();
        let union = self.interner.union(distributed.clone());
        self.interner.store_union_origin(union, distributed);
        Some(union)
    }

    /// Returns `true` when the application points to a distributive conditional
    /// alias whose `check_arg` is `boolean` or a union — i.e., the application
    /// would distribute via `distributed_conditional_application_display`. The
    /// display-alias chase should skip these so the formatter renders the
    /// structurally evaluated branches rather than redirecting back to the
    /// alias and re-entering the same evaluated form (which trips the
    /// `format_visiting` cycle protection and prints `...`).
    fn application_alias_distributes(&self, alias_origin: TypeId) -> bool {
        let Some(TypeData::Application(app_id)) = self.interner.lookup(alias_origin) else {
            return false;
        };
        let app = self.interner.type_application(app_id);
        let Some(def_store) = self.def_store else {
            return false;
        };
        let def_id = match self.interner.lookup(app.base) {
            Some(TypeData::Lazy(def_id)) => def_id,
            _ => match def_store.find_def_for_type(app.base) {
                Some(def_id) => def_id,
                None => return false,
            },
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
        let Some(TypeData::Conditional(cond_id)) = self.interner.lookup(body) else {
            return false;
        };
        let cond = self.interner.conditional_type(cond_id);
        if !cond.is_distributive {
            return false;
        }
        let Some(TypeData::TypeParameter(check_tp)) = self.interner.lookup(cond.check_type) else {
            return false;
        };
        let Some(check_index) = def
            .type_params
            .iter()
            .position(|param| param.name == check_tp.name)
        else {
            return false;
        };
        let Some(&check_arg) = app.args.get(check_index) else {
            return false;
        };
        if check_arg == TypeId::BOOLEAN {
            return true;
        }
        if let Some(TypeData::Union(member_list_id)) = self.interner.lookup(check_arg) {
            return self.interner.type_list(member_list_id).len() >= 2;
        }
        false
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
            display_alias_visiting: FxHashSet::default(),
            format_visiting: FxHashSet::default(),
            preserve_array_generic_form: false,
            skip_application_alias_names: false,
            skip_application_display_alias_chase: false,
            skip_type_alias_def_ids: FxHashSet::default(),
            skipped_type_alias_expansion_visiting: FxHashSet::default(),
            builtin_iterator_return_type: None,
            skip_intersection_display_alias: false,
            skip_application_alias_for_intersections: false,
            capitalize_primitive_intersection_members: false,
            skip_object_display_alias: false,
            long_property_receiver_display: false,
            long_property_receiver_object_elision_end_depth: 26,
            expand_scalar_mapped_alias_applications: false,
            expand_primitive_key_union: false,
            ignore_union_origins: false,
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
        let fallback_span = pending
            .span
            .clone()
            .unwrap_or_else(|| SourceSpan::new("<unknown>", 0, 0));
        for related in &pending.related {
            let related_msg =
                self.render_template(get_message_template(related.code), &related.args);
            let span = related
                .span
                .clone()
                .unwrap_or_else(|| fallback_span.clone());
            diag.related.push(RelatedInformation {
                span,
                message: related_msg,
            });
        }

        diag
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

    /// Format a type as a human-readable string.
    ///
    /// Returns `Cow::Borrowed` for static type names (e.g., `"never"`, `"any"`)
    /// and `Cow::Owned` for dynamically formatted types.
    pub fn format(&mut self, type_id: TypeId) -> Cow<'static, str> {
        if self.format_visiting.contains(&type_id) {
            return Cow::Borrowed("...");
        }
        let type_key = self.interner.lookup(type_id);
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
            TypeId::ANY => return Cow::Borrowed("any"),
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
            TypeId::ERROR => return Cow::Borrowed("error"),
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

        // For composite types that might be named (interfaces, type aliases, classes),
        // check if this TypeId maps to a definition name. This handles:
        // - Interfaces: `interface Foo { a: string }` displays as "Foo"
        // - Cross-file scenarios where ObjectShape's symbol can't be resolved
        //
        // NOTE: We deliberately do NOT use `find_type_alias_by_body` here because
        // tsc only shows alias names when the type was directly referenced through
        // that alias, not when a computed type happens to match an alias body.
        // The `display_alias` mechanism (below) handles the cases where tsc does
        // show alias names for evaluated types.
        //
        // Restricted to composite shapes to avoid false positives where a primitive
        // or literal type coincidentally matches an alias body (e.g. `type U = 1`).
        // Nested-if reads more cleanly than a long &&-chained let-chain here.
        // When `skip_object_display_alias` is set, do not redirect Object/
        // ObjectWithIndex types to a named definition (e.g. a JS constructor's
        // `prototype` property def whose body is this literal). Diagnostics
        // that opt into this mode want the literal's structural shape.
        let skip_object_def_lookup = self.skip_object_display_alias
            && matches!(&key, TypeData::Object(_) | TypeData::ObjectWithIndex(_));
        #[allow(clippy::collapsible_if)]
        if !skip_object_def_lookup
            && matches!(
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
            )
            && let Some(def_store) = self.def_store
        {
            if let Some(def_id) = def_store.find_def_for_type(type_id)
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
                let skip_alias = if !def_kind_matches_type_shape {
                    true
                } else if def.kind == DefKind::TypeAlias {
                    self.skip_type_alias_def_ids.contains(&def_id)
                        || def.body.is_some_and(|b| def_store.is_computed_body(b))
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
                    let prefer_array_shorthand =
                        name == "Array" && matches!(&key, TypeData::Array(_));
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
                        if def.kind == DefKind::TypeAlias {
                            if let Some(alias_origin) = self.interner.get_display_alias(type_id)
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
                                    crate::visitors::visitor_predicates::is_mapped_type(
                                        self.interner,
                                        body,
                                    )
                                } else {
                                    false
                                };
                                if base_has_mapped_body
                                    && self.display_alias_visiting.insert(alias_origin)
                                {
                                    let result = self.format(alias_origin);
                                    self.display_alias_visiting.remove(&alias_origin);
                                    return result;
                                }
                            }
                        }
                        // When a type resolves to a named definition (interface,
                        // class, or type alias), show that name. tsc preserves alias
                        // symbols: `type Bar = Omit<Foo, "c">` displays as "Bar".
                        return name.into();
                    }
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
            let use_keyof_alias =
                if let Some(TypeData::KeyOf(keyof_operand)) = self.interner.lookup(alias_origin) {
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
                || skip_primitive_key_union_type_alias
                || (self.skip_application_display_alias_chase
                    && matches!(
                        self.interner.lookup(alias_origin),
                        Some(TypeData::Application(_))
                    ))
                || (self.skip_application_alias_names
                    && self.display_alias_application_base_has_conditional_body(alias_origin))
                || (is_empty_object
                    && self.display_alias_application_base_is_type_alias(alias_origin));
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
        let inserted_visiting = self.format_visiting.insert(type_id);
        let result = self.format_key(type_id, &key);
        if inserted_visiting {
            self.format_visiting.remove(&type_id);
        }
        self.current_depth -= 1;
        result
    }

    pub fn format_union_members_in_order(&mut self, members: &[TypeId]) -> Cow<'static, str> {
        self.current_depth += 1;
        let result = self.format_union_preserving_member_order(members).into();
        self.current_depth -= 1;
        result
    }

    fn format_key(&mut self, type_id: TypeId, key: &TypeData) -> Cow<'static, str> {
        match key {
            TypeData::Intrinsic(kind) => Cow::Borrowed(intrinsic::format_intrinsic(*kind)),
            TypeData::Literal(lit) => self.format_literal(lit).into(),
            TypeData::Object(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
                }
                if let Some(record_display) = self.format_in_operator_record(&shape) {
                    return record_display.into();
                }
                // Use display properties (pre-widened literal types) when enabled.
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                {
                    return self.format_object(display_props.as_slice()).into();
                }
                self.format_object(shape.properties.as_slice()).into()
            }
            TypeData::ObjectWithIndex(shape_id) => {
                let shape = self.interner.object_shape(*shape_id);
                if let Some(record_display) = self.format_in_operator_record(&shape) {
                    return record_display.into();
                }
                if let Some(name) = self.resolve_object_shape_name(&shape) {
                    return name.into();
                }
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                {
                    let mut display_shape = shape.as_ref().clone();
                    display_shape.properties = display_props.as_ref().clone();
                    return self.format_object_with_index(&display_shape).into();
                }
                self.format_object_with_index(shape.as_ref()).into()
            }
            TypeData::Union(members) => {
                if self.diagnostic_mode
                    && !self.expand_primitive_key_union
                    && self.is_primitive_key_union_data(key)
                {
                    return Cow::Borrowed("PropertyKey");
                }
                // tsc preserves top-level alias names that would otherwise be
                // lost during union flattening (e.g., `T | null` should not
                // expand to T's body). The checker records the unflattened
                // input member list as a side-table "origin"; consult it here
                // before structural display.
                let members = self.interner.type_list(*members);
                if !self.ignore_union_origins
                    && let Some(origin) = self.interner.get_union_origin(type_id)
                {
                    return self.format_union(origin.as_slice()).into();
                }
                self.format_union(members.as_ref()).into()
            }
            TypeData::Intersection(members) => {
                let members = self.interner.type_list(*members);
                if self.use_display_properties
                    && let Some(display_props) = self.interner.get_display_properties(type_id)
                    && let Some(rendered) = self
                        .format_intersection_with_display(members.as_ref(), display_props.as_ref())
                {
                    return rendered.into();
                }
                self.format_intersection(members.as_ref()).into()
            }
            TypeData::Array(elem) => {
                if self.preserve_array_generic_form && !elem.is_intrinsic() {
                    let elem_formatted = self.format(*elem);
                    return format!("Array<{elem_formatted}>").into();
                }
                let elem_formatted = self.format(*elem);
                let needs_parens = matches!(
                    self.interner.lookup(*elem),
                    Some(
                        TypeData::Union(_)
                            | TypeData::Intersection(_)
                            | TypeData::Function(_)
                            | TypeData::Callable(_)
                    )
                );
                if needs_parens {
                    format!("({elem_formatted})[]").into()
                } else {
                    format!("{elem_formatted}[]").into()
                }
            }
            TypeData::Tuple(elements) => {
                let elements = self.interner.tuple_list(*elements);
                self.format_tuple(elements.as_ref()).into()
            }
            TypeData::Function(shape_id) => {
                let shape = self.interner.function_shape(*shape_id);
                self.format_function(shape.as_ref()).into()
            }
            TypeData::Callable(shape_id) => {
                let shape = self.interner.callable_shape(*shape_id);
                // Check for a named symbol (e.g. ObjectConstructor, SymbolConstructor)
                // before falling back to structural expansion.
                if let Some(sym_id) = shape.symbol
                    && let Some(name) = self.format_symbol_name(sym_id)
                {
                    // Class constructor types (callables with construct signatures
                    // linked to a class symbol) should display as "typeof ClassName"
                    // to match tsc behavior. The class instance type displays as
                    // just "ClassName".
                    if !shape.construct_signatures.is_empty()
                        && let Some(arena) = self.symbol_arena
                        && let Some(sym) = arena.get(sym_id)
                        && sym.has_flags(tsz_binder::symbol_flags::CLASS)
                    {
                        return format!("typeof {name}").into();
                    }
                    return name.into();
                }
                self.format_callable(shape.as_ref()).into()
            }
            TypeData::TypeParameter(info) => Cow::Owned(self.atom(info.name).to_string()),
            TypeData::UnresolvedTypeName(name) => {
                if self.atom(*name).as_ref() == "BuiltinIteratorReturn"
                    && let Some(replacement) = self.builtin_iterator_return_type
                {
                    return self.format(replacement);
                }
                if let Some((def_id, body)) = self.skipped_type_alias_body_by_name(*name) {
                    return self.format_skipped_type_alias_body(def_id, body);
                }
                Cow::Owned(self.atom(*name).to_string())
            }
            TypeData::Lazy(def_id) => {
                if let Some(replacement) = self.builtin_iterator_return_type
                    && let Some(def_store) = self.def_store
                    && let Some(def) = def_store.get(*def_id)
                    && def.kind == crate::def::DefKind::TypeAlias
                    && self.atom(def.name).as_ref() == "BuiltinIteratorReturn"
                {
                    return self.format(replacement);
                }
                if self.skip_type_alias_def_ids.contains(def_id)
                    && let Some(def_store) = self.def_store
                    && let Some(def) = def_store.get(*def_id)
                    && def.kind == crate::def::DefKind::TypeAlias
                    && let Some(body) = def.body
                {
                    return self.format_skipped_type_alias_body(*def_id, body);
                }
                self.format_def_id_with_type_params(*def_id, "Lazy").into()
            }
            TypeData::Recursive(idx) => format!("Recursive({idx})").into(),
            TypeData::BoundParameter(idx) => format!("BoundParameter({idx})").into(),
            TypeData::Application(app) => {
                let app = self.interner.type_application(*app);
                let base_key = self.interner.lookup(app.base);

                trace!(
                    base_type_id = %app.base.0,
                    ?base_key,
                    args_count = app.args.len(),
                    "Formatting Application"
                );

                // When the base type has already been evaluated to a concrete
                // type (Array, Tuple, etc.), the type arguments are already
                // incorporated into the base.  Formatting the base directly
                // produces the correct display (e.g., `D<number>[]`); appending
                // the Application's args would duplicate them (producing
                // `D<number>[]<D<number>>`).
                if matches!(base_key, Some(TypeData::Array(_) | TypeData::Tuple(_))) {
                    return self.format(app.base);
                }

                // If the application's base resolved to an error type,
                // rendering `error<args>` produces unreadable cascades in
                // diagnostics (e.g. `error<error<error<...>>>`). Collapse to
                // the bare "error" token — the caller's parent diagnostic
                // already signals the underlying failure.
                if app.base == TypeId::ERROR || matches!(base_key, Some(TypeData::Error)) {
                    return Cow::Borrowed("error");
                }

                if let Some(evaluated) =
                    self.scalar_mapped_alias_application_display(type_id, app.base, &app.args)
                {
                    return self.format(evaluated);
                }

                if let Some(distributed) =
                    self.distributed_conditional_application_display(app.base, &app.args)
                {
                    return self.format(distributed);
                }

                // Special handling for Application(Lazy(def_id), args)
                // Format as "TypeName<Args>" instead of "Lazy(def_id)<Args>"
                let base_str: Cow<'_, str> = if let Some(TypeData::Lazy(def_id)) = base_key {
                    let name = self.format_def_id(def_id, "Lazy");
                    trace!(
                        def_id = %def_id.0,
                        name = %name,
                        "Application base resolved from DefId"
                    );
                    Cow::Owned(name)
                } else if let Some(TypeData::TypeQuery(sym)) = base_key {
                    // For Application(TypeQuery(sym), args) — class instantiation
                    // like D<string>. Display as "D<string>" not "typeof D<string>",
                    // since typeof X<T> is not valid TS syntax and this represents
                    // the instantiated class type.
                    if let Some(name) = self.resolve_symbol_ref_name(sym) {
                        Cow::Owned(name)
                    } else {
                        Cow::Owned(format!("Ref({})", sym.0))
                    }
                } else {
                    // Check if the base type has a named definition (e.g., an
                    // interface or class body that was registered in the def store).
                    // If so, use just the name — the Application's own args replace
                    // the type parameters.  Without this guard, `self.format(app.base)`
                    // would render `Name<TypeParamNames>` and the Application would
                    // then append `<Args>`, producing `Name<T, U><actual, args>`.
                    if let Some(def_store) = self.def_store
                        && let Some(def_id) = def_store.find_def_for_type(app.base)
                        && let Some(def) = def_store.get(def_id)
                    {
                        let name = self.format_def_name(&def);
                        trace!(
                            base_formatted = %name,
                            "Application base resolved via def_store (no type params)"
                        );
                        use crate::def::DefKind;
                        if matches!(
                            def.kind,
                            DefKind::Enum | DefKind::Namespace | DefKind::ClassConstructor
                        ) {
                            if name.contains('.') {
                                Cow::Owned(name)
                            } else {
                                Cow::Owned(format!("typeof {name}"))
                            }
                        } else {
                            Cow::Owned(name)
                        }
                    } else {
                        let formatted = self.format(app.base);
                        trace!(
                            base_formatted = %formatted,
                            "Application base formatted (not Lazy)"
                        );
                        formatted
                    }
                };

                // TSC shorthand: Array<T> -> T[], ReadonlyArray<T> -> readonly T[]
                // and Readonly<T[]> -> readonly T[].
                // Skipped in constraint context (preserve_array_generic_form).
                if app.args.len() == 1 && !self.preserve_array_generic_form {
                    let single_arg = app.args[0];
                    if base_str == "Array"
                        && self
                            .interner
                            .get_array_base_type()
                            .is_some_and(|array_base| app.base == array_base)
                    {
                        // Array<T> -> T[]
                        let elem_formatted = self.format(single_arg);
                        let needs_parens = matches!(
                            self.interner.lookup(single_arg),
                            Some(
                                TypeData::Union(_)
                                    | TypeData::Intersection(_)
                                    | TypeData::Function(_)
                                    | TypeData::Callable(_)
                                    | TypeData::Conditional(_)
                            )
                        );
                        let result = if needs_parens {
                            format!("({elem_formatted})[]")
                        } else {
                            format!("{elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as array shorthand");
                        return result.into();
                    }
                    if base_str == "ReadonlyArray" {
                        // ReadonlyArray<T> -> readonly T[]
                        let elem_formatted = self.format(single_arg);
                        let needs_parens = matches!(
                            self.interner.lookup(single_arg),
                            Some(
                                TypeData::Union(_)
                                    | TypeData::Intersection(_)
                                    | TypeData::Function(_)
                                    | TypeData::Callable(_)
                                    | TypeData::Conditional(_)
                            )
                        );
                        let result = if needs_parens {
                            format!("readonly ({elem_formatted})[]")
                        } else {
                            format!("readonly {elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as readonly array shorthand");
                        return result.into();
                    }
                    if base_str == "Readonly"
                        && let Some(TypeData::Array(elem)) = self.interner.lookup(single_arg)
                    {
                        // Readonly<T[]> -> readonly T[]
                        let elem_formatted = self.format(elem);
                        let needs_parens = matches!(
                            self.interner.lookup(elem),
                            Some(
                                TypeData::Union(_)
                                    | TypeData::Intersection(_)
                                    | TypeData::Function(_)
                                    | TypeData::Callable(_)
                                    | TypeData::Conditional(_)
                            )
                        );
                        let result = if needs_parens {
                            format!("readonly ({elem_formatted})[]")
                        } else {
                            format!("readonly {elem_formatted}[]")
                        };
                        trace!(result = %result, "Application formatted as Readonly<T[]> shorthand");
                        return result.into();
                    }
                }

                // Elide trailing type arguments that equal their parameter's
                // default. tsc renders `AsyncIterable<number, any, any>` as
                // `AsyncIterable<number>` when the second and third type
                // parameters default to `any`. tsc only applies this to the
                // four iterable globals — see `typeReferenceToTypeNode` in
                // checker.ts: "Maybe we should do this for more types, but for
                // now we only elide type arguments that are identical to their
                // associated type parameters' defaults for `Iterable`,
                // `IterableIterator`, `AsyncIterable`, and
                // `AsyncIterableIterator` to provide backwards-compatible .d.ts
                // emit due to each now having three type parameters instead of
                // only one." Applying elision unconditionally would e.g. turn
                // `Generator<number, any, any>` into `Generator<number>`, which
                // tsc doesn't do.
                let should_elide_defaults = matches!(
                    base_str.as_ref(),
                    "Iterable" | "IterableIterator" | "AsyncIterable" | "AsyncIterableIterator"
                );
                // Load the base's declared type parameters. We need them in two
                // situations:
                //   1) `should_elide_defaults` — trim trailing args that equal
                //      their parameter defaults (for the 4 iterable globals).
                //   2) `app.args.len() < params.len()` — pad missing trailing
                //      args with their parameter defaults so tsc-style output
                //      shows all args (e.g. `Iterator<string>` renders as
                //      `Iterator<string, any, any>` when `TReturn = TNext = any`).
                let def_type_params: Option<Vec<TypeParamInfo>> =
                    if let Some(TypeData::Lazy(def_id)) = base_key {
                        self.def_store.and_then(|ds| ds.get_type_params(def_id))
                    } else if let Some(def_store) = self.def_store {
                        def_store
                            .find_def_for_type(app.base)
                            .and_then(|id| def_store.get_type_params(id))
                    } else {
                        None
                    };

                // Build the display arg list, padding missing trailing args
                // with their parameter defaults when available.
                let display_args: Vec<TypeId> = if let Some(params) = def_type_params.as_ref()
                    && params.len() > app.args.len()
                {
                    let mut out: Vec<TypeId> = app.args.to_vec();
                    for param in params.iter().skip(app.args.len()) {
                        // Only pad when the missing parameter carries a default;
                        // stop at the first parameter without a default.
                        let Some(default) = param.default else {
                            break;
                        };
                        out.push(default);
                    }
                    out
                } else {
                    app.args.to_vec()
                };

                let visible_arg_count = if let Some(params) = def_type_params.as_ref()
                    && should_elide_defaults
                    && params.len() == display_args.len()
                {
                    let mut n = display_args.len();
                    while n > 0 {
                        let idx = n - 1;
                        if let Some(default) = params[idx].default {
                            if display_args[idx] != default {
                                break;
                            }
                        } else if display_args[idx] != TypeId::ANY {
                            break;
                        }
                        n -= 1;
                    }
                    n
                } else {
                    display_args.len()
                };

                let previous_skip_application_display_alias_chase =
                    self.skip_application_display_alias_chase;
                if self.skip_application_alias_names && base_str.as_ref() == "Omit" {
                    self.skip_application_display_alias_chase = true;
                }
                let mut args: Vec<Cow<'static, str>> = display_args
                    .iter()
                    .take(visible_arg_count)
                    .map(|&arg| self.format(self.simplify_application_arg_for_display(arg)))
                    .collect();
                self.skip_application_display_alias_chase =
                    previous_skip_application_display_alias_chase;
                if base_str.as_ref() == "Defaultize"
                    && args.first().is_some_and(|arg| arg.len() > 120)
                {
                    for (idx, arg) in display_args
                        .iter()
                        .take(visible_arg_count)
                        .enumerate()
                        .skip(1)
                    {
                        if matches!(
                            self.interner.lookup(*arg),
                            Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_))
                        ) {
                            args[idx] = Cow::Borrowed("{ ...; }");
                        }
                    }
                }
                let result = if args.is_empty()
                    && matches!(
                        base_str.as_ref(),
                        "Iterable" | "IterableIterator" | "AsyncIterable" | "AsyncIterableIterator"
                    ) {
                    format!("{base_str}<any>")
                } else if args.is_empty() {
                    base_str.to_string()
                } else {
                    format!("{}<{}>", base_str, args.join(", "))
                };
                trace!(result = %result, "Application formatted");
                result.into()
            }
            TypeData::Conditional(cond_id) => {
                let cond = self.interner.conditional_type(*cond_id);
                self.format_conditional(cond.as_ref()).into()
            }
            TypeData::Mapped(mapped_id) => {
                let mapped = self.interner.mapped_type(*mapped_id);
                self.format_mapped(mapped.as_ref()).into()
            }
            TypeData::IndexAccess(obj, idx) => {
                let resolved = self.resolve_concrete_index_access_for_display(type_id);
                if resolved != type_id {
                    return self.format(resolved);
                }
                // Homomorphic mapped indexed access simplification:
                // tsc displays `M[K]` for a homomorphic identity Mapped
                // `M = { [P in keyof X]: X[P] }` (e.g. `Partial<X>`,
                // `Readonly<X>`) as `X[K]`, with `| undefined` appended when
                // the mapped's optional modifier is `+`. The structural
                // mapped form still appears when M is formatted directly,
                // but in indexed-access position tsc collapses to the
                // simpler X[K] form.
                if let Some(simplified) =
                    self.try_format_homomorphic_mapped_index_access(*obj, *idx)
                {
                    return simplified.into();
                }
                let obj_for_display = self
                    .interner
                    .get_display_alias(*obj)
                    .filter(|&alias| {
                        matches!(self.interner.lookup(alias), Some(TypeData::Application(_)))
                    })
                    .unwrap_or(*obj);
                let obj_str = if obj_for_display == *obj
                    && matches!(self.interner.lookup(*obj), Some(TypeData::Mapped(_)))
                    && let Some(def_store) = self.def_store
                    && let Some(def_id) = def_store.find_def_for_type(*obj)
                    && let Some(def) = def_store.get(def_id)
                    && !def.type_params.is_empty()
                {
                    let params: Vec<String> = def
                        .type_params
                        .iter()
                        .map(|tp| self.atom(tp.name).to_string())
                        .collect();
                    format!("{}<{}>", self.format_def_name(&def), params.join(", "))
                } else {
                    self.format(obj_for_display).into_owned()
                };
                // Parenthesize the object when it's a union or intersection AND
                // the formatted string actually shows the compound form (contains
                // ` & ` or ` | `). Named type aliases like `Errors<T>` may be
                // stored as intersections internally but display as a single name.
                let needs_parens = matches!(
                    self.interner.lookup(*obj),
                    Some(TypeData::Union(_) | TypeData::Intersection(_))
                ) && (obj_str.contains(" & ") || obj_str.contains(" | "));
                if needs_parens {
                    format!("({obj_str})[{}]", self.format(*idx)).into()
                } else {
                    format!("{obj_str}[{}]", self.format(*idx)).into()
                }
            }
            TypeData::TemplateLiteral(spans) => {
                let spans = self.interner.template_list(*spans);
                self.format_template_literal(spans.as_ref()).into()
            }
            TypeData::TypeQuery(sym) => {
                // Check if the symbol is a namespace import (import * as X from "mod")
                // — tsc displays these as `typeof import("mod")` rather than `typeof X`.
                if let Some(arena) = self.symbol_arena
                    && let Some(symbol) = arena.get(SymbolId(sym.0))
                    && symbol.import_name.as_deref() == Some("*")
                    && let Some(ref module_specifier) = symbol.import_module
                {
                    let display_name = Self::strip_module_extension(
                        module_specifier
                            .strip_prefix("./")
                            .or_else(|| module_specifier.strip_prefix("../"))
                            .unwrap_or(module_specifier),
                    );
                    return format!("typeof import(\"{display_name}\")").into();
                }
                if let Some(arena) = self.symbol_arena
                    && let Some(symbol) = arena.get(SymbolId(sym.0))
                    && symbol.has_any_flags(tsz_binder::symbol_flags::ENUM_MEMBER)
                    && let Some(name) = self.format_symbol_name(SymbolId(sym.0))
                {
                    return name.into();
                }
                let name = if let Some(name) = self.resolve_symbol_ref_name(*sym) {
                    name
                } else {
                    format!("Ref({})", sym.0)
                };
                // Enum member TypeQuery types: tsc resolves `typeof W.a` to the
                // enum member type `W.a` and displays without `typeof` prefix.
                // The `resolve_symbol_ref_name` qualifies with enum parents, so
                // a dot in the name reliably indicates an enum member reference.
                if name.contains('.') {
                    name.into()
                } else {
                    format!("typeof {name}").into()
                }
            }
            TypeData::KeyOf(operand) => {
                // `keyof null`, `keyof undefined`, and `keyof void` all
                // evaluate to `never`. tsc displays the reduced form, so
                // collapse to `never` whenever the operand evaluates there.
                // This catches both the direct intrinsic case and substituted
                // forms where a type parameter was bound to a nullish type.
                if matches!(*operand, TypeId::NULL | TypeId::UNDEFINED | TypeId::VOID)
                    || crate::evaluation::evaluate::evaluate_keyof(self.interner, *operand)
                        == TypeId::NEVER
                {
                    return self.format(TypeId::NEVER);
                }
                if *operand == TypeId::NEVER {
                    return self.format(crate::evaluation::evaluate::evaluate_keyof(
                        self.interner,
                        *operand,
                    ));
                }
                // For anonymous concrete object operands, evaluate `keyof` eagerly
                // so diagnostics show the literal key union (e.g. `"x"`) instead
                // of `keyof { x: number; }`. tsc only writes back `keyof <Name>`
                // when the operand has a user-visible name; anonymous shapes are
                // displayed as their evaluated `keyof` result.
                //
                // Skip:
                //   - named objects (preserve `keyof Foo`),
                //   - generic operands (a type parameter must remain visible),
                //   - arrays/tuples (`keyof T[]` widens to `number | "length" | ...`
                //     which is rarely useful in error text),
                //   - intrinsics (same reason).
                if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
                    self.interner.lookup(*operand)
                {
                    let shape = self.interner.object_shape(shape_id);
                    if shape.symbol.is_none() {
                        let evaluated =
                            crate::evaluation::evaluate::evaluate_keyof(self.interner, *operand);
                        // Guard against the evaluator returning the same KeyOf node
                        // (e.g. when the operand cannot be reduced) — that would
                        // recurse infinitely through `format`.
                        if !matches!(self.interner.lookup(evaluated), Some(TypeData::KeyOf(_))) {
                            return self.format(evaluated);
                        }
                    }
                }
                // tsc distributes `keyof` over union and intersection of non-structural types:
                //   keyof (A | B)  →  keyof A & keyof B
                //   keyof (A & B)  →  keyof A | keyof B
                // This applies when the union/intersection members are opaque (type params,
                // named/lazy refs, or applications), not concrete structural types like `{}`.
                // Exception: if any member is a structural object or intrinsic, preserve the
                // undistributed form (e.g. `keyof (T & {})` stays as-is).
                // tsc preserves `keyof (T & {})` undistributed because the
                // empty-object intersection is a non-nullish constraint, not
                // a structural-shape contributor. Restrict the no-distribute
                // guard to that specific shape — generic intersections with
                // *any* structural member (e.g. `T & string`) still
                // distribute as before.
                let any_member_empty_object = |list_id: TypeListId| -> bool {
                    self.interner.type_list(list_id).iter().any(|&m| {
                        matches!(
                            self.interner.lookup(m),
                            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id))
                                if {
                                    let shape = self.interner.object_shape(shape_id);
                                    shape.properties.is_empty()
                                        && shape.string_index.is_none()
                                        && shape.number_index.is_none()
                                }
                        )
                    })
                };
                let distributed = match self.interner.lookup(*operand) {
                    Some(TypeData::Union(list_id)) if !any_member_empty_object(list_id) => {
                        let members = self.interner.type_list(list_id);
                        let parts: Vec<String> = members
                            .iter()
                            .map(|&m| {
                                let inner = self.format(m);
                                // Add parens around complex member types
                                let member_needs_parens = matches!(
                                    self.interner.lookup(m),
                                    Some(
                                        TypeData::Union(_)
                                            | TypeData::Intersection(_)
                                            | TypeData::Conditional(_)
                                    )
                                );
                                if member_needs_parens {
                                    format!("keyof ({inner})")
                                } else {
                                    format!("keyof {inner}")
                                }
                            })
                            .collect();
                        Some(parts.join(" & "))
                    }
                    Some(TypeData::Intersection(list_id)) if !any_member_empty_object(list_id) => {
                        let members = self.interner.type_list(list_id);
                        let parts: Vec<String> = members
                            .iter()
                            .map(|&m| {
                                let inner = self.format(m);
                                let member_needs_parens = matches!(
                                    self.interner.lookup(m),
                                    Some(
                                        TypeData::Union(_)
                                            | TypeData::Intersection(_)
                                            | TypeData::Conditional(_)
                                    )
                                );
                                if member_needs_parens {
                                    format!("keyof ({inner})")
                                } else {
                                    format!("keyof {inner}")
                                }
                            })
                            .collect();
                        Some(parts.join(" | "))
                    }
                    _ => None,
                };
                if let Some(s) = distributed {
                    return s.into();
                }
                // When we suppressed distribution because a member is structural,
                // format the intersection/union members individually so we don't
                // re-collapse `T & {}` into a body-equivalent alias like `QQ<T>`
                // via the formatter's alias-reverse-lookup.  tsc preserves the
                // user's spelling (`keyof (T & {})`) in error messages.
                let inline_compound = match self.interner.lookup(*operand) {
                    Some(TypeData::Union(list_id)) if any_member_empty_object(list_id) => {
                        Some((list_id, " | "))
                    }
                    Some(TypeData::Intersection(list_id)) if any_member_empty_object(list_id) => {
                        Some((list_id, " & "))
                    }
                    _ => None,
                };
                if let Some((list_id, sep)) = inline_compound {
                    let members = self.interner.type_list(list_id);
                    let parts: Vec<String> = members
                        .iter()
                        .map(|&m| self.format(m).into_owned())
                        .collect();
                    return format!("keyof ({})", parts.join(sep)).into();
                }
                let operand_str = self.format(*operand);
                let needs_parens = matches!(
                    self.interner.lookup(*operand),
                    Some(
                        TypeData::Union(_)
                            | TypeData::Intersection(_)
                            | TypeData::Function(_)
                            | TypeData::Callable(_)
                            | TypeData::Conditional(_)
                    )
                );
                if needs_parens {
                    format!("keyof ({operand_str})").into()
                } else {
                    format!("keyof {operand_str}").into()
                }
            }
            TypeData::ReadonlyType(inner) => format!("readonly {}", self.format(*inner)).into(),
            // NoInfer<T> is transparent at the outermost layer of the
            // displayed type — matching tsc, which strips a single outer
            // `NoInfer<>` wrapper but preserves nested `NoInfer<>` markers
            // (e.g. inside a union member or function return). `format()`
            // increments `current_depth` from 0 → 1 before delegating here,
            // so the top-level call sees `current_depth == 1` and inner
            // recursions see `>= 2`.
            TypeData::NoInfer(inner) => {
                if self.current_depth == 1 {
                    self.format(*inner)
                } else {
                    format!("NoInfer<{}>", self.format(*inner)).into()
                }
            }
            TypeData::UniqueSymbol(sym_ref) => {
                // High bit clear = binder-anchored (`const x: unique symbol`); tsc
                // renders these as `typeof x` for nominal discrimination.
                // High bit set = node-anchored (inline annotation, no variable name).
                if sym_ref.0 & SymbolRef::NODE_ANCHOR_BIT == 0 {
                    if let Some(name) = self.resolve_unique_symbol_name(*sym_ref) {
                        return format!("typeof {name}").into();
                    }
                }
                Cow::Borrowed("unique symbol")
            }
            TypeData::Infer(info) => format!("infer {}", self.atom(info.name)).into(),
            TypeData::ThisType => Cow::Borrowed("this"),
            TypeData::StringIntrinsic { kind, type_arg } => {
                let kind_name = match kind {
                    StringIntrinsicKind::Uppercase => "Uppercase",
                    StringIntrinsicKind::Lowercase => "Lowercase",
                    StringIntrinsicKind::Capitalize => "Capitalize",
                    StringIntrinsicKind::Uncapitalize => "Uncapitalize",
                };
                format!("{}<{}>", kind_name, self.format(*type_arg)).into()
            }
            TypeData::Enum(def_id, _member_type) => {
                // Enum members should be qualified with their parent enum name
                // (e.g., `Foo.A` not just `A`). Try the symbol arena first, which
                // walks the parent chain and qualifies enum members correctly.
                // Use the definition's stored symbol_id (not the raw def_id) to
                // find the correct binder symbol.
                if let Some(def_store) = self.def_store
                    && let Some(def) = def_store.get(*def_id)
                    && let Some(sym_raw) = def.symbol_id
                    && let Some(name) = self.format_symbol_name(SymbolId(sym_raw))
                {
                    return name.into();
                }
                // NOTE: We do NOT use format_raw_def_id_symbol_fallback here.
                // DefId and SymbolId are independent ID spaces. Using the raw
                // def_id.0 as a SymbolId would return the name of an unrelated
                // symbol, causing bugs like "Foo.A" displaying as "timeout.A".
                self.format_def_id(*def_id, "Enum").into()
            }
            TypeData::ModuleNamespace(sym) => {
                let name = if let Some(name) = self.resolve_symbol_ref_name(*sym) {
                    name
                } else {
                    format!("module({})", sym.0)
                };
                let name = Self::strip_module_extension(&name);
                format!("typeof import(\"{name}\")").into()
            }
            TypeData::Error => Cow::Borrowed("error"),
        }
    }

    fn format_in_operator_record(&mut self, shape: &ObjectShape) -> Option<String> {
        if !shape.flags.contains(ObjectFlags::IN_OPERATOR_RECORD)
            || shape.properties.len() != 1
            || shape.string_index.is_some()
            || shape.number_index.is_some()
        {
            return None;
        }

        let prop = &shape.properties[0];
        if prop.type_id != TypeId::UNKNOWN || prop.optional || prop.is_method {
            return None;
        }

        let key = self.atom(prop.name);
        let key_display = if prop.is_symbol_named || key.parse::<f64>().is_ok() {
            key.to_string()
        } else {
            format!("\"{key}\"")
        };
        Some(format!("Record<{key_display}, unknown>"))
    }

    /// Strip TypeScript/JavaScript file extensions from module specifier
    /// display names. TSC omits extensions in `typeof import("mod")` output.
    fn strip_module_extension(module_name: &str) -> &str {
        tsz_common::file_extensions::strip_known_extension(module_name)
    }
}
