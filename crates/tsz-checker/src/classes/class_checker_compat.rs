//! Class and interface compatibility checking (TS2415, TS2430), member lookup
//! in class chains, and visibility conflict detection.

use crate::class_checker::MemberVisibility;
use crate::diagnostics::diagnostic_codes;
use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_property_type_mismatch,
};
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn check_class_index_signature_compatibility(
        &mut self,
        derived_class: &tsz_parser::parser::node::ClassData,
        base_class: &tsz_parser::parser::node::ClassData,
        derived_class_name: &str,
        base_class_name: &str,
        substitution: &tsz_solver::TypeSubstitution,
        mut class_extends_error_reported: bool,
    ) {
        use crate::query_boundaries::common::instantiate_type;
        use tsz_parser::parser::syntax_kind_ext::INDEX_SIGNATURE;

        // Collect derived class index signatures
        let mut derived_string_index: Option<(TypeId, NodeIndex)> = None;
        let mut derived_number_index: Option<(TypeId, NodeIndex)> = None;

        for &member_idx in &derived_class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != INDEX_SIGNATURE {
                continue;
            }
            let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                continue;
            };
            if self.has_static_modifier(&index_sig.modifiers) {
                continue;
            }

            let param_idx = index_sig
                .parameters
                .nodes
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE);
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let key_type = if param.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(param.type_annotation)
            };

            let value_type = if index_sig.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(index_sig.type_annotation)
            };

            if key_type == TypeId::NUMBER {
                derived_number_index = Some((value_type, member_idx));
            } else {
                derived_string_index = Some((value_type, member_idx));
            }
        }

        // Collect base class index signatures
        let mut base_string_index: Option<TypeId> = None;
        let mut base_number_index: Option<TypeId> = None;

        for &member_idx in &base_class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != INDEX_SIGNATURE {
                continue;
            }
            let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                continue;
            };
            if self.has_static_modifier(&index_sig.modifiers) {
                continue;
            }

            let param_idx = index_sig
                .parameters
                .nodes
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE);
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };

            let key_type = if param.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(param.type_annotation)
            };

            let value_type = if index_sig.type_annotation.is_none() {
                TypeId::ANY
            } else {
                self.get_type_from_type_node(index_sig.type_annotation)
            };

            if key_type == TypeId::NUMBER {
                base_number_index = Some(value_type);
            } else {
                base_string_index = Some(value_type);
            }
        }

        // Check string index signature compatibility
        if let (Some((derived_type, _derived_idx)), Some(base_type)) =
            (derived_string_index, base_string_index)
        {
            let base_type_instantiated = instantiate_type(self.ctx.types, base_type, substitution);
            if !self.is_assignable_to(derived_type, base_type_instantiated)
                && !class_extends_error_reported
            {
                let derived_type_str = self.format_type(derived_type);
                let base_type_str = self.format_type(base_type_instantiated);
                self.error_at_node(
                        derived_class.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'.\n  'string' index signatures are incompatible.\n    Type '{derived_type_str}' is not assignable to type '{base_type_str}'."
                        ),
                        crate::diagnostics::diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
                class_extends_error_reported = true;
            }
        }

        // Check number index signature compatibility
        if let (Some((derived_type, _derived_idx)), Some(base_type)) =
            (derived_number_index, base_number_index)
        {
            let base_type_instantiated = instantiate_type(self.ctx.types, base_type, substitution);
            if !self.is_assignable_to(derived_type, base_type_instantiated)
                && !class_extends_error_reported
            {
                let derived_type_str = self.format_type(derived_type);
                let base_type_str = self.format_type(base_type_instantiated);
                self.error_at_node(
                        derived_class.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'.\n  'number' index signatures are incompatible.\n    Type '{derived_type_str}' is not assignable to type '{base_type_str}'."
                        ),
                        crate::diagnostics::diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
            }
        }
    }

    /// Check that interface correctly extends its base interfaces (error 2430).
    /// For each member in the derived interface, checks if the same member in a base interface
    /// has an incompatible type.
    pub(crate) fn check_interface_extension_compatibility(
        &mut self,
        _iface_idx: NodeIndex,
        iface_data: &tsz_parser::parser::node::InterfaceData,
    ) {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, INDEX_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };

        fn decl_arena_for<'a>(
            binder: &'a tsz_binder::BinderState,
            current_arena: &'a tsz_parser::parser::node::NodeArena,
            sym_id: tsz_binder::SymbolId,
            decl_idx: NodeIndex,
        ) -> &'a tsz_parser::parser::node::NodeArena {
            binder
                .get_arena_for_declaration(sym_id, decl_idx)
                .map_or(current_arena, |arena| arena.as_ref())
        }

        let iface_sym_id = self.ctx.binder.node_symbols.get(&_iface_idx.0).copied();

        // Get heritage clauses (extends) — must have at least one across all declarations
        if iface_data.heritage_clauses.is_none() {
            // Check if other declarations of this interface have heritage clauses
            let has_heritage_elsewhere = iface_sym_id
                .and_then(|sym_id| self.ctx.binder.symbols.get(sym_id).map(|sym| (sym_id, sym)))
                .is_some_and(|(sym_id, sym)| {
                    sym.declarations.iter().any(|&decl_idx| {
                        let decl_arena =
                            decl_arena_for(self.ctx.binder, self.ctx.arena, sym_id, decl_idx);
                        decl_idx != _iface_idx
                            && decl_arena.get(decl_idx).is_some_and(|n| {
                                decl_arena
                                    .get_interface(n)
                                    .is_some_and(|iface| iface.heritage_clauses.is_some())
                            })
                    })
                });
            if !has_heritage_elsewhere {
                return;
            }
        }

        // Get the derived interface name for the error message
        let derived_name = if iface_data.name.is_some() {
            if let Some(name_node) = self.ctx.arena.get(iface_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    let mut name = ident.escaped_text.clone();
                    // Append type parameters for tsc parity: "Foo<T, U>"
                    self.append_type_param_names(&mut name, &iface_data.type_parameters);
                    name
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        // Collect derived member names and full member info across ALL declarations of this
        // interface (for merged interfaces, each declaration can contribute members).
        let mut derived_member_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        // (name, type, node_idx, kind, is_optional) — used for TS2430 derived-vs-base checks.
        let mut derived_members: Vec<(String, TypeId, NodeIndex, u16, bool)> = Vec::new();

        // Collect all interface declaration indices for this symbol
        let all_iface_decls: Vec<NodeIndex> = self
            .ctx
            .binder
            .node_symbols
            .get(&_iface_idx.0)
            .copied()
            .and_then(|sym_id| self.ctx.binder.symbols.get(sym_id).map(|sym| (sym_id, sym)))
            .map(|sym| {
                let (sym_id, sym) = sym;
                sym.declarations
                    .iter()
                    .copied()
                    .filter(|&decl_idx| {
                        let decl_arena =
                            decl_arena_for(self.ctx.binder, self.ctx.arena, sym_id, decl_idx);
                        decl_arena
                            .get(decl_idx)
                            .is_some_and(|n| decl_arena.get_interface(n).is_some())
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Only run the full cross-declaration check on the FIRST declaration to avoid
        // emitting the same TS2320 error multiple times.
        if all_iface_decls.first().copied() != Some(_iface_idx) && all_iface_decls.len() > 1 {
            return;
        }

        for &decl_idx in &all_iface_decls {
            let Some(sym_id) = iface_sym_id else {
                continue;
            };
            let decl_arena = decl_arena_for(self.ctx.binder, self.ctx.arena, sym_id, decl_idx);
            if let Some(decl_node) = decl_arena.get(decl_idx)
                && let Some(decl_iface) = decl_arena.get_interface(decl_node)
            {
                for &member_idx in &decl_iface.members.nodes {
                    let Some(member_node) = decl_arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind == CALL_SIGNATURE {
                        derived_member_names.insert(String::from("__call__"));
                    } else if (member_node.kind == METHOD_SIGNATURE
                        || member_node.kind == PROPERTY_SIGNATURE)
                        && let Some(sig) = decl_arena.get_signature(member_node)
                        && let Some(name) =
                            crate::types_domain::queries::core::get_literal_property_name(
                                decl_arena, sig.name,
                            )
                    {
                        derived_member_names.insert(name.clone());
                        let type_id = self
                            .delegate_cross_arena_interface_member_simple_type(
                                decl_idx, member_idx, decl_arena, None,
                            )
                            .unwrap_or_else(|| self.get_type_of_interface_member(member_idx));
                        derived_members.push((
                            name,
                            type_id,
                            member_idx,
                            member_node.kind,
                            sig.question_token,
                        ));
                    }
                }
            }
        }

        // Substitute `ThisType` in derived member types with the interface's self type.
        // In tsc, `this` in an interface refers to the interface's declared type. When
        // checking interface extension compatibility, derived member types containing
        // `this` (e.g., `oninit?(vnode: Vnode<A, this>)`) must be compared against base
        // member types where the type parameter has been concretized (e.g.,
        // `oninit?(vnode: Vnode<A, ClassComponent<A>>)`). Without this substitution,
        // the comparison fails because the solver has no constraint info for `ThisType`.
        {
            // Check if any derived member contains ThisType (fast path: skip if none do)
            let any_has_this = derived_members.iter().any(|(_, tid, _, _, _)| {
                crate::query_boundaries::common::contains_this_type(self.ctx.types, *tid)
            });
            if any_has_this {
                // Compute the interface's self type as a named type reference
                // (Lazy(DefId) or Application(Lazy(DefId), [type_params]))
                let interface_self_type = self
                    .ctx
                    .binder
                    .node_symbols
                    .get(&_iface_idx.0)
                    .copied()
                    .map(|sym_id| {
                        let def_id = self.ctx.get_or_create_def_id(sym_id);
                        let lazy_type = self.ctx.types.factory().lazy(def_id);

                        // Collect type parameter TypeIds from the current scope
                        if let Some(ref tp_list) = iface_data.type_parameters {
                            let mut tp_type_ids = Vec::new();
                            for &tp_idx in &tp_list.nodes {
                                if let Some(tp_node) = self.ctx.arena.get(tp_idx)
                                    && let Some(tp_data) =
                                        self.ctx.arena.get_type_parameter(tp_node)
                                    && let Some(name_node) = self.ctx.arena.get(tp_data.name)
                                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                    && let Some(&tp_type_id) =
                                        self.ctx.type_parameter_scope.get(&ident.escaped_text)
                                {
                                    tp_type_ids.push(tp_type_id);
                                }
                            }
                            if tp_type_ids.is_empty() {
                                lazy_type
                            } else {
                                self.ctx.types.factory().application(lazy_type, tp_type_ids)
                            }
                        } else {
                            lazy_type
                        }
                    });

                if let Some(self_type) = interface_self_type {
                    for member in &mut derived_members {
                        if crate::query_boundaries::common::contains_this_type(
                            self.ctx.types,
                            member.1,
                        ) {
                            member.1 = crate::query_boundaries::common::substitute_this_type(
                                self.ctx.types,
                                member.1,
                                self_type,
                            );
                        }
                    }
                }
            }
        }

        let mut derived_method_counts: rustc_hash::FxHashMap<String, usize> =
            rustc_hash::FxHashMap::default();
        for (name, _, _, kind, _) in &derived_members {
            if *kind == METHOD_SIGNATURE {
                *derived_method_counts.entry(name.clone()).or_insert(0) += 1;
            }
        }

        // Collect derived interface index signatures across all declarations.
        // These are checked against base index signatures for TS2430 compatibility.
        let mut derived_string_index_type: Option<TypeId> = None;
        let mut derived_number_index_type: Option<TypeId> = None;
        for &decl_idx in &all_iface_decls {
            let Some(sym_id) = iface_sym_id else {
                continue;
            };
            let decl_arena = decl_arena_for(self.ctx.binder, self.ctx.arena, sym_id, decl_idx);
            if let Some(decl_node) = decl_arena.get(decl_idx)
                && let Some(decl_iface) = decl_arena.get_interface(decl_node)
            {
                for &member_idx in &decl_iface.members.nodes {
                    let Some(member_node) = decl_arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != INDEX_SIGNATURE {
                        continue;
                    }
                    if let Some(index_sig) = decl_arena.get_index_signature(member_node) {
                        let param_idx = index_sig
                            .parameters
                            .nodes
                            .first()
                            .copied()
                            .unwrap_or(NodeIndex::NONE);
                        let key_type = if let Some(param_node) = decl_arena.get(param_idx)
                            && let Some(param) = decl_arena.get_parameter(param_node)
                            && param.type_annotation.is_some()
                        {
                            self.get_type_from_type_node(param.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        let value_type = if index_sig.type_annotation.is_some() {
                            self.get_type_from_type_node(index_sig.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        if key_type == TypeId::NUMBER {
                            derived_number_index_type = Some(value_type);
                        } else {
                            derived_string_index_type = Some(value_type);
                        }
                    }
                }
            }
        }

        // Maps member name -> (base_heritage_idx, base_name, type_id, is_optional)
        // base_heritage_idx uniquely identifies each extends-clause entry, so
        // `extends A<string>, A<number>` correctly detects conflicts even though
        // both entries share the base name "A".
        let mut inherited_member_sources: rustc_hash::FxHashMap<
            String,
            (NodeIndex, String, TypeId, bool),
        > = rustc_hash::FxHashMap::default();
        let mut inherited_non_public_class_member_sources: rustc_hash::FxHashMap<String, String> =
            rustc_hash::FxHashMap::default();

        // Track inherited index signatures for cross-base conflict detection (TS2430).
        // (base_heritage_idx, base_name, value_type) — if a new base has a conflicting
        // index signature, the interface "incorrectly extends" that base.
        let mut inherited_string_index: Option<(NodeIndex, String, TypeId)> = None;
        let mut inherited_number_index: Option<(NodeIndex, String, TypeId)> = None;

        // Collect ALL heritage clauses across ALL declarations of this interface.
        // When an interface is declaration-merged with a class, the class's `extends`
        // clause contributes an implicit base whose members must be checked for
        // cross-base conflicts with the interface's explicit `extends` bases (TS2320).
        // The class base is added first to match tsc's ordering in error messages.
        let mut all_heritage_types: Vec<(NodeIndex, NodeIndex)> = Vec::new(); // (clause_idx, type_idx)

        // First: collect heritage from merged class declaration (if any)
        if let Some(sym_id) = iface_sym_id
            && let Some(sym) = self.ctx.binder.symbols.get(sym_id)
        {
            for &decl_idx in &sym.declarations {
                let decl_arena = decl_arena_for(self.ctx.binder, self.ctx.arena, sym_id, decl_idx);
                if let Some(node) = decl_arena.get(decl_idx)
                    && decl_arena.get_class(node).is_some()
                {
                    let class_data = decl_arena
                        .get_class(node)
                        .expect("get_class guard above ensures Some");
                    if let Some(ref heritage_clauses) = class_data.heritage_clauses {
                        for &clause_idx in &heritage_clauses.nodes {
                            if let Some(clause_node) = decl_arena.get(clause_idx)
                                && let Some(heritage) = decl_arena.get_heritage_clause(clause_node)
                                && heritage.token == SyntaxKind::ExtendsKeyword as u16
                            {
                                for &type_idx in &heritage.types.nodes {
                                    all_heritage_types.push((clause_idx, type_idx));
                                }
                            }
                        }
                    }
                    break; // Only one class declaration per merged symbol
                }
            }
        }

        // Then: collect heritage from all interface declarations
        for &decl_idx in &all_iface_decls {
            let Some(sym_id) = iface_sym_id else {
                continue;
            };
            let decl_arena = decl_arena_for(self.ctx.binder, self.ctx.arena, sym_id, decl_idx);
            if let Some(decl_node) = decl_arena.get(decl_idx)
                && let Some(decl_iface) = decl_arena.get_interface(decl_node)
                && let Some(ref heritage_clauses) = decl_iface.heritage_clauses
            {
                for &clause_idx in &heritage_clauses.nodes {
                    if let Some(clause_node) = decl_arena.get(clause_idx)
                        && let Some(heritage) = decl_arena.get_heritage_clause(clause_node)
                        && heritage.token == SyntaxKind::ExtendsKeyword as u16
                    {
                        for &type_idx in &heritage.types.nodes {
                            all_heritage_types.push((clause_idx, type_idx));
                        }
                    }
                }
            }
        }

        // Process each extended type across all heritage clauses
        'heritage_type_loop: for &(_clause_idx, type_idx) in &all_heritage_types {
            let Some(type_node) = self.ctx.arena.get(type_idx) else {
                continue;
            };

            let (expr_idx, type_arguments) =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    (
                        expr_type_args.expression,
                        expr_type_args.type_arguments.as_ref(),
                    )
                } else {
                    (type_idx, None)
                };

            let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                continue;
            };

            let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                continue;
            };

            // Use the resolved symbol name (not the heritage expression text) for error
            // messages.  TSC uses `typeToString(baseType)` which resolves to the short
            // symbol name, e.g. "Mover" rather than "MoversAndShakers.Mover".
            let base_name_raw = base_symbol.escaped_name.clone();
            // Include type arguments in the base name for error messages, e.g. "A<string>"
            let base_name = if let Some(args) = type_arguments {
                let arg_strs: Vec<String> = args
                    .nodes
                    .iter()
                    .map(|&arg_idx| {
                        let tid = self.get_type_from_type_node(arg_idx);
                        self.format_type(tid)
                    })
                    .collect();
                if arg_strs.is_empty() {
                    base_name_raw
                } else {
                    format!("{}<{}>", base_name_raw, arg_strs.join(", "))
                }
            } else {
                base_name_raw
            };

            let mut base_iface_indices = Vec::new();
            for &decl_idx in &base_symbol.declarations {
                let decl_arena =
                    decl_arena_for(self.ctx.binder, self.ctx.arena, base_sym_id, decl_idx);
                if let Some(node) = decl_arena.get(decl_idx)
                    && decl_arena.get_interface(node).is_some()
                {
                    base_iface_indices.push(decl_idx);
                }
            }
            if base_iface_indices.is_empty() && base_symbol.value_declaration.is_some() {
                let decl_idx = base_symbol.value_declaration;
                let decl_arena =
                    decl_arena_for(self.ctx.binder, self.ctx.arena, base_sym_id, decl_idx);
                if let Some(node) = decl_arena.get(decl_idx)
                    && decl_arena.get_interface(node).is_some()
                {
                    base_iface_indices.push(decl_idx);
                }
            }

            // Collect ALL members from this base (direct + inherited from ancestors).
            // Use a worklist to walk the interface hierarchy without recursion.
            // Each entry: (interface_sym_id, interface_decl_idx, type_args_for_this_level)
            let mut worklist: Vec<(tsz_binder::SymbolId, NodeIndex, Option<Vec<TypeId>>)> =
                Vec::new();
            for &idx in &base_iface_indices {
                let initial_args = type_arguments.map(|args| {
                    args.nodes
                        .iter()
                        .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                        .collect::<Vec<_>>()
                });
                worklist.push((base_sym_id, idx, initial_args));
            }

            // Track which member keys we've already seen from THIS base entry.
            // Direct members shadow inherited ones, so we process closer bases first.
            let mut seen_member_keys: rustc_hash::FxHashSet<String> =
                rustc_hash::FxHashSet::default();
            // Prevent cycles in the interface hierarchy.
            let mut visited_ifaces: rustc_hash::FxHashSet<(u32, u32, usize)> =
                rustc_hash::FxHashSet::default();

            while let Some((iface_sym_id, iface_decl_idx, level_type_args)) = worklist.pop() {
                let iface_arena = decl_arena_for(
                    self.ctx.binder,
                    self.ctx.arena,
                    iface_sym_id,
                    iface_decl_idx,
                );
                let visit_key = (
                    iface_sym_id.0,
                    iface_decl_idx.0,
                    iface_arena as *const tsz_parser::parser::node::NodeArena as usize,
                );
                if !visited_ifaces.insert(visit_key) {
                    continue; // Already visited — cycle guard
                }

                let Some(iface_node) = iface_arena.get(iface_decl_idx) else {
                    continue;
                };
                let Some(iface) = iface_arena.get_interface(iface_node) else {
                    continue;
                };
                let (level_type_params, level_type_param_updates) =
                    self.push_type_parameters(&iface.type_parameters);

                let mut substitution_args = level_type_args.unwrap_or_default();
                if substitution_args.len() < level_type_params.len() {
                    for param in level_type_params.iter().skip(substitution_args.len()) {
                        let fallback = param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN);
                        substitution_args.push(fallback);
                    }
                }
                if substitution_args.len() > level_type_params.len() {
                    substitution_args.truncate(level_type_params.len());
                }

                let substitution = TypeSubstitution::from_args(
                    self.ctx.types,
                    &level_type_params,
                    &substitution_args,
                );

                let mut base_method_counts: rustc_hash::FxHashMap<String, usize> =
                    rustc_hash::FxHashMap::default();
                for &member_idx in &iface.members.nodes {
                    let Some(member_node) = iface_arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != METHOD_SIGNATURE {
                        continue;
                    }
                    let Some(sig) = iface_arena.get_signature(member_node) else {
                        continue;
                    };
                    let Some(name) = crate::types_domain::queries::core::get_literal_property_name(
                        iface_arena,
                        sig.name,
                    ) else {
                        continue;
                    };
                    *base_method_counts.entry(name).or_insert(0) += 1;
                }

                // Process direct members of this interface level
                for &member_idx in &iface.members.nodes {
                    let Some(member_node) = iface_arena.get(member_idx) else {
                        continue;
                    };

                    let (member_key, member_type, member_optional) =
                        if member_node.kind == CALL_SIGNATURE {
                            (
                                String::from("__call__"),
                                instantiate_type(
                                    self.ctx.types,
                                    self.get_type_of_node(member_idx),
                                    &substitution,
                                ),
                                false,
                            )
                        } else if member_node.kind == METHOD_SIGNATURE
                            || member_node.kind == PROPERTY_SIGNATURE
                        {
                            let Some(sig) = iface_arena.get_signature(member_node) else {
                                continue;
                            };
                            let Some(name) =
                                crate::types_domain::queries::core::get_literal_property_name(
                                    iface_arena,
                                    sig.name,
                                )
                            else {
                                continue;
                            };
                            (
                                name,
                                self.delegate_cross_arena_interface_member_simple_type(
                                    iface_decl_idx,
                                    member_idx,
                                    iface_arena,
                                    Some(&substitution_args),
                                )
                                .unwrap_or_else(|| {
                                    instantiate_type(
                                        self.ctx.types,
                                        self.get_type_of_interface_member_simple(member_idx),
                                        &substitution,
                                    )
                                }),
                                sig.question_token,
                            )
                        } else {
                            continue;
                        };

                    // Skip members already seen at a closer level in this base chain
                    if !seen_member_keys.insert(member_key.clone()) {
                        continue;
                    }

                    if let Some((
                        _derived_name,
                        derived_member_type,
                        derived_member_idx,
                        derived_kind,
                        _derived_optional,
                    )) = derived_members
                        .iter()
                        .find(|(derived_name, _, _, _, _)| derived_name == &member_key)
                    {
                        let overloaded_method_compare = *derived_kind == METHOD_SIGNATURE
                            && member_node.kind == METHOD_SIGNATURE
                            && (derived_method_counts.get(&member_key).copied().unwrap_or(0) > 1
                                || base_method_counts.get(&member_key).copied().unwrap_or(0) > 1);
                        if overloaded_method_compare {
                            continue;
                        }

                        let derived_prop_type =
                            crate::query_boundaries::common::find_property_by_str(
                                self.ctx.types,
                                *derived_member_type,
                                &member_key,
                            )
                            .map(|p| p.type_id)
                            .unwrap_or(*derived_member_type);
                        let base_prop_type = crate::query_boundaries::common::find_property_by_str(
                            self.ctx.types,
                            member_type,
                            &member_key,
                        )
                        .map(|p| p.type_id)
                        .unwrap_or(member_type);

                        let property_signature_pair = *derived_kind == PROPERTY_SIGNATURE
                            && member_node.kind == PROPERTY_SIGNATURE;
                        let callable_property_pair = property_signature_pair
                            && (crate::query_boundaries::common::callable_shape_for_type(
                                self.ctx.types,
                                derived_prop_type,
                            )
                            .is_some()
                                || crate::query_boundaries::common::has_function_shape(
                                    self.ctx.types,
                                    derived_prop_type,
                                ))
                            && (crate::query_boundaries::common::callable_shape_for_type(
                                self.ctx.types,
                                base_prop_type,
                            )
                            .is_some()
                                || crate::query_boundaries::common::has_function_shape(
                                    self.ctx.types,
                                    base_prop_type,
                                ));

                        let type_mismatch = if callable_property_pair {
                            should_report_property_type_mismatch(
                                self,
                                derived_prop_type,
                                base_prop_type,
                                *derived_member_idx,
                            )
                        } else {
                            should_report_member_type_mismatch(
                                self,
                                derived_prop_type,
                                base_prop_type,
                                *derived_member_idx,
                            )
                        };

                        if type_mismatch {
                            let derived_type_str = self.format_type(derived_prop_type);
                            let base_type_str = self.format_type(base_prop_type);
                            self.error_at_node(
                                iface_data.name,
                                &format!(
                                    "Interface '{derived_name}' incorrectly extends interface '{base_name}'.\n  Types of property '{member_key}' are incompatible.\n    Type '{derived_type_str}' is not assignable to type '{base_type_str}'."
                                ),
                                diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                            );
                            self.pop_type_parameters(level_type_param_updates);
                            continue 'heritage_type_loop;
                        }
                        continue;
                    }

                    if let Some((
                        prev_heritage_idx,
                        prev_base_name,
                        prev_member_type,
                        prev_optional,
                    )) = inherited_member_sources.get(&member_key)
                    {
                        if *prev_heritage_idx != type_idx {
                            let optionality_differs = member_optional != *prev_optional;
                            // Use identity checking (not assignability) — tsc uses
                            // isTypeIdenticalTo for TS2320. Assignability is too loose
                            // when `any` is involved (e.g., `f(x: any): any` vs `f<T>(x: T): T`
                            // are mutually assignable but not identical).
                            let type_incompatible =
                                !self.are_var_decl_types_compatible(member_type, *prev_member_type);
                            if type_incompatible || optionality_differs {
                                self.error_at_node(
                                        iface_data.name,
                                        &format!(
                                            "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
                                        ),
                                        diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                                    );
                                self.pop_type_parameters(level_type_param_updates);
                                return;
                            }
                        }
                    } else {
                        inherited_member_sources.insert(
                            member_key,
                            (type_idx, base_name.clone(), member_type, member_optional),
                        );
                    }
                }

                // Process index signatures from this base level.
                // Check for cross-base index signature conflicts (TS2430).
                for &member_idx in &iface.members.nodes {
                    let Some(member_node) = iface_arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != INDEX_SIGNATURE {
                        continue;
                    }
                    if let Some(idx_sig) = iface_arena.get_index_signature(member_node) {
                        let param_idx = idx_sig
                            .parameters
                            .nodes
                            .first()
                            .copied()
                            .unwrap_or(NodeIndex::NONE);
                        let key_type = if let Some(param_node) = iface_arena.get(param_idx)
                            && let Some(param) = iface_arena.get_parameter(param_node)
                            && param.type_annotation.is_some()
                        {
                            self.get_type_from_type_node(param.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        let value_type = if idx_sig.type_annotation.is_some() {
                            self.get_type_from_type_node(idx_sig.type_annotation)
                        } else {
                            TypeId::ANY
                        };
                        let value_type =
                            instantiate_type(self.ctx.types, value_type, &substitution);

                        let inherited_slot = if key_type == TypeId::NUMBER {
                            &mut inherited_number_index
                        } else {
                            &mut inherited_string_index
                        };

                        if let Some((prev_heritage_idx, ref _prev_base_name, prev_val)) =
                            *inherited_slot
                        {
                            if prev_heritage_idx != type_idx {
                                // Different bases provide conflicting index signatures.
                                // tsc emits TS2430 ("incorrectly extends") against the
                                // later base, not TS2320 ("cannot simultaneously extend").
                                if !self.is_assignable_to(prev_val, value_type)
                                    && !self.is_assignable_to(value_type, prev_val)
                                {
                                    // The later base's index signature conflicts with
                                    // what was inherited from earlier bases.
                                    // tsc reports TS2430 against the later base only.
                                    self.error_at_node(
                                        iface_data.name,
                                        &format!(
                                            "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                                        ),
                                        diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                    );
                                    self.pop_type_parameters(level_type_param_updates);
                                    return;
                                }
                            }
                        } else {
                            *inherited_slot = Some((type_idx, base_name.clone(), value_type));
                        }
                    }
                }

                // Enqueue this interface's own bases (grandparent interfaces)
                if let Some(ref heritage_clauses) = iface.heritage_clauses {
                    for &hc_idx in &heritage_clauses.nodes {
                        let Some(hc_node) = iface_arena.get(hc_idx) else {
                            continue;
                        };
                        let Some(hc) = iface_arena.get_heritage_clause(hc_node) else {
                            continue;
                        };
                        if hc.token != SyntaxKind::ExtendsKeyword as u16 {
                            continue;
                        }
                        for &ancestor_type_idx in &hc.types.nodes {
                            let (ancestor_expr, ancestor_type_args_opt) = if let Some(ancestor_node) =
                                iface_arena.get(ancestor_type_idx)
                                && let Some(eat) = iface_arena.get_expr_type_args(ancestor_node)
                            {
                                let args: Vec<TypeId> = eat
                                    .type_arguments
                                    .as_ref()
                                    .map(|a| {
                                        a.nodes
                                            .iter()
                                            .map(|&arg_idx| {
                                                instantiate_type(
                                                    self.ctx.types,
                                                    self.get_type_from_type_node(arg_idx),
                                                    &substitution,
                                                )
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                (eat.expression, Some(args))
                            } else {
                                (ancestor_type_idx, None)
                            };

                            let ancestor_resolution = self.resolve_heritage_symbol(ancestor_expr);
                            if let Some(ancestor_sym_id) = ancestor_resolution
                                && let Some(ancestor_sym) =
                                    self.ctx.binder.get_symbol(ancestor_sym_id)
                            {
                                for &decl_idx in &ancestor_sym.declarations {
                                    let decl_arena = decl_arena_for(
                                        self.ctx.binder,
                                        self.ctx.arena,
                                        ancestor_sym_id,
                                        decl_idx,
                                    );
                                    if let Some(dn) = decl_arena.get(decl_idx)
                                        && decl_arena.get_interface(dn).is_some()
                                    {
                                        worklist.push((
                                            ancestor_sym_id,
                                            decl_idx,
                                            ancestor_type_args_opt.clone(),
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }

                self.pop_type_parameters(level_type_param_updates);
            }

            // If the base is not an interface, check if it's a class
            if base_iface_indices.is_empty() {
                let mut base_class_idx = None;
                for &decl_idx in &base_symbol.declarations {
                    let decl_arena =
                        decl_arena_for(self.ctx.binder, self.ctx.arena, base_sym_id, decl_idx);
                    if let Some(node) = decl_arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                    {
                        base_class_idx = Some(decl_idx);
                        break;
                    }
                }

                if base_class_idx.is_none() && base_symbol.value_declaration.is_some() {
                    let decl_idx = base_symbol.value_declaration;
                    let decl_arena =
                        decl_arena_for(self.ctx.binder, self.ctx.arena, base_sym_id, decl_idx);
                    if let Some(node) = decl_arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                    {
                        base_class_idx = Some(decl_idx);
                    }
                }

                if let Some(class_idx) = base_class_idx
                    && let Some(class_node) =
                        decl_arena_for(self.ctx.binder, self.ctx.arena, base_sym_id, class_idx)
                            .get(class_idx)
                    && let Some(class_data) =
                        decl_arena_for(self.ctx.binder, self.ctx.arena, base_sym_id, class_idx)
                            .get_class(class_node)
                {
                    // Build type parameter substitution for generic class bases
                    // e.g. `extends C<string>` where `class C<T> { a: T; }` → a: string
                    let (class_type_params, class_type_param_updates) =
                        self.push_type_parameters(&class_data.type_parameters);
                    let mut class_subst_args: Vec<TypeId> = type_arguments
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                .collect()
                        })
                        .unwrap_or_default();
                    if class_subst_args.len() < class_type_params.len() {
                        for param in class_type_params.iter().skip(class_subst_args.len()) {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            class_subst_args.push(fallback);
                        }
                    }
                    if class_subst_args.len() > class_type_params.len() {
                        class_subst_args.truncate(class_type_params.len());
                    }
                    let class_substitution = TypeSubstitution::from_args(
                        self.ctx.types,
                        &class_type_params,
                        &class_subst_args,
                    );

                    // Check if any interface member redeclares a private/protected class member
                    for (member_name, _, _derived_member_idx, _, _) in &derived_members {
                        for &class_member_idx in &class_data.members.nodes {
                            let Some(class_member_node) = self.ctx.arena.get(class_member_idx)
                            else {
                                continue;
                            };

                            let (class_member_name, is_private_or_protected) =
                                match class_member_node.kind {
                                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                                        if let Some(prop) =
                                            self.ctx.arena.get_property_decl(class_member_node)
                                        {
                                            let name = self.get_property_name(prop.name);
                                            let is_priv_prot = self
                                                .has_private_modifier(&prop.modifiers)
                                                || self.has_protected_modifier(&prop.modifiers);
                                            (name, is_priv_prot)
                                        } else {
                                            continue;
                                        }
                                    }
                                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                        if let Some(method) =
                                            self.ctx.arena.get_method_decl(class_member_node)
                                        {
                                            let name = self.get_property_name(method.name);
                                            let is_priv_prot = self
                                                .has_private_modifier(&method.modifiers)
                                                || self.has_protected_modifier(&method.modifiers);
                                            (name, is_priv_prot)
                                        } else {
                                            continue;
                                        }
                                    }
                                    k if k == syntax_kind_ext::GET_ACCESSOR
                                        || k == syntax_kind_ext::SET_ACCESSOR =>
                                    {
                                        if let Some(accessor) =
                                            self.ctx.arena.get_accessor(class_member_node)
                                        {
                                            let name = self.get_property_name(accessor.name);
                                            let is_priv_prot = self
                                                .has_private_modifier(&accessor.modifiers)
                                                || self.has_protected_modifier(&accessor.modifiers);
                                            (name, is_priv_prot)
                                        } else {
                                            continue;
                                        }
                                    }
                                    _ => continue,
                                };

                            if let Some(class_member_name) = class_member_name
                                && &class_member_name == member_name
                                && is_private_or_protected
                            {
                                // Interface redeclares a private/protected member as public - TS2430.
                                // tsc reports this at the interface NAME identifier (matching tsc parity).
                                // tsc does NOT emit a secondary "Property 'x' is private..." detail.
                                self.error_at_node(
                                        iface_data.name,
                                        &format!(
                                            "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                                        ),
                                        diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                    );
                            }
                        }
                    }

                    // TS2320: Collect ALL class members and check for cross-base conflicts.
                    // This handles:
                    //  - public member type conflicts between class bases or class+interface bases
                    //  - visibility conflicts (public vs private/protected) between bases
                    //  - private/protected member name conflicts between different class bases
                    for &class_member_idx in &class_data.members.nodes {
                        let Some(member_info) =
                            self.extract_class_member_info(class_member_idx, false)
                        else {
                            continue;
                        };

                        if member_info.is_static {
                            continue;
                        }

                        if derived_member_names.contains(&member_info.name) {
                            continue;
                        }

                        let member_type = instantiate_type(
                            self.ctx.types,
                            member_info.type_id,
                            &class_substitution,
                        );

                        if member_info.visibility != MemberVisibility::Public {
                            // Non-public: check against other non-public class members
                            if let Some(prev_base_name) =
                                inherited_non_public_class_member_sources.get(&member_info.name)
                            {
                                if prev_base_name != &base_name {
                                    self.error_at_node(
                                        iface_data.name,
                                        &format!(
                                            "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
                                        ),
                                        diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                                    );
                                    self.pop_type_parameters(class_type_param_updates);
                                    return;
                                }
                            } else {
                                inherited_non_public_class_member_sources
                                    .insert(member_info.name.clone(), base_name.clone());
                            }

                            // Also check visibility conflict: non-public here vs public from
                            // another base stored in inherited_member_sources
                            if let Some((prev_heritage_idx, prev_base_name, _, _)) =
                                inherited_member_sources.get(&member_info.name)
                                && *prev_heritage_idx != type_idx
                            {
                                self.error_at_node(
                                        iface_data.name,
                                        &format!(
                                            "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
                                        ),
                                        diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                                    );
                                self.pop_type_parameters(class_type_param_updates);
                                return;
                            }
                        } else {
                            // Public member: check type conflicts against inherited_member_sources
                            // (which contains members from previous interface AND class bases)
                            if let Some((
                                prev_heritage_idx,
                                prev_base_name,
                                prev_member_type,
                                _prev_optional,
                            )) = inherited_member_sources.get(&member_info.name)
                            {
                                if *prev_heritage_idx != type_idx {
                                    let type_incompatible = !self.are_var_decl_types_compatible(
                                        member_type,
                                        *prev_member_type,
                                    );
                                    if type_incompatible {
                                        self.error_at_node(
                                            iface_data.name,
                                            &format!(
                                                "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
                                            ),
                                            diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                                        );
                                        self.pop_type_parameters(class_type_param_updates);
                                        return;
                                    }
                                }
                            } else {
                                // Also check: public member vs non-public from another class base
                                if let Some(prev_base_name) =
                                    inherited_non_public_class_member_sources.get(&member_info.name)
                                    && prev_base_name != &base_name
                                {
                                    self.error_at_node(
                                            iface_data.name,
                                            &format!(
                                                "Interface '{derived_name}' cannot simultaneously extend types '{prev_base_name}' and '{base_name}'."
                                            ),
                                            diagnostic_codes::INTERFACE_CANNOT_SIMULTANEOUSLY_EXTEND_TYPES_AND,
                                        );
                                    self.pop_type_parameters(class_type_param_updates);
                                    return;
                                }
                                inherited_member_sources.insert(
                                    member_info.name.clone(),
                                    (type_idx, base_name.clone(), member_type, false),
                                );
                            }
                        }
                    }

                    self.pop_type_parameters(class_type_param_updates);
                }

                // If the base is neither an interface nor a class, it may be a type alias
                // (e.g., `interface I extends T1 { ... }` where `type T1 = { a: number }`).
                // Resolve the base type and check property compatibility.
                if base_class_idx.is_none() {
                    // Resolve the base type. For non-generic type aliases,
                    // get_type_of_symbol returns the resolved type directly.
                    // For generic aliases with type arguments, build an Application
                    // using DefId-first resolution so the evaluator can instantiate.
                    let base_type = if let Some(args) = type_arguments {
                        let type_arg_ids: Vec<TypeId> = args
                            .nodes
                            .iter()
                            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                            .collect();
                        if !type_arg_ids.is_empty() {
                            // Generic: Application(Lazy(DefId), [args])
                            let def_id = self.ctx.get_or_create_def_id(base_sym_id);
                            let factory = self.ctx.types.factory();
                            let lazy_type = factory.lazy(def_id);
                            let app = factory.application(lazy_type, type_arg_ids);
                            self.evaluate_type_with_env(app)
                        } else {
                            self.get_type_of_symbol(base_sym_id)
                        }
                    } else {
                        self.get_type_of_symbol(base_sym_id)
                    };

                    if base_type != TypeId::ERROR {
                        // Check: when base has numeric index signature but derived doesn't,
                        // all derived named properties must be assignable to base's index value type.
                        // This catches cases like:
                        //   interface HTMLElement { [index: number]: HTMLElement; }
                        //   interface HTMLFormElement extends HTMLElement {
                        //     acceptCharset: string;  // Error: string not assignable to HTMLElement
                        //   }
                        if derived_number_index_type.is_none() {
                            // Check if base has numeric index signature
                            let base_num_index_value =
                                crate::query_boundaries::common::object_shape_for_type(
                                    self.ctx.types,
                                    base_type,
                                )
                                .and_then(|shape| {
                                    shape.number_index.as_ref().map(|idx| idx.value_type)
                                });

                            if let Some(base_index_val) = base_num_index_value {
                                // Skip the index signature check when the base index value type
                                // contains type parameters (e.g., `Array<E>` has `[index: number]: E`).
                                // When the base is generic, property compatibility depends on the
                                // actual instantiation and should be deferred.
                                let base_index_is_generic =
                                    crate::query_boundaries::common::contains_type_parameters(
                                        self.ctx.types.as_type_database(),
                                        base_index_val,
                                    );
                                if !base_index_is_generic {
                                    for (
                                        member_name,
                                        member_type,
                                        _derived_member_idx,
                                        _derived_kind,
                                        _,
                                    ) in &derived_members
                                    {
                                        // Extract the derived property's raw type
                                        let derived_prop_type =
                                            crate::query_boundaries::common::find_property_by_str(
                                                self.ctx.types,
                                                *member_type,
                                                member_name,
                                            )
                                            .map(|p| p.type_id)
                                            .unwrap_or(*member_type);

                                        // Check if property type is assignable to base index value type
                                        if !self.is_assignable_to(derived_prop_type, base_index_val)
                                        {
                                            self.error_at_node(
                                            iface_data.name,
                                            &format!(
                                                "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                                            ),
                                            diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                        );
                                            // Don't return — continue checking other bases
                                            break;
                                        }
                                    }
                                } // end if !base_index_is_generic
                            }
                        }

                        // Check each derived member against the base type's properties
                        for (member_name, member_type, derived_member_idx, _derived_kind, _) in
                            &derived_members
                        {
                            // Look up the property in the base type
                            let base_prop = crate::query_boundaries::common::find_property_by_str(
                                self.ctx.types,
                                base_type,
                                member_name,
                            );

                            // For intersection types, search each member
                            let base_prop = base_prop.or_else(|| {
                                if let Some(members) =
                                    crate::query_boundaries::common::intersection_members(
                                        self.ctx.types,
                                        base_type,
                                    )
                                {
                                    for &member in &members {
                                        let prop =
                                            crate::query_boundaries::common::find_property_by_str(
                                                self.ctx.types,
                                                member,
                                                member_name,
                                            );
                                        if prop.is_some() {
                                            return prop;
                                        }
                                    }
                                }
                                None
                            });

                            // Resolve the base property type: use shallow lookup result if available,
                            // otherwise fall back to the solver's comprehensive property access
                            // (handles Array, Tuple, Mapped types, etc.)
                            let base_prop_type = if let Some(ref bp) = base_prop {
                                Some(bp.type_id)
                            } else {
                                use crate::query_boundaries::common::PropertyAccessResult;
                                match self.resolve_property_access_with_env(base_type, member_name)
                                {
                                    PropertyAccessResult::Success { type_id, .. } => Some(type_id),
                                    _ => None,
                                }
                            };

                            if let Some(base_prop_type_id) = base_prop_type {
                                // Extract the derived property's raw type from its ObjectShape
                                // (get_type_of_interface_member returns ObjectShape { name: type },
                                // but we need the raw property type for comparison with base)
                                let derived_prop_type =
                                    crate::query_boundaries::common::find_property_by_str(
                                        self.ctx.types,
                                        *member_type,
                                        member_name,
                                    )
                                    .map(|p| p.type_id)
                                    .unwrap_or(*member_type);

                                if should_report_member_type_mismatch(
                                    self,
                                    derived_prop_type,
                                    base_prop_type_id,
                                    *derived_member_idx,
                                ) {
                                    self.error_at_node(
                                        iface_data.name,
                                        &format!(
                                            "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                                        ),
                                        diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                    );
                                    break; // Report one incompatibility per base type
                                }
                            }
                        }
                    }
                }

                continue;
            }

            let Some(&base_root_idx) = base_iface_indices.first() else {
                continue;
            };

            let Some(base_root_node) = self.ctx.arena.get(base_root_idx) else {
                continue;
            };

            let Some(base_root_iface) = self.ctx.arena.get_interface(base_root_node) else {
                continue;
            };

            let mut type_args = Vec::new();
            if let Some(args) = type_arguments {
                for &arg_idx in &args.nodes {
                    type_args.push(self.get_type_from_type_node(arg_idx));
                }
            }

            let (base_type_params, base_type_param_updates) =
                self.push_type_parameters(&base_root_iface.type_parameters);

            if type_args.len() < base_type_params.len() {
                for param in base_type_params.iter().skip(type_args.len()) {
                    let fallback = param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN);
                    type_args.push(fallback);
                }
            }
            if type_args.len() > base_type_params.len() {
                type_args.truncate(base_type_params.len());
            }

            let substitution =
                TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);

            // When a base interface has overloaded methods, defer mismatch reporting
            // for those method names to the dedicated overload coverage pass below.
            let overloaded_base_method_names: rustc_hash::FxHashSet<String> = {
                let mut counts: rustc_hash::FxHashMap<String, usize> =
                    rustc_hash::FxHashMap::default();
                for &base_iface_idx in &base_iface_indices {
                    let Some(base_node) = self.ctx.arena.get(base_iface_idx) else {
                        continue;
                    };
                    let Some(base_iface) = self.ctx.arena.get_interface(base_node) else {
                        continue;
                    };
                    for &base_member_idx in &base_iface.members.nodes {
                        let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                            continue;
                        };
                        if base_member_node.kind != METHOD_SIGNATURE {
                            continue;
                        }
                        let Some(sig) = self.ctx.arena.get_signature(base_member_node) else {
                            continue;
                        };
                        let Some(name) = self.get_property_name(sig.name) else {
                            continue;
                        };
                        *counts.entry(name).or_insert(0) += 1;
                    }
                }
                counts
                    .into_iter()
                    .filter_map(|(name, count)| (count > 1).then_some(name))
                    .collect()
            };

            let mut ts2430_emitted_for_base = false;
            'derived_loop: for (
                member_name,
                member_type,
                derived_member_idx,
                derived_kind,
                derived_is_optional,
            ) in &derived_members
            {
                let mut found = false;

                for &base_iface_idx in &base_iface_indices {
                    let Some(base_node) = self.ctx.arena.get(base_iface_idx) else {
                        continue;
                    };
                    let Some(base_iface) = self.ctx.arena.get_interface(base_node) else {
                        continue;
                    };

                    for &base_member_idx in &base_iface.members.nodes {
                        let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                            continue;
                        };

                        let (base_member_name, base_type, base_is_optional) =
                            if base_member_node.kind == METHOD_SIGNATURE
                                || base_member_node.kind == PROPERTY_SIGNATURE
                            {
                                if let Some(sig) = self.ctx.arena.get_signature(base_member_node) {
                                    if let Some(name) = self.get_property_name(sig.name) {
                                        let type_id =
                                            self.get_type_of_interface_member(base_member_idx);
                                        (name, type_id, sig.question_token)
                                    } else {
                                        continue;
                                    }
                                } else {
                                    continue;
                                }
                            } else {
                                continue;
                            };

                        if *member_name != base_member_name {
                            continue;
                        }

                        found = true;
                        let base_type = instantiate_type(self.ctx.types, base_type, &substitution);

                        if *derived_kind == METHOD_SIGNATURE
                            && base_member_node.kind == METHOD_SIGNATURE
                            && overloaded_base_method_names.contains(member_name)
                        {
                            // Overloaded method names are validated by the
                            // overload coverage check after this loop.
                            break;
                        }

                        // For method signatures, also check required parameter
                        // count: derived methods must not require more parameters
                        // than the base method provides. This catches the
                        // "target signature provides too few arguments" case.
                        // Skip this check when the base method is optional (`?`):
                        // a derived interface may override an optional method with
                        // a required one that has any number of required parameters.
                        let param_count_incompatible = if *derived_kind == METHOD_SIGNATURE
                            && base_member_node.kind == METHOD_SIGNATURE
                            && !base_is_optional
                        {
                            let derived_required =
                                self.count_required_params_from_signature_node(*derived_member_idx);
                            let base_required =
                                self.count_required_params_from_signature_node(base_member_idx);
                            derived_required > base_required
                        } else {
                            false
                        };

                        // For property signatures, use regular assignability
                        // (allows generic instantiation). For method signatures,
                        // use no_erase_generics mode (tsc's compareSignaturesRelated).
                        let type_mismatch = if *derived_kind == PROPERTY_SIGNATURE
                            && base_member_node.kind == PROPERTY_SIGNATURE
                        {
                            should_report_property_type_mismatch(
                                self,
                                *member_type,
                                base_type,
                                *derived_member_idx,
                            )
                        } else if *derived_kind == METHOD_SIGNATURE
                            && base_member_node.kind == METHOD_SIGNATURE
                        {
                            let derived_method_type =
                                crate::query_boundaries::common::find_property_by_str(
                                    self.ctx.types,
                                    *member_type,
                                    member_name,
                                )
                                .map(|p| p.type_id)
                                .unwrap_or(*member_type);
                            let base_method_type =
                                crate::query_boundaries::common::find_property_by_str(
                                    self.ctx.types,
                                    base_type,
                                    member_name,
                                )
                                .map(|p| p.type_id)
                                .unwrap_or(base_type);
                            should_report_member_type_mismatch(
                                self,
                                derived_method_type,
                                base_method_type,
                                *derived_member_idx,
                            )
                        } else {
                            should_report_member_type_mismatch(
                                self,
                                *member_type,
                                base_type,
                                *derived_member_idx,
                            )
                        };

                        // Making a required base property optional is an error (TS2430):
                        // an S value with Foo=undefined would not satisfy T which requires Foo.
                        let optionality_widened = *derived_is_optional && !base_is_optional;

                        if param_count_incompatible || type_mismatch || optionality_widened {
                            self.error_at_node(
                                    iface_data.name,
                                    &format!(
                                        "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                                    ),
                                    diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                                );
                            // Don't return — continue checking other base types.
                            // Each incompatible base gets its own TS2430 diagnostic.
                            ts2430_emitted_for_base = true;
                            break 'derived_loop;
                        }

                        break;
                    }

                    if found {
                        break;
                    }
                }
            }

            // Method overload coverage check: the 'derived_loop above only compares
            // each derived overload against the FIRST matching base overload. When the
            // base has multiple overloads for the same method, we must verify that EACH
            // base overload is matched by at least one derived overload. If any base
            // overload is unmatched, emit TS2430.
            if !ts2430_emitted_for_base {
                // Collect all base method overloads grouped by name
                let base_method_overloads: Vec<(String, Vec<TypeId>)>;
                {
                    let mut by_name: rustc_hash::FxHashMap<String, Vec<TypeId>> =
                        rustc_hash::FxHashMap::default();
                    for &base_iface_idx in &base_iface_indices {
                        if let Some(base_node) = self.ctx.arena.get(base_iface_idx)
                            && let Some(base_iface) = self.ctx.arena.get_interface(base_node)
                        {
                            for &base_member_idx in &base_iface.members.nodes {
                                let Some(base_member_node) = self.ctx.arena.get(base_member_idx)
                                else {
                                    continue;
                                };
                                if base_member_node.kind != METHOD_SIGNATURE {
                                    continue;
                                }
                                let Some(sig) = self.ctx.arena.get_signature(base_member_node)
                                else {
                                    continue;
                                };
                                let Some(name) = self.get_property_name(sig.name) else {
                                    continue;
                                };
                                if !derived_member_names.contains(&name) {
                                    continue;
                                }
                                let base_type = instantiate_type(
                                    self.ctx.types,
                                    self.get_type_of_interface_member(base_member_idx),
                                    &substitution,
                                );
                                by_name.entry(name).or_default().push(base_type);
                            }
                        }
                    }
                    base_method_overloads =
                        by_name.into_iter().filter(|(_, v)| v.len() > 1).collect();
                }

                // Collect derived method overloads grouped by name
                let mut derived_method_overloads: rustc_hash::FxHashMap<
                    String,
                    Vec<(TypeId, NodeIndex)>,
                > = rustc_hash::FxHashMap::default();
                for (name, type_id, idx, kind, _) in &derived_members {
                    if *kind == METHOD_SIGNATURE {
                        derived_method_overloads
                            .entry(name.clone())
                            .or_default()
                            .push((*type_id, *idx));
                    }
                }

                let signature_has_literal_parameter = |type_id: TypeId| -> bool {
                    let has_literal_param =
                        |params: &[crate::query_boundaries::common::ParamInfo]| {
                            params.iter().any(|param| {
                                crate::query_boundaries::common::is_literal_type(
                                    self.ctx.types,
                                    param.type_id,
                                )
                            })
                        };

                    if let Some(signatures) =
                        crate::query_boundaries::common::call_signatures_for_type(
                            self.ctx.types,
                            type_id,
                        )
                    {
                        return signatures
                            .iter()
                            .any(|signature| has_literal_param(&signature.params));
                    }

                    if let Some(shape) = crate::query_boundaries::common::function_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) {
                        return has_literal_param(&shape.params);
                    }

                    if let Some(shape) = crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) {
                        return shape
                            .call_signatures
                            .iter()
                            .any(|signature| has_literal_param(&signature.params));
                    }

                    if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                        self.ctx.types,
                        type_id,
                    ) && shape.properties.len() == 1
                        && shape.properties[0].is_method
                    {
                        let method_type = shape.properties[0].type_id;
                        if let Some(signatures) =
                            crate::query_boundaries::common::call_signatures_for_type(
                                self.ctx.types,
                                method_type,
                            )
                        {
                            return signatures
                                .iter()
                                .any(|signature| has_literal_param(&signature.params));
                        }
                        if let Some(shape) =
                            crate::query_boundaries::common::function_shape_for_type(
                                self.ctx.types,
                                method_type,
                            )
                        {
                            return has_literal_param(&shape.params);
                        }
                        if let Some(shape) =
                            crate::query_boundaries::common::callable_shape_for_type(
                                self.ctx.types,
                                method_type,
                            )
                        {
                            return shape
                                .call_signatures
                                .iter()
                                .any(|signature| has_literal_param(&signature.params));
                        }
                    }

                    false
                };

                let select_implementation_signature = |signatures: &[TypeId]| -> Option<TypeId> {
                    if signatures.is_empty() {
                        return None;
                    }

                    let mut last_non_specialized: Option<TypeId> = None;
                    for &signature in signatures {
                        if !signature_has_literal_parameter(signature) {
                            last_non_specialized = Some(signature);
                        }
                    }

                    last_non_specialized.or_else(|| signatures.last().copied())
                };

                let has_non_specialized_signature = |signatures: &[TypeId]| -> bool {
                    signatures
                        .iter()
                        .any(|&signature| !signature_has_literal_parameter(signature))
                };

                let select_implementation_signature_with_node =
                    |signatures: &[(TypeId, NodeIndex)]| -> Option<(TypeId, NodeIndex)> {
                        if signatures.is_empty() {
                            return None;
                        }

                        let mut last_non_specialized: Option<(TypeId, NodeIndex)> = None;
                        for &(signature, node_idx) in signatures {
                            if !signature_has_literal_parameter(signature) {
                                last_non_specialized = Some((signature, node_idx));
                            }
                        }

                        last_non_specialized.or_else(|| signatures.last().copied())
                    };

                let has_non_specialized_signature_with_node =
                    |signatures: &[(TypeId, NodeIndex)]| -> bool {
                        signatures
                            .iter()
                            .any(|&(signature, _)| !signature_has_literal_parameter(signature))
                    };
                let signature_contains_error = |signature: TypeId| {
                    crate::query_boundaries::common::contains_error_type_in_args(
                        self.ctx.types,
                        signature,
                    )
                };

                // For overloaded method inheritance, tsc compatibility hinges on
                // the trailing (implementation) signature.
                'overload_check: for (method_name, base_sigs) in &base_method_overloads {
                    let Some(derived_sigs) = derived_method_overloads.get(method_name) else {
                        continue;
                    };
                    // The overload coverage pass runs after ordinary member
                    // compatibility, so it must apply the same cascading-error
                    // suppression. Post-merge lib validation can leave event-map
                    // overload parameters unresolved; those should not become
                    // TS2430 diagnostics on unrelated default-lib interfaces.
                    if base_sigs.iter().copied().any(signature_contains_error)
                        || derived_sigs
                            .iter()
                            .any(|(signature, _)| signature_contains_error(*signature))
                    {
                        continue;
                    }
                    if has_non_specialized_signature(base_sigs)
                        && !has_non_specialized_signature_with_node(derived_sigs)
                    {
                        self.error_at_node(
                            iface_data.name,
                            &format!(
                                "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                            ),
                            diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                        );
                        break 'overload_check;
                    }
                    let Some(base_trailing_sig) = select_implementation_signature(base_sigs) else {
                        continue;
                    };
                    let Some((derived_trailing_sig, derived_trailing_idx)) =
                        select_implementation_signature_with_node(derived_sigs)
                    else {
                        continue;
                    };

                    if !self
                        .is_assignable_to_no_erase_generics(derived_trailing_sig, base_trailing_sig)
                        && !self.should_suppress_assignability_for_parse_recovery(
                            derived_trailing_idx,
                            derived_trailing_idx,
                        )
                    {
                        self.error_at_node(
                            iface_data.name,
                            &format!(
                                "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                            ),
                            diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                        );
                        break 'overload_check;
                    }
                }
            }

            // Check index signature compatibility: if the derived interface declares
            // an index signature, the base interface's index signature (if any) must be
            // compatible. E.g., `interface F extends E` where F has `[s: string]: number`
            // and E has `[s: string]: string` → TS2430.
            if derived_string_index_type.is_some() || derived_number_index_type.is_some() {
                let mut base_string_index_value: Option<TypeId> = None;
                let mut base_number_index_value: Option<TypeId> = None;

                for &base_iface_idx in &base_iface_indices {
                    let Some(base_node) = self.ctx.arena.get(base_iface_idx) else {
                        continue;
                    };
                    let Some(base_iface) = self.ctx.arena.get_interface(base_node) else {
                        continue;
                    };

                    for &base_member_idx in &base_iface.members.nodes {
                        let Some(base_member_node) = self.ctx.arena.get(base_member_idx) else {
                            continue;
                        };
                        if base_member_node.kind != INDEX_SIGNATURE {
                            continue;
                        }
                        if let Some(base_idx_sig) =
                            self.ctx.arena.get_index_signature(base_member_node)
                        {
                            let param_idx = base_idx_sig
                                .parameters
                                .nodes
                                .first()
                                .copied()
                                .unwrap_or(NodeIndex::NONE);
                            let key_type = if let Some(param_node) = self.ctx.arena.get(param_idx)
                                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                                && param.type_annotation.is_some()
                            {
                                self.get_type_from_type_node(param.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            let value_type = if base_idx_sig.type_annotation.is_some() {
                                self.get_type_from_type_node(base_idx_sig.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            let value_type =
                                instantiate_type(self.ctx.types, value_type, &substitution);
                            if key_type == TypeId::NUMBER {
                                base_number_index_value = Some(value_type);
                            } else {
                                base_string_index_value = Some(value_type);
                            }
                        }
                    }
                }

                // Check string index compatibility
                if let (Some(derived_val), Some(base_val)) =
                    (derived_string_index_type, base_string_index_value)
                    && !self.is_assignable_to(derived_val, base_val)
                {
                    self.error_at_node(
                            iface_data.name,
                            &format!(
                                "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                            ),
                            diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                        );
                }

                // Check number index compatibility
                if let (Some(derived_val), Some(base_val)) =
                    (derived_number_index_type, base_number_index_value)
                    && !self.is_assignable_to(derived_val, base_val)
                {
                    self.error_at_node(
                            iface_data.name,
                            &format!(
                                "Interface '{derived_name}' incorrectly extends interface '{base_name}'."
                            ),
                            diagnostic_codes::INTERFACE_INCORRECTLY_EXTENDS_INTERFACE,
                        );
                }
            }

            self.pop_type_parameters(base_type_param_updates);
        }
    }
}
