impl<'a> CheckerState<'a> {
    /// Check that a class properly implements all interfaces from its implements clauses.
    /// Emits TS2420 when a class incorrectly implements an interface.
    /// Checks for:
    /// - Missing members (properties and methods)
    /// - Incompatible member types (property type or method signature mismatch)
    pub(crate) fn check_implements_clauses(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        // Abstract classes don't need to implement interface members —
        // their abstract members satisfy the interface contract.
        if self.has_abstract_modifier(&class_data.modifiers) {
            return;
        }

        let mut class_type_param_names: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        if let Some(params) = class_data.type_parameters.as_ref() {
            for &param_idx in &params.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_type_parameter(param_node) else {
                    continue;
                };
                let Some(name_node) = self.ctx.arena.get(param_data.name) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                    continue;
                };
                class_type_param_names.insert(ident.escaped_text.clone());
            }
        }

        // Collect implemented members from the class (name -> node_idx).
        // Member types are computed lazily only when needed for an interface match.
        let mut class_members: rustc_hash::FxHashMap<String, NodeIndex> =
            rustc_hash::FxHashMap::default();
        // Track method names with multiple declarations (overloads).
        // For overloaded methods, individual declaration types are incomplete —
        // the combined overloaded type must be used instead.
        let mut overloaded_methods: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx) {
                if class_members.contains_key(&name) {
                    overloaded_methods.insert(name.clone());
                }
                class_members.insert(name, member_idx);
            }
            if let Some(node) = self.ctx.arena.get(member_idx)
                && node.kind == tsz_parser::parser::syntax_kind_ext::CONSTRUCTOR
                && let Some(ctor) = self.ctx.arena.get_constructor(node)
            {
                for &param_idx in &ctor.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && self.has_parameter_property_modifier(&param.modifiers)
                        && let Some(name) = self.get_property_name(param.name)
                    {
                        class_members.insert(name, param_idx);
                    }
                }
            }
        }
        let mut class_member_types: rustc_hash::FxHashMap<NodeIndex, TypeId> =
            rustc_hash::FxHashMap::default();

        // For overloaded methods, get the combined type from the class instance type.
        // The instance type builder already aggregates all overload signatures into a
        // single callable type, which is what tsc checks against the interface.
        let mut overloaded_member_types: rustc_hash::FxHashMap<String, TypeId> =
            rustc_hash::FxHashMap::default();
        if !overloaded_methods.is_empty() {
            let class_instance_type = self.get_class_instance_type(class_idx, class_data);
            overloaded_member_types = crate::query_boundaries::class::instance_member_types_by_name(
                self.ctx.types,
                class_instance_type,
            );
            overloaded_member_types.retain(|name, _| overloaded_methods.contains(name));
        }

        // Build a map of inherited PUBLIC instance members from the base class chain.
        // Only public members can satisfy interface requirements — private/protected inherited
        // members do NOT count, matching tsc's behavior.
        let mut inherited_member_types: rustc_hash::FxHashMap<String, TypeId> =
            rustc_hash::FxHashMap::default();
        self.collect_inherited_public_members(
            class_data,
            &class_members,
            &mut inherited_member_types,
        );

        // Also collect inherited PRIVATE/PROTECTED members. These don't
        // satisfy interface requirements, but when an interface extends the same base
        // class, these members appear in the interface type shape and must not be
        // reported as "missing" — they're inherited through the shared base class.
        let mut inherited_non_public_members: rustc_hash::FxHashMap<String, Visibility> =
            rustc_hash::FxHashMap::default();
        self.collect_inherited_non_public_members(class_data, &mut inherited_non_public_members);

        // Get the class name for error messages
        let class_name = self.get_class_name_with_type_params_from_decl(class_idx);
        let class_error_idx = if class_data.name.is_some() {
            class_data.name
        } else {
            class_idx
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check implements clauses
            if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                continue;
            };

            // Check each interface in the implements clause
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression and type arguments from either
                // ExpressionWithTypeArguments or TypeReference.
                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                            (type_ref.type_name, type_ref.type_arguments.as_ref())
                        } else {
                            (type_idx, None)
                        }
                    } else {
                        (type_idx, None)
                    };
                // TS2422: a class cannot implement one of its own type parameters.
                // This must be checked even when the type parameter resolves successfully.
                if !class_type_param_names.is_empty()
                    && let Some(expr_node) = self.ctx.arena.get(expr_idx)
                    && expr_node.kind == SyntaxKind::Identifier as u16
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                    && class_type_param_names.contains(&ident.escaped_text)
                {
                    self.error_at_node(
                        expr_idx,
                        diagnostic_messages::A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S,
                        diagnostic_codes::A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S,
                    );
                    continue;
                }

                // Resolve interface/class symbols through canonical heritage resolution so
                // qualified names (e.g. `Promise.Thenable`) are handled correctly.
                if let Some(raw_sym_id) = self.resolve_heritage_symbol(expr_idx) {
                    let mut visited_aliases =
                        crate::symbols_domain::alias_cycle::AliasCycleTracker::new();
                    let sym_id = self
                        .resolve_alias_symbol(raw_sym_id, &mut visited_aliases)
                        .unwrap_or(raw_sym_id);
                    let Some(symbol) = self
                        .get_cross_file_symbol(sym_id)
                        .or_else(|| self.ctx.binder.get_symbol(sym_id))
                    else {
                        continue;
                    };
                    let symbol_name = symbol.escaped_name.clone();
                    let symbol_flags = symbol.flags;
                    let symbol_declarations = symbol.declarations.clone();
                    let interface_name = self
                        .heritage_name_text(expr_idx)
                        .unwrap_or_else(|| symbol_name.clone());

                    let is_class = (symbol_flags & tsz_binder::symbol_flags::CLASS) != 0;

                    let mut interface_type_params = None;
                    let mut has_private_members = false;

                    // Track whether any merged interface declaration extends a class
                    // with private members that the implementing class CAN access vs
                    // ones it CANNOT access. When both exist, the conflict is already
                    // reported as TS2320 on the interface itself, so we suppress TS2420.
                    let mut any_inaccessible_privates = false;
                    let mut any_accessible_privates = false;

                    for &decl_idx in &symbol_declarations {
                        if let Some(node) = self.ctx.arena.get(decl_idx) {
                            if node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION {
                                if let Some(base_class_data) = self.ctx.arena.get_class(node) {
                                    if self.class_has_private_or_protected_members(base_class_data)
                                    {
                                        has_private_members = true;
                                    }
                                    if interface_type_params.is_none() {
                                        interface_type_params =
                                            base_class_data.type_parameters.clone();
                                    }
                                }
                            } else if node.kind
                                == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                                && let Some(interface_decl) = self.ctx.arena.get_interface(node)
                            {
                                if self.interface_extends_class_with_inaccessible_members(
                                    decl_idx,
                                    interface_decl,
                                    class_idx,
                                    class_data,
                                ) {
                                    any_inaccessible_privates = true;
                                } else if self
                                    .interface_extends_class_with_accessible_private_members(
                                        interface_decl,
                                        class_data,
                                    )
                                {
                                    any_accessible_privates = true;
                                }
                                if interface_type_params.is_none() {
                                    interface_type_params = interface_decl.type_parameters.clone();
                                }
                            }
                        }
                    }

                    // Only emit TS2420 for inaccessible private base members if
                    // there are no accessible ones from other merged declarations,
                    // and only after member checks confirm there is not a more
                    // specific missing/incompatible member diagnostic to report.
                    // When both private-base shapes exist, the interface itself
                    // has TS2320 (conflicting base types), which already covers
                    // the error.
                    let report_inaccessible_privates =
                        any_inaccessible_privates && !any_accessible_privates;

                    if has_private_members {
                        let message = format!(
                            "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?"
                        );
                        self.error_at_node(class_error_idx, &message, diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER);
                        continue;
                    }

                    // Check that all interface members are implemented with compatible types
                    let mut missing_members: Vec<String> = Vec::new();
                    let mut incompatible_members: Vec<(NodeIndex, String, TypeId, TypeId)> =
                        Vec::new(); // (node_idx, name, expected_type, actual_type)
                    // Build type arguments vector from implements clause (e.g., A<boolean> -> [boolean])
                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    // Push interface type parameters into scope so they're available when
                    // checking member types (fixes TS2304 false positive for interface type params)
                    let (mut interface_type_params, interface_type_param_updates) =
                        self.push_type_parameters(&interface_type_params);

                    // Fallback: when the interface declaration's AST lives in a different
                    // arena (e.g. lib types like `AsyncIterator<T, TReturn, TNext>`), the
                    // local arena walk above leaves `interface_type_params` empty. Look up
                    // the canonical type parameters via the solver-side definition store
                    // so the substitution we build below correctly maps interface type
                    // parameters to the supplied type arguments.
                    if interface_type_params.is_empty()
                        && let Some(def_id) = self.ctx.definition_store.find_def_by_symbol(sym_id.0)
                        && let Some(store_params) =
                            self.ctx.definition_store.get_type_params(def_id)
                        && !store_params.is_empty()
                    {
                        interface_type_params = store_params;
                    }

                    // Fill in missing type arguments with defaults/constraints/unknown
                    if type_args.len() < interface_type_params.len() {
                        for param in interface_type_params.iter().skip(type_args.len()) {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(tsz_solver::TypeId::UNKNOWN);
                            type_args.push(fallback);
                        }
                    }
                    if type_args.len() > interface_type_params.len() {
                        type_args.truncate(interface_type_params.len());
                    }

                    // Create substitution to instantiate interface type parameters with actual type arguments
                    let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                        self.ctx.types,
                        &interface_type_params,
                        &type_args,
                    );

                    let raw_interface_type = if is_class {
                        let mut instance_type = None;
                        for &decl_idx in &symbol_declarations {
                            if let Some(node) = self.ctx.arena.get(decl_idx)
                                && node.kind == syntax_kind_ext::CLASS_DECLARATION
                                && let Some(target_class_data) = self.ctx.arena.get_class(node)
                            {
                                instance_type =
                                    Some(self.get_class_instance_type(decl_idx, target_class_data));
                                break;
                            }
                        }
                        instance_type.unwrap_or_else(|| self.get_type_of_symbol(sym_id))
                    } else {
                        self.delegate_cross_arena_interface_type(sym_id)
                            .unwrap_or_else(|| self.get_type_of_symbol(sym_id))
                    };
                    let interface_type = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        raw_interface_type,
                        &substitution,
                    );
                    let interface_type = self.evaluate_type_for_assignability(interface_type);
                    // `symbol_is_from_actual_lib` matches arena-Arc identity which fails
                    // for cloned or merged lib symbols; keep both fallbacks so the lib
                    // `Array` is recognized and the display collapses `Array<T>` to `T[]`.
                    let use_global_array_implements_path = interface_name == "Array"
                        && type_args.len() == 1
                        && (self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
                            || self.ctx.binder.lib_symbol_ids.contains(&sym_id));
                    let (
                        interface_properties,
                        interface_has_index_signature,
                        interface_display_name,
                    ) = self.implemented_interface_members(
                        &interface_name,
                        interface_type,
                        &type_args,
                        &symbol_declarations,
                        &substitution,
                        use_global_array_implements_path,
                    );
                    let interface_display_name = self
                        .implemented_interface_display_name_from_syntax(
                            type_idx,
                            &interface_display_name,
                            use_global_array_implements_path,
                        );
                    // tsc shows the expanded intersection form (e.g., "Foo & Bar")
                    // instead of the type alias name (e.g., "Wrapper") when the
                    // implements target resolves to an intersection type.
                    // Check if the symbol is a type alias whose body is an
                    // intersection — use the AST source text since the type
                    // formatter resolves back to the alias name.
                    let interface_display_name = {
                        let mut intersection_text = None;
                        for &decl_idx in &symbol_declarations {
                            if let Some(node) = self.ctx.arena.get(decl_idx)
                                && node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                && let Some(ta) = self.ctx.arena.get_type_alias(node)
                                && let Some(type_node) = self.ctx.arena.get(ta.type_node)
                                && type_node.kind == syntax_kind_ext::INTERSECTION_TYPE
                            {
                                intersection_text = self.node_text(ta.type_node);
                                break;
                            }
                        }
                        intersection_text
                            .map(|t| t.trim().trim_end_matches(';').trim().to_string())
                            .unwrap_or(interface_display_name)
                    };
                    // Compute the derived class instance type for `this` substitution.
                    // Interface methods may use `this` type (e.g. `view(vnode: Vnode<A, this>)`).
                    // When checking if the class implements the interface, `this` must be
                    // replaced with the class instance type.
                    let class_this_type = self
                        .ctx
                        .binder
                        .get_node_symbol(class_idx)
                        .and_then(|sym_id| self.class_instance_type_from_symbol(sym_id))
                        .or_else(|| self.current_this_type());

                    for prop in &interface_properties {
                        let member_name = self.ctx.types.resolve_atom(prop.name);
                        let mut interface_member_type = prop.type_id;
                        // Substitute `this` type in interface members
                        if let Some(this_type) = class_this_type
                            && crate::query_boundaries::common::contains_this_type(
                                self.ctx.types,
                                interface_member_type,
                            )
                        {
                            interface_member_type =
                                crate::query_boundaries::common::substitute_this_type(
                                    self.ctx.types,
                                    interface_member_type,
                                    this_type,
                                );
                        }

                        // Skip optional properties
                        if prop.optional {
                            continue;
                        }

                        // Skip private brand properties — these are synthetic markers
                        // for private member compatibility and are handled by the
                        // type-level assignability check, not member-by-member.
                        if tsz_solver::utils::is_synthetic_private_brand_name(&member_name) {
                            continue;
                        }

                        // Check if class has this member
                        if let Some(&class_member_idx) = class_members.get(&member_name) {
                            // For overloaded methods, use the combined type from the
                            // class instance type (all overload signatures merged).
                            // For non-overloaded members, use the single declaration type.
                            let mut class_member_type = if let Some(&overloaded_type) =
                                overloaded_member_types.get(&member_name)
                            {
                                overloaded_type
                            } else if let Some(&cached) = class_member_types.get(&class_member_idx)
                            {
                                cached
                            } else {
                                let computed = self.get_type_of_class_member(class_member_idx);
                                class_member_types.insert(class_member_idx, computed);
                                computed
                            };
                            if matches!(
                                class_member_type,
                                tsz_solver::TypeId::ANY | tsz_solver::TypeId::ERROR
                            ) {
                                let class_instance_type =
                                    self.get_class_instance_type(class_idx, class_data);
                                if let Some(shape) =
                                    crate::query_boundaries::common::object_shape_for_type(
                                        self.ctx.types,
                                        class_instance_type,
                                    )
                                {
                                    let member_atom = self.ctx.types.intern_string(&member_name);
                                    if let Some(prop) =
                                        shape.properties.iter().find(|p| p.name == member_atom)
                                    {
                                        class_member_type = prop.type_id;
                                    }
                                }
                            }
                            // Substitute `this` type in class members too — the class method
                            // may return `this` (polymorphic), which must be replaced with the
                            // concrete class instance type for a fair comparison against the
                            // interface member (which has already been this-substituted above).
                            if let Some(this_type) = class_this_type
                                && crate::query_boundaries::common::contains_this_type(
                                    self.ctx.types,
                                    class_member_type,
                                )
                            {
                                class_member_type =
                                    crate::query_boundaries::common::substitute_this_type(
                                        self.ctx.types,
                                        class_member_type,
                                        this_type,
                                    );
                            }

                            // Check visibility (TS2420)
                            let sym_flags = self
                                .ctx
                                .binder
                                .get_node_symbol(class_member_idx)
                                .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
                                .map(|s| s.flags)
                                .unwrap_or(0);
                            let is_class_member_private =
                                (sym_flags & tsz_binder::symbol_flags::PRIVATE) != 0;
                            let is_class_member_protected =
                                (sym_flags & tsz_binder::symbol_flags::PROTECTED) != 0;
                            let interface_visibility = prop.visibility;
                            if is_class_member_private {
                                // When BOTH class member and interface member are private,
                                // they're nominally separate declarations (different brands).
                                // tsc behavior:
                                //   - Types compatible: emit TS2420 with
                                //     "Types have separate declarations of a private property 'x'."
                                //   - Types incompatible: emit TS2416 (per-property type mismatch),
                                //     suppress the visibility-form TS2420 entirely.
                                if interface_visibility == tsz_solver::Visibility::Private {
                                    let types_incompatible = interface_member_type
                                        != tsz_solver::TypeId::ANY
                                        && class_member_type != tsz_solver::TypeId::ANY
                                        && interface_member_type != tsz_solver::TypeId::ERROR
                                        && class_member_type != tsz_solver::TypeId::ERROR
                                        && should_report_own_member_type_mismatch(
                                            self,
                                            class_member_type,
                                            interface_member_type,
                                            class_member_idx,
                                        );
                                    if types_incompatible {
                                        incompatible_members.push((
                                            class_member_idx,
                                            member_name.clone(),
                                            interface_member_type,
                                            class_member_type,
                                        ));
                                    } else {
                                        self.error_at_node(
                                                class_error_idx,
                                                &format!("Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Types have separate declarations of a private property '{member_name}'."),
                                                diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                            );
                                    }
                                    continue;
                                }
                                self.error_at_node(
                                        class_error_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is private in type '{class_name}' but not in type '{interface_display_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                continue;
                            }
                            if is_class_member_protected {
                                self.error_at_node(
                                        class_error_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is protected in type '{class_name}' but not in type '{interface_display_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                continue;
                            }
                            // Interface-side private/protected: an interface may inherit a
                            // private/protected member from a base class (e.g., `interface I
                            // extends Foo`). A class implementing that interface with a
                            // non-private same-named property breaks nominal compatibility.
                            //
                            // For *protected*, if the class also extends the same base (so it
                            // has the inherited protected brand), tsc allows the widened
                            // public redeclaration. Skip the error in that case.
                            //
                            // For *private*, no such leniency — redeclaring a private member
                            // is always a nominal mismatch even when the class extends the
                            // declaring base.
                            if interface_visibility == tsz_solver::Visibility::Private {
                                self.error_at_node(
                                        class_error_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is private in type '{interface_display_name}' but not in type '{class_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                continue;
                            }
                            if interface_visibility == tsz_solver::Visibility::Protected
                                && !inherited_non_public_members.contains_key(&member_name)
                            {
                                self.error_at_node(
                                        class_error_idx,
                                        &format!("Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is protected in type '{interface_display_name}' but not in type '{class_name}'."),
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                    );
                                continue;
                            }

                            // Visibility widening (TS2420): interface member is
                            // PRIVATE (because the interface extends a class with
                            // a private member) but the class declares the same
                            // name as public. Private members are nominal in tsc,
                            // so a public member cannot satisfy a private slot
                            // even when the class extends the same base class.
                            // Protected widening to public is NOT an error here:
                            // tsc allows a subclass to override a protected member
                            // with public visibility, and the implementing-class
                            // check delegates to that rule.
                            if prop.visibility == Visibility::Private {
                                self.error_at_node(
                                    class_error_idx,
                                    &format!(
                                        "Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is private in type '{interface_display_name}' but not in type '{class_name}'."
                                    ),
                                    diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                );
                                continue;
                            }

                            // Check type compatibility using regular assignability.
                            // tsc uses the assignable relation (not bivariant) for
                            // implements clause member type checking.
                            if interface_member_type != tsz_solver::TypeId::ANY
                                && class_member_type != tsz_solver::TypeId::ANY
                                && interface_member_type != tsz_solver::TypeId::ERROR
                                && class_member_type != tsz_solver::TypeId::ERROR
                                && should_report_own_member_type_mismatch(
                                    self,
                                    class_member_type,
                                    interface_member_type,
                                    class_member_idx,
                                )
                            {
                                incompatible_members.push((
                                    class_member_idx,
                                    member_name.clone(),
                                    interface_member_type,
                                    class_member_type,
                                ));
                            }
                        } else if let Some(&inherited_type) =
                            inherited_member_types.get(&member_name)
                        {
                            // Member inherited from base class — check type compatibility
                            // tsc uses the assignable relation for implements clause checks.
                            if interface_member_type != tsz_solver::TypeId::ANY
                                && inherited_type != tsz_solver::TypeId::ANY
                                && interface_member_type != tsz_solver::TypeId::ERROR
                                && inherited_type != tsz_solver::TypeId::ERROR
                                && should_report_member_type_mismatch(
                                    self,
                                    inherited_type,
                                    interface_member_type,
                                    class_idx,
                                )
                            {
                                incompatible_members.push((
                                    class_error_idx,
                                    member_name.clone(),
                                    interface_member_type,
                                    inherited_type,
                                ));
                            }
                        } else if let Some(&visibility) =
                            inherited_non_public_members.get(&member_name)
                        {
                            if prop.visibility == Visibility::Public {
                                let visibility_text = match visibility {
                                    Visibility::Private => "private",
                                    Visibility::Protected => "protected",
                                    Visibility::Public => "public",
                                };
                                self.error_at_node(
                                    class_error_idx,
                                    &format!(
                                        "Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  Property '{member_name}' is {visibility_text} in type '{class_name}' but not in type '{interface_display_name}'."
                                    ),
                                    diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                                );
                            }
                        } else {
                            // Before reporting as missing, check the class instance type.
                            // Members from module augmentations or declaration merging appear
                            // in the computed instance type but not in the AST body or
                            // inheritance chain. E.g., `class X implements X {}` where X is
                            // augmented from another file via `declare module`.
                            let in_instance_type = {
                                let inst = self.get_class_instance_type(class_idx, class_data);
                                if let Some(shape) =
                                    crate::query_boundaries::common::object_shape_for_type(
                                        self.ctx.types,
                                        inst,
                                    )
                                {
                                    let member_atom = self.ctx.types.intern_string(&member_name);
                                    shape.properties.iter().any(|p| p.name == member_atom)
                                } else {
                                    false
                                }
                            };
                            if !in_instance_type {
                                missing_members.push(member_name);
                            }
                        }
                    }

                    // TS2559: Weak type detection for implements clauses.
                    // When the interface is a "weak type" (all properties optional,
                    // at least one property, no index signatures) and the class has
                    // no properties in common with the interface, tsc emits TS2559
                    // instead of silently passing. We detect this by checking
                    // assignability through the solver, which includes weak type
                    // detection via the compat layer.
                    if missing_members.is_empty() && incompatible_members.is_empty() {
                        // Check if the interface is a weak type: all properties optional
                        let is_weak = !interface_properties.is_empty()
                            && interface_properties.iter().all(|p| p.optional)
                            && !interface_has_index_signature;

                        if is_weak {
                            let class_instance_type =
                                self.get_class_instance_type(class_idx, class_data);
                            let analysis = self
                                .analyze_assignability_failure(class_instance_type, interface_type);
                            if matches!(
                                analysis.failure_reason,
                                Some(tsz_solver::SubtypeFailureReason::NoCommonProperties { .. })
                            ) {
                                let class_str = self.format_type(class_instance_type);
                                let iface_str = self.format_type(interface_type);
                                let message = crate::diagnostics::format_message(
                                    diagnostic_messages::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                                    &[&class_str, &iface_str],
                                );
                                self.error_at_node(
                                    class_error_idx,
                                    &message,
                                    diagnostic_codes::TYPE_HAS_NO_PROPERTIES_IN_COMMON_WITH_TYPE,
                                );
                            }
                        }
                    }

                    // Type-level assignability check (TS2420/TS2720).
                    //
                    // When the class extends the same base it implements with different
                    // type args (e.g., `class D extends C<string> implements C<number>`),
                    // tsc prefers TS2720 over member-level TS2416. When implementing a
                    // class that is NOT the extends base, member-by-member TS2416 applies.
                    //
                    // For interfaces, the type-level check is only done when
                    // member-by-member found no issues (catches index signature
                    // incompatibilities that member-by-member misses).
                    let extends_same_base =
                        is_class && self.class_extends_same_base(class_data, &interface_name);
                    let check_whole_type = extends_same_base
                        || (interface_has_index_signature
                            && missing_members.is_empty()
                            && incompatible_members.is_empty());
                    if check_whole_type {
                        let class_instance_type =
                            self.get_class_instance_type(class_idx, class_data);
                        // Substitute `this` type in the interface type before the
                        // whole-type assignability check, matching the per-property
                        // substitution done above. Without this, interfaces using
                        // `this` types (e.g. `Vnode<A, this>`) retain an abstract
                        // `this` that cannot be satisfied, causing false TS2430.
                        let target_type = if let Some(this_type) = class_this_type
                            && crate::query_boundaries::common::contains_this_type(
                                self.ctx.types,
                                interface_type,
                            ) {
                            crate::query_boundaries::common::substitute_this_type(
                                self.ctx.types,
                                interface_type,
                                this_type,
                            )
                        } else {
                            interface_type
                        };
                        if !self
                            .class_implements_whole_type_relation_outcome(
                                class_instance_type,
                                target_type,
                            )
                            .related
                        {
                            let analysis = self
                                .analyze_assignability_failure(class_instance_type, target_type);
                            let suppress_index_member_duplicate = !is_class
                                && interface_has_index_signature
                                && self.class_index_signatures_satisfy_interface(
                                    class_instance_type,
                                    target_type,
                                )
                                && matches!(
                                    analysis.failure_reason,
                                    Some(
                                        tsz_solver::SubtypeFailureReason::IndexSignatureMismatch {
                                            ..
                                        } | tsz_solver::SubtypeFailureReason::PropertyTypeMismatch {
                                            ..
                                        }
                                    )
                                );
                            if !is_class
                                && let Some(
                                    tsz_solver::SubtypeFailureReason::PropertyTypeMismatch {
                                        property_name,
                                        source_property_type,
                                        target_property_type,
                                        ..
                                    },
                                ) = analysis.failure_reason
                            {
                                let member_name =
                                    self.ctx.types.resolve_atom(property_name).to_string();
                                let class_member_idx = class_members
                                    .get(&member_name)
                                    .copied()
                                    .unwrap_or(class_error_idx);
                                incompatible_members.push((
                                    class_member_idx,
                                    member_name,
                                    target_property_type,
                                    source_property_type,
                                ));
                            } else if suppress_index_member_duplicate {
                                // Class member compatibility with its own declared index
                                // signature is reported separately as TS2411. If the class
                                // index signature itself satisfies the implemented interface,
                                // do not add a duplicate class-level TS2420 just because a
                                // named method/property is incompatible with that index value.
                            } else {
                                let suppress_computed_name_class_diagnostic = is_class
                                    && !extends_same_base
                                    && self.class_data_has_computed_member_name(class_data);
                                if !suppress_computed_name_class_diagnostic {
                                    let message = if is_class {
                                        format!(
                                            "Class '{class_name}' incorrectly implements class '{interface_display_name}'. Did you mean to extend '{interface_display_name}' and inherit its members as a subclass?"
                                        )
                                    } else {
                                        format!(
                                            "Class '{class_name}' incorrectly implements interface '{interface_display_name}'."
                                        )
                                    };
                                    let diagnostic_code = if is_class {
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER
                                    } else {
                                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE
                                    };
                                    self.error_at_node(class_error_idx, &message, diagnostic_code);
                                    if extends_same_base {
                                        // tsc suppresses member-level TS2416 when TS2720 is emitted
                                        // for extends+implements same base patterns
                                        incompatible_members.clear();
                                    }
                                }
                            }
                        }
                    }

                    if report_inaccessible_privates
                        && missing_members.is_empty()
                        && incompatible_members.is_empty()
                    {
                        self.error_at_node(
                            class_error_idx,
                            &format!("Class '{class_name}' incorrectly implements interface '{interface_name}'."),
                            diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE,
                        );
                    }

                    // Report error for missing members
                    let diagnostic_code = if is_class {
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_CLASS_DID_YOU_MEAN_TO_EXTEND_AND_INHERIT_ITS_MEMBER
                    } else {
                        diagnostic_codes::CLASS_INCORRECTLY_IMPLEMENTS_INTERFACE
                    };

                    // tsc suppresses TS2420 (missing members) when there are
                    // incompatible members (TS2416). Only report missing members
                    // when no type mismatches were found.
                    if !missing_members.is_empty() && incompatible_members.is_empty() {
                        let missing_message = if missing_members.len() == 1 {
                            format!(
                                "Property '{}' is missing in type '{}' but required in type '{}'.",
                                missing_members[0], class_name, interface_display_name
                            )
                        } else {
                            let missing_list = missing_members.clone();
                            let formatted_list = if missing_list.len() > 4 {
                                let first_four = missing_list
                                    .iter()
                                    .take(4)
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                format!("{}, and {} more", first_four, missing_list.len() - 4)
                            } else {
                                missing_list.join(", ")
                            };
                            format!(
                                "Type '{class_name}' is missing the following properties from type '{interface_display_name}': {formatted_list}"
                            )
                        };

                        let full_message = if is_class {
                            format!(
                                "Class '{class_name}' incorrectly implements class '{interface_name}'. Did you mean to extend '{interface_name}' and inherit its members as a subclass?\n  {missing_message}"
                            )
                        } else {
                            format!(
                                "Class '{class_name}' incorrectly implements interface '{interface_display_name}'.\n  {missing_message}"
                            )
                        };

                        self.error_at_node(class_error_idx, &full_message, diagnostic_code);
                    }

                    // TS2416 for incompatible member types in the implements
                    // clause.  Emit per-property errors for both interfaces and
                    // classes.
                    {
                        for (class_member_idx, member_name, expected_type, actual_type) in
                            incompatible_members
                        {
                            let error_node_idx =
                                if let Some(member_node) = self.ctx.arena.get(class_member_idx) {
                                    self.get_member_name_node(member_node)
                                        .unwrap_or(class_member_idx)
                                } else {
                                    class_member_idx
                                };
                            let display_name = format_property_name_for_diagnostic(&member_name);
                            self.error_at_node(
                                error_node_idx,
                                &format!(
                                    "Property '{display_name}' in type '{class_name}' is not assignable to the same property in base type '{interface_display_name}'."
                                ),
                                diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                            );
                            self.report_type_override_incompatibility_detail(
                                error_node_idx,
                                actual_type,
                                expected_type,
                                diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                            );
                        }
                    }

                    // Pop interface type parameters from scope
                    self.pop_type_parameters(interface_type_param_updates);
                }
            }
        }
    }

    /// Check that JSDoc `@extends`/`@augments` tag argument matches the actual `extends` clause.
    ///
    /// In JS files, if a class has both `@extends {Foo}` and `extends Bar`,
    /// TSC emits TS8023: "JSDoc '@extends Foo' does not match the 'extends Bar' clause."
    pub(crate) fn check_jsdoc_extends_name_mismatch(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        if !self.ctx.is_js_file() {
            return;
        }

        self.check_jsdoc_extends_tag_type_arguments(class_idx);
        self.check_jsdoc_extends_tag_type_argument_constraints(class_idx);
        self.check_missing_jsdoc_extends_type_arguments(class_idx, class_data);

        // Get the actual extends clause base class name
        let actual_extends_name = self.get_extends_clause_name(class_data);
        let Some(actual_name) = actual_extends_name else {
            return; // No extends clause, nothing to check
        };

        // Get the JSDoc comment range and search the raw source text
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return;
        };

        // Find the leading JSDoc comment range
        use tsz_common::comments::{get_leading_comments_from_cache, is_jsdoc_comment};
        let leading = get_leading_comments_from_cache(comments, node.pos, source_text);
        let Some(comment) = leading.last() else {
            return;
        };
        if !is_jsdoc_comment(comment, source_text) {
            return;
        }

        let comment_text = comment.get_text(source_text);

        // Search for @extends or @augments in the raw comment text
        for tag in ["augments", "extends"] {
            let needle = format!("@{tag}");
            for (match_pos, _) in comment_text.match_indices(&needle) {
                let after = match_pos + needle.len();
                if after >= comment_text.len() {
                    continue;
                }
                let next_ch = comment_text[after..]
                    .chars()
                    .next()
                    .expect("after < len checked above");
                if next_ch.is_ascii_alphanumeric() {
                    continue;
                }
                let rest = comment_text[after..].trim_start();
                if rest.is_empty() {
                    continue;
                }

                // Extract type name from {TypeName<...>} or TypeName
                let (jsdoc_type_name, type_name_in_rest) = if rest.starts_with('{') {
                    if let Some(close) = rest.find('}') {
                        let name = rest[1..close].trim();
                        (name, &rest[1..close])
                    } else {
                        continue;
                    }
                } else {
                    let end = rest
                        .find(|c: char| c.is_whitespace() || c == '*')
                        .unwrap_or(rest.len());
                    let name = rest[..end].trim();
                    (name, &rest[..end])
                };

                if jsdoc_type_name.is_empty() {
                    // Empty @extends/@augments tag (e.g. `/** @augments */`):
                    // emit TS1003 + TS8023 at the position right after the tag keyword.
                    let error_pos = comment.pos + after as u32;

                    self.ctx.error(
                        error_pos,
                        1,
                        diagnostic_messages::IDENTIFIER_EXPECTED.to_string(),
                        diagnostic_codes::IDENTIFIER_EXPECTED,
                    );

                    let message = format_message(
                        diagnostic_messages::JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE,
                        &[tag, "", &actual_name],
                    );
                    self.ctx.error(
                        error_pos,
                        1,
                        message,
                        diagnostic_codes::JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE,
                    );
                    return;
                }

                // Strip type arguments: "Foo<Bar>" → "Foo"
                let jsdoc_base_name = jsdoc_type_name
                    .find('<')
                    .map_or(jsdoc_type_name, |i| &jsdoc_type_name[..i]);

                // Check if the JSDoc @extends type name actually exists. If not,
                // emit TS2304 "Cannot find name" (tsc emits this alongside TS8023,
                // not instead of it).
                if !self.ctx.binder.file_locals.has(jsdoc_base_name) {
                    let type_name_offset =
                        type_name_in_rest.as_ptr() as usize - comment_text.as_ptr() as usize;
                    let error_pos = comment.pos + type_name_offset as u32;
                    let error_len = jsdoc_base_name.len() as u32;
                    let message =
                        format_message(diagnostic_messages::CANNOT_FIND_NAME, &[jsdoc_base_name]);
                    self.ctx.error(
                        error_pos,
                        error_len,
                        message,
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );
                }

                if jsdoc_base_name != actual_name {
                    let message = format_message(
                        diagnostic_messages::JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE,
                        &[tag, jsdoc_type_name, &actual_name],
                    );
                    // Anchor at the type name argument in the JSDoc (matches TSC behavior)
                    let type_name_offset =
                        type_name_in_rest.as_ptr() as usize - comment_text.as_ptr() as usize;
                    let error_pos = comment.pos + type_name_offset as u32;
                    let error_len = jsdoc_type_name.len() as u32;
                    self.ctx.error(
                        error_pos,
                        error_len,
                        message,
                        diagnostic_codes::JSDOC_DOES_NOT_MATCH_THE_EXTENDS_CLAUSE,
                    );
                }
                return; // Only check first @extends/@augments tag
            }
        }
    }
}
