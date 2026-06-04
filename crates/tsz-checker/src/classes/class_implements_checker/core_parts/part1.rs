impl<'a> CheckerState<'a> {
    fn class_index_signatures_satisfy_interface(
        &mut self,
        class_instance_type: TypeId,
        interface_type: TypeId,
    ) -> bool {
        let Some(class_shape) = crate::query_boundaries::common::object_shape_for_type(
            self.ctx.types,
            class_instance_type,
        ) else {
            return false;
        };
        let Some(interface_shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, interface_type)
        else {
            return false;
        };

        let mut checked_index = false;
        if let Some(target_index) = interface_shape.string_index.as_ref() {
            checked_index = true;
            let Some(source_index) = class_shape.string_index.as_ref() else {
                return false;
            };
            if !self
                .class_implements_index_value_relation_outcome(
                    source_index.value_type,
                    target_index.value_type,
                )
                .related
            {
                return false;
            }
        }
        if let Some(target_index) = interface_shape.number_index.as_ref() {
            checked_index = true;
            let Some(source_index) = class_shape.number_index.as_ref() else {
                return false;
            };
            if !self
                .class_implements_index_value_relation_outcome(
                    source_index.value_type,
                    target_index.value_type,
                )
                .related
            {
                return false;
            }
        }

        checked_index
    }

    fn class_member_name_is_computed(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.ctx.arena.get(member_idx) else {
            return false;
        };
        let name_idx = match member_node.kind {
            syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(member_node)
                .map(|prop| prop.name),
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(member_node)
                .map(|method| method.name),
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(member_node)
                .map(|accessor| accessor.name),
            _ => None,
        };
        name_idx
            .and_then(|idx| self.ctx.arena.get(idx))
            .is_some_and(|node| node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
    }

    fn class_data_has_computed_member_name(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        class_data
            .members
            .nodes
            .iter()
            .any(|&member_idx| self.class_member_name_is_computed(member_idx))
    }

    fn implemented_interface_members(
        &mut self,
        interface_name: &str,
        interface_type: TypeId,
        type_args: &[TypeId],
        interface_declarations: &[NodeIndex],
        substitution: &crate::query_boundaries::common::TypeSubstitution,
        use_global_array_members: bool,
    ) -> (Vec<PropertyInfo>, bool, String) {
        let array_display_name = |state: &Self| format!("{}[]", state.format_type(type_args[0]));

        if use_global_array_members {
            let display_name = array_display_name(self);

            if let Some(array_base) = TypeResolver::get_array_base_type(&self.ctx.types)
                && let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    array_base,
                )
            {
                let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                    self.ctx.types,
                    TypeResolver::get_array_base_type_params(&self.ctx.types),
                    type_args,
                );
                let properties = shape
                    .properties
                    .iter()
                    .cloned()
                    .map(|mut prop| {
                        prop.type_id = crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            prop.type_id,
                            &substitution,
                        );
                        prop
                    })
                    .collect();
                let has_index_signature =
                    shape.string_index.is_some() || shape.number_index.is_some();
                return (properties, has_index_signature, display_name);
            }
        }

        let display_name = if !type_args.is_empty() {
            self.format_type(interface_type)
        } else {
            interface_name.to_string()
        };

        let (mut properties, mut has_index_signature) = if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, interface_type)
        {
            (
                shape.properties.to_vec(),
                shape.string_index.is_some() || shape.number_index.is_some(),
            )
        } else {
            (Vec::new(), false)
        };

        // Track interface method-signature names already rebuilt from the AST in
        // this loop. The first declaration of a name replaces the object-shape
        // property (which only stores the return type for methods); subsequent
        // declarations of the same name are overload signatures and must be
        // *combined* into one callable rather than overwriting each other.
        let mut method_sig_rebuilt: rustc_hash::FxHashSet<tsz_common::interner::Atom> =
            rustc_hash::FxHashSet::default();

        for &decl_idx in interface_declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface_decl) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };

            for &member_idx in &interface_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                    has_index_signature = true;
                    continue;
                }
                if member_node.kind != syntax_kind_ext::METHOD_SIGNATURE
                    && member_node.kind != syntax_kind_ext::PROPERTY_SIGNATURE
                {
                    continue;
                }

                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let Some(name) = self.get_property_name(sig.name) else {
                    continue;
                };

                // For method signatures, always build the full function type
                // (including parameters and method-level type parameters) via
                // get_type_of_interface_member_simple rather than using the
                // object-shape property type which only stores the return type.
                // This ensures proper TS2416 detection when comparing a class
                // method against a generic interface method signature.
                let member_type = if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE {
                    let member_type = self.get_type_of_interface_member_simple(member_idx);
                    crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        member_type,
                        substitution,
                    )
                } else {
                    match self.resolve_property_access_with_env(interface_type, &name) {
                        PropertyAccessResult::Success {
                            type_id,
                            write_type,
                            ..
                        } => write_type.unwrap_or(type_id),
                        _ => {
                            let member_type = self.get_type_of_interface_member_simple(member_idx);
                            crate::query_boundaries::common::instantiate_type(
                                self.ctx.types,
                                member_type,
                                substitution,
                            )
                        }
                    }
                };

                let member_atom = self.ctx.types.intern_string(&name);
                let property_info = PropertyInfo {
                    name: member_atom,
                    type_id: member_type,
                    write_type: member_type,
                    optional: sig.question_token,
                    readonly: false,
                    is_method: member_node.kind == syntax_kind_ext::METHOD_SIGNATURE,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: properties.len() as u32,
                    is_string_named: false,
                    is_symbol_named: false,
                    single_quoted_name: false,
                };
                if let Some(existing) = properties.iter_mut().find(|p| p.name == member_atom) {
                    if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE
                        && existing.is_method
                        && method_sig_rebuilt.contains(&member_atom)
                    {
                        // Overloaded interface method: this is a second (or later)
                        // overload of an already-rebuilt method. Combine the
                        // accumulated signature(s) with this declaration's
                        // signature(s) into one callable so the implements-compat
                        // check relates the class member against the FULL overload
                        // set. tsc's `signaturesRelatedTo` erases type parameters
                        // for the multi-signature (N×M) case; comparing the class
                        // member against a single overload in isolation produces a
                        // false TS2416 when the overload's return type depends on
                        // the method type parameter.
                        let mut sigs = crate::query_boundaries::class::member_call_signatures(
                            self.ctx.types,
                            existing.type_id,
                        );
                        sigs.extend(crate::query_boundaries::class::member_call_signatures(
                            self.ctx.types,
                            member_type,
                        ));
                        if sigs.is_empty() {
                            // Defensive: if neither declaration yielded a call
                            // signature, an empty callable would print as `{}` and
                            // accept anything. Fall back to the single-declaration
                            // type rather than silently dropping the check.
                            *existing = property_info;
                        } else {
                            // Relate the overload set as a plain call-signature list
                            // (`is_method = false`). tsc compares a class member
                            // against an overloaded target with contravariant
                            // parameters and type-parameter erasure (the N×M
                            // `signaturesRelatedTo` path), not the bivariant
                            // single-method parameter rule. Keeping `is_method =
                            // true` would over-accept — e.g. a narrower impl
                            // parameter would pass bivariantly — and miss real
                            // TS2416s.
                            for sig in &mut sigs {
                                sig.is_method = false;
                            }
                            let combined =
                                self.ctx
                                    .types
                                    .factory()
                                    .callable(tsz_solver::CallableShape {
                                        call_signatures: sigs,
                                        ..tsz_solver::CallableShape::default()
                                    });
                            existing.type_id = combined;
                            existing.write_type = combined;
                            existing.is_method = false;
                        }
                    } else {
                        *existing = property_info;
                    }
                } else {
                    properties.push(property_info);
                }
                if member_node.kind == syntax_kind_ext::METHOD_SIGNATURE {
                    method_sig_rebuilt.insert(member_atom);
                }
            }
        }

        (properties, has_index_signature, display_name)
    }

    fn implemented_interface_display_name_from_syntax(
        &self,
        type_idx: NodeIndex,
        fallback: &str,
        use_global_array_display: bool,
    ) -> String {
        let Some(type_node) = self.ctx.arena.get(type_idx) else {
            return fallback.to_string();
        };

        if use_global_array_display
            && type_node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(type_name) = self.node_text(type_ref.type_name)
            && type_name == "Array"
            && let Some(type_args) = type_ref.type_arguments.as_ref()
            && type_args.nodes.len() == 1
            && let Some(arg_text) = self.node_text(type_args.nodes[0])
        {
            return format!("{}[]", arg_text.trim().trim_end_matches('>'));
        }

        if use_global_array_display
            && type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(type_ref) = self.ctx.arena.get_expr_type_args(type_node)
            && let Some(type_name) = self.node_text(type_ref.expression)
            && type_name == "Array"
            && let Some(type_args) = type_ref.type_arguments.as_ref()
            && type_args.nodes.len() == 1
            && let Some(arg_text) = self.node_text(type_args.nodes[0])
        {
            return format!("{}[]", arg_text.trim().trim_end_matches('>'));
        }

        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(type_name) = self.node_text(type_ref.type_name)
        {
            let type_name = type_name
                .split('<')
                .next()
                .unwrap_or(type_name.as_str())
                .trim();
            let type_name = type_name.rsplit('.').next().unwrap_or(type_name).trim();
            if let Some(type_args) = type_ref.type_arguments.as_ref()
                && !type_args.nodes.is_empty()
            {
                let args = type_args
                    .nodes
                    .iter()
                    .filter_map(|&arg_idx| self.node_text(arg_idx))
                    .map(|text| {
                        text.trim()
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .trim()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return format!("{type_name}<{args}>");
            }
            return type_name.to_string();
        }

        if type_node.kind == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS
            && let Some(type_ref) = self.ctx.arena.get_expr_type_args(type_node)
            && let Some(type_name) = self.node_text(type_ref.expression)
        {
            let type_name = type_name
                .split('<')
                .next()
                .unwrap_or(type_name.as_str())
                .trim();
            let type_name = type_name.rsplit('.').next().unwrap_or(type_name).trim();
            if let Some(type_args) = type_ref.type_arguments.as_ref()
                && !type_args.nodes.is_empty()
            {
                let args = type_args
                    .nodes
                    .iter()
                    .filter_map(|&arg_idx| self.node_text(arg_idx))
                    .map(|text| {
                        text.trim()
                            .trim_start_matches('<')
                            .trim_end_matches('>')
                            .trim()
                            .to_string()
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                return format!("{type_name}<{args}>");
            }
            return type_name.to_string();
        }

        if let Some(text) = self.node_text(type_idx) {
            return text.trim().to_string();
        }

        fallback.to_string()
    }

    pub(crate) fn report_type_not_assignable_detail(
        &mut self,
        node_idx: NodeIndex,
        source_type: &str,
        target_type: &str,
        code: u32,
    ) {
        let detail = format!("Type '{source_type}' is not assignable to type '{target_type}'.");
        if self.attach_elaboration_frames_to_lead(
            node_idx,
            code,
            std::iter::once((detail.clone(), 0u8)),
        ) {
            return;
        }
        // Fallback: emit as a standalone diagnostic when no matching lead is
        // present (e.g., the lead error was suppressed).
        if let Some((pos, end)) = self.get_node_span(node_idx) {
            self.error(pos, end.saturating_sub(pos), detail, code);
        }
    }

    /// Attach the full structural elaboration under a property-override
    /// incompatibility lead (TS2416 / TS2417), routed through the shared
    /// `relation -> reason -> diagnostic` assignability gateway.
    ///
    /// The single-frame [`Self::report_type_not_assignable_detail`] only ever
    /// renders the top `Type 'S' is not assignable to type 'T'.` line, which
    /// truncates `tsc`'s multi-line override elaboration (parameter
    /// incompatibility, missing/optional property, nested property path,
    /// type-argument variance, return-type mismatch, ...). Routing override and
    /// `implements` mismatches through the same reason machinery the
    /// TS2322/TS2345 assignment paths use restores parity: the rendered reason's
    /// own lead becomes the first elaboration frame and its nested frames are
    /// re-parented one level deeper under the override lead.
    pub(crate) fn report_type_override_incompatibility_detail(
        &mut self,
        node_idx: NodeIndex,
        source_type: TypeId,
        target_type: TypeId,
        code: u32,
    ) {
        if source_type != target_type
            && let Some(reason) = self
                .analyze_assignability_failure(source_type, target_type)
                .failure_reason
        {
            // `render_failure_reason` is consumed for its returned elaboration
            // only. Its top-level (`depth == 0`) display branch is written for
            // assignment *expression* anchors and can incidentally resolve a
            // non-expression anchor (here the overridden member's name node) as
            // a value identifier, emitting spurious name-resolution diagnostics.
            // Snapshot the diagnostic buffer and restore it so only the
            // re-parented elaboration frames survive.
            let diagnostics_before = self.ctx.diagnostics.len();
            let inner = self.render_failure_reason(&reason, source_type, target_type, node_idx, 0);
            self.ctx.diagnostics.truncate(diagnostics_before);
            // The rendered reason's own lead becomes the first elaboration frame
            // (depth 0); its nested frames sit one level deeper (depth + 1).
            let frames = std::iter::once((inner.message_text, 0u8)).chain(
                inner
                    .related_information
                    .into_iter()
                    .map(|child| (child.message_text, child.depth.saturating_add(1))),
            );
            if self.attach_elaboration_frames_to_lead(node_idx, code, frames) {
                return;
            }
        }

        // Fallback: no structured reason available (e.g. suppressed/opaque
        // types). Preserve the single-frame elaboration so the lead still
        // carries the canonical `Type 'S' is not assignable to type 'T'.` line.
        let source_str = self.format_type(source_type);
        let target_str = self.format_type(target_type);
        self.report_type_not_assignable_detail(node_idx, &source_str, &target_str, code);
    }

    /// Attach `frames` (each an elaboration `(message, depth)`) as related
    /// information to the most recent diagnostic with `code` whose start matches
    /// the raw span of `node_idx`, de-duplicating by `(message, depth)`. Returns
    /// `false` when no matching lead diagnostic is present so callers can fall
    /// back to a standalone diagnostic.
    ///
    /// Matching by `(code, start)` rather than `(code, start, length)` is
    /// deliberate: `error_at_node` normalizes the lead's span (e.g. trimming to
    /// the leading identifier of a declaration) while `get_node_span` returns
    /// the raw node span, so the lengths can legitimately differ. Producing the
    /// elaboration as indented related-information (instead of separate
    /// top-level diagnostics) is what the conformance fingerprinter expects.
    fn attach_elaboration_frames_to_lead(
        &mut self,
        node_idx: NodeIndex,
        code: u32,
        frames: impl IntoIterator<Item = (String, u8)>,
    ) -> bool {
        let Some((pos, end)) = self.get_node_span(node_idx) else {
            return false;
        };
        let length = end.saturating_sub(pos);
        let Some(parent) = self
            .ctx
            .diagnostics
            .iter_mut()
            .rev()
            .find(|diag| diag.code == code && diag.start == pos)
        else {
            return false;
        };

        let file = parent.file.clone();
        for (message_text, depth) in frames {
            if parent
                .related_information
                .iter()
                .any(|info| info.message_text == message_text && info.depth == depth)
            {
                continue;
            }
            parent
                .related_information
                .push(crate::diagnostics::DiagnosticRelatedInformation {
                    file: file.clone(),
                    start: pos,
                    length,
                    message_text,
                    category: crate::diagnostics::DiagnosticCategory::Message,
                    code,
                    depth,
                });
        }
        true
    }

    /// Check that non-abstract class implements all abstract members from base class (error 2654).
    /// Reports "Non-abstract class 'X' is missing implementations for the following members of 'Y': {members}."
    pub(crate) fn check_abstract_member_implementations(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        // Only check non-abstract classes
        if self.has_abstract_modifier(&class_data.modifiers) {
            return;
        }

        // Find base class from heritage clauses
        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        let mut base_class_idx: Option<NodeIndex> = None;
        let mut base_class_name = String::new();
        let mut heritage_expr_idx: Option<NodeIndex> = None;
        let mut heritage_type_idx: Option<NodeIndex> = None;

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the base class
            if let Some(&type_idx) = heritage.types.nodes.first()
                && let Some(type_node) = self.ctx.arena.get(type_idx)
            {
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                heritage_expr_idx = Some(expr_idx);
                heritage_type_idx = Some(type_idx);

                if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                {
                    base_class_name = ident.escaped_text.clone();

                    if let Some(sym_id) = self.resolve_heritage_symbol(expr_idx) {
                        base_class_idx = self.get_class_declaration_from_symbol(sym_id);
                    }
                }
            }
            break;
        }

        // If the base class was resolved to a non-class declaration (e.g., a const variable
        // holding a mixin result), clear it so we fall through to the type-level fallback.
        if let Some(base_idx) = base_class_idx
            && let Some(base_node) = self.ctx.arena.get(base_idx)
            && self.ctx.arena.get_class(base_node).is_none()
        {
            base_class_idx = None;
        }

        let Some(base_idx) = base_class_idx else {
            // Type-level fallback: resolve via the solver for expression-based heritage
            self.check_abstract_members_from_type(
                class_idx,
                class_data,
                heritage_expr_idx,
                heritage_type_idx,
                &base_class_name,
            );
            return;
        };

        let Some(base_node) = self.ctx.arena.get(base_idx) else {
            return;
        };

        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            return;
        };

        let mut implemented_members =
            self.collect_concrete_member_names_for_abstract_impl(class_data);

        // TSC also considers members provided through declaration merging
        // (class + interface with same name).  Look up the class symbol and
        // check if any merged interface declarations contribute members that
        // satisfy the abstract requirement.
        if let Some(name_node) = self.ctx.arena.get(class_data.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let class_name = &ident.escaped_text;
            if let Some(sym_id) = self.ctx.binder.file_locals.get(class_name)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                for &decl_idx in &symbol.declarations {
                    // Skip the class declaration itself
                    if decl_idx == class_idx {
                        continue;
                    }
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    // Only consider interface declarations (declaration merging)
                    if decl_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                        continue;
                    }
                    let Some(iface) = self.ctx.arena.get_interface(decl_node) else {
                        continue;
                    };
                    // Collect own members from the merged interface
                    for &member_idx in &iface.members.nodes {
                        if let Some(name) = self.get_member_name(member_idx) {
                            implemented_members.insert(name);
                        }
                    }
                    // Also collect inherited members from extends clauses
                    // via the solver's resolved type
                    if let Some(ref heritage) = iface.heritage_clauses {
                        for &clause_idx in &heritage.nodes {
                            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                                continue;
                            };
                            let Some(heritage_clause) =
                                self.ctx.arena.get_heritage_clause(clause_node)
                            else {
                                continue;
                            };
                            for &type_idx in &heritage_clause.types.nodes {
                                let base_type = self.get_type_from_type_node(type_idx);
                                let base_type = self.evaluate_type_for_assignability(base_type);
                                if let Some(shape) =
                                    crate::query_boundaries::common::object_shape_for_type(
                                        self.ctx.types,
                                        base_type,
                                    )
                                {
                                    for prop in &shape.properties {
                                        let member_name = self.ctx.types.resolve_atom(prop.name);
                                        implemented_members.insert(member_name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Collect abstract members from base class that are not implemented.
        // Multiple declarations can share one member name (a get/set accessor
        // pair, or overload signatures) yet form a single inherited abstract
        // member, so dedup by name to avoid inflating the count (which would
        // flip TS2515 -> TS2654) and duplicating the rendered name.
        let mut missing_members: Vec<String> = Vec::new();
        for &member_idx in &base_class.members.nodes {
            if self.member_is_abstract(member_idx)
                && let Some(name) = self.get_member_name(member_idx)
                && !implemented_members.contains(&name)
                && !missing_members.contains(&name)
            {
                missing_members.push(name);
            }
        }

        // Report error if there are missing implementations
        let is_ambient = self.has_declare_modifier(&class_data.modifiers);
        if !is_ambient && !missing_members.is_empty() {
            let derived_class_name = if class_data.name.is_some() {
                if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                    if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                        ident.escaped_text.clone()
                    } else {
                        String::from("<anonymous>")
                    }
                } else {
                    String::from("<anonymous>")
                }
            } else {
                String::from("<anonymous>")
            };

            let is_class_expression = self
                .ctx
                .arena
                .get(class_idx)
                .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION);

            // TypeScript uses different error codes based on the number of missing members and whether it's an expression:
            // - TS2515: Single missing member: "Non-abstract class 'C' does not implement inherited abstract member bar from class 'B'."
            // - TS2653: Single missing member (class expression): "Non-abstract class expression does not implement inherited abstract member 'bar' from class 'B'."
            // - TS2654: Multiple missing members: "Non-abstract class 'C' is missing implementations for the following members of 'B': 'foo', 'bar'."
            // - TS2656: Multiple missing members (class expression): "Non-abstract class expression is missing implementations for the following members of 'B': 'foo', 'bar'."
            if missing_members.len() == 1 {
                if is_class_expression {
                    self.error_at_node(
                        class_idx,
                        &format!(
                            "Non-abstract class expression does not implement inherited abstract member '{}' from class '{}'.",
                            missing_members[0], base_class_name
                        ),
                        2653,
                    );
                } else {
                    // tsc points at the class name, not the `class` keyword
                    let error_node = if class_data.name.is_some() {
                        class_data.name
                    } else {
                        class_idx
                    };
                    self.error_at_node(
                        error_node,
                        &format!(
                            "Non-abstract class '{}' does not implement inherited abstract member {} from class '{}'.",
                            derived_class_name, missing_members[0], base_class_name
                        ),
                        diagnostic_codes::NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS, // TS2515
                    );
                }
            } else {
                // tsc points at the class name for declarations, not the `class` keyword
                let error_node = if is_class_expression {
                    class_idx
                } else if class_data.name.is_some() {
                    class_data.name
                } else {
                    class_idx
                };

                // TSC uses different error codes and message format based on count:
                // - 2-4 members: TS2654/TS2656, lists all members
                // - 5+ members: TS2655/TS2650, shows first 4 then "and N more"
                if missing_members.len() > 4 {
                    let truncated_list = missing_members[..4]
                        .iter()
                        .map(|s| format!("'{s}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let remaining = missing_members.len() - 4;

                    if is_class_expression {
                        self.error_at_node(
                            error_node,
                            &format!(
                                "Non-abstract class expression is missing implementations for the following members of '{base_class_name}': {truncated_list} and {remaining} more."
                            ),
                            2650,
                        );
                    } else {
                        self.error_at_node(
                            error_node,
                            &format!(
                                "Non-abstract class '{derived_class_name}' is missing implementations for the following members of '{base_class_name}': {truncated_list} and {remaining} more."
                            ),
                            2655,
                        );
                    }
                } else {
                    let missing_list = missing_members
                        .iter()
                        .map(|s| format!("'{s}'"))
                        .collect::<Vec<_>>()
                        .join(", ");

                    if is_class_expression {
                        self.error_at_node(
                            error_node,
                            &format!(
                                "Non-abstract class expression is missing implementations for the following members of '{base_class_name}': {missing_list}."
                            ),
                            2656,
                        );
                    } else {
                        self.error_at_node(
                            error_node,
                            &format!(
                                "Non-abstract class '{derived_class_name}' is missing implementations for the following members of '{base_class_name}': {missing_list}."
                            ),
                            diagnostic_codes::NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF,
                        );
                    }
                }
            }
        }
    }

    /// Check if a class member has the abstract modifier.
    pub(crate) fn member_is_abstract(&self, member_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.ctx.arena.get_property_decl(node) {
                    self.has_abstract_modifier(&prop.modifiers)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.ctx.arena.get_method_decl(node) {
                    self.has_abstract_modifier(&method.modifiers)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.ctx.arena.get_accessor(node) {
                    self.has_abstract_modifier(&accessor.modifiers)
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}
