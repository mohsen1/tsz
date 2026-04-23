//! Computed symbol type analysis: `compute_type_of_symbol`, contextual literal types,
//! and private property access checking.

mod type_alias_variable_alias;

use crate::query_boundaries::common::{contains_infer_types, contains_type_parameters};
use crate::query_boundaries::state::type_environment;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{PropertyInfo, TypeId, Visibility};
impl<'a> CheckerState<'a> {
    pub(crate) fn normalize_namespace_export_declaration_order(props: &mut [PropertyInfo]) {
        props.sort_by(
            |a, b| match (a.declaration_order > 0, b.declaration_order > 0) {
                (true, true) => a.declaration_order.cmp(&b.declaration_order),
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                (false, false) => std::cmp::Ordering::Equal,
            },
        );

        for (idx, prop) in props.iter_mut().enumerate() {
            prop.declaration_order = idx as u32 + 1;
        }
    }

    pub(crate) fn ordered_namespace_export_entries<'b>(
        &self,
        exports_table: &'b tsz_binder::SymbolTable,
    ) -> Vec<(&'b str, SymbolId)> {
        let mut entries: Vec<_> = exports_table
            .iter()
            .map(|(name, &sym_id)| {
                let span = self
                    .get_symbol_globally(sym_id)
                    .or_else(|| self.get_cross_file_symbol(sym_id))
                    .and_then(|symbol| {
                        symbol
                            .first_declaration_span
                            .or(symbol.value_declaration_span)
                    });
                (name.as_str(), sym_id, span)
            })
            .collect();

        entries.sort_by_key(|(name, sym_id, span)| {
            let (start, end) = span.unwrap_or((u32::MAX, u32::MAX));
            (span.is_none(), start, end, *name, sym_id.0)
        });

        entries
            .into_iter()
            .map(|(name, sym_id, _)| (name, sym_id))
            .collect()
    }

    pub(crate) fn type_has_unresolved_inference_holes(&self, type_id: TypeId) -> bool {
        contains_type_parameters(self.ctx.types, type_id)
            || contains_infer_types(self.ctx.types, type_id)
    }

    pub(crate) fn should_skip_namespace_export_name(
        &self,
        exports_table: &tsz_binder::SymbolTable,
        export_name: &str,
        export_sym_id: SymbolId,
    ) -> bool {
        if export_name == "export=" {
            return true;
        }
        if export_name == "_default" && exports_table.get("default").is_some() {
            return true;
        }
        if !export_name.starts_with('_') {
            return false;
        }

        let Some(default_sym_id) = exports_table.get("default") else {
            return false;
        };
        if export_name == "_default" {
            return true;
        }
        if default_sym_id == export_sym_id {
            return true;
        }

        let lookup_symbol = |sym_id: SymbolId| {
            self.get_symbol_globally(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))
        };

        let resolve_alias_target = |sym_id: SymbolId| {
            let mut visited = AliasCycleTracker::new();
            self.resolve_alias_symbol(sym_id, &mut visited)
        };
        let default_target = resolve_alias_target(default_sym_id).unwrap_or(default_sym_id);
        let export_target = resolve_alias_target(export_sym_id).unwrap_or(export_sym_id);
        if export_target == default_target {
            return true;
        }

        let Some(export_symbol) = lookup_symbol(export_sym_id) else {
            return false;
        };
        let Some(default_symbol) = lookup_symbol(default_sym_id) else {
            return false;
        };

        (export_symbol.value_declaration.is_some()
            && export_symbol.value_declaration == default_symbol.value_declaration)
            || export_symbol
                .declarations
                .iter()
                .any(|decl| default_symbol.declarations.contains(decl))
    }

    pub(crate) fn namespace_import_export_property_type(
        &mut self,
        module_name: &str,
        export_sym_id: SymbolId,
    ) -> TypeId {
        let symbol_flags_opt = self
            .get_cross_file_symbol(export_sym_id)
            .or_else(|| self.get_symbol_globally(export_sym_id))
            .map(|symbol| symbol.flags);
        let is_pure_namespace = symbol_flags_opt.is_some_and(|flags| {
            (flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE)) != 0
                && (flags & (symbol_flags::CLASS | symbol_flags::FUNCTION)) == 0
        });
        if is_pure_namespace {
            let prop_type = self.build_namespace_object_type(export_sym_id);
            self.ctx.namespace_module_names.insert(
                prop_type,
                self.imported_namespace_display_module_name(module_name),
            );
            return prop_type;
        }

        let mut prop_type = self.get_type_of_symbol(export_sym_id);
        if symbol_flags_opt.is_some_and(|flags| {
            (flags & symbol_flags::ENUM) != 0 && (flags & symbol_flags::ENUM_MEMBER) == 0
        }) {
            prop_type = self.get_enum_namespace_type_for_value(prop_type);
        }
        prop_type
    }
    pub(crate) fn append_export_equals_import_type_namespace_props(
        &mut self,
        module_name: &str,
        declaring_file_idx: Option<usize>,
        exports_table: &tsz_binder::SymbolTable,
        props: &mut Vec<PropertyInfo>,
    ) {
        let Some(export_equals_sym_id) = exports_table.get("export=") else {
            return;
        };
        let Some(mut export_equals_symbol) = self
            .get_symbol_globally(export_equals_sym_id)
            .or_else(|| self.get_cross_file_symbol(export_equals_sym_id))
        else {
            return;
        };

        if export_equals_symbol.decl_file_idx == u32::MAX
            && let Some(target_idx) = self.ctx.resolve_symbol_file_index(export_equals_sym_id)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            let target_file_name = self
                .ctx
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.clone());
            let target_export_equals_sym = target_file_name
                .as_ref()
                .and_then(|file_name| {
                    target_binder
                        .module_exports
                        .get(file_name)
                        .and_then(|table| table.get("export="))
                })
                .or_else(|| {
                    target_binder
                        .module_exports
                        .get(module_name)
                        .and_then(|table| table.get("export="))
                })
                .or_else(|| {
                    target_binder
                        .module_exports
                        .values()
                        .find_map(|table| table.get("export="))
                });
            if let Some(target_export_equals_sym) = target_export_equals_sym
                && let Some(target_symbol) = target_binder.get_symbol(target_export_equals_sym)
            {
                export_equals_symbol = target_symbol;
            }
        }

        let mut nested_exports = tsz_binder::SymbolTable::new();
        self.merge_export_equals_import_type_members(
            export_equals_symbol,
            declaring_file_idx,
            &mut nested_exports,
        );

        for (name, export_sym_id) in self.ordered_namespace_export_entries(&nested_exports) {
            if self.should_skip_namespace_export_name(&nested_exports, name, export_sym_id) {
                continue;
            }
            if props
                .iter()
                .any(|p| self.ctx.types.resolve_atom_ref(p.name).as_ref() == name)
            {
                continue;
            }
            let export_is_namespace_module = self
                .get_symbol_globally(export_sym_id)
                .or_else(|| self.get_cross_file_symbol(export_sym_id))
                .is_some_and(|symbol| {
                    (symbol.flags
                        & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                            | tsz_binder::symbol_flags::VALUE_MODULE))
                        != 0
                });
            if self.is_type_only_export_symbol(export_sym_id)
                || self.is_export_from_type_only_wildcard(module_name, name)
                || (self.export_symbol_has_no_value(export_sym_id) && !export_is_namespace_module)
                || self.is_export_type_only_from_file(module_name, name, declaring_file_idx)
            {
                continue;
            }

            self.record_cross_file_symbol_if_needed(export_sym_id, name, module_name);
            let mut prop_type =
                self.namespace_import_export_property_type(module_name, export_sym_id);
            prop_type = self.apply_module_augmentations(module_name, name, prop_type);
            let declaration_order = if name == "default" {
                1
            } else {
                props.len() as u32 + 2
            };
            let name_atom = self.ctx.types.intern_string(name);
            props.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order,
                is_string_named: false,
            });
        }
    }

    /// Compute type of a symbol (internal, not cached).
    ///
    /// Uses `TypeLowering` to bridge symbol declarations to solver types.
    /// Returns the computed type and the type parameters used (if any).
    /// IMPORTANT: The type params returned must be the same ones used when lowering
    /// the type body, so that instantiation works correctly.
    pub(crate) fn compute_type_of_symbol(
        &mut self,
        sym_id: SymbolId,
    ) -> (TypeId, Vec<tsz_solver::TypeParamInfo>) {
        let factory = self.ctx.types.factory();
        use tsz_lowering::TypeLowering;

        // Handle cross-file symbol resolution via delegation
        if let Some(result) = self.delegate_cross_arena_symbol_resolution(sym_id) {
            tracing::trace!(
                sym_id = sym_id.0,
                result_type = result.0.0,
                file = self.ctx.file_name.as_str(),
                "compute_type_of_symbol: delegated to cross-arena"
            );
            return result;
        }

        // Use get_symbol_globally to find symbols in lib files and other files
        // Extract needed data to avoid holding borrow across mutable operations
        let (flags, value_decl, declarations, import_module, import_name, escaped_name) =
            match self.get_symbol_globally(sym_id) {
                Some(symbol) => (
                    symbol.flags,
                    symbol.value_declaration,
                    symbol.declarations.clone(),
                    symbol.import_module.clone(),
                    symbol.import_name.clone(),
                    symbol.escaped_name.clone(),
                ),
                None => {
                    // Also try the cross-file symbol
                    match self.get_cross_file_symbol(sym_id) {
                        Some(symbol) => (
                            symbol.flags,
                            symbol.value_declaration,
                            symbol.declarations.clone(),
                            symbol.import_module.clone(),
                            symbol.import_name.clone(),
                            symbol.escaped_name.clone(),
                        ),
                        None => return (TypeId::UNKNOWN, Vec::new()),
                    }
                }
            };

        tracing::trace!(
        sym_id = sym_id.0,
        flags = format!("{flags:#x}").as_str(),
        name = escaped_name.as_str(),
        import_module = ?import_module,
        import_name = ?import_name,
        value_decl = value_decl.0,
        file = self.ctx.file_name.as_str(),
        "compute_type_of_symbol: resolved symbol"
        );
        if (flags & symbol_flags::ALIAS) != 0
            && let Some(ref module_spec) = import_module
            && let Some(imported_name) = import_name.as_deref()
            && imported_name != "*"
            && imported_name != "default"
            && let Some(js_export_type) = self.resolve_js_export_named_type(
                module_spec,
                imported_name,
                Some(self.ctx.current_file_idx),
            )
        {
            return (js_export_type, Vec::new());
        }

        // Export-value wrapper symbols should delegate to their wrapped declaration symbol.
        // This preserves the actual value type for `export var` / `export function` members
        // instead of falling back to implicit `any`.
        if flags & symbol_flags::EXPORT_VALUE != 0
            && let Some(target_sym_id) =
                self.resolve_export_value_wrapper_target_symbol(value_decl, &escaped_name)
            && target_sym_id != sym_id
        {
            let target_type = self
                .merged_value_type_for_symbol_if_available(target_sym_id)
                .unwrap_or_else(|| self.get_type_of_symbol(target_sym_id));
            return (target_type, Vec::new());
        }
        if flags & symbol_flags::EXPORT_VALUE != 0
            && flags & symbol_flags::ALIAS != 0
            && import_module.is_none()
            && let Some(wrapped_type) =
                self.compute_local_export_value_wrapper_type(sym_id, value_decl, &escaped_name)
        {
            return (wrapped_type, Vec::new());
        }

        // Import alias targeting a cross-file class+namespace merge.
        //
        // When `import { X } from "./m"` imports a symbol that has both CLASS and
        // NAMESPACE_MODULE flags, the local import alias only carries
        // ALIAS | NAMESPACE_MODULE (not CLASS).  Without this guard the
        // NAMESPACE_MODULE branch below would return Lazy(DefId) — a type that
        // only exposes namespace exports and misses class constructor properties
        // like `prototype`.
        //
        // Resolve the import target to its original symbol and delegate to
        // `get_type_of_symbol`, which sees the full CLASS | NAMESPACE_MODULE flags
        // and produces the class constructor type merged with namespace exports.
        if flags & symbol_flags::ALIAS != 0
            && flags & symbol_flags::CLASS == 0
            && flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
            && let Some(ref module_spec) = import_module
        {
            let target_name = import_name.as_deref().unwrap_or(&escaped_name);
            if !(target_name == "default"
                && self.source_file_import_uses_system_default_namespace_fallback(module_spec))
            {
                let target_sym_id = self
                    .ctx
                    .binder
                    .resolve_import_with_reexports_type_only(module_spec, target_name)
                    .map(|(sym_id, _is_type_only)| sym_id);

                if let Some(target_sym_id) = target_sym_id
                    && target_sym_id != sym_id
                    && let Some(target_symbol) = self.get_symbol_globally(target_sym_id)
                {
                    if (target_symbol.flags & symbol_flags::CLASS) != 0 {
                        let target_type = self.get_type_of_symbol(target_sym_id);
                        // Also cache the instance type so type-position references
                        // (`let x: Observable<number>`) continue to work.
                        if let Some(&inst) = self.ctx.symbol_instance_types.get(&target_sym_id) {
                            self.ctx.symbol_instance_types.insert(sym_id, inst);
                        }
                        return (target_type, Vec::new());
                    }

                    // Plain imported namespaces like `import React from "react"` do not
                    // need eager structuralization at import time. Keep the alias lazy and
                    // let qualified/member resolution use the namespace export tables on
                    // demand. This avoids building the entire React namespace object for
                    // every import site.
                    let def_id = self.ctx.get_or_create_def_id(sym_id);
                    return (self.ctx.types.factory().lazy(def_id), Vec::new());
                }
            }
        }

        // Class - return class constructor type (merging namespace exports when present)
        // Also compute and cache instance type for TYPE position resolution
        if flags & symbol_flags::CLASS != 0 {
            return self.compute_class_symbol_type(sym_id, flags, value_decl, &declarations);
        }

        // Enum - return TypeData::Enum with DefId for nominal identity checking.
        // The Enum type provides proper enum subtype checking via DefId-based
        // symbol resolution and type equality.
        //
        // CRITICAL: We must compute and cache a structural type (union of member types)
        // before returning TypeData::Enum to prevent infinite recursion in ensure_refs_resolved.
        //
        // IMPORTANT: This check must come BEFORE the NAMESPACE_MODULE check below because
        // enum-namespace merges have both ENUM and NAMESPACE_MODULE flags. We want to
        // handle them as enums (returning TypeData::Enum) rather than as namespaces (returning Lazy).
        if flags & symbol_flags::ENUM != 0 {
            // Create DefId first
            let def_id = self.ctx.get_or_create_def_id(sym_id);

            // Collect all enum declaration nodes. Merged enums (multiple
            // `const enum E { ... }` blocks) contribute members from every
            // declaration, so we must iterate all of them.
            let enum_decl_indices: Vec<NodeIndex> = {
                let mut indices = Vec::new();
                for &decl in &declarations {
                    if decl.is_some() && self.ctx.arena.get_enum_at(decl).is_some() {
                        indices.push(decl);
                    }
                }
                // Fallback: if no declaration matched as enum, try value_decl
                if indices.is_empty()
                    && value_decl.is_some()
                    && self.ctx.arena.get_enum_at(value_decl).is_some()
                {
                    indices.push(value_decl);
                }
                indices
            };

            // Compute the union type of all enum member types.
            // Also pre-cache each member symbol type so `E.Member` property access
            // can hit `ctx.symbol_types` directly instead of running full symbol
            // resolution for each distinct member.
            let mut member_types = Vec::new();
            // Track auto-increment counter for numeric enum members.
            // TypeScript auto-increments from 0 for the first member, and from
            // previous_value + 1 for subsequent members without initializers.
            // When a member has an explicit numeric initializer, the counter
            // resets to initializer_value + 1. String initializers break auto-increment.
            // The counter resets at the start of each declaration block.
            //
            // We collect (member_type, member_name, member_idx) tuples first,
            // then do env updates in a separate pass to avoid borrow conflicts
            // with `self.enum_member_type_from_decl` / `self.evaluate_constant_expression`.
            let mut member_entries: Vec<(TypeId, Option<String>, NodeIndex)> = Vec::new();
            for &decl_idx in &enum_decl_indices {
                let Some(enum_decl) = self.ctx.arena.get_enum_at(decl_idx) else {
                    continue;
                };
                member_types.reserve(enum_decl.members.nodes.len());
                let mut auto_value: Option<f64> = Some(0.0);
                for &member_idx in &enum_decl.members.nodes {
                    if let Some(member) = self.ctx.arena.get_enum_member_at(member_idx) {
                        let has_initializer = member.initializer.is_some();
                        let mut member_type = self.enum_member_type_from_decl(member_idx);

                        if has_initializer {
                            // Member has explicit initializer. Evaluate it to determine
                            // the next auto-increment value.
                            if let Some(val) = self.evaluate_constant_expression(member.initializer)
                            {
                                auto_value = Some(val + 1.0);
                            } else {
                                // String literal or unevaluable — auto-increment is broken
                                auto_value = None;
                            }
                        } else if member_type == TypeId::NUMBER {
                            // No explicit initializer — use auto-increment if available.
                            // This fixes mapped types over numeric enums: { [k in E]?: string }
                            // needs individual property keys ("0", "1", "2"), not `number`.
                            if let Some(val) = auto_value {
                                member_type = factory.literal_number(val);
                                auto_value = Some(val + 1.0);
                            }
                        }

                        if member_type != TypeId::ERROR {
                            member_types.push(member_type);
                        }

                        // Collect member info for env caching below.
                        let member_name = self.get_property_name(member.name);
                        member_entries.push((member_type, member_name, member_idx));
                    }
                }
            }

            // Pre-cache member symbol types (separate pass to avoid borrow conflicts).
            // This avoids per-member `get_type_of_symbol` overhead in
            // hot paths such as large enum property-access switches.
            //
            // Collect (member_def_id, member_enum_type) pairs so we can mirror
            // them into type_environment after releasing the type_env borrow.
            let mut member_def_entries: Vec<(tsz_solver::DefId, TypeId)> = Vec::new();
            {
                let mut maybe_env = self.ctx.type_env.try_borrow_mut().ok();
                for &(member_type, ref member_name, _member_idx) in &member_entries {
                    if let Some(name) = member_name
                        && let Some(member_sym_id) = self
                            .ctx
                            .binder
                            .get_symbol(sym_id)
                            .and_then(|enum_symbol| enum_symbol.exports.as_ref())
                            .and_then(|exports| exports.get(name))
                    {
                        let member_def_id = self.ctx.get_or_create_def_id(member_sym_id);
                        let member_enum_type = factory.enum_type(member_def_id, member_type);
                        self.ctx
                            .symbol_types
                            .insert(member_sym_id, member_enum_type);
                        if let Some(env) = maybe_env.as_mut() {
                            env.insert(tsz_solver::SymbolRef(member_sym_id.0), member_enum_type);
                            if member_def_id != tsz_solver::DefId::INVALID {
                                env.insert_def(member_def_id, member_enum_type);
                                // Register parent-child relationship for enum member widening
                                env.register_enum_parent(member_def_id, def_id);
                                member_def_entries.push((member_def_id, member_enum_type));
                            }
                        }
                    }
                }
            }
            // Mirror enum member DefId entries into type_environment for consistency
            if !member_def_entries.is_empty()
                && let Ok(mut env) = self.ctx.type_environment.try_borrow_mut()
            {
                for &(member_def_id, member_enum_type) in &member_def_entries {
                    env.insert_def(member_def_id, member_enum_type);
                    env.register_enum_parent(member_def_id, def_id);
                }
            }

            // Create the structural type (union of member types, or NUMBER/STRING for homogeneous enums)
            let structural_type = if member_types.is_empty() {
                // Empty enum - default to NUMBER
                TypeId::NUMBER
            } else if member_types.len() == 1 {
                // Single member - use that type
                member_types[0]
            } else {
                // Multiple members - create a union
                factory.union(member_types)
            };

            // Cache the structural type in both environments for compatibility.
            // Note: Enum types now use TypeData::Enum(def_id, member_type) directly.
            self.ctx.register_def_in_envs(def_id, structural_type);

            // CRITICAL: Return TypeData::Enum(def_id, structural_type) NOT Lazy(def_id)
            // - Lazy(def_id) creates infinite recursion in ensure_refs_resolved
            // - structural_type alone loses nominal identity (E1 becomes 0 | 1)
            // - Enum(def_id, structural_type) preserves both:
            //   1. DefId for nominal identity (E1 != E2)
            //   2. structural_type for assignability to primitives (E1 <: number)
            let enum_type = factory.enum_type(def_id, structural_type);

            // Compute and cache the enum namespace object type for `typeof Enum` / `keyof typeof Enum`.
            // This object has member names as properties (e.g., { Up: Direction.Up, Down: Direction.Down }).
            // Always compute this — both plain enums and enum+namespace merges need it.
            let ns_type = self.merge_namespace_exports_into_object(sym_id, enum_type);
            self.ctx.enum_namespace_types.insert(sym_id, ns_type);
            // Register in both TypeEnvironment instances so the solver's evaluator
            // and the flow analyzer can both access enum namespace types.
            if let Ok(mut env) = self.ctx.type_env.try_borrow_mut() {
                env.register_enum_namespace_type(def_id, ns_type);
            }
            if let Ok(mut env) = self.ctx.type_environment.try_borrow_mut() {
                env.register_enum_namespace_type(def_id, ns_type);
            }
            // Register DefId <-> SymbolId mapping for enum type resolution
            self.ctx
                .register_resolved_type(sym_id, enum_type, Vec::new());

            return (enum_type, Vec::new());
        }

        // Namespace / Module
        // Return a Ref type AND register DefId mapping for gradual migration.
        // The Ref type is needed because resolve_qualified_name and other code
        // extracts SymbolRef from the type to look up the symbol's exports map.
        // Skip this when the symbol is also a FUNCTION — the FUNCTION branch below
        // handles merging namespace exports into the function's callable type.
        //
        // IMPORTANT: This check must come AFTER the ENUM check above because
        // enum-namespace merges have both ENUM and NAMESPACE_MODULE flags. We want to
        // handle them as enums (returning TypeData::Enum) rather than as namespaces.
        //
        // IMPORTANT: Skip when the symbol also has TYPE_ALIAS flag — a merged
        // `type Foo = ...; namespace Foo { ... }` declaration must be resolved
        // as a type alias (returning the body type) not as a namespace module.
        // The namespace exports are still accessible via the symbol's exports map.
        if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0
            && flags & symbol_flags::FUNCTION == 0
            && flags & symbol_flags::VARIABLE == 0
            && flags & symbol_flags::TYPE_ALIAS == 0
        {
            return self.compute_namespace_symbol_type(sym_id, flags);
        }

        // Enum member - determine type from parent enum
        if flags & symbol_flags::ENUM_MEMBER != 0 {
            return self.compute_enum_member_symbol_type(sym_id, value_decl);
        }

        // Get/Set accessors - resolve type from the accessor declaration's type annotation.
        // For get accessors, the type is the return type annotation (or inferred from body).
        // For set accessors, the type is the first parameter's type annotation.
        if flags & (symbol_flags::GET_ACCESSOR | symbol_flags::SET_ACCESSOR) != 0 {
            for &decl_idx in &declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(accessor) = self.ctx.arena.get_accessor(node) else {
                    continue;
                };
                if node.kind == syntax_kind_ext::GET_ACCESSOR {
                    // Get accessor: return type is the type annotation
                    if accessor.type_annotation.is_some() {
                        let return_type = self.get_type_from_type_node(accessor.type_annotation);
                        return (return_type, Vec::new());
                    }
                    // No type annotation - try to infer from body return type
                    // Fall through to use get_type_of_node if body exists
                    if accessor.body.is_some() {
                        let body_type = self.get_type_of_node(accessor.body);
                        if body_type != TypeId::ERROR && body_type != TypeId::UNKNOWN {
                            return (body_type, Vec::new());
                        }
                    }
                } else if node.kind == syntax_kind_ext::SET_ACCESSOR {
                    // Set accessor: type is the first parameter's type annotation
                    if let Some(&param_idx) = accessor.parameters.nodes.first()
                        && let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && param.type_annotation.is_some()
                        && !self.ctx.is_js_file()
                    {
                        let param_type = self.get_type_from_type_node(param.type_annotation);
                        return (param_type, Vec::new());
                    }
                }
            }
        }

        // Methods merged across lib/interface declarations should preserve overloads from
        // every declaration arena, not just the first value declaration.
        if flags & symbol_flags::METHOD != 0 {
            let mut merged_method_type = None;

            for &decl_idx in &declarations {
                let decl_type = self.type_of_declaration_node_for_symbol(sym_id, decl_idx);
                if matches!(decl_type, TypeId::ERROR | TypeId::UNKNOWN) {
                    continue;
                }

                merged_method_type = Some(if let Some(current) = merged_method_type {
                    self.merge_interface_types(decl_type, current)
                } else {
                    decl_type
                });
            }

            if let Some(method_type) = merged_method_type {
                return (method_type, Vec::new());
            }
        }

        // Function - build function type or callable overload set.
        // For symbols merged as interface+function, prefer the interface path below
        // when computing the symbol's semantic type (type-position behavior).
        if flags & symbol_flags::FUNCTION != 0 && flags & symbol_flags::INTERFACE == 0 {
            let declaration_is_function_value_in_arena =
                |arena: &tsz_parser::parser::node::NodeArena, decl_idx: NodeIndex| -> bool {
                    if decl_idx.is_none() {
                        return false;
                    }
                    let Some(node) = arena.get(decl_idx) else {
                        return false;
                    };
                    match node.kind {
                        syntax_kind_ext::FUNCTION_DECLARATION => true,
                        syntax_kind_ext::BINARY_EXPRESSION => {
                            let Some(binary_node) = arena.get(decl_idx) else {
                                return false;
                            };
                            let Some(binary) = arena.get_binary_expr(binary_node) else {
                                return false;
                            };
                            if !self.is_assignment_operator(binary.operator_token) {
                                return false;
                            }
                            arena
                                .get(binary.right)
                                .is_some_and(|rhs| rhs.is_function_expression_or_arrow())
                        }
                        syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                            let Some(ext) = arena.get_extended(decl_idx) else {
                                return false;
                            };
                            if ext.parent.is_none() {
                                return false;
                            }
                            let parent_idx = ext.parent;
                            let Some(parent_node) = arena.get(parent_idx) else {
                                return false;
                            };
                            let Some(binary) = arena.get_binary_expr(parent_node) else {
                                return false;
                            };
                            if binary.left != decl_idx
                                || !self.is_assignment_operator(binary.operator_token)
                            {
                                return false;
                            }
                            arena
                                .get(binary.right)
                                .is_some_and(|rhs| rhs.is_function_expression_or_arrow())
                        }
                        syntax_kind_ext::VARIABLE_DECLARATION => {
                            let Some(var_decl) = arena.get_variable_declaration(node) else {
                                return false;
                            };
                            let Some(init_node) = arena.get(var_decl.initializer) else {
                                return false;
                            };
                            init_node.is_function_expression_or_arrow()
                        }
                        _ => false,
                    }
                };

            let declaration_arenas_for_declaration = |sym_id: SymbolId, decl_idx: NodeIndex| {
                let mut arenas = Vec::new();

                if self.ctx.arena.get(decl_idx).is_some() {
                    arenas.push(self.ctx.arena);
                }

                if let Some(symbol_arena) = self.ctx.binder.symbol_arenas.get(&sym_id) {
                    let symbol_arena_ref = symbol_arena.as_ref();
                    if !std::ptr::eq(symbol_arena_ref, self.ctx.arena) {
                        arenas.push(symbol_arena_ref);
                    }
                }

                if let Some(arenas_for_decl) =
                    self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                {
                    for arena in arenas_for_decl.iter() {
                        let arena_ref = arena.as_ref();
                        if !arenas.iter().any(|a| std::ptr::eq(*a, arena_ref)) {
                            arenas.push(arena_ref);
                        }
                    }
                }

                arenas
            };

            let declaration_is_function_value = |decl_idx: NodeIndex| -> bool {
                let mut observed = false;
                for arena in declaration_arenas_for_declaration(sym_id, decl_idx) {
                    if arena.get(decl_idx).is_none() {
                        continue;
                    }
                    observed = true;
                    if !declaration_is_function_value_in_arena(arena, decl_idx) {
                        return false;
                    }
                }
                observed
            };

            let mut declaration_indices = declarations.to_vec();
            if value_decl.is_some() && !declaration_indices.contains(&value_decl) {
                declaration_indices.push(value_decl);
            }
            for (&(entry_sym_id, decl_idx), _) in self.ctx.binder.declaration_arenas.iter() {
                if entry_sym_id == sym_id && !declaration_indices.contains(&decl_idx) {
                    declaration_indices.push(decl_idx);
                }
            }

            let has_mixed_non_callable_declaration = if self.is_js_file()
                && self.ctx.compiler_options.check_js
            {
                declaration_indices.iter().copied().any(|decl_idx| {
                    !self.declaration_is_checked_js_constructor_value_declaration(sym_id, decl_idx)
                        && !declaration_is_function_value(decl_idx)
                })
            } else {
                false
            };

            if has_mixed_non_callable_declaration {
                return self.compute_type_of_symbol_type_alias_variable_alias(
                    sym_id,
                    flags,
                    value_decl,
                    &declarations,
                    &import_module,
                    &import_name,
                    &escaped_name,
                    &factory,
                );
            }

            use tsz_solver::CallableShape;

            let mut overloads = Vec::new();
            let mut implementation_decl = NodeIndex::NONE;

            for &decl_idx in &declarations {
                let Some(node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(func) = self.ctx.arena.get_function(node) else {
                    continue;
                };

                if func.body.is_none() {
                    overloads.push(self.call_signature_from_function(func, decl_idx));
                } else {
                    implementation_decl = decl_idx;
                }
            }

            let function_type = if !overloads.is_empty() {
                let shape = CallableShape {
                    call_signatures: overloads,
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                };
                factory.callable(shape)
            } else if value_decl.is_some() {
                self.get_type_of_function(value_decl)
            } else if implementation_decl.is_some() {
                self.get_type_of_function(implementation_decl)
            } else {
                TypeId::UNKNOWN
            };

            let function_type =
                self.augment_callable_type_with_expandos(&escaped_name, sym_id, function_type);

            // If function is merged with namespace, merge namespace exports into function type
            // This allows accessing namespace members through the function name: Model.Options
            if flags & (symbol_flags::NAMESPACE_MODULE | symbol_flags::VALUE_MODULE) != 0 {
                // Pre-cache the function type before merging namespace exports.
                // This breaks circularity when the namespace body references the function
                // itself (e.g., `namespace point { export var origin = point(0, 0); }`).
                // Without this, the placeholder is Lazy(DefId) with no call signatures,
                // causing false TS2349 "not callable" errors.
                self.ctx.symbol_types.insert(sym_id, function_type);
                return self.merge_namespace_exports_into_function(sym_id, function_type);
            }

            return (function_type, Vec::new());
        }

        // NOTE: When a symbol is BOTH an interface AND a variable (e.g., `interface Error` +
        // `declare var Error: ErrorConstructor`), we fall through to the INTERFACE block below.
        // The interface type is the correct type for TYPE position (e.g., `var e: Error`).
        // VALUE position (e.g., `new Error()`) is handled separately by `get_type_of_identifier`
        // which has its own merged-symbol resolution via `type_of_value_declaration_for_symbol`.

        // Interface - return interface type with call signatures
        if flags & symbol_flags::INTERFACE != 0 {
            // Merged lib symbols can live in the main binder but still carry
            // declaration nodes from other arenas. Lowering those declarations
            // against the current arena produces incomplete interface shapes
            // (e.g. Date without getTime, PromiseConstructor without resolve/race/new).
            //
            // We check two conditions (either triggers the lib path):
            // 1. Per-declaration check: NodeIndex is out of range OR declaration_arenas
            //    has an entry pointing to a different arena
            // 2. Fallback: symbol_arenas has an entry for this symbol, meaning it was
            //    merged from a lib file. This catches cross-arena NodeIndex collisions
            //    where the index is valid in the main arena but maps to a different node
            let has_out_of_arena_decl = declarations.iter().any(|&decl_idx| {
                if self.ctx.arena.get(decl_idx).is_none() {
                    return true;
                }
                if let Some(decl_arena) = self
                    .ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .and_then(|v| v.first())
                {
                    return !std::ptr::eq(decl_arena.as_ref(), self.ctx.arena);
                }
                false
            });
            // Detect cross-file declarations sharing the same NodeIndex as
            // a local declaration. The binder merge skips duplicate NodeIndex
            // values, so `declarations` has only one entry but
            // `declaration_arenas` stores multiple arenas for it.
            let has_cross_file_same_index = declarations.iter().any(|&decl_idx| {
                self.ctx
                    .binder
                    .declaration_arenas
                    .get(&(sym_id, decl_idx))
                    .is_some_and(|arenas| {
                        arenas.len() > 1
                            && arenas
                                .iter()
                                .any(|a| !std::ptr::eq(a.as_ref(), self.ctx.arena))
                    })
            });
            // Only use the is_lib_symbol fallback when the per-declaration check
            // couldn't determine the arena origin (i.e. no declaration_arenas entry
            // AND the declaration exists in the current arena). The is_lib_symbol
            // flag is set for ALL symbols that were merged during multi-file
            // compilation, including user-defined interfaces. Using it unconditionally
            // causes user interfaces to skip merge_interface_heritage_types, which
            // loses inherited call/construct signatures (TS2345 false positives).
            let is_lib_symbol = if has_out_of_arena_decl {
                false // Already determined cross-arena by per-decl check
            } else {
                // When all declarations are in the current arena, check if any
                // actually maps to an InterfaceDeclaration node. User-defined
                // interfaces will have real interface nodes; cross-arena collisions
                // will have NodeIndexes that point to unrelated nodes. Only fall
                // back to lib resolution when there's no real interface decl.
                let has_real_interface_decl = declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|node| self.ctx.arena.get_interface(node))
                        .is_some()
                });
                !has_real_interface_decl && self.ctx.binder.symbol_arenas.contains_key(&sym_id)
            };
            // When all declarations are from lib arenas (no local interface
            // declarations), resolve via the lib type directly. But when the
            // user has local interface declarations that augment/extend the lib
            // type (e.g., `interface Node { forEachChild(...) }`), we must fall
            // through to the full merge path so user-declared members are included.
            let has_local_interface_decl = declarations.iter().any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .and_then(|node| self.ctx.arena.get_interface(node))
                    .is_some()
            });
            if (has_out_of_arena_decl || is_lib_symbol)
                && !has_local_interface_decl
                && !self.ctx.lib_contexts.is_empty()
                && let Some(lib_type) = self.resolve_lib_type_by_name(&escaped_name)
            {
                // Preserve diagnostic formatting for canonical lib interfaces
                // by recording the resolved object shape on this symbol's DefId.
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                if let Some(shape) = type_environment::object_shape(self.ctx.types, lib_type) {
                    self.ctx.definition_store.set_instance_shape(def_id, shape);
                }

                // Register the TypeId→DefId mapping so the type formatter can
                // display "ObjectConstructor", "SymbolConstructor", etc. instead
                // of expanding the full object literal.  Guard: only register
                // when no mapping exists yet, to avoid overwriting a mapping
                // established by a prior, more specific resolution.
                if self
                    .ctx
                    .definition_store
                    .find_def_for_type(lib_type)
                    .is_none()
                {
                    self.ctx
                        .definition_store
                        .register_type_to_def(lib_type, def_id);
                }

                return (lib_type, Vec::new());
            }

            if !declarations.is_empty() {
                // Get type parameters from the first interface declaration.
                // When cross-file declarations exist, the first declaration may be
                // from another arena. Try all local declarations to find type params.
                let mut params = Vec::new();
                let mut updates = Vec::new();

                if has_out_of_arena_decl {
                    for &decl_idx in declarations.iter() {
                        if let Some(node) = self.ctx.arena.get(decl_idx)
                            && let Some(interface) = self.ctx.arena.get_interface(node)
                        {
                            (params, updates) =
                                self.push_type_parameters(&interface.type_parameters);
                            break;
                        }
                    }
                } else {
                    let first_decl = declarations.first().copied().unwrap_or(NodeIndex::NONE);
                    if let Some(node) = self.ctx.arena.get(first_decl)
                        && let Some(interface) = self.ctx.arena.get_interface(node)
                    {
                        (params, updates) = self.push_type_parameters(&interface.type_parameters);
                    }
                }

                // Pre-compute computed property names that the lowering can't resolve from AST alone.
                let computed_names = self.precompute_computed_property_names(&declarations);
                let prewarmed_type_params =
                    self.prewarm_member_type_reference_params(&declarations);
                let namespace_prefix = declarations.iter().copied().find_map(|decl_idx| {
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|node| self.ctx.arena.get_interface(node))
                        .and_then(|_| self.declaration_namespace_prefix(self.ctx.arena, decl_idx))
                });

                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                // Use DefId resolver so interface member types like `inner: Inner`
                // produce Lazy(DefId) instead of TypeId::ERROR. Without this, any
                // type reference to another interface/type alias in an interface body
                // fails to resolve.
                let def_id_resolver = |node_idx: NodeIndex| -> Option<tsz_solver::def::DefId> {
                    self.resolve_type_symbol_for_lowering(node_idx)
                        .map(|sym_id_raw| {
                            self.ctx
                                .get_or_create_def_id(tsz_binder::SymbolId(sym_id_raw))
                        })
                };
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let computed_name_resolver = |expr_idx: NodeIndex| -> Option<tsz_common::Atom> {
                    computed_names.get(&expr_idx).copied()
                };
                let lazy_type_params_resolver = |def_id: tsz_solver::def::DefId| {
                    prewarmed_type_params
                        .get(&def_id)
                        .cloned()
                        .or_else(|| self.ctx.get_def_type_params(def_id))
                };
                let name_resolver = |type_name: &str| -> Option<tsz_solver::def::DefId> {
                    namespace_prefix
                        .as_ref()
                        .and_then(|prefix| {
                            let mut scoped =
                                String::with_capacity(prefix.len() + 1 + type_name.len());
                            scoped.push_str(prefix);
                            scoped.push('.');
                            scoped.push_str(type_name);
                            self.resolve_entity_name_text_to_def_id_for_lowering(&scoped)
                        })
                        .or_else(|| self.resolve_entity_name_text_to_def_id_for_lowering(type_name))
                };
                let lowering = TypeLowering::with_hybrid_resolver(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &def_id_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings)
                .with_computed_name_resolver(&computed_name_resolver)
                .with_lazy_type_params_resolver(&lazy_type_params_resolver)
                .with_name_def_id_resolver(&name_resolver);
                let mut interface_type =
                    lowering.lower_interface_declarations_with_symbol(&declarations, sym_id);

                // Cross-file interface declaration merging: when declarations from
                // other arenas exist, lower each with a TypeLowering bound to its
                // source arena and merge the members structurally.
                // Handles both cases:
                //  - Different NodeIndex (has_out_of_arena_decl): decl not in local arena
                //  - Same NodeIndex collision (has_cross_file_same_index): decl IS in
                //    local arena, but declaration_arenas has additional non-local arenas
                if has_out_of_arena_decl || has_cross_file_same_index {
                    for &decl_idx in declarations.iter() {
                        let Some(arenas) =
                            self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                        else {
                            continue;
                        };
                        for arena in arenas.iter() {
                            // Skip the local arena — already lowered above
                            if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                                continue;
                            }
                            if let Some(node) = arena.get(decl_idx)
                                && arena.get_interface(node).is_some()
                            {
                                let cross_type =
                                    self.lower_cross_file_interface_decl(arena, decl_idx, sym_id);
                                if cross_type != TypeId::ERROR {
                                    interface_type =
                                        self.merge_interface_types(interface_type, cross_type);
                                }
                            }
                        }
                    }
                }

                let mut interface_type =
                    self.merge_interface_heritage_types(&declarations, interface_type);

                // Merge heritage types from cross-file declarations that
                // merge_interface_heritage_types couldn't process (it uses
                // self.ctx.arena which doesn't contain cross-file nodes).
                if has_out_of_arena_decl || has_cross_file_same_index {
                    interface_type =
                        self.merge_cross_file_heritage(&declarations, sym_id, interface_type);
                }

                if let Some(shape) = type_environment::object_shape(self.ctx.types, interface_type)
                {
                    self.ctx
                        .definition_store
                        .set_instance_shape(self.ctx.get_or_create_def_id(sym_id), shape);
                }

                // Restore the type parameter scope
                self.pop_type_parameters(updates);

                // Return the interface type along with the type parameters that were used
                return (interface_type, params);
            }
            if value_decl.is_some() {
                return (self.get_type_of_interface(value_decl), Vec::new());
            }
            return (TypeId::UNKNOWN, Vec::new());
        }

        // Remaining symbol kinds (type alias, class property, variable, alias)
        // are handled in a separate submodule to keep file sizes manageable.
        self.compute_type_of_symbol_type_alias_variable_alias(
            sym_id,
            flags,
            value_decl,
            &declarations,
            &import_module,
            &import_name,
            &escaped_name,
            &factory,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::{QueryDatabase, TypeInterner};

    fn make_prop(name: tsz_common::Atom, declaration_order: u32) -> PropertyInfo {
        PropertyInfo {
            name,
            type_id: TypeId::ANY,
            write_type: TypeId::ANY,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order,
            is_string_named: false,
        }
    }

    #[test]
    fn synthetic_namespace_default_normalization_preserves_default_before_augmentations() {
        let types = TypeInterner::new();
        let default_atom = types.intern_string("default");
        let configs_atom = types.intern_string("configs");
        let mut props = vec![make_prop(configs_atom, 0), make_prop(default_atom, 1)];

        CheckerState::normalize_namespace_export_declaration_order(&mut props);
        let namespace_type = types.factory().object(props);
        let shape = crate::query_boundaries::common::object_shape_for_type(&types, namespace_type)
            .expect("namespace type should have an object shape");
        let shape_props: Vec<_> = shape
            .properties
            .iter()
            .map(|prop| {
                (
                    types.resolve_atom_ref(prop.name).to_string(),
                    prop.declaration_order,
                )
            })
            .collect();

        assert_eq!(
            shape_props,
            vec![("configs".to_string(), 2), ("default".to_string(), 1)]
        );
    }

    #[test]
    fn ordered_namespace_export_entries_follow_first_declaration_span() {
        use tsz_binder::{BinderState, SymbolTable, symbol_flags};
        use tsz_checker::context::{CheckerOptions, ScriptTarget};
        use tsz_parser::parser::ParserState;

        let mut parser = ParserState::new("/test.ts".to_string(), String::new());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let third = binder
            .symbols
            .alloc(symbol_flags::EXPORT_VALUE, "third".to_string());
        binder
            .symbols
            .get_mut(third)
            .expect("third symbol should exist")
            .add_declaration(NodeIndex::NONE, Some((30, 31)));

        let first = binder
            .symbols
            .alloc(symbol_flags::EXPORT_VALUE, "first".to_string());
        binder
            .symbols
            .get_mut(first)
            .expect("first symbol should exist")
            .add_declaration(NodeIndex::NONE, Some((10, 11)));

        let second = binder
            .symbols
            .alloc(symbol_flags::EXPORT_VALUE, "second".to_string());
        binder
            .symbols
            .get_mut(second)
            .expect("second symbol should exist")
            .add_declaration(NodeIndex::NONE, Some((20, 21)));

        let missing_span = binder
            .symbols
            .alloc(symbol_flags::EXPORT_VALUE, "missingSpan".to_string());

        let mut exports = SymbolTable::new();
        exports.set("third".to_string(), third);
        exports.set("missingSpan".to_string(), missing_span);
        exports.set("first".to_string(), first);
        exports.set("second".to_string(), second);

        let types = TypeInterner::new();
        let checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "/test.ts".to_string(),
            CheckerOptions {
                target: ScriptTarget::ES2020,
                ..CheckerOptions::default()
            },
        );

        let ordered = checker.ordered_namespace_export_entries(&exports);
        let names: Vec<_> = ordered
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect();
        assert_eq!(
            names,
            vec![
                "first".to_string(),
                "second".to_string(),
                "third".to_string(),
                "missingSpan".to_string()
            ]
        );
    }
}
