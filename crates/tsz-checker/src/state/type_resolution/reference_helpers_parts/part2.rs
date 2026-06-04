impl<'a> CheckerState<'a> {
    /// Returns `true` when the type alias body is **directly** a homomorphic mapped type
    /// whose template is an identity self-reference: `type A<T> = { [P in keyof T]: A<T> }`.
    ///
    /// The constraint of the mapped type's type parameter must be `keyof X` where `X`
    /// is a type parameter of the outer alias (homomorphic form). The template must
    /// contain an identity self-reference — the same alias applied to the exact same
    /// argument list as the outer alias's type parameters.
    ///
    /// This is the pattern that causes infinite instantiation when the type argument is
    /// a tuple: the homomorphic expansion produces a tuple of self-referential applications,
    /// and checking element assignability recurses infinitely.
    ///
    /// Returns `false` for:
    /// - Bodies that are NOT directly a mapped type (indexed-access wrapping is handled
    ///   separately by `alias_has_self_referencing_mapped_body`)
    /// - Bounded recursive aliases (`type M<T,R> = {[K in keyof T]: T[K] extends any[]? M<T[K],R>:R}`)
    /// - Templates with union-or-ground-type wrapping (`type R<K,V> = {[P in K]: V | R<K,V>}`)
    pub(crate) fn alias_body_is_direct_homomorphic_self_mapped(
        &self,
        sym_id: SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_ALIAS_DECLARATION {
            return false;
        }
        let Some(type_alias) = self.ctx.arena.get_type_alias(node) else {
            return false;
        };

        // Body must be DIRECTLY a MAPPED_TYPE (not wrapped in union/indexed-access/etc.)
        let Some(body_node) = self.ctx.arena.get(type_alias.type_node) else {
            return false;
        };
        if body_node.kind != syntax_kind_ext::MAPPED_TYPE {
            return false;
        }
        let Some(mapped) = self.ctx.arena.get_mapped_type(body_node) else {
            return false;
        };

        // Collect alias type parameter names for identity self-ref detection
        let sym_name = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|s| s.escaped_name.clone())
            .unwrap_or_default();
        let param_names: Vec<String> = type_alias
            .type_parameters
            .as_ref()
            .map(|tpl| {
                tpl.nodes
                    .iter()
                    .filter_map(|&param_idx| {
                        let param_node = self.ctx.arena.get(param_idx)?;
                        let param = self.ctx.arena.get_type_parameter(param_node)?;
                        let name_node = self.ctx.arena.get(param.name)?;
                        let ident = self.ctx.arena.get_identifier(name_node)?;
                        Some(self.ctx.arena.resolve_identifier_text(ident).to_string())
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Mapped type must have a homomorphic constraint: the type_parameter's constraint
        // must be `keyof X` where X is a type parameter of the outer alias.
        // This distinguishes `{ [P in keyof T]: ... }` from `{ [P in K]: ... }`.
        if !self
            .mapped_type_param_has_keyof_alias_param_constraint(mapped.type_parameter, &param_names)
        {
            return false;
        }

        // The template must BE a direct identity self-reference (not merely contain one).
        // A union-ground template like `number | Self<T>` would cause coinductive termination
        // because the `number` branch is a base case. Only a bare `Self<T>` as the template
        // (no union/intersection wrapping) causes non-terminating element-by-element comparison.
        self.template_is_direct_identity_self_ref(mapped.type_node, &sym_name, &param_names)
    }

    /// Returns `true` when `node_idx` IS an identity self-reference — the same alias applied
    /// to its own type parameters — without recursing into children.
    ///
    /// Unlike [`Self::template_has_identity_self_ref`], this check does NOT recurse into union
    /// or intersection children. It is used by [`Self::alias_body_is_direct_homomorphic_self_mapped`]
    /// to distinguish a bare self-referential template (`Circular<T>`) from a union-ground
    /// template (`number | Circular<T>`), which terminates coinductively.
    fn template_is_direct_identity_self_ref(
        &self,
        node_idx: NodeIndex,
        name: &str,
        param_names: &[String],
    ) -> bool {
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        if self.ctx.arena.resolve_identifier_text(ident) != name {
            return false;
        }
        let Some(args) = &type_ref.type_arguments else {
            return false;
        };
        if args.nodes.len() != param_names.len() {
            return false;
        }
        args.nodes
            .iter()
            .zip(param_names.iter())
            .all(|(&arg_idx, param_name)| {
                self.ctx
                    .arena
                    .get(arg_idx)
                    .and_then(|n| {
                        if n.kind == syntax_kind_ext::TYPE_REFERENCE {
                            let tr = self.ctx.arena.get_type_ref(n)?;
                            let name_n = self.ctx.arena.get(tr.type_name)?;
                            let id = self.ctx.arena.get_identifier(name_n)?;
                            Some(self.ctx.arena.resolve_identifier_text(id) == *param_name)
                        } else if n.kind == SyntaxKind::Identifier as u16 {
                            let id = self.ctx.arena.get_identifier(n)?;
                            Some(self.ctx.arena.resolve_identifier_text(id) == *param_name)
                        } else {
                            Some(false)
                        }
                    })
                    .unwrap_or(false)
            })
    }

    /// Returns `true` when the mapped type's iteration variable has a `keyof X` constraint
    /// where `X` is one of the alias type parameter names.
    fn mapped_type_param_has_keyof_alias_param_constraint(
        &self,
        type_param_idx: NodeIndex,
        alias_param_names: &[String],
    ) -> bool {
        let Some(tp_node) = self.ctx.arena.get(type_param_idx) else {
            return false;
        };
        let Some(tp_data) = self.ctx.arena.get_type_parameter(tp_node) else {
            return false;
        };
        // Constraint must exist and be a TYPE_OPERATOR node
        let Some(constraint_node) = self.ctx.arena.get(tp_data.constraint) else {
            return false;
        };
        if constraint_node.kind != syntax_kind_ext::TYPE_OPERATOR {
            return false;
        }
        let Some(op_data) = self.ctx.arena.get_type_operator(constraint_node) else {
            return false;
        };
        // Operator must be `keyof`
        if op_data.operator != SyntaxKind::KeyOfKeyword as u16 {
            return false;
        }
        // Operand must be a simple type reference to one of the alias's type parameters
        let Some(operand_node) = self.ctx.arena.get(op_data.type_node) else {
            return false;
        };
        if operand_node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(operand_ref) = self.ctx.arena.get_type_ref(operand_node) else {
            return false;
        };
        // No type arguments — bare `T`, not `T<...>`
        if operand_ref.type_arguments.is_some() {
            return false;
        }
        let Some(name_node) = self.ctx.arena.get(operand_ref.type_name) else {
            return false;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let operand_name = self.ctx.arena.resolve_identifier_text(ident);
        alias_param_names.iter().any(|p| p == operand_name)
    }

    /// Returns `true` when `source` is a type application of an alias whose body is
    /// directly a homomorphic self-referential mapped type, any of the application's
    /// type arguments is a tuple type, and `target` is also a tuple type.
    ///
    /// This is the structural condition under which tsc emits TS2589 instead of TS2322:
    /// the homomorphic expansion of the alias over a tuple argument produces a tuple of
    /// self-referential applications, and checking those against the tuple target recurses
    /// infinitely through element-by-element comparison.
    pub(crate) fn source_is_homomorphic_self_mapped_tuple_arg_vs_tuple_target(
        &self,
        source: TypeId,
        target: TypeId,
    ) -> bool {
        // Target must be a tuple-like type (the structural check that causes infinite expansion)
        if !query::is_tuple_like_type(self.ctx.types.as_type_database(), target) {
            return false;
        }

        // Source must be an Application type
        let Some((base, args)) = query::get_application_info(self.ctx.types, source) else {
            return false;
        };

        // At least one type argument must resolve to a tuple type.
        // Named type aliases (e.g. `type tup = [number, number, number]`) are
        // stored as `Lazy(def_id)` inside Application args and won't match
        // `is_tuple_type` directly. Follow one level of Lazy indirection through
        // the symbol_types cache so aliased tuples are recognised.
        let db = self.ctx.types.as_type_database();
        let any_arg_is_tuple = args.iter().any(|&arg| {
            if query::is_tuple_type(db, arg) {
                return true;
            }
            let Some(def_id) = query::lazy_def_id(db, arg) else {
                return false;
            };
            let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
                return false;
            };
            self.ctx
                .symbol_types
                .get(&sym_id)
                .is_some_and(|&resolved| query::is_tuple_type(db, resolved))
        });
        if !any_arg_is_tuple {
            return false;
        }

        // The base alias must have a direct homomorphic self-referential mapped body
        let Some(def_id) = query::lazy_def_id(self.ctx.types, base) else {
            return false;
        };
        let Some(sym_id) = self.ctx.def_to_symbol_id(def_id) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
            return false;
        }
        symbol
            .declarations
            .iter()
            .any(|&decl_idx| self.alias_body_is_direct_homomorphic_self_mapped(sym_id, decl_idx))
    }
}
