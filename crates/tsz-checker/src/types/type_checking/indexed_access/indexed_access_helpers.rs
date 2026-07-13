use crate::query_boundaries::indexed_access_key_space as key_space_query;
use crate::query_boundaries::property_access::PropertyAccessResult;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::syntax_kind_ext::PARENTHESIZED_TYPE;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// Check if a property with the given name is private or protected on the given type.
/// Delegates to the solver's type query via `query_boundaries`.
pub(super) fn has_nonpublic_property(
    db: &dyn tsz_solver::construction::TypeDatabase,
    type_id: TypeId,
    name: &str,
) -> bool {
    crate::query_boundaries::common::has_nonpublic_property(db, type_id, name)
}

pub(super) fn is_broad_index_type(
    db: &dyn tsz_solver::construction::TypeDatabase,
    ty: TypeId,
) -> bool {
    if matches!(ty, TypeId::STRING | TypeId::NUMBER | TypeId::SYMBOL) {
        return true;
    }

    crate::query_boundaries::common::union_members(db, ty).is_some_and(|members| {
        !members.is_empty()
            && members
                .iter()
                .all(|&member| is_broad_index_type(db, member))
    })
}

pub(super) fn generic_constrained_index(
    db: &dyn tsz_solver::construction::TypeDatabase,
    object_type: TypeId,
    index_type: TypeId,
    index_constraint: Option<TypeId>,
) -> bool {
    index_constraint.is_some()
        && crate::query_boundaries::common::is_type_parameter_like(db, object_type)
        && crate::query_boundaries::common::is_type_parameter_like(db, index_type)
}

pub(super) fn same_type_param_name(
    db: &dyn tsz_solver::construction::TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> bool {
    crate::query_boundaries::common::type_param_info(db, left)
        .zip(crate::query_boundaries::common::type_param_info(db, right))
        .is_some_and(|(l, r)| l.name == r.name)
}

pub(super) fn same_object_key_space(
    db: &dyn tsz_solver::construction::TypeDatabase,
    left: TypeId,
    right: TypeId,
) -> bool {
    left == right || same_type_param_name(db, left, right)
}

pub(super) fn is_unconstrained_type_param_object(
    db: &dyn tsz_solver::construction::TypeDatabase,
    object_type: TypeId,
) -> bool {
    crate::query_boundaries::checkers::generic::is_unconstrained_type_parameter_like(
        db,
        object_type,
    )
}

pub(super) fn remapped_mapped_type_template_index_should_report_ts2536(
    db: &dyn tsz_solver::construction::TypeDatabase,
    object_type_for_check: TypeId,
    index_type: TypeId,
    index_type_for_check: TypeId,
) -> bool {
    let Some(mapped_id) =
        crate::query_boundaries::common::mapped_type_id(db, object_type_for_check)
    else {
        return false;
    };
    let mapped = db.mapped_type(mapped_id);
    if mapped.name_type.is_none() {
        return false;
    }
    if !crate::query_boundaries::common::is_template_literal_type(db, index_type)
        && !crate::query_boundaries::common::is_template_literal_type(db, index_type_for_check)
    {
        return false;
    }
    crate::query_boundaries::common::contains_type_parameters(db, index_type)
        || crate::query_boundaries::common::contains_type_parameters(db, index_type_for_check)
}

pub(super) fn indexed_access_object_alias_application_exceeds_depth(
    checker: &mut CheckerState<'_>,
    object_node_idx: NodeIndex,
) -> bool {
    let Some(object_node) = checker.ctx.arena.get(object_node_idx) else {
        return false;
    };
    let type_name = checker
        .ctx
        .arena
        .get_type_ref(object_node)
        .map_or(object_node_idx, |type_ref| type_ref.type_name);
    let Some(raw_sym_id) = checker.resolve_type_symbol_for_lowering(type_name) else {
        return false;
    };
    let sym_id = tsz_binder::SymbolId(raw_sym_id);
    let Some(symbol) = checker.ctx.binder.get_symbol(sym_id) else {
        return false;
    };
    if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS) {
        return false;
    }
    if checker.ctx.symbol_resolution_set.contains(&sym_id) {
        return false;
    }
    let declarations = symbol.declarations.clone();

    declarations.into_iter().any(|decl_idx| {
        let Some(decl_node) = checker.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(type_alias) = checker.ctx.arena.get_type_alias(decl_node) else {
            return false;
        };
        let body_type = checker.get_type_from_type_node(type_alias.type_node);
        let Some((base, _)) =
            crate::query_boundaries::common::application_info(checker.ctx.types, body_type)
        else {
            return false;
        };
        let Some(app_def_id) =
            crate::query_boundaries::common::lazy_def_id(checker.ctx.types, base)
        else {
            return false;
        };
        let Some(app_sym_id) = checker.ctx.def_to_symbol_id(app_def_id) else {
            return false;
        };
        if !checker.type_alias_symbol_direct_conditional_branches_are_array_like(app_sym_id) {
            return false;
        }
        checker.ctx.depth_exceeded.set(false);
        checker.evaluate_type_for_ts2589_check(body_type, app_def_id)
    })
}

impl<'a> CheckerState<'a> {
    /// Only suppresses TS2536 when the base of the indexed access is a type parameter
    /// (e.g., `Shape[k]` where Shape is a generic param), NOT when it's a concrete type
    /// (e.g., `DataFetchFns[T]` where `DataFetchFns` is a known type).
    pub(crate) fn is_deferred_indexed_access_object(&self, ty: TypeId) -> bool {
        if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, ty) {
            return false;
        }
        if let Some((base, _index)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, ty)
        {
            // A generic indexed access (`Base[I]`) defers its index-validity
            // check to instantiation time (tsc's `checkIndexedAccessIndexType`).
            // The base is generic when it is a type parameter or the polymorphic
            // `this` type: tsc models `this` as a type parameter whose constraint
            // is the enclosing class/interface, so `this["arg0"][number]` is as
            // deferred as `T["arg0"][number]` and must not eagerly emit TS2536.
            return crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, base)
                || crate::query_boundaries::common::is_this_type(self.ctx.types, base);
        }
        false
    }

    /// Whether a `keyof` result is a concrete key space rather than a still-deferred
    /// `keyof`. Used to decide whether a deferred conditional's apparent constraint
    /// has a usable key space for indexed-access validation. Key-space callers
    /// should compute the result through [`Self::indexed_access_keyof_with_env`]
    /// so `Lazy(DefId)` operands are resolved before `keyof` is classified.
    pub(super) fn indexed_access_key_space_is_resolved(&self, keyof_result: TypeId) -> bool {
        !(keyof_result == TypeId::ERROR
            || keyof_result == TypeId::ANY
            || crate::query_boundaries::common::is_keyof_type(self.ctx.types, keyof_result))
    }

    /// Compute `keyof operand` through the checker's resolver-backed evaluator.
    ///
    /// Raw `ctx.types.evaluate_keyof` uses a resolverless evaluator; for indexed
    /// access validation that can make `Lazy(DefId)` object members inside
    /// deferred/apparent constraints look empty. Routing through
    /// `evaluate_type_with_env` keeps the solver-owned `keyof` semantics while
    /// giving the evaluator the checker resolver environment.
    pub(super) fn indexed_access_keyof_with_env(&mut self, operand: TypeId) -> TypeId {
        let keyof = key_space_query::keyof_type(self.ctx.types, operand);
        self.evaluate_type_with_env(keyof)
    }

    /// TS4105: Emit "Private or protected member '{name}' cannot be accessed on
    /// a type parameter." for each type-parameter portion of `object_type` whose
    /// constraint has a non-public property with the given `name`.
    ///
    /// tsc treats both explicit type parameters and the polymorphic `this` type
    /// as type parameters here (`this` is a type parameter whose constraint is
    /// the enclosing class/interface). So `T["secret"]`, `this["secret"]`, and
    /// `(T | this)["secret"]` all report TS4105 when `secret` is non-public,
    /// while a concrete class index (`Base["secret"]`) does not.
    ///
    /// For union object types each member is checked individually. The
    /// "constraint" against which the property accessibility is tested is the
    /// type parameter's declared constraint, or — for the `this` type — the
    /// enclosing class/interface instance type.
    pub(crate) fn check_ts4105_private_on_type_parameter(
        &mut self,
        error_node: NodeIndex,
        object_type: TypeId,
        property_name: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // tsc reports TS4105 only when the indexed object is a type parameter or
        // the polymorphic `this` type (a concrete class index does not). The
        // member accessibility is decided against the type parameter's declared
        // constraint, or — for `this` — the enclosing class/interface instance
        // type. Union object types are checked per member.
        let members: smallvec::SmallVec<[TypeId; 4]> =
            match crate::query_boundaries::common::union_members(self.ctx.types, object_type) {
                Some(members) => members.iter().copied().collect(),
                None => smallvec::smallvec![object_type],
            };
        let emits_ts4105 = members.iter().any(|&member| {
            let constraint = if crate::query_boundaries::common::is_type_parameter_like(
                self.ctx.types,
                member,
            ) {
                crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, member)
            } else if crate::query_boundaries::type_predicates::is_this_type(self.ctx.types, member)
            {
                self.resolve_enclosing_this_instance_type(error_node)
            } else {
                None
            };
            constraint.is_some_and(|c| has_nonpublic_property(self.ctx.types, c, property_name))
        });
        if emits_ts4105 {
            let message = format_message(
                diagnostic_messages::PRIVATE_OR_PROTECTED_MEMBER_CANNOT_BE_ACCESSED_ON_A_TYPE_PARAMETER,
                &[property_name],
            );
            self.error_at_node(
                error_node,
                &message,
                diagnostic_codes::PRIVATE_OR_PROTECTED_MEMBER_CANNOT_BE_ACCESSED_ON_A_TYPE_PARAMETER,
            );
        }
    }

    /// Resolve the concrete instance type of the class/interface that lexically
    /// encloses `node` — the constraint of the polymorphic `this` type in that
    /// scope. Walks up the AST to the nearest class/interface declaration (or
    /// class expression) and reads its binder symbol's instance type.
    ///
    /// Used by [`check_ts4105_private_on_type_parameter`] because the indexed-
    /// access type-checking pass does not push the runtime `this` binding, so
    /// the enclosing declaration is the reliable source for the `this` type's
    /// member accessibility.
    fn resolve_enclosing_this_instance_type(&self, node: NodeIndex) -> Option<TypeId> {
        let mut current = node;
        let mut iterations = 0;
        while current.is_some() {
            iterations += 1;
            if iterations > 1024 {
                return None;
            }
            let n = self.ctx.arena.get(current)?;
            if matches!(
                n.kind,
                syntax_kind_ext::CLASS_DECLARATION
                    | syntax_kind_ext::CLASS_EXPRESSION
                    | syntax_kind_ext::INTERFACE_DECLARATION
            ) {
                let sym_id = self.ctx.binder.get_node_symbol(current)?;
                return self
                    .ctx
                    .symbol_instance_types
                    .get(&sym_id)
                    .or_else(|| self.ctx.symbol_types.get(&sym_id));
            }
            current = self.ctx.arena.get_extended(current)?.parent;
        }
        None
    }

    pub(super) fn is_numeric_index_on_parameters_utility(
        &self,
        object_type_node: NodeIndex,
        index_type: TypeId,
    ) -> bool {
        crate::query_boundaries::common::number_literal_value(self.ctx.types, index_type).is_some()
            && self.type_node_is_parameters_utility_reference(object_type_node)
    }

    fn type_node_is_parameters_utility_reference(&self, type_node: NodeIndex) -> bool {
        let type_node = self.unwrap_parenthesized_type_node(type_node);
        let Some(node) = self.ctx.arena.get(type_node) else {
            return false;
        };
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return false;
        }
        let Some(type_ref) = self.ctx.arena.get_type_ref(node) else {
            return false;
        };
        let Some(name) = self
            .ctx
            .arena
            .get_identifier_at(type_ref.type_name)
            .map(|ident| ident.escaped_text.as_str())
        else {
            return false;
        };
        matches!(name, "Parameters" | "ConstructorParameters")
    }

    fn unwrap_parenthesized_type_node(&self, mut type_node: NodeIndex) -> NodeIndex {
        for _ in 0..8 {
            let Some(node) = self.ctx.arena.get(type_node) else {
                return type_node;
            };
            if node.kind != PARENTHESIZED_TYPE {
                return type_node;
            }
            let Some(wrapped) = self.ctx.arena.get_wrapped_type(node) else {
                return type_node;
            };
            type_node = wrapped.type_node;
        }
        type_node
    }

    pub(super) fn index_constraint_keyof_matches_mapped_constraint(
        &mut self,
        index_constraint: Option<TypeId>,
        mapped_constraint: TypeId,
        keyof: TypeId,
    ) -> bool {
        let Some(index_constraint) = index_constraint else {
            return false;
        };
        let index_constraint_eval = self.evaluate_type_with_env(index_constraint);

        // Collect keyof operands from each candidate, seeing through the
        // `keyof X & string` intersection idiom so the index operand can still be
        // matched against the mapped constraint's key space.
        let keyof_operands: Vec<TypeId> = [index_constraint, index_constraint_eval]
            .into_iter()
            .flat_map(|candidate| {
                crate::query_boundaries::state::checking::keyof_operands_through_filters(
                    self.ctx.types,
                    candidate,
                )
            })
            .collect();

        keyof_operands.into_iter().any(|index_operand| {
            crate::query_boundaries::state::checking::keyof_target(
                self.ctx.types,
                mapped_constraint,
            )
            .is_some_and(|constraint_operand| {
                same_object_key_space(self.ctx.types, index_operand, constraint_operand)
            }) || crate::query_boundaries::state::checking::keyof_target(self.ctx.types, keyof)
                .is_some_and(|keyof_operand| {
                    same_object_key_space(self.ctx.types, index_operand, keyof_operand)
                })
        })
    }

    pub(super) fn indexed_access_literal_property_exists_in_alias_union(
        &self,
        object_node_idx: NodeIndex,
        index_node_idx: NodeIndex,
    ) -> bool {
        let Some(property_name) = self.type_index_string_literal(index_node_idx) else {
            return false;
        };
        self.alias_body_for_non_generic_type_reference_from_node(object_node_idx)
            .is_some_and(|body_idx| {
                self.alias_union_members_have_property(body_idx, &property_name)
            })
    }

    fn type_index_string_literal(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(node_idx)?;
        if let Some(literal) = self.ctx.arena.get_literal(node) {
            return Some(literal.text.to_string());
        }
        if let Some(literal_type) = self.ctx.arena.get_literal_type(node) {
            let literal_node = self.ctx.arena.get(literal_type.literal)?;
            let literal = self.ctx.arena.get_literal(literal_node)?;
            return Some(literal.text.to_string());
        }
        None
    }

    fn alias_union_members_have_property(
        &self,
        object_node_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(object_node) = self.ctx.arena.get(object_node_idx) else {
            return false;
        };

        if object_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(object_node)
        {
            return self.alias_union_members_have_property(wrapped.type_node, property_name);
        }

        if object_node.kind == syntax_kind_ext::TYPE_REFERENCE {
            return self
                .alias_body_for_non_generic_type_reference(object_node)
                .is_some_and(|body_idx| {
                    self.alias_union_members_have_property(body_idx, property_name)
                });
        }

        if object_node.kind == syntax_kind_ext::UNION_TYPE {
            let Some(composite) = self.ctx.arena.get_composite_type(object_node) else {
                return false;
            };
            return !composite.types.nodes.is_empty()
                && composite.types.nodes.iter().all(|&member_idx| {
                    self.alias_union_members_have_property(member_idx, property_name)
                });
        }

        if object_node.kind == syntax_kind_ext::TYPE_LITERAL {
            return self.type_literal_has_declared_property(object_node, property_name);
        }

        false
    }

    fn alias_body_for_non_generic_type_reference(
        &self,
        object_node: &tsz_parser::parser::node::Node,
    ) -> Option<NodeIndex> {
        let type_ref = self.ctx.arena.get_type_ref(object_node)?;
        if type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty())
        {
            return None;
        }

        let raw_sym_id = self.resolve_type_symbol_for_lowering(type_ref.type_name)?;
        let sym_id = tsz_binder::SymbolId(raw_sym_id);
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
            || symbol.declarations.len() != 1
        {
            return None;
        }

        let decl_node = self.ctx.arena.get(symbol.declarations[0])?;
        let type_alias = self.ctx.arena.get_type_alias(decl_node)?;
        if type_alias
            .type_parameters
            .as_ref()
            .is_some_and(|params| !params.nodes.is_empty())
        {
            return None;
        }
        Some(type_alias.type_node)
    }

    fn alias_body_for_non_generic_type_reference_from_node(
        &self,
        mut node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        loop {
            let object_node = self.ctx.arena.get(node_idx)?;
            if object_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
                node_idx = self.ctx.arena.get_wrapped_type(object_node)?.type_node;
                continue;
            }
            return self.alias_body_for_non_generic_type_reference(object_node);
        }
    }

    fn type_literal_has_declared_property(
        &self,
        type_literal_node: &tsz_parser::parser::node::Node,
        property_name: &str,
    ) -> bool {
        let Some(type_literal) = self.ctx.arena.get_type_literal(type_literal_node) else {
            return false;
        };
        type_literal.members.nodes.iter().any(|&member_idx| {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                return false;
            };
            self.ctx
                .arena
                .get_signature(member_node)
                .map(|signature| signature.name)
                .or_else(|| {
                    self.ctx
                        .arena
                        .get_property_decl(member_node)
                        .map(|property| property.name)
                })
                .and_then(|name| {
                    crate::types_domain::queries::core::get_literal_property_name(
                        self.ctx.arena,
                        name,
                    )
                })
                .as_deref()
                == Some(property_name)
        })
    }

    pub(super) fn type_literal_dispatch_index_is_declared_key_subset(
        &self,
        object_type_node: NodeIndex,
        index_type_node: NodeIndex,
    ) -> bool {
        let Some(keys) = self.type_literal_declared_property_keys(object_type_node) else {
            return false;
        };
        !keys.is_empty()
            && self.type_node_declared_literal_subset(index_type_node, &keys, &mut Vec::new(), 0)
    }

    fn type_literal_declared_property_keys(&self, type_node_idx: NodeIndex) -> Option<Vec<String>> {
        let obj_node = self.ctx.arena.get(type_node_idx)?;
        if obj_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let type_lit = self.ctx.arena.get_type_literal(obj_node)?;
        let mut keys = Vec::new();
        for &member_idx in &type_lit.members.nodes {
            let member_node = self.ctx.arena.get(member_idx)?;
            if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                return None;
            }
            let (name_idx, _type_annotation) =
                self.type_literal_member_name_and_type(member_node)?;
            let name = crate::types_domain::queries::core::get_literal_property_name(
                self.ctx.arena,
                name_idx,
            )?;
            keys.push(name);
        }
        Some(keys)
    }

    fn type_node_declared_literal_subset(
        &self,
        node_idx: NodeIndex,
        allowed_keys: &[String],
        visited_aliases: &mut Vec<tsz_binder::SymbolId>,
        depth: usize,
    ) -> bool {
        if depth > 12 || node_idx == NodeIndex::NONE {
            return false;
        }
        let Some(node) = self.ctx.arena.get(node_idx) else {
            return false;
        };

        if let Some(literal) = self.ctx.arena.get_literal(node) {
            return allowed_keys.iter().any(|key| key == &literal.text);
        }
        if let Some(literal_type) = self.ctx.arena.get_literal_type(node) {
            return self.type_node_declared_literal_subset(
                literal_type.literal,
                allowed_keys,
                visited_aliases,
                depth + 1,
            );
        }

        match node.kind {
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                self.ctx
                    .arena
                    .get_wrapped_type(node)
                    .is_some_and(|wrapped| {
                        self.type_node_declared_literal_subset(
                            wrapped.type_node,
                            allowed_keys,
                            visited_aliases,
                            depth + 1,
                        )
                    })
            }
            k if k == syntax_kind_ext::UNION_TYPE => self
                .ctx
                .arena
                .get_composite_type(node)
                .is_some_and(|composite| {
                    !composite.types.nodes.is_empty()
                        && composite.types.nodes.iter().copied().all(|member_idx| {
                            self.type_node_declared_literal_subset(
                                member_idx,
                                allowed_keys,
                                visited_aliases,
                                depth + 1,
                            )
                        })
                }),
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => self
                .ctx
                .arena
                .get_conditional_type(node)
                .is_some_and(|conditional| {
                    self.type_node_declared_literal_subset(
                        conditional.true_type,
                        allowed_keys,
                        visited_aliases,
                        depth + 1,
                    ) && self.type_node_declared_literal_subset(
                        conditional.false_type,
                        allowed_keys,
                        visited_aliases,
                        depth + 1,
                    )
                }),
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => self
                .ctx
                .arena
                .get_indexed_access_type(node)
                .is_some_and(|indexed| {
                    self.indexed_access_declared_values_subset(
                        indexed.object_type,
                        allowed_keys,
                        visited_aliases,
                        depth + 1,
                    )
                }),
            k if k == syntax_kind_ext::TYPE_REFERENCE => self
                .ctx
                .arena
                .get_type_ref(node)
                .and_then(|type_ref| {
                    self.type_reference_alias_body(type_ref.type_name, visited_aliases)
                })
                .is_some_and(|body| {
                    self.type_node_declared_literal_subset(
                        body,
                        allowed_keys,
                        visited_aliases,
                        depth + 1,
                    )
                }),
            _ => false,
        }
    }

    fn indexed_access_declared_values_subset(
        &self,
        object_type_node: NodeIndex,
        allowed_keys: &[String],
        visited_aliases: &mut Vec<tsz_binder::SymbolId>,
        depth: usize,
    ) -> bool {
        let Some(type_literals) =
            self.possible_type_literals_from_indexed_dispatch(object_type_node, depth)
        else {
            return false;
        };
        !type_literals.is_empty()
            && type_literals.into_iter().all(|type_lit_idx| {
                let Some(type_lit_node) = self.ctx.arena.get(type_lit_idx) else {
                    return false;
                };
                let Some(type_lit) = self.ctx.arena.get_type_literal(type_lit_node) else {
                    return false;
                };
                !type_lit.members.nodes.is_empty()
                    && type_lit.members.nodes.iter().copied().all(|member_idx| {
                        let Some(member_node) = self.ctx.arena.get(member_idx) else {
                            return false;
                        };
                        let Some((_name_idx, type_annotation)) =
                            self.type_literal_member_name_and_type(member_node)
                        else {
                            return false;
                        };
                        self.type_node_declared_literal_subset(
                            type_annotation,
                            allowed_keys,
                            visited_aliases,
                            depth + 1,
                        )
                    })
            })
    }

    fn possible_type_literals_from_indexed_dispatch(
        &self,
        node_idx: NodeIndex,
        depth: usize,
    ) -> Option<Vec<NodeIndex>> {
        if depth > 12 {
            return None;
        }
        let node = self.ctx.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::TYPE_LITERAL {
            return Some(vec![node_idx]);
        }
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
            && let Some(wrapped) = self.ctx.arena.get_wrapped_type(node)
        {
            return self.possible_type_literals_from_indexed_dispatch(wrapped.type_node, depth + 1);
        }
        let indexed = self.ctx.arena.get_indexed_access_type(node)?;
        let inner_literals =
            self.possible_type_literals_from_indexed_dispatch(indexed.object_type, depth + 1)?;
        let mut values = Vec::new();
        for type_lit_idx in inner_literals {
            let type_lit_node = self.ctx.arena.get(type_lit_idx)?;
            let type_lit = self.ctx.arena.get_type_literal(type_lit_node)?;
            for &member_idx in &type_lit.members.nodes {
                let member_node = self.ctx.arena.get(member_idx)?;
                let (_name_idx, type_annotation) =
                    self.type_literal_member_name_and_type(member_node)?;
                let value_node = self.ctx.arena.get(type_annotation)?;
                let value_idx = if value_node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
                    self.ctx.arena.get_wrapped_type(value_node)?.type_node
                } else {
                    type_annotation
                };
                if self
                    .ctx
                    .arena
                    .get(value_idx)
                    .is_some_and(|value_node| value_node.kind == syntax_kind_ext::TYPE_LITERAL)
                {
                    values.push(value_idx);
                } else {
                    return None;
                }
            }
        }
        Some(values)
    }

    fn type_reference_alias_body(
        &self,
        type_name: NodeIndex,
        visited_aliases: &mut Vec<tsz_binder::SymbolId>,
    ) -> Option<NodeIndex> {
        let raw_sym_id = self.resolve_type_symbol_for_lowering(type_name)?;
        let sym_id = tsz_binder::SymbolId(raw_sym_id);
        if visited_aliases.contains(&sym_id) {
            return None;
        }
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if !symbol.has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
            || symbol.declarations.len() != 1
        {
            return None;
        }
        let decl_node = self.ctx.arena.get(symbol.declarations[0])?;
        let alias = self.ctx.arena.get_type_alias(decl_node)?;
        visited_aliases.push(sym_id);
        Some(alias.type_node)
    }

    fn type_literal_member_name_and_type(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> Option<(NodeIndex, NodeIndex)> {
        if let Some(signature) = self.ctx.arena.get_signature(member_node) {
            return Some((signature.name, signature.type_annotation));
        }
        self.ctx
            .arena
            .get_property_decl(member_node)
            .map(|property| (property.name, property.type_annotation))
    }

    fn indexed_access_tuple_selector_part(
        &mut self,
        node_idx: NodeIndex,
    ) -> Option<(String, usize)> {
        let node = self.ctx.arena.get(node_idx)?;
        let indexed = self.ctx.arena.get_indexed_access_type(node)?;
        let selector_name = self
            .ctx
            .arena
            .get(indexed.object_type)
            .and_then(|object_node| self.ctx.arena.get_type_ref(object_node))
            .and_then(|type_ref| self.ctx.arena.get(type_ref.type_name))
            .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
            .map(|ident| ident.escaped_text.as_str())?
            .to_owned();
        let element_index = self.type_index_numeric_literal(indexed.index_type)?;
        Some((selector_name, element_index))
    }

    fn type_index_numeric_literal(&self, node_idx: NodeIndex) -> Option<usize> {
        let node = self.ctx.arena.get(node_idx)?;
        let literal_node = if let Some(literal_type) = self.ctx.arena.get_literal_type(node) {
            self.ctx.arena.get(literal_type.literal)?
        } else {
            node
        };
        let literal = self.ctx.arena.get_literal(literal_node)?;
        let value = literal
            .value
            .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&literal.text))?;
        if value.fract() != 0.0 || value < 0.0 {
            return None;
        }
        Some(value as usize)
    }

    fn tuple_selector_element_type(
        &mut self,
        selector_name: &str,
        element_index: usize,
    ) -> Option<TypeId> {
        let selector_type = self.ctx.type_parameter_scope.get(selector_name).copied()?;
        let elements = crate::query_boundaries::type_computation::access::tuple_elements(
            self.ctx.types,
            selector_type,
        )?;
        let element = elements.get(element_index)?;
        if element.rest {
            return None;
        }
        Some(element.type_id)
    }

    pub(super) fn type_literal_keyof_from_node(
        &mut self,
        type_node_idx: NodeIndex,
    ) -> Option<TypeId> {
        let obj_node = self.ctx.arena.get(type_node_idx)?;
        if obj_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return None;
        }
        let type_lit = self.ctx.arena.get_type_literal(obj_node)?;
        let mut key_types = Vec::new();
        for &member_idx in &type_lit.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::INDEX_SIGNATURE {
                return None;
            }
            if let Some((name_idx, _type_annotation)) =
                self.type_literal_member_name_and_type(member_node)
                && let Some(name) = self.get_property_name(name_idx)
            {
                let key_type = self
                    .ctx
                    .arena
                    .get(name_idx)
                    .filter(|name_node| name_node.kind == SyntaxKind::NumericLiteral as u16)
                    .and_then(|name_node| self.ctx.arena.get_literal(name_node))
                    .and_then(|lit| {
                        lit.value
                            .or_else(|| tsz_common::numeric::parse_numeric_literal_value(&lit.text))
                    })
                    .map(|value| key_space_query::literal_number_key(self.ctx.types, value))
                    .unwrap_or_else(|| {
                        let atom = self.ctx.types.intern_string(&name);
                        key_space_query::literal_string_key(self.ctx.types, atom)
                    });
                key_types.push(key_type);
            }
        }

        key_space_query::literal_key_union(self.ctx.types, key_types)
    }

    pub(super) fn type_literal_ast_key_space_accepts_index(
        &mut self,
        object_type_node: NodeIndex,
        index_type: TypeId,
    ) -> bool {
        let Some(keyof_type) = self.type_literal_keyof_from_node(object_type_node) else {
            return false;
        };
        let index_for_check = self.evaluate_type_with_env(index_type);
        if self
            .indexed_access_key_space_relation_outcome(index_for_check, keyof_type)
            .related
            || self
                .indexed_access_key_space_relation_outcome(index_type, keyof_type)
                .related
        {
            return true;
        }
        crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, index_for_check)
            .or_else(|| {
                crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    index_type,
                )
            })
            .is_some_and(|constraint| {
                let constraint = self.evaluate_type_with_env(constraint);
                self.indexed_access_key_space_relation_outcome(constraint, keyof_type)
                    .related
            })
    }

    pub(super) fn type_literal_member_values_accept_index(
        &mut self,
        type_node_idx: NodeIndex,
        index_type: TypeId,
        index_constraint: Option<TypeId>,
    ) -> bool {
        let Some(obj_node) = self.ctx.arena.get(type_node_idx) else {
            return false;
        };
        if obj_node.kind != syntax_kind_ext::TYPE_LITERAL {
            return false;
        }
        let Some(type_lit) = self.ctx.arena.get_type_literal(obj_node) else {
            return false;
        };
        let index_for_check = self.evaluate_type_with_env(index_type);
        let constraint_for_check =
            index_constraint.map(|constraint| self.evaluate_type_with_env(constraint));
        let mut saw_value = false;

        for &member_idx in &type_lit.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let Some((_name_idx, type_annotation)) =
                self.type_literal_member_name_and_type(member_node)
            else {
                continue;
            };
            if type_annotation == NodeIndex::NONE {
                return false;
            }
            let Some(value_keyof) = self.type_literal_keyof_from_node(type_annotation) else {
                return false;
            };
            if !self
                .indexed_access_key_space_relation_outcome(index_for_check, value_keyof)
                .related
                && !constraint_for_check.is_some_and(|constraint| {
                    self.indexed_access_key_space_relation_outcome(constraint, value_keyof)
                        .related
                })
            {
                return false;
            }
            saw_value = true;
        }

        saw_value
    }

    pub(super) fn nested_type_literal_index_access_allows_index(
        &mut self,
        object_type_node_idx: NodeIndex,
        outer_index_node_idx: NodeIndex,
        outer_index_type: TypeId,
    ) -> bool {
        let Some(object_node) = self.ctx.arena.get(object_type_node_idx) else {
            return false;
        };
        let Some(nested) = self.ctx.arena.get_indexed_access_type(object_node) else {
            return false;
        };

        if let (
            Some((nested_selector, nested_element_index)),
            Some((outer_selector, outer_element_index)),
        ) = (
            self.indexed_access_tuple_selector_part(nested.index_type),
            self.indexed_access_tuple_selector_part(outer_index_node_idx),
        ) && nested_selector == outer_selector
            && let (Some(nested_element_type), Some(outer_element_type)) = (
                self.tuple_selector_element_type(&nested_selector, nested_element_index),
                self.tuple_selector_element_type(&outer_selector, outer_element_index),
            )
            && let Some(nested_base_keyof) = self.type_literal_keyof_from_node(nested.object_type)
            && self
                .indexed_access_key_space_relation_outcome(nested_element_type, nested_base_keyof)
                .related
            && self.type_literal_member_values_accept_index(
                nested.object_type,
                outer_element_type,
                None,
            )
        {
            return true;
        }

        let nested_index_type = self.get_type_from_type_node(nested.index_type);
        let mut nested_index_constraint =
            crate::query_boundaries::common::type_parameter_constraint(
                self.ctx.types,
                nested_index_type,
            );
        if crate::query_boundaries::common::is_type_parameter_like(
            self.ctx.types,
            nested_index_type,
        ) && nested_index_constraint.is_none()
        {
            nested_index_constraint = self
                .resolve_index_constraint_from_declaration(nested.index_type, nested.object_type);
        }

        let mut outer_index_constraint = crate::query_boundaries::common::type_parameter_constraint(
            self.ctx.types,
            outer_index_type,
        );
        if crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, outer_index_type)
            && outer_index_constraint.is_none()
        {
            outer_index_constraint = self.resolve_index_constraint_from_declaration(
                outer_index_node_idx,
                object_type_node_idx,
            );
        }

        let Some(nested_base_keyof) = self.type_literal_keyof_from_node(nested.object_type) else {
            return false;
        };
        let nested_index_for_check = nested_index_constraint.unwrap_or(nested_index_type);
        let nested_index_for_check = self.evaluate_type_with_env(nested_index_for_check);

        self.indexed_access_key_space_relation_outcome(nested_index_for_check, nested_base_keyof)
            .related
            && self.type_literal_member_values_accept_index(
                nested.object_type,
                outer_index_type,
                outer_index_constraint,
            )
    }

    fn array_like_kind_has_length(
        &self,
        kind: crate::query_boundaries::type_checking_utilities::ArrayLikeKind,
    ) -> bool {
        match kind {
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Array(_)
            | crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Tuple => true,
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Readonly(inner) => {
                self.indexed_access_type_has_array_like_length(inner)
            }
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Union(members) => {
                !members.is_empty()
                    && members
                        .iter()
                        .all(|&member| self.indexed_access_type_has_array_like_length(member))
            }
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Intersection(
                members,
            ) => members
                .iter()
                .any(|&member| self.indexed_access_type_has_array_like_length(member)),
            crate::query_boundaries::type_checking_utilities::ArrayLikeKind::Other => false,
        }
    }

    fn indexed_access_type_has_array_like_length(&self, type_id: TypeId) -> bool {
        let kind = crate::query_boundaries::type_checking_utilities::classify_array_like(
            self.ctx.types,
            type_id,
        );
        self.array_like_kind_has_length(kind)
    }

    pub(super) fn indexed_access_object_allows_length_property(
        &mut self,
        object_type: TypeId,
        object_type_for_check: TypeId,
    ) -> bool {
        let candidates = [
            object_type,
            object_type_for_check,
            self.evaluate_type_with_env(object_type),
            self.evaluate_type_with_env(object_type_for_check),
        ];

        candidates.iter().copied().any(|candidate| {
            self.indexed_access_type_has_array_like_length(candidate)
                || crate::query_boundaries::common::type_parameter_constraint(
                    self.ctx.types,
                    candidate,
                )
                .is_some_and(|constraint| {
                    self.indexed_access_type_has_array_like_length(constraint)
                })
        })
    }

    /// Indexed-access (`(A | B)["k"]`) form of the union restricted-property
    /// rule. Delegates to the shared
    /// [`CheckerState::union_restricted_property_is_missing`] so the type-level
    /// and expression-level (`x.k`) paths stay in lockstep, including the
    /// intersection-constituent "common declaration" handling.
    pub(super) fn union_restricted_literal_property_is_missing(
        &mut self,
        property_name: &str,
        object_type: TypeId,
    ) -> bool {
        self.union_restricted_property_is_missing(property_name, object_type)
    }

    pub(super) fn error_at_index_type_span(
        &mut self,
        error_anchor: NodeIndex,
        message: &str,
        code: u32,
    ) {
        let Some(anchor_node) = self.ctx.arena.get(error_anchor) else {
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let source = source_file.text.as_ref();
        let start = anchor_node.pos as usize;
        let end = anchor_node.end as usize;
        let Some(text) = source.get(start..end) else {
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let Some(open_bracket) = text.rfind('[') else {
            if let Some(index_text) = text.trim().strip_suffix(']').map(str::trim_end)
                && !index_text.is_empty()
            {
                let leading_ws = text.len() - text.trim_start().len();
                self.ctx.error(
                    (start + leading_ws) as u32,
                    index_text.len() as u32,
                    message.to_string(),
                    code,
                );
                return;
            }
            self.error_at_node(error_anchor, message, code);
            return;
        };
        let close_bracket = text.rfind(']').unwrap_or(text.len());
        if close_bracket <= open_bracket + 1 {
            self.error_at_node(error_anchor, message, code);
            return;
        }

        let inner = &text[open_bracket + 1..close_bracket];
        let leading_ws = inner.len() - inner.trim_start().len();
        let trailing_ws = inner.len() - inner.trim_end().len();
        let pos = start + open_bracket + 1 + leading_ws;
        let len = inner.len().saturating_sub(leading_ws + trailing_ws).max(1);
        self.ctx
            .error(pos as u32, len as u32, message.to_string(), code);
    }

    /// The canonical array-index a string-literal *type* spells (`'0'` -> `0`,
    /// `'42'` -> `42`), or `None` when `index_type` is not a string-literal type
    /// or its text is not a canonical unsigned-integer index string (`'01'`,
    /// `'foo'`, `'+1'` are property names, not indices). `tsc` treats
    /// `Base['0']` and `Base[0]` identically for tuple/array element lookup.
    pub(super) fn string_literal_numeric_index(&self, index_type: TypeId) -> Option<usize> {
        let prop_atom =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, index_type)?;
        let property_name = self.ctx.types.resolve_atom_ref(prop_atom);
        self.get_numeric_index_from_string(&property_name)
    }

    /// Whether a canonical numeric string-literal key (`'0'`, `'1'`, …) indexes
    /// `object_type`.
    ///
    /// `tsc` resolves a string-literal index against the *declared* keys, never
    /// through a numeric index signature: `[1, 2]['0']` resolves, while
    /// `[1, 2]['5']`, `string[]['0']`, and `[1, 2, ...number[]]['5']` are all
    /// `TS2536`. Only a tuple's fixed positions are declared keys, so a rest
    /// element never widens the domain and a plain array has no such keys at all.
    pub(super) fn canonical_numeric_string_literal_valid_for_object(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
    ) -> bool {
        let Some(index) = self.string_literal_numeric_index(index_type) else {
            return false;
        };
        let Some((len, _has_rest)) = self.tuple_fixed_length(object_type) else {
            return false;
        };
        index < len
    }

    pub(super) fn union_index_members_valid_for_object(
        &mut self,
        index_type: TypeId,
        object_type: TypeId,
        keyof_object: TypeId,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, index_type)
        else {
            return false;
        };

        let object_is_opaque_param =
            is_unconstrained_type_param_object(self.ctx.types, object_type);
        members.iter().all(|&member| {
            self.indexed_access_key_space_relation_outcome(member, keyof_object)
                .related
                || (!object_is_opaque_param
                    && self.get_index_key_kind(member).is_some_and(
                        |(wants_string, wants_number)| {
                            self.is_element_indexable(object_type, wants_string, wants_number)
                        },
                    ))
                || crate::query_boundaries::common::numeric_literal_index_valid_for_object(
                    self.ctx.types,
                    member,
                    object_type,
                )
                || self.canonical_numeric_string_literal_valid_for_object(member, object_type)
        })
    }

    pub(super) fn conditional_true_branch_constraint_allows_index(
        &mut self,
        node_idx: NodeIndex,
        object_node_idx: NodeIndex,
        index_type_for_check: TypeId,
    ) -> bool {
        let object_name = self.simple_type_reference_name(object_node_idx);
        let Some(object_name) = object_name.as_deref() else {
            return false;
        };

        let mut current = self.ctx.arena.parent_of(node_idx);
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::CONDITIONAL_TYPE
                && let Some(cond) = self.ctx.arena.get_conditional_type(parent_node)
                && self.is_descendant_of(node_idx, cond.true_type)
                && self.simple_type_reference_name(cond.check_type).as_deref() == Some(object_name)
            {
                let extends_type = self.get_type_from_type_node(cond.extends_type);
                let extends_type = self.evaluate_type_with_env(extends_type);
                let keyof_extends = self.indexed_access_keyof_with_env(extends_type);
                if self
                    .indexed_access_key_space_relation_outcome(index_type_for_check, keyof_extends)
                    .related
                {
                    return true;
                }
                if let Some(prop_atom) =
                    crate::query_boundaries::checkers::generic::string_literal_value(
                        self.ctx.types,
                        index_type_for_check,
                    )
                {
                    let property_name = self.ctx.types.resolve_atom(prop_atom);
                    let prop_type =
                        self.resolve_property_access_with_env(extends_type, &property_name);
                    if matches!(
                        prop_type,
                        PropertyAccessResult::Success { .. }
                            | PropertyAccessResult::PossiblyNullOrUndefined { .. }
                    ) {
                        return true;
                    }
                }
                if let Some((wants_string, wants_number)) =
                    self.get_index_key_kind(index_type_for_check)
                    && self.is_element_indexable(extends_type, wants_string, wants_number)
                {
                    return true;
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }

        false
    }

    pub(super) fn indexed_access_constraint_values_allow_index(
        &mut self,
        base_type: TypeId,
        index_type: TypeId,
    ) -> bool {
        if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, base_type)
        {
            let mapped = self.ctx.types.mapped_type(mapped_id);
            let template_keyof = self.indexed_access_keyof_with_env(mapped.template);
            return self
                .indexed_access_key_space_relation_outcome(index_type, template_keyof)
                .related;
        }

        let Some(constraint) =
            crate::query_boundaries::common::type_parameter_constraint(self.ctx.types, base_type)
        else {
            return false;
        };
        let constraint = self.evaluate_type_with_env(constraint);
        if matches!(constraint, TypeId::ERROR | TypeId::ANY) {
            return false;
        }

        let key_space = self.indexed_access_keyof_with_env(constraint);
        let values = self.evaluate_type_with_env(key_space_query::indexed_access_type(
            self.ctx.types,
            constraint,
            key_space,
        ));
        if matches!(values, TypeId::ERROR | TypeId::UNDEFINED) {
            return false;
        }
        let values_keyof = self.indexed_access_keyof_with_env(values);
        self.indexed_access_key_space_relation_outcome(index_type, values_keyof)
            .related
    }

    pub(super) fn mapped_object_index_matches_own_key_constraint(
        &mut self,
        object_node_idx: NodeIndex,
        index_type: TypeId,
        index_type_for_check: TypeId,
    ) -> bool {
        let Some(object_node) = self.ctx.arena.get(object_node_idx) else {
            return false;
        };
        let Some(mapped) = self.ctx.arena.get_mapped_type(object_node) else {
            return false;
        };
        if mapped.name_type != NodeIndex::NONE {
            return false;
        }
        let Some(tp_node) = self.ctx.arena.get(mapped.type_parameter) else {
            return false;
        };
        let Some(tp) = self.ctx.arena.get_type_parameter(tp_node) else {
            return false;
        };
        if tp.constraint == NodeIndex::NONE {
            return false;
        }

        let constraint_type = self.get_type_from_type_node(tp.constraint);
        let constraint_eval = self.evaluate_type_with_env(constraint_type);

        index_type == constraint_type
            || index_type_for_check == constraint_eval
            || (self
                .indexed_access_key_space_relation_outcome(index_type_for_check, constraint_eval)
                .related
                && self
                    .indexed_access_key_space_relation_outcome(
                        constraint_eval,
                        index_type_for_check,
                    )
                    .related)
    }

    pub(super) fn index_constraint_keyof_targets_foreign_indexed_object(
        &mut self,
        object_type: TypeId,
        object_type_for_check: TypeId,
        index_type: TypeId,
        index_constraint: Option<TypeId>,
    ) -> bool {
        if !crate::query_boundaries::common::is_type_parameter_like(self.ctx.types, index_type) {
            return false;
        }
        let Some(index_constraint) = index_constraint else {
            return false;
        };
        let Some((constraint_target, constraint_base, constraint_index)) =
            self.keyof_indexed_access_target(index_constraint)
        else {
            // Simple `keyof A` constraint: T extends `keyof A` but indexes B.
            // No TS2536 when every key of A is also a key of B, i.e. `keyof A ≤ keyof B`.
            //
            // Avoid comparing the keyof unions directly — for large DOM maps (e.g.
            // `ElementTagNameMap` vs `HTMLElementTagNameMap`) this compares hundreds of
            // string-literal union members and can hit iteration limits in the solver,
            // causing a spurious "assignable" result and silently dropping the error.
            //
            // Instead, exploit the keyof contravariance: `keyof A ≤ keyof B` iff `B ≤ A`
            // (B is assignable to A). Extract A from the constraint (`keyof A` → A) and
            // check `is_assignable_to(B, A)` directly on the object types.
            //
            // Fire TS2536 only when we can *prove* the key spaces differ. The proof
            // requires both the constraint operand and the indexed object to be named
            // Lazy aliases with distinct DefIds. For Application types, inline object
            // types, and conditional types — where `lazy_def_id` returns `None` — we
            // cannot prove a difference, so we default to suppressing the error.
            let mut provably_different_named_type = false;
            for current_object in [object_type, object_type_for_check] {
                if crate::query_boundaries::common::is_type_parameter_like(
                    self.ctx.types,
                    current_object,
                ) {
                    return false; // Can't determine key space statically; defer.
                }
                if let Some(constraint_operand) =
                    crate::query_boundaries::state::checking::keyof_target(
                        self.ctx.types,
                        index_constraint,
                    )
                {
                    // Fast-path 1: same TypeId → trivially valid.
                    if same_object_key_space(self.ctx.types, constraint_operand, current_object) {
                        return false;
                    }
                    // Fast-path 2: same DefId (both operands are Lazy for the same alias).
                    let constraint_def = crate::query_boundaries::common::lazy_def_id(
                        self.ctx.types,
                        constraint_operand,
                    );
                    let current_def = crate::query_boundaries::common::lazy_def_id(
                        self.ctx.types,
                        current_object,
                    );
                    if constraint_def.is_some() && constraint_def == current_def {
                        return false;
                    }
                    // Fast-path 3: structural evaluation match — Lazy(X) vs evaluated-X.
                    if self.same_key_space_after_evaluation(constraint_operand, current_object) {
                        return false;
                    }
                    // Fast-path 4: body-TypeId match for Lazy alias vs its stored body.
                    //
                    // Two evaluation paths for the same type alias can produce different TypeIds:
                    // `get_type_from_type_node(ETM)` returns the stored body TypeId directly,
                    // while `evaluate_type_with_env(Lazy(DefId))` recursively evaluates members
                    // and interns a NEW intersection TypeId. Because the intersection is not
                    // structurally interned to the same ID, TypeId equality fails in fast-paths
                    // 1-3 even when both represent the exact same type alias.
                    //
                    // The DefinitionStore.get_body(DefId) gives the stored body TypeId (same one
                    // returned by `get_type_from_type_node`), so comparing it against
                    // `current_object` detects this case without any structural comparison.
                    if let Some(def_id) = constraint_def
                        && let Some(body_type) = self.ctx.get_semantic_def_body(def_id)
                        && body_type == current_object
                    {
                        return false;
                    }
                    // Both are named Lazy aliases with DIFFERENT DefIds (fast-path 2 didn't
                    // fire). This is the only case where we can prove distinct key spaces.
                    if constraint_def.is_some() && current_def.is_some() {
                        provably_different_named_type = true;
                    }
                } else {
                    // Non-keyof constraint (e.g. `string`, `number`): compare the
                    // constraint directly against `keyof B`. This path is rare; the
                    // large-union issue doesn't apply here since the constraint is not
                    // itself a computed keyof union.
                    let keyof_current = key_space_query::keyof_type(self.ctx.types, current_object);
                    if self
                        .indexed_access_foreign_keyof_constraint_relation_outcome(
                            index_constraint,
                            keyof_current,
                        )
                        .related
                    {
                        return false;
                    }
                }
            }
            return provably_different_named_type;
        };

        for current_object in [object_type, object_type_for_check] {
            // Compare constraint_target against current_object for structural identity.
            // When both are IndexAccess types, compare their base and index components
            // directly rather than by evaluated value: two different `IndexAccess(A, T)` and
            // `IndexAccess(B, T)` can evaluate to the same union when A and B share identical
            // property values for T's constrained key range, even though they are different
            // key-space targets (e.g. `ElementTagNameMap[T]` vs `HTMLElementTagNameMap[T]`).
            let targets_same_object = match (
                crate::query_boundaries::common::index_access_types(
                    self.ctx.types,
                    constraint_target,
                ),
                crate::query_boundaries::common::index_access_types(self.ctx.types, current_object),
            ) {
                (Some((ct_base, ct_index)), Some((co_base, co_index))) => {
                    same_object_key_space(self.ctx.types, ct_base, co_base)
                        && same_object_key_space(self.ctx.types, ct_index, co_index)
                }
                _ => self.same_key_space_after_evaluation(constraint_target, current_object),
            };
            if targets_same_object {
                return false;
            }
            let Some((current_base, current_index)) =
                crate::query_boundaries::common::index_access_types(self.ctx.types, current_object)
            else {
                continue;
            };
            if self.same_key_space_after_evaluation(constraint_index, current_index) {
                return !self.same_key_space_after_evaluation(constraint_base, current_base);
            }
        }
        false
    }

    pub(super) fn ast_index_constraint_keyof_targets_foreign_indexed_object(
        &mut self,
        object_node_idx: NodeIndex,
        index_node_idx: NodeIndex,
    ) -> bool {
        let Some(constraint_node_idx) =
            self.resolve_index_constraint_node_from_declaration(index_node_idx)
        else {
            return false;
        };
        let Some(constraint_node) = self.ctx.arena.get(constraint_node_idx) else {
            return false;
        };
        let Some(type_op) = self.ctx.arena.get_type_operator(constraint_node) else {
            return false;
        };
        if type_op.operator != SyntaxKind::KeyOfKeyword as u16 {
            return false;
        }
        let Some(constraint_target_node) = self.ctx.arena.get(type_op.type_node) else {
            return false;
        };
        let Some(constraint_target) = self
            .ctx
            .arena
            .get_indexed_access_type(constraint_target_node)
        else {
            return false;
        };
        let Some(object_node) = self.ctx.arena.get(object_node_idx) else {
            return false;
        };
        let Some(current_object) = self.ctx.arena.get_indexed_access_type(object_node) else {
            return false;
        };
        if !self.nodes_have_same_text(constraint_target.index_type, current_object.index_type) {
            return false;
        }

        let constraint_base_name = self.simple_type_reference_name(constraint_target.object_type);
        let current_base_name = self.simple_type_reference_name(current_object.object_type);
        if constraint_base_name.is_some()
            && current_base_name.is_some()
            && constraint_base_name == current_base_name
        {
            return false;
        }
        if self.nodes_have_same_text(constraint_target.object_type, current_object.object_type) {
            return false;
        }

        let current_base = self.get_type_from_type_node(current_object.object_type);
        let current_index = self.get_type_from_type_node(current_object.index_type);
        let current_base_keyof = self.indexed_access_keyof_with_env(current_base);
        let current_index_for_check = self.evaluate_type_with_env(current_index);
        !self
            .indexed_access_key_space_relation_outcome(current_index_for_check, current_base_keyof)
            .related
    }

    fn resolve_index_constraint_node_from_declaration(
        &self,
        index_node_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let index_name = self.simple_type_reference_name(index_node_idx)?;
        let mut current = self
            .ctx
            .arena
            .get_extended(index_node_idx)
            .map(|ext| ext.parent);
        while let Some(parent_idx) = current {
            let parent_node = self.ctx.arena.get(parent_idx)?;
            let type_params = match parent_node.kind {
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    self.ctx
                        .arena
                        .get_function(parent_node)
                        .and_then(|f| f.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION
                    || k == syntax_kind_ext::METHOD_SIGNATURE
                    || k == syntax_kind_ext::CALL_SIGNATURE
                    || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
                {
                    self.ctx
                        .arena
                        .get_signature(parent_node)
                        .and_then(|s| s.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => self
                    .ctx
                    .arena
                    .get_interface(parent_node)
                    .and_then(|i| i.type_parameters.as_ref()),
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::CLASS_EXPRESSION =>
                {
                    self.ctx
                        .arena
                        .get_class(parent_node)
                        .and_then(|c| c.type_parameters.as_ref())
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => self
                    .ctx
                    .arena
                    .get_type_alias(parent_node)
                    .and_then(|ta| ta.type_parameters.as_ref()),
                k if k == syntax_kind_ext::FUNCTION_TYPE
                    || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
                {
                    self.ctx
                        .arena
                        .get_function_type(parent_node)
                        .and_then(|ft| ft.type_parameters.as_ref())
                }
                _ => None,
            };
            if let Some(tp_list) = type_params {
                for &tp_idx in &tp_list.nodes {
                    let Some(tp_node) = self.ctx.arena.get(tp_idx) else {
                        continue;
                    };
                    let Some(tp) = self.ctx.arena.get_type_parameter(tp_node) else {
                        continue;
                    };
                    let Some(name_node) = self.ctx.arena.get(tp.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    if ident.escaped_text == index_name && tp.constraint != NodeIndex::NONE {
                        return Some(tp.constraint);
                    }
                }
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        None
    }

    fn keyof_indexed_access_target(&mut self, type_id: TypeId) -> Option<(TypeId, TypeId, TypeId)> {
        for candidate in [type_id, self.evaluate_type_with_env(type_id)] {
            let Some(target) =
                crate::query_boundaries::state::checking::keyof_target(self.ctx.types, candidate)
            else {
                continue;
            };
            if let Some((base, index)) =
                crate::query_boundaries::common::index_access_types(self.ctx.types, target)
            {
                return Some((target, base, index));
            }
            let evaluated_target = self.evaluate_type_with_env(target);
            if let Some((base, index)) = crate::query_boundaries::common::index_access_types(
                self.ctx.types,
                evaluated_target,
            ) {
                return Some((evaluated_target, base, index));
            }
        }
        None
    }

    fn same_key_space_after_evaluation(&mut self, left: TypeId, right: TypeId) -> bool {
        same_object_key_space(self.ctx.types, left, right) || {
            let left_eval = self.evaluate_type_with_env(left);
            let right_eval = self.evaluate_type_with_env(right);
            same_object_key_space(self.ctx.types, left_eval, right)
                || same_object_key_space(self.ctx.types, left, right_eval)
                || same_object_key_space(self.ctx.types, left_eval, right_eval)
        }
    }

    pub(super) fn simple_type_reference_name(&self, node_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(node_idx)?;
        if node.kind == syntax_kind_ext::TYPE_REFERENCE {
            let type_ref = self.ctx.arena.get_type_ref(node)?;
            let name_node = self.ctx.arena.get(type_ref.type_name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            return Some(ident.escaped_text.to_string());
        }
        if node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.ctx.arena.get_identifier(node)?;
            return Some(ident.escaped_text.to_string());
        }
        None
    }

    pub(super) fn type_node_refers_to_type_parameter(&self, node_idx: NodeIndex) -> bool {
        // Resolve the name in its lexical scope rather than matching *any* symbol
        // with the same spelling. A global `find_all_by_name` match also catches
        // unrelated type parameters declared by lib/global generics — e.g. a user
        // alias named `T` collides with `Array<T>`'s parameter `T` — and would
        // mis-classify the concrete alias receiver as a type parameter, routing a
        // missing literal key to TS2536 instead of TS2339 (#14804). The in-scope
        // resolution keys off the actual binding (type-parameter scope or the
        // identifier's resolved symbol), so it is independent of the chosen name.
        crate::query_boundaries::type_checking_utilities::ast_index_node_is_in_scope_type_parameter(
            self.ctx.arena,
            self.ctx.binder,
            &self.ctx.type_parameter_scope,
            node_idx,
        )
    }

    /// Structural rule: when the object has a plain string index signature and the index
    /// type is assignable to `string | number`, suppress TS2536.
    ///
    /// Plain string index signatures (`key_type == STRING`) accept both string and number
    /// keys per JS coercion semantics. If the index is provably within `string | number`
    /// (no symbol members possible), it is always a valid key.
    pub(super) fn keyof_index_valid_for_string_indexed_object(
        &mut self,
        object_type: TypeId,
        index_type_for_check: TypeId,
        index_constraint: Option<TypeId>,
    ) -> bool {
        let has_plain_string_index = self
            .ctx
            .types
            .get_index_signatures(object_type)
            .string_index
            .is_some_and(|sig| sig.key_type == TypeId::STRING);
        if !has_plain_string_index {
            return false;
        }
        let string_or_number = key_space_query::string_or_number_key_space(self.ctx.types);
        if self
            .string_index_candidate_is_string_or_number_key(index_type_for_check, string_or_number)
        {
            return true;
        }
        if let Some(constraint) = index_constraint {
            let evaluated_constraint = self.evaluate_type_with_env(constraint);
            return self
                .string_index_candidate_is_string_or_number_key(constraint, string_or_number)
                || self.string_index_candidate_is_string_or_number_key(
                    evaluated_constraint,
                    string_or_number,
                );
        }
        false
    }

    fn string_index_candidate_is_string_or_number_key(
        &mut self,
        candidate: TypeId,
        string_or_number: TypeId,
    ) -> bool {
        self.indexed_access_key_space_relation_outcome(candidate, string_or_number)
            .related
            || self.keyof_candidate_target_is_array_like(candidate)
    }

    fn keyof_candidate_target_is_array_like(&mut self, candidate: TypeId) -> bool {
        let Some(target) =
            crate::query_boundaries::state::checking::keyof_target(self.ctx.types, candidate)
        else {
            return false;
        };
        self.type_or_constraint_is_array_like(target)
    }

    fn type_or_constraint_is_array_like(&mut self, type_id: TypeId) -> bool {
        if self.indexed_access_type_has_array_like_length(type_id) {
            return true;
        }
        let evaluated = self.evaluate_type_with_env(type_id);
        evaluated != type_id && self.indexed_access_type_has_array_like_length(evaluated)
    }
}
