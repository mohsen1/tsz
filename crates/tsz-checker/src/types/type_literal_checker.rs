//! Type literal checking (type resolution, references, and signatures within type literals).
//!
//! Type literals represent inline object types like `{ x: string; y: number }` or
//! callable types with call/construct signatures.

use super::type_node_helpers::type_node_includes_explicit_undefined;
use crate::query_boundaries::signature_building as signature_building_boundary;
use crate::query_boundaries::type_construction as construction_boundary;
use crate::state::{CheckerState, ParamTypeResolutionMode};
use crate::symbol_resolver::TypeSymbolResolution;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

// =============================================================================
// Type Literal Type Checking
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Type Node Resolution in Type Literals
    // =========================================================================

    /// Get type from a type node within a type literal context.
    ///
    /// This handles special resolution needed for types declared within
    /// type literals, such as recursive type references.
    pub(crate) fn get_type_from_type_node_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return self.get_type_from_type_reference_in_type_literal(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_QUERY {
            // Check node_types cache first — resolve_type_queries_with_flow may have
            // pre-resolved this typeof with flow narrowing.
            if !self.is_type_query_in_non_flow_sensitive_signature_parameter(idx)
                && let Some(&cached) = self.ctx.node_types.get(&idx.0)
                && cached != TypeId::ERROR
            {
                return cached;
            }
            return self.get_type_from_type_query(idx);
        }
        if node.kind == syntax_kind_ext::UNION_TYPE {
            if let Some(composite) = self.ctx.arena.get_composite_type(node) {
                let members = composite
                    .types
                    .nodes
                    .iter()
                    .map(|&member_idx| self.get_type_from_type_node_in_type_literal(member_idx))
                    .collect::<Vec<_>>();
                if let Some(collapsed) =
                    crate::query_boundaries::type_predicates::collapse_pure_nullish_union_nonstrict(
                        self.ctx.compiler_options.strict_null_checks,
                        &members,
                    )
                {
                    return collapsed;
                }
                // #16580: same non-strict scalar null/undefined absorption
                // as the top-level union-type-node resolvers, generalized to
                // a union written inside a type literal member.
                if let Some(reduced) =
                    crate::query_boundaries::type_predicates::nonstrict_union_members_absorb_nullish_scalars(
                        self.ctx.compiler_options.strict_null_checks,
                        &members,
                    )
                {
                    if reduced.len() == 1 {
                        return reduced[0];
                    }
                    return construction_boundary::type_node_union(self.ctx.types, reduced);
                }
                return construction_boundary::type_node_union(self.ctx.types, members);
            }
            return TypeId::ERROR;
        }
        if node.kind == syntax_kind_ext::ARRAY_TYPE {
            if let Some(array_type) = self.ctx.arena.get_array_type(node) {
                let elem_type =
                    self.get_type_from_type_node_in_type_literal(array_type.element_type);
                return construction_boundary::type_node_array(self.ctx.types, elem_type);
            }
            return TypeId::ERROR; // Missing array type data - propagate error
        }
        if node.kind == syntax_kind_ext::TYPE_OPERATOR {
            // Handle readonly and other type operators in type literals
            return self.get_type_from_type_operator(idx);
        }
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return self.get_type_from_type_literal(idx);
        }

        self.get_type_from_type_node(idx)
    }

    fn get_type_from_type_reference_in_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return TypeId::ERROR; // Missing type reference data - propagate error
        };

        let type_name_idx = type_ref.type_name;
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());

        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && name_node.kind == syntax_kind_ext::QUALIFIED_NAME
        {
            let sym_id = match self.resolve_qualified_symbol_in_type_position(type_name_idx) {
                TypeSymbolResolution::Type(sym_id) => sym_id,
                TypeSymbolResolution::ValueOnly(sym_id) => {
                    let name = self
                        .entity_name_text(type_name_idx)
                        .unwrap_or_else(|| "<unknown>".to_string());
                    self.report_wrong_meaning(
                        &name,
                        type_name_idx,
                        sym_id,
                        crate::query_boundaries::name_resolution::NameLookupKind::Value,
                        crate::query_boundaries::name_resolution::NameLookupKind::Type,
                    );
                    return TypeId::ERROR;
                }
                TypeSymbolResolution::NotFound => {
                    let _ = self.resolve_qualified_name(type_name_idx);
                    return TypeId::ERROR;
                }
            };
            // Stable-identity helper: resolve symbol body + create Lazy(DefId)
            let base_type = self.resolve_symbol_as_lazy_type(sym_id);
            if has_type_args {
                let type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .map(|args| {
                        args.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                return construction_boundary::type_node_application(
                    self.ctx.types,
                    base_type,
                    type_args,
                );
            }
            return base_type;
        }

        if let Some(name_node) = self.ctx.arena.get(type_name_idx)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = ident.escaped_text.as_str();

            // Type literal members inside namespaces should prefer same-namespace
            // type declarations before falling back to file/global symbols.
            if self.lookup_type_parameter(name).is_none()
                && let Some(sym_id) =
                    self.resolve_unqualified_name_in_enclosing_namespace(type_name_idx, name)
            {
                // Validate type arguments against constraints (TS2344)
                if has_type_args
                    && let Some(args) = &type_ref.type_arguments
                    && !self.is_inside_type_parameter_declaration(idx)
                {
                    self.validate_type_reference_type_arguments(sym_id, args, idx);
                }
                // Stable-identity helper: resolve symbol body + create Lazy(DefId)
                let base_type = self.resolve_symbol_as_lazy_type_named(sym_id, name);
                if has_type_args {
                    let type_args = type_ref
                        .type_arguments
                        .as_ref()
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| {
                                    self.get_type_from_type_node_in_type_literal(arg_idx)
                                })
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    return construction_boundary::type_node_application(
                        self.ctx.types,
                        base_type,
                        type_args,
                    );
                }
                return base_type;
            }

            if has_type_args {
                // Handle compiler-intrinsic types that need special TypeData
                // variants instead of generic Application types.
                // NoInfer, Uppercase, etc. are intrinsic — their DefId has no body,
                // so Application(Lazy(DefId), args) can never be evaluated.
                let is_builtin_array = name == "Array" || name == "ReadonlyArray";
                let type_param = self.lookup_type_parameter(name);
                let type_resolution =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx);
                let sym_id = match type_resolution {
                    TypeSymbolResolution::Type(sym_id) => Some(sym_id),
                    TypeSymbolResolution::ValueOnly(sym_id) => {
                        self.report_wrong_meaning(
                            name,
                            type_name_idx,
                            sym_id,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                        return TypeId::ERROR;
                    }
                    TypeSymbolResolution::NotFound => None,
                };
                let sym_id = sym_id.or_else(|| {
                    self.ctx
                        .binder
                        .file_locals
                        .get(name)
                        .filter(|&sym_id| self.symbol_has_declared_type_meaning(sym_id))
                        .map(|sym_id| {
                            self.ctx
                                .binder
                                .get_symbol(sym_id)
                                .and_then(|symbol| {
                                    symbol.import_module().and_then(|module_name| {
                                        symbol.import_name().and_then(|import_name| {
                                            self.resolve_cross_file_export_from_file(
                                                module_name,
                                                import_name,
                                                Some(self.ctx.current_file_idx),
                                            )
                                        })
                                    })
                                })
                                .unwrap_or_else(|| {
                                    let mut visited = AliasCycleTracker::new();
                                    self.resolve_alias_symbol(sym_id, &mut visited)
                                        .unwrap_or(sym_id)
                                })
                        })
                        .or_else(|| {
                            let lib_binders = self.get_lib_binders();
                            self.ctx
                                .binder
                                .get_global_type_with_libs(name, &lib_binders)
                        })
                });
                let sym_id = sym_id.map(|sym_id| {
                    self.ctx
                        .binder
                        .get_symbol(sym_id)
                        .and_then(|symbol| {
                            symbol.import_module().and_then(|module_name| {
                                symbol.import_name().and_then(|import_name| {
                                    self.resolve_cross_file_export_from_file(
                                        module_name,
                                        import_name,
                                        Some(self.ctx.current_file_idx),
                                    )
                                })
                            })
                        })
                        .unwrap_or(sym_id)
                });
                let intrinsic_reference_is_unshadowed = type_param.is_none()
                    && match sym_id {
                        Some(sym_id) => self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id),
                        None => true,
                    };
                if intrinsic_reference_is_unshadowed {
                    match name {
                        "NoInfer" => {
                            if let Some(args) = &type_ref.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let inner = self.get_type_from_type_node_in_type_literal(first_arg);
                                return construction_boundary::type_node_no_infer(
                                    self.ctx.types,
                                    inner,
                                );
                            }
                            return TypeId::ERROR;
                        }
                        "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize" => {
                            if let Some(args) = &type_ref.type_arguments
                                && let Some(&first_arg) = args.nodes.first()
                            {
                                let type_arg =
                                    self.get_type_from_type_node_in_type_literal(first_arg);
                                return crate::query_boundaries::type_construction::string_intrinsic_by_name(
                                    self.ctx.types,
                                    name,
                                    type_arg,
                                );
                            }
                            return TypeId::ERROR;
                        }
                        _ => {}
                    }
                }

                if is_builtin_array
                    && type_param.is_none()
                    && sym_id.is_none()
                    && !self.ctx.file_local_type_shadow_for_lib_name(name)
                {
                    // Array/ReadonlyArray not found - check if lib files are loaded
                    // When --noLib is used, emit TS2318 instead of silently creating Array type
                    if !self.ctx.has_lib_loaded() {
                        // No lib files loaded - emit TS2318 for missing global type
                        self.error_cannot_find_global_type(name, type_name_idx);
                        // Still process type arguments to avoid cascading errors
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                            }
                        }
                        return TypeId::ERROR;
                    }
                    // Lib files are loaded but Array not found - fall back to creating Array type
                    let elem_type = type_ref
                        .type_arguments
                        .as_ref()
                        .and_then(|args| args.nodes.first().copied())
                        .map_or(TypeId::UNKNOWN, |idx| {
                            self.get_type_from_type_node_in_type_literal(idx)
                        });
                    return construction_boundary::type_node_array_reference(
                        self.ctx.types,
                        elem_type,
                        name == "ReadonlyArray",
                    );
                }

                if !self.ctx.compiler_options.no_lib
                    && type_param.is_none()
                    && sym_id.is_none()
                    && matches!(name, "Promise" | "PromiseLike")
                    && let Some(args) = &type_ref.type_arguments
                {
                    let type_args: Vec<TypeId> = args
                        .nodes
                        .iter()
                        .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                        .collect();
                    if !type_args.is_empty() {
                        let promise_base =
                            crate::types_domain::queries::lib_resolution::resolve_name_to_lib_symbol(
                                name,
                                self.ctx.binder,
                                self.ctx.global_file_locals_index.as_deref(),
                                self.ctx
                                    .all_binders
                                    .as_ref()
                                    .map(|binders| binders.as_ref().as_slice()),
                                &self.ctx.lib_contexts,
                            )
                            .map(|sym_id| {
                                let _ = self.resolve_lib_type_by_name(name);
                                let def_id = self.ctx.get_canonical_lib_def_id(name, sym_id);
                                construction_boundary::type_node_lazy_type(
                                    self.ctx.types,
                                    def_id,
                                )
                            })
                            .unwrap_or(TypeId::PROMISE_BASE);
                        return construction_boundary::type_node_application(
                            self.ctx.types,
                            promise_base,
                            type_args,
                        );
                    }
                }

                if !is_builtin_array && type_param.is_none() && sym_id.is_none() {
                    if self.has_special_missing_lib_type_diagnostic(name) {
                        // TS2318/TS2583: Emit error for missing global type
                        // Process type arguments for validation first
                        if let Some(args) = &type_ref.type_arguments {
                            for &arg_idx in &args.nodes {
                                let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                            }
                        }
                        self.report_missing_lib_type_name(name, type_name_idx);
                        return TypeId::ERROR;
                    }
                    if name == "await" {
                        self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                        return TypeId::ERROR;
                    }
                    // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
                    if self.is_unresolved_import_symbol(type_name_idx) {
                        return TypeId::ANY;
                    }
                    // Route through boundary for TS2304/TS2552 with spelling suggestions
                    let _ = self.resolve_type_name_or_report(name, type_name_idx);
                    // Preserve the user-written name in subsequent diagnostic
                    // displays (e.g., TS2322 message for `Foo<HTMLDivElement>`
                    // when `HTMLDivElement` is undeclared) by interning an
                    // `UnresolvedTypeName`, which is treated structurally as
                    // `Error` everywhere but renders the original identifier.
                    let mut lowered_args: Vec<TypeId> = Vec::new();
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            lowered_args
                                .push(self.get_type_from_type_node_in_type_literal(arg_idx));
                        }
                    }
                    let atom = self.ctx.types.intern_string(name);
                    return construction_boundary::type_node_unresolved_application(
                        self.ctx.types,
                        atom,
                        lowered_args,
                    );
                }
                let array_is_unshadowed = is_builtin_array
                    && type_param.is_none()
                    && !self.ctx.file_local_type_shadow_for_lib_name(name);

                // For Array<T> / ReadonlyArray<T> with type arguments, convert to
                // proper array types (Array(T) / Readonly(Array(T))) instead of
                // Application(Lazy(DefId), [T]). This matches what TypeLowering does
                // and ensures assignability with `T[]` / `readonly T[]`.
                if array_is_unshadowed
                    && let Some(args) = &type_ref.type_arguments
                    && let Some(&first_arg) = args.nodes.first()
                {
                    let elem_type = self.get_type_from_type_node_in_type_literal(first_arg);
                    return construction_boundary::type_node_array_reference(
                        self.ctx.types,
                        elem_type,
                        name == "ReadonlyArray",
                    );
                }

                // A reference whose name resolves to an import alias from a module
                // that never resolved (TS2307 already emitted) must not bind to a
                // stable `Lazy(DefId)` base here. `resolve_symbol_as_lazy_type_named`
                // would yield `Application(Lazy(alias_def), args)`, a non-error shape
                // that retains its type arguments for structural comparison; two
                // instantiations differing only in an argument then fail to relate.
                // tsc poisons such a reference to `any`, so the application relates
                // freely. Route to `UnresolvedTypeName` (error-like for any args,
                // display-preserving) to match tsc and prevent self-assignability
                // cascades through generic interfaces parameterized by members of
                // unresolved imports (e.g. `interface Decoder<I,A> extends
                // K.Kleisli<E.URI, I, E, A>` where `E`/`Kind2` are unresolved).
                if type_param.is_none() && self.is_unresolved_import_symbol(type_name_idx) {
                    let lowered_args: Vec<TypeId> = type_ref
                        .type_arguments
                        .as_ref()
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| {
                                    self.get_type_from_type_node_in_type_literal(arg_idx)
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let atom = self.ctx.types.intern_string(name);
                    return construction_boundary::type_node_unresolved_application(
                        self.ctx.types,
                        atom,
                        lowered_args,
                    );
                }

                // Validate type arguments against constraints (TS2344)
                // This mirrors the check in get_type_from_type_reference for the
                // normal type resolution path. Without this, type references inside
                // interface/type literal bodies (e.g., method return types) would
                // not check that type arguments satisfy their constraints.
                if let Some(sym_id) = sym_id
                    && let Some(args) = &type_ref.type_arguments
                    && !self.is_inside_type_parameter_declaration(idx)
                {
                    self.validate_type_reference_type_arguments(sym_id, args, idx);
                }

                let base_type = if let Some(type_param) = type_param {
                    type_param
                } else if let Some(sym_id) = sym_id {
                    // Stable-identity helper: resolve symbol body + create Lazy(DefId)
                    self.resolve_symbol_as_lazy_type_named(sym_id, name)
                } else {
                    TypeId::ERROR
                };

                let type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .map(|args| {
                        args.nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node_in_type_literal(arg_idx))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                return construction_boundary::type_node_application(
                    self.ctx.types,
                    base_type,
                    type_args,
                );
            }

            if name == "Array" || name == "ReadonlyArray" {
                if let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx)
                {
                    // Stable-identity helper: resolve symbol body + create Lazy(DefId)
                    return self.resolve_symbol_as_lazy_type_named(sym_id, name);
                }
                if let Some(type_param) = self.lookup_type_parameter(name) {
                    return type_param;
                }
                // Array/ReadonlyArray not found - check if lib files are loaded
                // When --noLib is used, emit TS2318 instead of silently creating Array type
                if !self.ctx.has_lib_loaded() {
                    // No lib files loaded - emit TS2318 for missing global type
                    self.error_cannot_find_global_type(name, type_name_idx);
                    // Still process type arguments to avoid cascading errors
                    if let Some(args) = &type_ref.type_arguments {
                        for &arg_idx in &args.nodes {
                            let _ = self.get_type_from_type_node_in_type_literal(arg_idx);
                        }
                    }
                    return TypeId::ERROR;
                }
                // Lib files are loaded but Array not found - fall back to creating Array type
                let elem_type = type_ref
                    .type_arguments
                    .as_ref()
                    .and_then(|args| args.nodes.first().copied())
                    .map_or(TypeId::UNKNOWN, |idx| {
                        self.get_type_from_type_node_in_type_literal(idx)
                    });
                return construction_boundary::type_node_array_reference(
                    self.ctx.types,
                    elem_type,
                    name == "ReadonlyArray",
                );
            }

            match name {
                "number" => return TypeId::NUMBER,
                "string" => return TypeId::STRING,
                "boolean" => return TypeId::BOOLEAN,
                "void" => return TypeId::VOID,
                "any" => return TypeId::ANY,
                "never" => return TypeId::NEVER,
                "unknown" => return TypeId::UNKNOWN,
                "undefined" => return TypeId::UNDEFINED,
                "null" => return TypeId::NULL,
                "object" => return TypeId::OBJECT,
                "bigint" => return TypeId::BIGINT,
                "symbol" => return TypeId::SYMBOL,
                _ => {}
            }

            // A bare type-position identifier that names an in-scope type
            // parameter (e.g. the `<E>` declared on a call/method signature of
            // this type literal) binds to that type parameter, even when an
            // enclosing value of the same name is visible. Mirror the canonical
            // type-reference path, which consults the type-parameter scope before
            // the value-used-as-type (TS2749) diagnostic.
            if let Some(type_param) = self.lookup_type_parameter(name) {
                return type_param;
            }

            let recovered_type_symbol = if name != "Array"
                && let TypeSymbolResolution::ValueOnly(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx)
            {
                match self
                    .resolve_type_symbol_for_lowering(type_name_idx)
                    .map(tsz_binder::SymbolId)
                {
                    Some(type_sym_id) => Some(type_sym_id),
                    None => {
                        self.report_wrong_meaning(
                            name,
                            type_name_idx,
                            sym_id,
                            crate::query_boundaries::name_resolution::NameLookupKind::Value,
                            crate::query_boundaries::name_resolution::NameLookupKind::Type,
                        );
                        return TypeId::ERROR;
                    }
                }
            } else {
                None
            };

            if let Some(sym_id) = recovered_type_symbol.or_else(|| {
                if let TypeSymbolResolution::Type(sym_id) =
                    self.resolve_identifier_symbol_in_type_position(type_name_idx)
                {
                    Some(sym_id)
                } else {
                    None
                }
            }) {
                // Prime lib generic metadata before resolving the symbol body so
                // bare lib references inside type literals keep their default
                // type arguments instead of caching an uninstantiated Lazy type.
                if self.ctx.has_lib_loaded() && self.ctx.symbol_is_from_lib(sym_id) {
                    self.prime_lib_type_params(name);
                }
                let type_params = self.get_type_params_for_symbol(sym_id);
                let is_interface = self.get_cross_file_symbol(sym_id).is_some_and(|symbol| {
                    symbol.has_any_flags(tsz_binder::symbol_flags::INTERFACE)
                });
                if is_interface
                    && Self::in_cross_arena_interface_delegation()
                    && type_params.is_empty()
                    && !self.ctx.symbol_resolution_set.contains(&sym_id)
                {
                    self.ctx.symbol_resolution_set.insert(sym_id);
                    let interface_type = self.compute_interface_type_from_declarations(sym_id);
                    self.ctx.symbol_resolution_set.remove(&sym_id);
                    if interface_type != TypeId::ERROR && interface_type != TypeId::UNKNOWN {
                        let has_members = crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            interface_type,
                        )
                        .is_some_and(|shape| !shape.properties.is_empty());
                        if has_members {
                            let def_id = self.ctx.get_or_create_def_id(sym_id);
                            self.ctx
                                .definition_store
                                .register_type_to_def(interface_type, def_id);
                            return interface_type;
                        }
                    }
                } else {
                    // Resolve the symbol's structural body first.
                    let _ = self.type_reference_symbol_type(sym_id);
                }
                // Mirror `resolve_simple_type_reference`: when a bare type reference
                // omits required type arguments, return ERROR so cascading TS2322
                // checks against the naked-type-parameter form are suppressed.  The
                // TS2314 diagnostic is emitted independently by
                // `check_type_for_missing_names`, so we don't double-emit here.
                let required_count = type_params
                    .iter()
                    .filter(|param| param.default.is_none())
                    .count();
                if required_count > 0 {
                    return TypeId::ERROR;
                }
                let is_class = self
                    .get_cross_file_symbol(sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
                    || self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                    });
                if is_class && type_params.is_empty() {
                    return self.type_reference_symbol_type(sym_id);
                }
                // For generic types with all-default type parameters (e.g., Uint8Array<T = ArrayBufferLike>),
                // wrap in Application(Lazy(DefId), defaults) to match resolve_simple_type_reference behavior.
                // Without this, bare Lazy(DefId) misses the default instantiation and causes false
                // TS2322 when compared against an explicit Application (e.g., Uint8Array<ArrayBuffer>).
                if !type_params.is_empty() && type_params.iter().all(|p| p.default.is_some()) {
                    let default_args: Vec<TypeId> =
                        crate::query_boundaries::common::resolve_default_type_args(
                            self.ctx.types,
                            &type_params,
                        );
                    let def_id = self
                        .ctx
                        .get_or_create_def_id_with_params(sym_id, type_params);
                    return construction_boundary::type_node_lazy_application(
                        self.ctx.types,
                        def_id,
                        default_args,
                    );
                }
                let is_class = self
                    .get_cross_file_symbol(sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
                    || self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                    });
                if is_class {
                    return self.type_reference_symbol_type(sym_id);
                }
                // Stable-identity: create Lazy(DefId) (body already resolved above)
                return self.ctx.create_lazy_type_ref(sym_id);
            }
            if let Some(sym_id) = self.ctx.binder.file_locals.get(name)
                && self.symbol_has_declared_type_meaning(sym_id)
            {
                let mut visited = AliasCycleTracker::new();
                let sym_id = self
                    .ctx
                    .binder
                    .get_symbol(sym_id)
                    .and_then(|symbol| {
                        symbol.import_module().and_then(|module_name| {
                            symbol.import_name().and_then(|import_name| {
                                self.resolve_cross_file_export_from_file(
                                    module_name,
                                    import_name,
                                    Some(self.ctx.current_file_idx),
                                )
                            })
                        })
                    })
                    .unwrap_or_else(|| {
                        self.resolve_alias_symbol(sym_id, &mut visited)
                            .unwrap_or(sym_id)
                    });
                let _ = self.type_reference_symbol_type(sym_id);
                let type_params = self.get_type_params_for_symbol(sym_id);
                let required_count = type_params
                    .iter()
                    .filter(|param| param.default.is_none())
                    .count();
                if required_count > 0 {
                    return TypeId::ERROR;
                }
                let is_class = self
                    .get_cross_file_symbol(sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
                    || self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                    });
                if is_class && type_params.is_empty() {
                    return self.type_reference_symbol_type(sym_id);
                }
                if !type_params.is_empty() && type_params.iter().all(|p| p.default.is_some()) {
                    let default_args: Vec<TypeId> =
                        crate::query_boundaries::common::resolve_default_type_args(
                            self.ctx.types,
                            &type_params,
                        );
                    let def_id = self
                        .ctx
                        .get_or_create_def_id_with_params(sym_id, type_params);
                    return construction_boundary::type_node_lazy_application(
                        self.ctx.types,
                        def_id,
                        default_args,
                    );
                }
                let is_class = self
                    .get_cross_file_symbol(sym_id)
                    .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::CLASS))
                    || self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                        symbol.has_any_flags(tsz_binder::symbol_flags::CLASS)
                    });
                if is_class {
                    return self.type_reference_symbol_type(sym_id);
                }
                return self.ctx.create_lazy_type_ref(sym_id);
            }

            if name == "await" {
                self.error_cannot_find_name_did_you_mean_at(name, "Awaited", type_name_idx);
                return TypeId::ERROR;
            }
            if self.has_special_missing_lib_type_diagnostic(name) {
                // TS2318/TS2583: Emit error for missing global type
                self.report_missing_lib_type_name(name, type_name_idx);
                return TypeId::ERROR;
            }
            // Suppress TS2304 if this is an unresolved import (TS2307 was already emitted)
            if self.is_unresolved_import_symbol(type_name_idx) {
                return TypeId::ANY;
            }
            // Route through boundary for TS2304/TS2552 with spelling suggestions
            let _ = self.resolve_type_name_or_report(name, type_name_idx);
            // Preserve the user-written name as `UnresolvedTypeName` so
            // downstream display in TS2322/TS2345 messages prints the
            // original identifier rather than the bare `error` token.
            let atom = self.ctx.types.intern_string(name);
            return construction_boundary::type_node_unresolved_type_name(self.ctx.types, atom);
        }

        TypeId::ANY
    }

    // =========================================================================
    // Parameter Extraction
    // =========================================================================

    pub(crate) fn extract_params_from_signature_in_type_literal(
        &mut self,
        sig: &tsz_parser::parser::node::SignatureData,
    ) -> (Vec<tsz_solver::ParamInfo>, Option<TypeId>) {
        let Some(ref params_list) = sig.parameters else {
            return (Vec::new(), None);
        };

        self.extract_params_from_parameter_list_impl(
            params_list,
            ParamTypeResolutionMode::InTypeLiteral,
        )
    }

    fn enclosing_type_literal_owner_name(&self, idx: NodeIndex) -> Option<String> {
        let mut current = idx;
        let mut depth = 0usize;
        while depth < 64 {
            depth += 1;
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
            let node = self.ctx.arena.get(current)?;
            match node.kind {
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    let alias = self.ctx.arena.get_type_alias(node)?;
                    let ident = self.ctx.arena.get_identifier_at(alias.name)?;
                    return Some(ident.escaped_text.to_string());
                }
                k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.ctx.arena.get_variable_declaration(node)?;
                    let ident = self.ctx.arena.get_identifier_at(decl.name)?;
                    return Some(ident.escaped_text.to_string());
                }
                _ => {}
            }
        }
        None
    }

    fn type_literal_accessor_circular_reference(
        &self,
        type_node_idx: NodeIndex,
        accessor_name_idx: NodeIndex,
        owner_name: &str,
    ) -> bool {
        let Some(accessor_name) = self.get_property_name(accessor_name_idx) else {
            return false;
        };
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };

        if type_node.kind == syntax_kind_ext::TYPE_QUERY {
            let Some(query) = self.ctx.arena.get_type_query(type_node) else {
                return false;
            };
            let Some(expr_node) = self.ctx.arena.get(query.expr_name) else {
                return false;
            };

            if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return false;
                };
                let object_name = self
                    .ctx
                    .arena
                    .get_identifier_at(access.expression)
                    .map(|ident| ident.escaped_text.as_str());
                let property_name = self
                    .ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.as_str());
                return object_name == Some(owner_name)
                    && property_name == Some(accessor_name.as_str());
            }

            if expr_node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let Some(qn) = self.ctx.arena.get_qualified_name(expr_node) else {
                    return false;
                };
                let object_name = self
                    .ctx
                    .arena
                    .get_identifier_at(qn.left)
                    .map(|ident| ident.escaped_text.as_str());
                let property_name = self
                    .ctx
                    .arena
                    .get_identifier_at(qn.right)
                    .map(|ident| ident.escaped_text.as_str());
                return object_name == Some(owner_name)
                    && property_name == Some(accessor_name.as_str());
            }
        }

        if type_node.kind == syntax_kind_ext::INDEXED_ACCESS_TYPE {
            let Some(indexed) = self.ctx.arena.get_indexed_access_type(type_node) else {
                return false;
            };
            let Some(object_type_node) = self.ctx.arena.get(indexed.object_type) else {
                return false;
            };
            if object_type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                return false;
            }
            let Some(type_ref) = self.ctx.arena.get_type_ref(object_type_node) else {
                return false;
            };
            let object_name = self
                .ctx
                .arena
                .get_identifier_at(type_ref.type_name)
                .map(|ident| ident.escaped_text.as_str());
            if object_name != Some(owner_name) {
                return false;
            }

            let Some(index_node) = self.ctx.arena.get(indexed.index_type) else {
                return false;
            };
            if let Some(lit) = self.ctx.arena.get_literal(index_node) {
                return lit.text == accessor_name;
            }
            if let Some(lit_type) = self.ctx.arena.get_literal_type(index_node)
                && let Some(inner) = self.ctx.arena.get(lit_type.literal)
                && let Some(lit) = self.ctx.arena.get_literal(inner)
            {
                return lit.text == accessor_name;
            }
        }

        false
    }

    pub(crate) fn indexed_access_references_owner_property(
        &self,
        type_node_idx: NodeIndex,
        owner_name: &str,
        property_name: &str,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::INDEXED_ACCESS_TYPE {
            return false;
        }
        let Some(indexed) = self.ctx.arena.get_indexed_access_type(type_node) else {
            return false;
        };
        let Some(object_type_node) = self.ctx.arena.get(indexed.object_type) else {
            return false;
        };
        if object_type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(object_type_node) else {
            return false;
        };
        let object_name = self
            .ctx
            .arena
            .get_identifier_at(type_ref.type_name)
            .map(|ident| ident.escaped_text.as_str());
        if object_name != Some(owner_name) {
            return false;
        }

        let Some(index_node) = self.ctx.arena.get(indexed.index_type) else {
            return false;
        };
        if let Some(lit) = self.ctx.arena.get_literal(index_node) {
            return lit.text == property_name;
        }
        if let Some(lit_type) = self.ctx.arena.get_literal_type(index_node)
            && let Some(inner) = self.ctx.arena.get(lit_type.literal)
            && let Some(lit) = self.ctx.arena.get_literal(inner)
        {
            return lit.text == property_name;
        }

        false
    }

    pub(crate) fn check_type_literal_self_indexed_property_annotations(
        &mut self,
        type_node_idx: NodeIndex,
        owner_name: &str,
    ) {
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return;
        };
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return;
        }
        let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
            return;
        };

        let members: Vec<NodeIndex> = type_lit.members.nodes.to_vec();
        for member_idx in members {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member.kind != syntax_kind_ext::PROPERTY_SIGNATURE {
                continue;
            }
            let Some(sig) = self.ctx.arena.get_signature(member) else {
                continue;
            };
            if sig.type_annotation.is_none() {
                continue;
            }
            let Some(name) = self.get_property_name_resolved(sig.name) else {
                continue;
            };
            if !self.indexed_access_references_owner_property(
                sig.type_annotation,
                owner_name,
                &name,
            ) {
                continue;
            }
            let message = format!(
                "'{name}' is referenced directly or indirectly in its own type annotation."
            );
            self.error_at_node(sig.name, &message, 2502);
        }
    }

    pub(crate) fn type_literal_has_circular_accessor_reference(
        &self,
        type_node_idx: NodeIndex,
    ) -> bool {
        struct AccessorMemberInfo {
            circular_self_reference: bool,
        }

        #[derive(Default)]
        struct AccessorAggregate {
            getter: Option<AccessorMemberInfo>,
            setter: Option<AccessorMemberInfo>,
        }

        let Some(owner_name) = self.enclosing_type_literal_owner_name(type_node_idx) else {
            return false;
        };
        let Some(type_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return false;
        }
        let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
            return false;
        };

        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();

        for &member_idx in &type_lit.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if (member.kind != syntax_kind_ext::GET_ACCESSOR
                && member.kind != syntax_kind_ext::SET_ACCESSOR)
                || self.ctx.arena.get_accessor(member).is_none()
            {
                continue;
            }
            let Some(accessor) = self.ctx.arena.get_accessor(member) else {
                continue;
            };
            let Some(name) = self.get_property_name(accessor.name) else {
                continue;
            };
            let name_atom = self.ctx.types.intern_string(&name);
            let entry = accessors.entry(name_atom).or_default();

            if member.kind == syntax_kind_ext::GET_ACCESSOR {
                let circular_self_reference = accessor.type_annotation.is_some()
                    && self.type_literal_accessor_circular_reference(
                        accessor.type_annotation,
                        accessor.name,
                        &owner_name,
                    );
                entry.getter = Some(AccessorMemberInfo {
                    circular_self_reference,
                });
            } else {
                let mut circular_self_reference = false;
                if let Some(&param_idx) = accessor.parameters.nodes.first()
                    && let Some(param_node) = self.ctx.arena.get(param_idx)
                    && let Some(param) = self.ctx.arena.get_parameter(param_node)
                {
                    circular_self_reference = param.type_annotation.is_some()
                        && self.type_literal_accessor_circular_reference(
                            param.type_annotation,
                            accessor.name,
                            &owner_name,
                        );
                }
                entry.setter = Some(AccessorMemberInfo {
                    circular_self_reference,
                });
            }
        }

        accessors.values().any(|accessor| {
            accessor
                .getter
                .as_ref()
                .is_some_and(|getter| getter.circular_self_reference)
                || accessor
                    .setter
                    .as_ref()
                    .is_some_and(|setter| setter.circular_self_reference)
        })
    }

    // =========================================================================
    // Type Literal Resolution
    // =========================================================================

    /// Merge one wide-`symbol`-keyed computed member's value into a type
    /// literal's symbol index signature. Several `symbol`-keyed members
    /// contribute to ONE `[key: symbol]: V` signature whose value type is the
    /// UNION of their values, matching tsc: `{ [s1]: number; [s2]: string }`
    /// reads as `[key: symbol]: string | number`, and the index is `readonly`
    /// only when every contributor is (a getter contributes readonly, a setter
    /// or property writable). Unlike two EXPLICIT `[k: symbol]` index signatures
    /// — a genuine duplicate — distinct computed keys never collide.
    fn merge_type_literal_symbol_index(
        &self,
        symbol_index: &mut Option<tsz_solver::IndexSignature>,
        value_type: TypeId,
        readonly: bool,
    ) {
        let info = crate::query_boundaries::type_construction::declared_index_signature(
            TypeId::SYMBOL,
            value_type,
            readonly,
            None,
        );
        match symbol_index.as_mut() {
            None => *symbol_index = Some(info),
            Some(existing) => {
                super::interface_type::merge_string_index_by_union(
                    existing,
                    info,
                    self.ctx.types.factory(),
                );
            }
        }
    }

    /// Does this type-literal member's name node key off a plain (non-unique)
    /// `symbol` binding — `type T = { [s]: V }` with `declare const s: symbol`?
    ///
    /// Classifying the key evaluates its expression in VALUE position (see
    /// `computed_member_key_is_wide_symbol`), and a value-position evaluation
    /// reports its own diagnostics. Several of those are suppressed only inside
    /// a computed-property-name context — `is_in_ambient_computed_property_context`
    /// reads `ctx.checking_computed_property_name` and returns early for an
    /// interface/type-literal member. Publishing that context here, exactly like
    /// `class_member_computed_key_is_wide_symbol` does for classes, is what keeps
    /// a type-only-imported key from spuriously reporting TS1361 on
    /// `type T = { [key]: any }` (#16466: this call site went in unwrapped when
    /// #16462 wired the type-literal builder into the shared classifier).
    fn type_literal_member_computed_key_is_wide_symbol(&mut self, name_idx: NodeIndex) -> bool {
        let prev_checking = self.ctx.checking_computed_property_name;
        self.ctx.checking_computed_property_name = Some(name_idx);
        let is_wide = self.computed_member_key_is_wide_symbol(name_idx);
        self.ctx.checking_computed_property_name = prev_checking;
        is_wide
    }

    /// Get type from a type literal node (anonymous object types).
    ///
    /// Type literals represent inline object types like `{ x: string; y: number }` or
    /// callable types with call/construct signatures. This function parses the type
    /// literal and creates the appropriate type representation.
    ///
    /// ## Type Literal Members:
    /// - **Property Signatures**: Named properties with types (`{ x: string }`)
    /// - **Method Signatures**: Function-typed methods (`{ method(): void }`)
    /// - **Call Signatures**: Callable objects (`{ (): string }`)
    /// - **Construct Signatures**: Constructor functions (`{ new(): T }`)
    /// - **Index Signatures**: Dynamic property access (`{ [key: string]: T }`)
    ///
    /// ## Modifiers:
    /// - `?`: Optional property (can be undefined)
    /// - `readonly`: Read-only property (cannot be assigned to)
    ///
    /// ## Type Resolution:
    /// - Property types are resolved via `get_type_from_type_node_in_type_literal`
    /// - Type parameters are pushed/popped for each member
    /// - Index signatures are tracked by key type (string or number)
    ///
    /// ## Result Type:
    /// - **Callable**: If has call/construct signatures
    /// - **`ObjectWithIndex`**: If has index signatures
    /// - **Object**: Plain object type otherwise
    pub(crate) fn get_type_from_type_literal(&mut self, idx: NodeIndex) -> TypeId {
        use crate::query_boundaries::construct_signatures::{
            call_only_callable_type, method_function_type_from_call_signature,
            type_literal_callable_type,
        };
        use crate::query_boundaries::type_construction::{
            raw_intersection_pair, type_literal_extra_number_index_object, type_literal_object,
            type_literal_object_with_index,
        };
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::CallSignature;
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(data) = self.ctx.arena.get_type_literal(node) else {
            return TypeId::ERROR; // Missing type literal data - propagate error
        };
        let owner_name = self.enclosing_type_literal_owner_name(idx);

        struct AccessorMemberInfo {
            name_idx: NodeIndex,
            type_annotation: NodeIndex,
            resolved_type: TypeId,
            circular_self_reference: bool,
        }

        struct AccessorAggregate {
            getter: Option<AccessorMemberInfo>,
            setter: Option<AccessorMemberInfo>,
            declaration_order: u32,
        }

        let mut properties = Vec::new();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut call_signatures = Vec::new();
        let mut construct_signatures = Vec::new();
        let mut string_index = None;
        let mut number_index = None;
        let mut symbol_index = None;
        let mut extra_number_indices = Vec::new();
        let mut has_abstract_construct_sig = false;
        let mut has_late_bound_members = false;
        // Global member counter for preserving source declaration order across
        // both properties and methods. Using properties.len() would give methods
        // higher declaration_order than all properties since methods are merged
        // after the loop, breaking tsc's interleaved display order.
        let mut member_order: u32 = 0;
        struct OverloadEntry {
            signature: CallSignature,
            optional: bool,
            readonly: bool,
            is_symbol_named: bool,
        }
        struct OverloadOrderKey {
            name: Atom,
            decl_order: u32,
            is_string_named: bool,
            single_quoted_name: bool,
        }
        let mut method_overloads: FxHashMap<Atom, Vec<OverloadEntry>> = FxHashMap::default();
        let mut method_overload_order: Vec<OverloadOrderKey> = Vec::new();

        for &member_idx in &data.members.nodes {
            let Some(member) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if let Some(sig) = self.ctx.arena.get_signature(member) {
                // TS2300: a call, construct, or method signature of an object
                // type literal is a function-like signature whose parameter list
                // carries the same duplicate-name grammar as an interface member
                // or a function-type signature. tsc runs
                // `checkGrammarParameterList` for all of them and blames every
                // occurrence of a repeated name; this construction path is
                // reached once per written type-literal node (alias body, inline
                // annotation, nested), so it is the position-complete home for
                // the check.
                if matches!(
                    member.kind,
                    CALL_SIGNATURE | CONSTRUCT_SIGNATURE | METHOD_SIGNATURE
                ) && let Some(ref params) = sig.parameters
                {
                    super::type_node_helpers::check_duplicate_parameters_in_type(
                        &mut self.ctx,
                        params,
                    );
                }
                match member.kind {
                    CALL_SIGNATURE => {
                        if let Some(ref _params) = sig.parameters {}
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        // Check for unused type parameters (TS6133)
                        self.check_unused_type_params(&sig.type_parameters, member_idx);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) = self
                            .return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                                crate::signature_builder::signature_param_nodes(&sig.parameters),
                            );
                        call_signatures.push(signature_building_boundary::call_signature(
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            false,
                        ));
                        self.pop_type_parameters(type_param_updates);
                    }
                    CONSTRUCT_SIGNATURE => {
                        if let Some(ref _params) = sig.parameters {}
                        if self.has_abstract_modifier(&sig.modifiers) {
                            has_abstract_construct_sig = true;
                        }
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        // Check for unused type parameters (TS6133)
                        self.check_unused_type_params(&sig.type_parameters, member_idx);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        let (return_type, type_predicate) = self
                            .return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                                crate::signature_builder::signature_param_nodes(&sig.parameters),
                            );
                        construct_signatures.push(signature_building_boundary::call_signature(
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            false,
                        ));
                        self.pop_type_parameters(type_param_updates);
                    }
                    METHOD_SIGNATURE | PROPERTY_SIGNATURE => {
                        // A computed key whose expression is a plain (non-unique)
                        // `symbol` binding contributes a `[key: symbol]: V` index
                        // signature to the containing type, exactly like tsc
                        // (#16307) — never a synthetic named member. The interface
                        // lowering and object-literal paths already route this way;
                        // this checker-side type-literal builder must match, or a
                        // `type`/inline `{ [s]: T }` mints a `__symbol_<file>_<sym>`
                        // named member instead, and two independently `symbol`-keyed
                        // type literals become mutually unassignable (TS2741) with
                        // the placeholder key leaking into diagnostic text. Well-known
                        // `[Symbol.x]` syntax, `typeof Symbol.x` aliases, and genuine
                        // `unique symbol` keys are excluded by
                        // `computed_member_key_is_wide_symbol` and keep named identity.
                        if self.type_literal_member_computed_key_is_wide_symbol(sig.name) {
                            let readonly = self.has_readonly_modifier(&sig.modifiers);
                            let value_type = if member.kind == METHOD_SIGNATURE {
                                let (type_params, type_param_updates) =
                                    self.push_type_parameters(&sig.type_parameters);
                                let (params, this_type) =
                                    self.extract_params_from_signature_in_type_literal(sig);
                                let (return_type, type_predicate) = self
                                    .return_type_and_predicate_in_type_literal(
                                        sig.type_annotation,
                                        &params,
                                        crate::signature_builder::signature_param_nodes(
                                            &sig.parameters,
                                        ),
                                    );
                                let call_sig = signature_building_boundary::call_signature(
                                    type_params,
                                    params,
                                    this_type,
                                    return_type,
                                    type_predicate,
                                    true,
                                );
                                self.pop_type_parameters(type_param_updates);
                                method_function_type_from_call_signature(self.ctx.types, &call_sig)
                            } else if sig.type_annotation.is_some() {
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            self.merge_type_literal_symbol_index(
                                &mut symbol_index,
                                value_type,
                                readonly,
                            );
                            continue;
                        }
                        let Some(name) = self.get_property_name_resolved(sig.name) else {
                            if self
                                .ctx
                                .arena
                                .get(sig.name)
                                .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                            {
                                has_late_bound_members = true;
                            }
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let is_symbol_named = self.is_symbol_property_name(sig.name);
                        let (is_string_named, single_quoted_name) =
                            self.ctx.arena.string_property_name_flags(sig.name);

                        if member.kind == METHOD_SIGNATURE {
                            if let Some(ref _params) = sig.parameters {}
                            let (type_params, type_param_updates) =
                                self.push_type_parameters(&sig.type_parameters);
                            let (params, this_type) =
                                self.extract_params_from_signature_in_type_literal(sig);
                            let (return_type, type_predicate) = self
                                .return_type_and_predicate_in_type_literal(
                                    sig.type_annotation,
                                    &params,
                                    crate::signature_builder::signature_param_nodes(
                                        &sig.parameters,
                                    ),
                                );
                            let call_sig = signature_building_boundary::call_signature(
                                type_params,
                                params,
                                this_type,
                                return_type,
                                type_predicate,
                                true,
                            );
                            self.pop_type_parameters(type_param_updates);
                            let optional = sig.question_token;
                            let readonly = self.has_readonly_modifier(&sig.modifiers);
                            let entry = method_overloads.entry(name_atom).or_default();
                            if entry.is_empty() {
                                member_order += 1;
                                method_overload_order.push(OverloadOrderKey {
                                    name: name_atom,
                                    decl_order: member_order,
                                    is_string_named,
                                    single_quoted_name,
                                });
                            }
                            entry.push(OverloadEntry {
                                signature: call_sig,
                                optional,
                                readonly,
                                is_symbol_named,
                            });
                        } else {
                            let circular_self_reference = sig.type_annotation.is_some()
                                && owner_name.as_deref().is_some_and(|owner_name| {
                                    self.indexed_access_references_owner_property(
                                        sig.type_annotation,
                                        owner_name,
                                        &name,
                                    )
                                });
                            let type_id = if circular_self_reference {
                                let message = format!(
                                    "'{name}' is referenced directly or indirectly in its own type annotation."
                                );
                                self.error_at_node(sig.name, &message, 2502);
                                TypeId::ANY
                            } else if sig.type_annotation.is_some() {
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            let write_type =
                                if self.ctx.compiler_options.exact_optional_property_types
                                    && sig.question_token
                                    && sig.type_annotation.is_some()
                                    && !type_node_includes_explicit_undefined(
                                        self.ctx.arena,
                                        sig.type_annotation,
                                    )
                                {
                                    crate::query_boundaries::common::remove_undefined(
                                        self.ctx.types.as_type_database(),
                                        type_id,
                                    )
                                } else {
                                    type_id
                                };
                            member_order += 1;
                            properties.push(construction_boundary::declared_surface_property(
                                construction_boundary::DeclaredSurfaceProperty {
                                    name: name_atom,
                                    type_id,
                                    write_type,
                                    optional: sig.question_token,
                                    readonly: self.has_readonly_modifier(&sig.modifiers),
                                    is_method: false,
                                    declaration_order: member_order,
                                    is_string_named,
                                    is_symbol_named,
                                    single_quoted_name,
                                },
                            ));
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if let Some(index_sig) = self.ctx.arena.get_index_signature(member) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if param_data.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(param_data.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };

                // TS1337 / TS1268: Validate index signature parameter type.
                // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                let has_param_grammar_error =
                    param_data.dot_dot_dot_token || param_data.question_token;
                let is_valid_index_type = if !has_param_grammar_error
                    && param_data.type_annotation.is_some()
                {
                    let (is_generic_or_literal, is_valid) =
                        self.classify_index_sig_param_type(key_type, param_data.type_annotation);
                    if !is_valid {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        if is_generic_or_literal {
                            self.error_at_node(
                                param_idx,
                                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                            );
                        } else {
                            self.error_at_node(
                                param_idx,
                                diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                                diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                            );
                        }
                    }
                    is_valid
                } else {
                    false
                };

                // TS2693: Check if parameter name without type annotation
                // refers to a type (e.g., `[K]: number` where `K` is a type alias).
                if !has_param_grammar_error
                    && param_data.type_annotation.is_none()
                    && let Some(name_node) = self.ctx.arena.get(param_data.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && let Some(sym_id) = self
                        .ctx
                        .binder
                        .resolve_identifier(self.ctx.arena, param_data.name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    let name = &ident.escaped_text;
                    // Check if this identifier resolves to a type symbol
                    let has_type = symbol.has_any_flags(
                        tsz_binder::symbol_flags::TYPE
                            | tsz_binder::symbol_flags::TYPE_ALIAS
                            | tsz_binder::symbol_flags::INTERFACE,
                    );
                    let has_value = symbol.has_any_flags(tsz_binder::symbol_flags::VALUE);
                    if has_type && !has_value {
                        // The identifier refers to a type-only symbol
                        // Emit TS2693: Type only used as value
                        use crate::diagnostics::{
                            diagnostic_codes, diagnostic_messages, format_message,
                        };
                        let message = format_message(
                            diagnostic_messages::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                            &[name],
                        );
                        self.ctx.error(
                            name_node.pos,
                            name_node.end - name_node.pos,
                            message,
                            diagnostic_codes::ONLY_REFERS_TO_A_TYPE_BUT_IS_BEING_USED_AS_A_VALUE_HERE,
                        );
                    }
                }

                let value_type = if index_sig.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(index_sig.type_annotation)
                } else {
                    // Missing annotation defaults to ANY (TS7011 reported separately)
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let param_name = self
                    .ctx
                    .arena
                    .get(param_data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));
                let info = construction_boundary::declared_index_signature(
                    key_type, value_type, readonly, param_name,
                );
                if is_valid_index_type {
                    if key_type == TypeId::NUMBER {
                        if number_index.is_none() {
                            number_index = Some(info);
                        } else {
                            extra_number_indices.push(info);
                        }
                    } else if key_type == TypeId::SYMBOL {
                        if symbol_index.is_none() {
                            symbol_index = Some(info);
                        } else if let Some(existing) = symbol_index.as_mut()
                            && (existing.value_type != info.value_type
                                || existing.readonly != info.readonly)
                        {
                            existing.value_type = TypeId::ERROR;
                            existing.readonly = false;
                        }
                    } else {
                        match string_index.as_mut() {
                            None => string_index = Some(info),
                            Some(existing) => {
                                super::interface_type::merge_string_index_by_union(
                                    existing, info, factory,
                                );
                            }
                        }
                    }
                }
                continue;
            }

            // A get/set accessor keyed by a plain `symbol` binding routes into
            // the symbol index signature (getter contributes a readonly value,
            // setter a writable one) exactly like the interface-lowering path
            // (#16307), rather than minting a synthetic `__symbol_` named member.
            if (member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || member.kind == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(member)
                && self.type_literal_member_computed_key_is_wide_symbol(accessor.name)
            {
                let is_getter = member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR;
                let value_type = if is_getter {
                    if accessor.type_annotation.is_some() {
                        self.get_type_from_type_node_in_type_literal(accessor.type_annotation)
                    } else {
                        TypeId::ANY
                    }
                } else {
                    accessor
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                        .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                        .map_or(TypeId::UNKNOWN, |param| {
                            if param.type_annotation.is_some() {
                                self.get_type_from_type_node_in_type_literal(param.type_annotation)
                            } else {
                                TypeId::ANY
                            }
                        })
                };
                self.merge_type_literal_symbol_index(&mut symbol_index, value_type, is_getter);
                continue;
            }

            // Handle accessor declarations (get/set) in type literals
            if (member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR
                || member.kind == tsz_parser::parser::syntax_kind_ext::SET_ACCESSOR)
                && let Some(accessor) = self.ctx.arena.get_accessor(member)
                && let Some(name) = self.get_property_name_resolved(accessor.name)
            {
                let name_atom = self.ctx.types.intern_string(&name);
                let is_new_accessor = !accessors.contains_key(&name_atom);
                if is_new_accessor {
                    member_order += 1;
                }
                let current_order = member_order;
                let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                    getter: None,
                    setter: None,
                    declaration_order: current_order,
                });

                if member.kind == tsz_parser::parser::syntax_kind_ext::GET_ACCESSOR {
                    let circular_self_reference = accessor.type_annotation.is_some()
                        && owner_name.as_deref().is_some_and(|owner_name| {
                            self.type_literal_accessor_circular_reference(
                                accessor.type_annotation,
                                accessor.name,
                                owner_name,
                            )
                        });
                    let resolved_type =
                        if accessor.type_annotation.is_some() && !circular_self_reference {
                            self.get_type_from_type_node_in_type_literal(accessor.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                    entry.getter = Some(AccessorMemberInfo {
                        name_idx: accessor.name,
                        type_annotation: accessor.type_annotation,
                        resolved_type,
                        circular_self_reference,
                    });
                } else {
                    let mut type_annotation = NodeIndex::NONE;
                    let mut circular_self_reference = false;
                    let mut resolved_type = TypeId::UNKNOWN;
                    if let Some(&param_idx) = accessor.parameters.nodes.first()
                        && let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                    {
                        type_annotation = param.type_annotation;
                        circular_self_reference = param.type_annotation.is_some()
                            && owner_name.as_deref().is_some_and(|owner_name| {
                                self.type_literal_accessor_circular_reference(
                                    param.type_annotation,
                                    accessor.name,
                                    owner_name,
                                )
                            });
                        if param.type_annotation.is_some() && !circular_self_reference {
                            resolved_type =
                                self.get_type_from_type_node_in_type_literal(param.type_annotation);
                        }
                    }
                    entry.setter = Some(AccessorMemberInfo {
                        name_idx: accessor.name,
                        type_annotation,
                        resolved_type,
                        circular_self_reference,
                    });
                }
            } else if member.is_accessor()
                && let Some(accessor) = self.ctx.arena.get_accessor(member)
                && self
                    .ctx
                    .arena
                    .get(accessor.name)
                    .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
            {
                has_late_bound_members = true;
            }
        }

        // Convert accessors to properties (getter-only implies readonly)
        for (name, accessor) in accessors {
            let getter_requires_ts2502 = accessor.getter.as_ref().is_some_and(|getter| {
                getter.circular_self_reference
                    && accessor.setter.as_ref().is_none_or(|setter| {
                        setter.type_annotation.is_none() || setter.circular_self_reference
                    })
            });
            let setter_requires_ts2502 = accessor.setter.as_ref().is_some_and(|setter| {
                setter.circular_self_reference
                    && accessor.getter.as_ref().is_none_or(|getter| {
                        getter.type_annotation.is_none() || getter.circular_self_reference
                    })
            });

            let getter_type = accessor.getter.as_ref().map(|getter| {
                if getter_requires_ts2502 {
                    let name = self.ctx.types.resolve_atom_ref(name).to_string();
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(getter.name_idx, &message, 2502);
                    TypeId::ANY
                } else if getter.circular_self_reference {
                    accessor
                        .setter
                        .as_ref()
                        .map_or(TypeId::ANY, |setter| setter.resolved_type)
                } else {
                    getter.resolved_type
                }
            });
            let setter_type = accessor.setter.as_ref().map(|setter| {
                if setter_requires_ts2502 {
                    let name = self.ctx.types.resolve_atom_ref(name).to_string();
                    let message = format!(
                        "'{name}' is referenced directly or indirectly in its own type annotation."
                    );
                    self.error_at_node(setter.name_idx, &message, 2502);
                    TypeId::ANY
                } else if setter.circular_self_reference {
                    accessor
                        .getter
                        .as_ref()
                        .map_or(TypeId::UNKNOWN, |getter| getter.resolved_type)
                } else {
                    setter.resolved_type
                }
            });

            let read_type = getter_type.or(setter_type).unwrap_or(TypeId::UNKNOWN);
            let write_type = setter_type.or(getter_type).unwrap_or(read_type);
            let readonly = getter_type.is_some() && setter_type.is_none();
            let primary_name_idx = accessor
                .getter
                .as_ref()
                .or(accessor.setter.as_ref())
                .map(|member| member.name_idx);
            let is_symbol_named =
                primary_name_idx.is_some_and(|name_idx| self.is_symbol_property_name(name_idx));
            let (is_string_named, single_quoted_name) = primary_name_idx
                .map(|name_idx| self.ctx.arena.string_property_name_flags(name_idx))
                .unwrap_or((false, false));
            properties.push(construction_boundary::declared_surface_property(
                construction_boundary::DeclaredSurfaceProperty {
                    name,
                    type_id: read_type,
                    write_type,
                    optional: false,
                    readonly,
                    is_method: false,
                    declaration_order: accessor.declaration_order,
                    is_string_named,
                    is_symbol_named,
                    single_quoted_name,
                },
            ));
        }

        // Merge overloaded method signatures into properties.
        // Single-signature methods become Function types; multi-signature become Callable types.
        for key in method_overload_order {
            if let Some(sigs) = method_overloads.remove(&key.name) {
                let optional = sigs.iter().all(|entry| entry.optional);
                let readonly = sigs.iter().any(|entry| entry.readonly);
                let is_symbol_named = sigs.iter().any(|entry| entry.is_symbol_named);
                let method_type = if sigs.len() == 1 {
                    let sig = sigs
                        .into_iter()
                        .next()
                        .expect("sigs.len() == 1 guard ensures at least one element")
                        .signature;
                    method_function_type_from_call_signature(self.ctx.types, &sig)
                } else {
                    let merged_sigs: Vec<CallSignature> =
                        sigs.into_iter().map(|entry| entry.signature).collect();
                    call_only_callable_type(self.ctx.types, merged_sigs)
                };
                properties.push(construction_boundary::declared_surface_property(
                    construction_boundary::DeclaredSurfaceProperty {
                        name: key.name,
                        type_id: method_type,
                        write_type: method_type,
                        optional,
                        readonly,
                        is_method: true,
                        declaration_order: key.decl_order,
                        is_string_named: key.is_string_named,
                        is_symbol_named,
                        single_quoted_name: key.single_quoted_name,
                    },
                ));
            }
        }

        if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            let mut result = type_literal_callable_type(
                self.ctx.types,
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                symbol_index,
                has_abstract_construct_sig,
            );
            for idx in extra_number_indices {
                let member = type_literal_extra_number_index_object(self.ctx.types, idx);
                result = raw_intersection_pair(self.ctx.types, result, member);
            }
            return result;
        }

        if string_index.is_some() || number_index.is_some() || symbol_index.is_some() {
            let mut result = type_literal_object_with_index(
                self.ctx.types,
                properties,
                string_index,
                number_index,
                symbol_index,
                has_late_bound_members,
            );
            // Record the hand-written `{ ... }` annotation so the printer never
            // repaints it with a utility-application display alias that shares
            // this content-interned id.
            self.ctx.types.mark_literal_object_annotation(result);
            for idx in extra_number_indices {
                let member = type_literal_extra_number_index_object(self.ctx.types, idx);
                result = raw_intersection_pair(self.ctx.types, result, member);
            }
            return result;
        }

        let result = type_literal_object(self.ctx.types, properties, has_late_bound_members);
        self.ctx.types.mark_literal_object_annotation(result);
        result
    }
}
