impl<'a> Printer<'a> {
    // =========================================================================
    // Classes — Decorator Helpers
    // =========================================================================

    fn emit_legacy_member_decorator_calls_filtered(
        &mut self,
        class_name: &str,
        members: &[NodeIndex],
        scope_filter: LegacyMemberDecoratorScopeFilter,
    ) {
        if class_name.is_empty() {
            return;
        }

        let emit_metadata = self.ctx.options.emit_decorator_metadata;

        // Track accessor names that have already been emitted so that
        // getter/setter pairs produce only one __decorate call (the first one).
        let mut emitted_accessor_names = std::collections::HashSet::<String>::new();

        // Metadata info extracted per member
        enum MemberMetadata {
            Property {
                type_annotation: NodeIndex,
            },
            Method {
                parameters: NodeList,
                return_type: NodeIndex,
                async_returns_promise: bool,
            },
            Accessor {
                name: NodeIndex,
                is_static: bool,
            },
        }

        // `tsc` emits per-member `__decorate` calls for all decorated
        // instance/prototype members (in declaration order) before any decorated
        // static members (in declaration order). Pre-partition the members into
        // instance-first then static order, preserving source order within each
        // partition. The split is keyed structurally on the `static` modifier,
        // not on member names.
        let mut ordered_members: Vec<NodeIndex> = Vec::with_capacity(members.len());
        for &member_idx in members {
            let is_static = self
                .arena
                .get(member_idx)
                .map(|n| self.member_node_is_static(n))
                .unwrap_or(false);
            if !is_static {
                ordered_members.push(member_idx);
            }
        }
        for &member_idx in members {
            let is_static = self
                .arena
                .get(member_idx)
                .map(|n| self.member_node_is_static(n))
                .unwrap_or(false);
            if is_static {
                ordered_members.push(member_idx);
            }
        }

        for &member_idx in &ordered_members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let (modifiers, name_idx, is_property, is_accessor, metadata) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    // Skip overload signatures (no body) — decorators on overloads
                    // are not emitted as __decorate targets
                    if !method.body.is_some() {
                        continue;
                    }
                    let has_async_modifier = self
                        .arena
                        .has_modifier(&method.modifiers, SyntaxKind::AsyncKeyword);
                    let has_generator_asterisk = method.asterisk_token
                        || crate::transforms::emit_utils::source_header_has_async_generator_asterisk(
                            self.source_text,
                            member_node.pos,
                            self.arena
                                .get(method.body)
                                .map_or(member_node.end, |body| body.pos),
                        );
                    let meta = MemberMetadata::Method {
                        parameters: method.parameters.clone(),
                        return_type: method.type_annotation,
                        async_returns_promise: has_async_modifier && !has_generator_asterisk,
                    };
                    (&method.modifiers, method.name, false, false, meta)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    let is_auto_accessor = self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                    let meta = MemberMetadata::Property {
                        type_annotation: prop.type_annotation,
                    };
                    (&prop.modifiers, prop.name, !is_auto_accessor, false, meta)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (
                        &accessor.modifiers,
                        accessor.name,
                        false,
                        true,
                        MemberMetadata::Accessor {
                            name: accessor.name,
                            is_static: self.arena.is_static(&accessor.modifiers),
                        },
                    )
                }
                _ => continue,
            };

            let is_static = self.arena.is_static(modifiers);

            // Collect member-level decorators. Accessors merge the member
            // decorators from the whole getter/setter pair into the single
            // `__decorate` call `tsc` emits; other members use their own.
            let decorators = if is_accessor {
                self.collect_accessor_member_decorators(members, name_idx, is_static)
            } else {
                self.collect_class_decorators(modifiers)
            };

            // Collect parameter decorators. Methods use their own parameters.
            // Accessors merge the parameter decorators from the whole
            // getter/setter pair (the setter's `@dec param`) into the single
            // `__decorate` call `tsc` emits for the accessor.
            let param_decorators: Vec<(usize, Vec<NodeIndex>)> = if is_accessor {
                self.collect_accessor_param_decorators(members, name_idx, is_static)
            } else if let MemberMetadata::Method { ref parameters, .. } = metadata {
                self.collect_param_decorators(parameters)
            } else {
                Vec::new()
            };

            // Skip members with no decorators at all (neither member nor parameter level)
            if decorators.is_empty() && param_decorators.is_empty() {
                continue;
            }

            let Some(member_name) = self.get_decorator_member_name(name_idx) else {
                continue;
            };
            let member_key = member_name.dedupe_key();

            // For getter/setter pairs, tsc emits only one __decorate call
            // for the first accessor of the pair. Skip the second; its parameter
            // decorators were already merged in above.
            if is_accessor && !emitted_accessor_names.insert(member_key) {
                continue;
            }

            let needs_private_name_scope =
                self.legacy_member_decorator_needs_private_name_scope(member_idx);
            if !scope_filter.matches(needs_private_name_scope) {
                continue;
            }

            self.write_helper("__decorate");
            self.write("([");
            self.write_line();
            self.increase_indent();

            // Determine if metadata or param decorators will follow
            let will_emit_metadata = emit_metadata;
            let has_more = will_emit_metadata || !param_decorators.is_empty();

            let emitted_decorators: Vec<NodeIndex> = decorators
                .iter()
                .copied()
                .filter(|&dec_idx| {
                    let Some(dec_node) = self.arena.get(dec_idx) else {
                        return false;
                    };
                    let Some(dec) = self.arena.get_decorator(dec_node) else {
                        return false;
                    };
                    !self.legacy_decorator_expression_is_elided(dec.expression)
                })
                .collect();

            for (i, &dec_idx) in emitted_decorators.iter().enumerate() {
                if let Some(dec_node) = self.arena.get(dec_idx)
                    && let Some(dec) = self.arena.get_decorator(dec_node)
                {
                    let line_comment = self.emit_legacy_decorator_expression(dec.expression);
                    if i + 1 != emitted_decorators.len() || has_more {
                        if line_comment {
                            self.write_line();
                        }
                        self.write(",");
                    }
                    self.write_line();
                }
            }

            // Emit __param(index, decorator) for each parameter decorator
            for (pi, (param_idx, param_decs)) in param_decorators.iter().enumerate() {
                for (di, &dec_idx) in param_decs.iter().enumerate() {
                    if let Some(dec_node) = self.arena.get(dec_idx)
                        && let Some(dec) = self.arena.get_decorator(dec_node)
                    {
                        self.write_helper("__param");
                        self.write("(");
                        self.write(&param_idx.to_string());
                        self.write(", ");
                        self.emit_legacy_decorator_expression_without_trailing_comments(
                            dec.expression,
                        );
                        self.write(")");
                        let line_comment =
                            self.emit_legacy_decorator_trailing_comments(dec.expression);
                        let is_last_dec = di + 1 >= param_decs.len();
                        let is_last_param = pi + 1 >= param_decorators.len();
                        if !(is_last_dec && is_last_param) || will_emit_metadata {
                            if line_comment {
                                self.write_line();
                            }
                            self.write(",");
                        }
                        self.write_line();
                    }
                }
            }

            // Emit metadata calls after decorators
            if will_emit_metadata {
                match metadata {
                    MemberMetadata::Property { type_annotation } => {
                        self.emit_metadata_for_property(type_annotation);
                        self.write_line();
                    }
                    MemberMetadata::Method {
                        ref parameters,
                        return_type,
                        async_returns_promise,
                    } => {
                        self.emit_metadata_for_method(
                            parameters,
                            return_type,
                            async_returns_promise,
                        );
                        self.write_line();
                    }
                    MemberMetadata::Accessor { name, is_static } => {
                        self.emit_metadata_for_accessor(members, name, is_static);
                        self.write_line();
                    }
                }
            }

            self.decrease_indent();
            self.write("], ");
            self.write(class_name);
            if !is_static {
                self.write(".prototype");
            }
            self.write(", ");
            self.emit_decorator_member_name(&member_name);
            if is_property {
                self.write(", void 0);");
            } else {
                self.write(", null);");
            }
            self.write_line();
        }
    }
}
