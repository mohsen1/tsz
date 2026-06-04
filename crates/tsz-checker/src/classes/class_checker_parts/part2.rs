impl<'a> CheckerState<'a> {
    fn current_file_import_binds_name(&self, name: &str) -> bool {
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                return false;
            }
            let Some(import_decl) = self.ctx.arena.get_import_decl(stmt_node) else {
                return false;
            };
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                return false;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                return false;
            };

            if self.ctx.arena.get_identifier_text(clause.name) == Some(name) {
                return true;
            }

            let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
                return false;
            };
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                return self
                    .ctx
                    .arena
                    .get_named_imports(bindings_node)
                    .is_some_and(|ns| self.ctx.arena.get_identifier_text(ns.name) == Some(name));
            }
            if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
                return false;
            }

            self.ctx
                .arena
                .get_named_imports(bindings_node)
                .is_some_and(|named| {
                    named.elements.nodes.iter().any(|&spec_idx| {
                        let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                            return false;
                        };
                        let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                            return false;
                        };
                        let local_name_idx = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        self.ctx.arena.get_identifier_text(local_name_idx) == Some(name)
                    })
                })
        })
    }

    /// Walk the inheritance chain from `class_idx` upward and compose type parameter
    /// substitutions into `substitution`. This ensures that type parameters from
    /// ancestor classes (not just the immediate base) are correctly mapped.
    ///
    /// For example, given `X extends L<X>` where `L<RT> extends T<RT[RT['a']]>`:
    /// - The initial substitution maps `RT -> X_type`
    /// - This method walks L -> T, finding `T<A>` with extends arg `RT[RT['a']]`
    /// - It instantiates the extends arg with the current substitution: `X[X['a']]`
    /// - It adds `A -> X[X['a']]` to the substitution
    fn compose_ancestor_substitutions(
        &mut self,
        class_idx: NodeIndex,
        substitution: &mut TypeSubstitution,
    ) {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited = FxHashSet::default();

        while visited.insert(current) {
            let Some(class) = self.ctx.arena.get_class_at(current) else {
                break;
            };

            let heritage_clauses = match class.heritage_clauses.as_ref() {
                Some(hc) => hc.clone(),
                None => break,
            };

            let mut next_class = None;

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_ta) = self.ctx.arena.get_expr_type_args(type_node) {
                        (expr_ta.expression, expr_ta.type_arguments.as_ref().cloned())
                    } else {
                        (type_idx, None)
                    };

                // No type arguments means no intermediate substitution needed
                let Some(ta) = type_arguments else {
                    // Still need to walk up the chain in case there are further ancestors
                    if let Some(parent_idx) = self.get_base_class_idx(current) {
                        next_class = Some(parent_idx);
                    }
                    break;
                };

                // Resolve the parent class
                let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    break;
                };
                let Some(parent_class_idx) = self.get_class_declaration_from_symbol(base_sym_id)
                else {
                    break;
                };
                let Some(parent_class) = self.ctx.arena.get_class_at(parent_class_idx) else {
                    break;
                };

                // Push current class's type params so we can resolve extends type args
                let (_, current_tp_updates) = self.push_type_parameters(&class.type_parameters);

                // Resolve extends clause type arguments
                let mut extends_type_args = Vec::new();
                for &arg_idx in &ta.nodes {
                    extends_type_args.push(self.get_type_from_type_node(arg_idx));
                }

                self.pop_type_parameters(current_tp_updates);

                // Get parent's type parameters
                let (parent_type_params, parent_tp_updates) =
                    self.push_type_parameters(&parent_class.type_parameters);
                self.pop_type_parameters(parent_tp_updates);

                // For each parent type parameter, instantiate the extends type arg
                // with the current (accumulated) substitution and add the mapping
                for (i, param) in parent_type_params.iter().enumerate() {
                    if substitution.get(param.name).is_some() {
                        continue; // Already mapped
                    }
                    let arg_type = if i < extends_type_args.len() {
                        extends_type_args[i]
                    } else {
                        param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN)
                    };
                    let instantiated = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        arg_type,
                        substitution,
                    );
                    substitution.insert(param.name, instantiated);
                }

                next_class = Some(parent_class_idx);
                break; // Only process first extends clause
            }

            match next_class {
                Some(nc) => current = nc,
                None => break,
            }
        }
    }

    // Index signature compatibility (TS2415), interface extension compatibility (TS2430),
    // member lookup in class chains, and visibility conflict detection are in
    // `class_checker_compat.rs`.
}
