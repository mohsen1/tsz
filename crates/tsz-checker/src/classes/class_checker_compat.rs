//! Class and interface compatibility checking (TS2415, TS2430), member lookup
//! in class chains, and visibility conflict detection.

use crate::class_checker::{ClassMemberInfo, MemberVisibility};
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

        // Get heritage clauses (extends) — must have at least one across all declarations
        if iface_data.heritage_clauses.is_none() {
            // Check if other declarations of this interface have heritage clauses
            let has_heritage_elsewhere = self
                .ctx
                .binder
                .node_symbols
                .get(&_iface_idx.0)
                .and_then(|&sym_id| self.ctx.binder.symbols.get(sym_id))
                .is_some_and(|sym| {
                    sym.declarations.iter().any(|&decl_idx| {
                        decl_idx != _iface_idx
                            && self.ctx.arena.get(decl_idx).is_some_and(|n| {
                                self.ctx
                                    .arena
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
        // (name, type, node_idx, kind) — used for TS2430 derived-vs-base compatibility checks.
        let mut derived_members: Vec<(String, TypeId, NodeIndex, u16)> = Vec::new();

        // Collect all interface declaration indices for this symbol
        let all_iface_decls: Vec<NodeIndex> = self
            .ctx
            .binder
            .node_symbols
            .get(&_iface_idx.0)
            .and_then(|&sym_id| self.ctx.binder.symbols.get(sym_id))
            .map(|sym| {
                sym.declarations
                    .iter()
                    .copied()
                    .filter(|&decl_idx| {
                        self.ctx
                            .arena
                            .get(decl_idx)
                            .is_some_and(|n| self.ctx.arena.get_interface(n).is_some())
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
            if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                && let Some(decl_iface) = self.ctx.arena.get_interface(decl_node)
            {
                for &member_idx in &decl_iface.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind == CALL_SIGNATURE {
                        derived_member_names.insert(String::from("__call__"));
                    } else if (member_node.kind == METHOD_SIGNATURE
                        || member_node.kind == PROPERTY_SIGNATURE)
                        && let Some(sig) = self.ctx.arena.get_signature(member_node)
                        && let Some(name) = self.get_property_name(sig.name)
                    {
                        derived_member_names.insert(name.clone());
                        let type_id = self.get_type_of_interface_member(member_idx);
                        derived_members.push((name, type_id, member_idx, member_node.kind));
                    }
                }
            }
        }

        // Collect derived interface index signatures across all declarations.
        // These are checked against base index signatures for TS2430 compatibility.
        let mut derived_string_index_type: Option<TypeId> = None;
        let mut derived_number_index_type: Option<TypeId> = None;
        for &decl_idx in &all_iface_decls {
            if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                && let Some(decl_iface) = self.ctx.arena.get_interface(decl_node)
            {
                for &member_idx in &decl_iface.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != INDEX_SIGNATURE {
                        continue;
                    }
                    if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
                        let param_idx = index_sig
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
        if let Some(&sym_id) = self.ctx.binder.node_symbols.get(&_iface_idx.0)
            && let Some(sym) = self.ctx.binder.symbols.get(sym_id)
        {
            for &decl_idx in &sym.declarations {
                if let Some(node) = self.ctx.arena.get(decl_idx)
                    && self.ctx.arena.get_class(node).is_some()
                {
                    let class_data = self
                        .ctx
                        .arena
                        .get_class(node)
                        .expect("get_class guard above ensures Some");
                    if let Some(ref heritage_clauses) = class_data.heritage_clauses {
                        for &clause_idx in &heritage_clauses.nodes {
                            if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                                && let Some(heritage) =
                                    self.ctx.arena.get_heritage_clause(clause_node)
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
            if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                && let Some(decl_iface) = self.ctx.arena.get_interface(decl_node)
                && let Some(ref heritage_clauses) = decl_iface.heritage_clauses
            {
                for &clause_idx in &heritage_clauses.nodes {
                    if let Some(clause_node) = self.ctx.arena.get(clause_idx)
                        && let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node)
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
        for &(_clause_idx, type_idx) in &all_heritage_types {
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
                if let Some(node) = self.ctx.arena.get(decl_idx)
                    && self.ctx.arena.get_interface(node).is_some()
                {
                    base_iface_indices.push(decl_idx);
                }
            }
            if base_iface_indices.is_empty() && base_symbol.value_declaration.is_some() {
                let decl_idx = base_symbol.value_declaration;
                if let Some(node) = self.ctx.arena.get(decl_idx)
                    && self.ctx.arena.get_interface(node).is_some()
                {
                    base_iface_indices.push(decl_idx);
                }
            }

            // Collect ALL members from this base (direct + inherited from ancestors).
            // Use a worklist to walk the interface hierarchy without recursion.
            // Each entry: (interface_decl_idx, type_args_for_this_level)
            let mut worklist: Vec<(NodeIndex, Option<Vec<TypeId>>)> = Vec::new();
            for &idx in &base_iface_indices {
                let initial_args = type_arguments.map(|args| {
                    args.nodes
                        .iter()
                        .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                        .collect::<Vec<_>>()
                });
                worklist.push((idx, initial_args));
            }

            // Track which member keys we've already seen from THIS base entry.
            // Direct members shadow inherited ones, so we process closer bases first.
            let mut seen_member_keys: rustc_hash::FxHashSet<String> =
                rustc_hash::FxHashSet::default();
            // Prevent cycles in the interface hierarchy.
            let mut visited_ifaces: rustc_hash::FxHashSet<u32> = rustc_hash::FxHashSet::default();

            while let Some((iface_decl_idx, level_type_args)) = worklist.pop() {
                if !visited_ifaces.insert(iface_decl_idx.0) {
                    continue; // Already visited — cycle guard
                }

                let Some(iface_node) = self.ctx.arena.get(iface_decl_idx) else {
                    continue;
                };
                let Some(iface) = self.ctx.arena.get_interface(iface_node) else {
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

                // Process direct members of this interface level
                for &member_idx in &iface.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
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
                            let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                                continue;
                            };
                            let Some(name) = self.get_property_name(sig.name) else {
                                continue;
                            };
                            (
                                name,
                                instantiate_type(
                                    self.ctx.types,
                                    self.get_type_of_interface_member_simple(member_idx),
                                    &substitution,
                                ),
                                sig.question_token,
                            )
                        } else {
                            continue;
                        };

                    // Skip members already seen at a closer level in this base chain
                    if !seen_member_keys.insert(member_key.clone()) {
                        continue;
                    }

                    if derived_member_names.contains(&member_key) {
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
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != INDEX_SIGNATURE {
                        continue;
                    }
                    if let Some(idx_sig) = self.ctx.arena.get_index_signature(member_node) {
                        let param_idx = idx_sig
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
                        if let Some(hc_node) = self.ctx.arena.get(hc_idx)
                            && let Some(hc) = self.ctx.arena.get_heritage_clause(hc_node)
                            && hc.token == SyntaxKind::ExtendsKeyword as u16
                        {
                            for &ancestor_type_idx in &hc.types.nodes {
                                let (ancestor_expr, ancestor_type_args_opt) =
                                    if let Some(ancestor_node) =
                                        self.ctx.arena.get(ancestor_type_idx)
                                        && let Some(eat) =
                                            self.ctx.arena.get_expr_type_args(ancestor_node)
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

                                if let Some(ancestor_sym_id) =
                                    self.resolve_heritage_symbol(ancestor_expr)
                                    && let Some(ancestor_sym) =
                                        self.ctx.binder.get_symbol(ancestor_sym_id)
                                {
                                    for &decl_idx in &ancestor_sym.declarations {
                                        if let Some(dn) = self.ctx.arena.get(decl_idx)
                                            && self.ctx.arena.get_interface(dn).is_some()
                                        {
                                            worklist
                                                .push((decl_idx, ancestor_type_args_opt.clone()));
                                        }
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
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                    {
                        base_class_idx = Some(decl_idx);
                        break;
                    }
                }

                if base_class_idx.is_none() && base_symbol.value_declaration.is_some() {
                    let decl_idx = base_symbol.value_declaration;
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && node.kind == syntax_kind_ext::CLASS_DECLARATION
                    {
                        base_class_idx = Some(decl_idx);
                    }
                }

                if let Some(class_idx) = base_class_idx
                    && let Some(class_node) = self.ctx.arena.get(class_idx)
                    && let Some(class_data) = self.ctx.arena.get_class(class_node)
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
                    for (member_name, _, _derived_member_idx, _) in &derived_members {
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
                        // Check each derived member against the base type's properties
                        for (member_name, member_type, derived_member_idx, _derived_kind) in
                            &derived_members
                        {
                            // Look up the property in the base type
                            let base_prop = tsz_solver::type_queries::find_property_in_type_by_str(
                                self.ctx.types,
                                base_type,
                                member_name,
                            );

                            // For intersection types, search each member
                            let base_prop = base_prop.or_else(|| {
                                if let Some(members) =
                                    tsz_solver::type_queries::get_intersection_members(
                                        self.ctx.types,
                                        base_type,
                                    )
                                {
                                    for &member in &members {
                                        let prop =
                                            tsz_solver::type_queries::find_property_in_type_by_str(
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
                                    tsz_solver::type_queries::find_property_in_type_by_str(
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

            let mut ts2430_emitted_for_base = false;
            'derived_loop: for (member_name, member_type, derived_member_idx, derived_kind) in
                &derived_members
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
                        } else {
                            should_report_member_type_mismatch(
                                self,
                                *member_type,
                                base_type,
                                *derived_member_idx,
                            )
                        };

                        if param_count_incompatible || type_mismatch {
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
                for (name, type_id, idx, kind) in &derived_members {
                    if *kind == METHOD_SIGNATURE {
                        derived_method_overloads
                            .entry(name.clone())
                            .or_default()
                            .push((*type_id, *idx));
                    }
                }

                // For each method with multiple base overloads, check coverage
                'overload_check: for (method_name, base_sigs) in &base_method_overloads {
                    let Some(derived_sigs) = derived_method_overloads.get(method_name) else {
                        continue;
                    };
                    for &base_type in base_sigs {
                        let mut matched = false;
                        for &(derived_type, derived_idx) in derived_sigs {
                            if !should_report_member_type_mismatch(
                                self,
                                derived_type,
                                base_type,
                                derived_idx,
                            ) {
                                matched = true;
                                break;
                            }
                        }
                        if !matched {
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

    /// Find a member by name in a class, searching up the inheritance chain.
    /// Returns the member info if found, or None.
    /// Uses cycle detection to handle circular inheritance safely.
    pub(crate) fn find_member_in_class_chain(
        &mut self,
        class_idx: NodeIndex,
        target_name: &str,
        target_is_static: bool,
        _depth: usize,
        skip_private: bool,
    ) -> Option<ClassMemberInfo> {
        self.summarize_class_chain(class_idx)
            .lookup(target_name, target_is_static, skip_private)
            .cloned()
    }

    /// Internal implementation of `find_member_in_class_chain` with recursion guard.
    #[allow(dead_code)]
    fn find_member_in_class_chain_impl(
        &mut self,
        class_idx: NodeIndex,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
        guard: &mut tsz_solver::recursion::RecursionGuard<NodeIndex>,
    ) -> Option<ClassMemberInfo> {
        use tsz_solver::recursion::RecursionResult;

        // Check for cycles using the recursion guard
        match guard.enter(class_idx) {
            RecursionResult::Cycle
            | RecursionResult::DepthExceeded
            | RecursionResult::IterationExceeded => {
                // Circular inheritance/depth/iteration limits detected - return None gracefully
                // Exceeded limits - bail out
                return None;
            }
            RecursionResult::Entered => {
                // Proceed with the search
            }
        }
        let result = (|| {
            let class_node = self.ctx.arena.get(class_idx)?;
            let class_data = self.ctx.arena.get_class(class_node)?;

            // Search direct members
            for &member_idx in &class_data.members.nodes {
                if let Some(info) = self.extract_class_member_info(member_idx, skip_private)
                    && info.name == target_name
                    && info.is_static == target_is_static
                {
                    return Some(info);
                }

                if !target_is_static
                    && let Some(member_node) = self.ctx.arena.get(member_idx)
                    && member_node.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.ctx.arena.get_constructor(member_node)
                {
                    for &param_idx in &ctor.parameters.nodes {
                        if let Some(param_node) = self.ctx.arena.get(param_idx)
                            && let Some(param) = self.ctx.arena.get_parameter(param_node)
                            && self.has_parameter_property_modifier(&param.modifiers)
                            && let Some(name) = self.get_property_name(param.name)
                            && name == target_name
                        {
                            if skip_private && self.has_private_modifier(&param.modifiers) {
                                continue;
                            }
                            let visibility = if self.has_private_modifier(&param.modifiers) {
                                MemberVisibility::Private
                            } else if self.has_protected_modifier(&param.modifiers) {
                                MemberVisibility::Protected
                            } else {
                                MemberVisibility::Public
                            };
                            let prop_type = if param.type_annotation.is_some() {
                                self.get_type_from_type_node(param.type_annotation)
                            } else {
                                tsz_solver::TypeId::ANY
                            };
                            let info = ClassMemberInfo {
                                name,
                                type_id: prop_type,
                                name_idx: param.name,
                                visibility,
                                is_method: false,
                                is_static: false,
                                is_accessor: false,
                                is_abstract: false,
                                has_override: self.has_override_modifier(&param.modifiers)
                                    || self.has_jsdoc_override_tag(param_idx),
                                is_jsdoc_override: !self.has_override_modifier(&param.modifiers)
                                    && self.has_jsdoc_override_tag(param_idx),
                                has_dynamic_name: false,
                                has_computed_non_literal_name: false,
                            };
                            return Some(info);
                        }
                    }
                }
            }

            // Walk up to base class
            let heritage_clauses = class_data.heritage_clauses.as_ref()?;

            for &clause_idx in &heritage_clauses.nodes {
                let clause_node = self.ctx.arena.get(clause_idx)?;
                let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                let type_idx = *heritage.types.nodes.first()?;
                let type_node = self.ctx.arena.get(type_idx)?;
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };
                let expr_node = self.ctx.arena.get(expr_idx)?;
                let ident = self.ctx.arena.get_identifier(expr_node)?;
                let base_name = &ident.escaped_text;
                let sym_id = self.ctx.binder.file_locals.get(base_name)?;
                let symbol = self.ctx.binder.get_symbol(sym_id)?;
                let base_idx = if symbol.value_declaration.is_some() {
                    symbol.value_declaration
                } else {
                    *symbol.declarations.first()?
                };

                return self.find_member_in_class_chain_impl(
                    base_idx,
                    target_name,
                    target_is_static,
                    skip_private,
                    guard,
                );
            }

            None
        })();

        guard.leave(class_idx);
        result
    }

    pub(crate) const fn class_member_visibility_conflicts(
        &self,
        derived_visibility: MemberVisibility,
        base_visibility: MemberVisibility,
    ) -> bool {
        matches!(
            (derived_visibility, base_visibility),
            (
                MemberVisibility::Private,
                MemberVisibility::Private | MemberVisibility::Protected | MemberVisibility::Public
            ) | (
                MemberVisibility::Protected,
                MemberVisibility::Public | MemberVisibility::Private
            ) | (MemberVisibility::Public, MemberVisibility::Private)
        )
    }

    /// Count required (non-optional, non-rest, no-initializer) parameters in a
    /// method/function signature node, excluding `this` parameters.
    fn count_required_params_from_signature_node(&self, node_idx: NodeIndex) -> usize {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return 0;
        };
        let Some(sig) = self.ctx.arena.get_signature(node) else {
            return 0;
        };
        let Some(ref params) = sig.parameters else {
            return 0;
        };
        let mut count = 0;
        for &param_idx in &params.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                continue;
            };
            // Skip `this` pseudo-parameter
            if let Some(name_node) = self.ctx.arena.get(param.name)
                && name_node.kind == SyntaxKind::ThisKeyword as u16
            {
                continue;
            }
            // Rest parameters are not counted as required
            if param.dot_dot_dot_token {
                continue;
            }
            // Optional or has-default parameters are not required
            if param.question_token || param.initializer.is_some() {
                continue;
            }
            count += 1;
        }
        count
    }
}
