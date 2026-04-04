use crate::context::TypingRequest;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    pub(super) fn collect_direct_commonjs_assignment_exports(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        pending_props: &mut FxHashMap<String, Vec<(NodeIndex, Option<String>)>>,
        ordered_names: &mut Vec<String>,
        export_aliases: &FxHashSet<String>,
    ) {
        let Some(expr_node) = arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = arena.get_binary_expr(expr_node) else {
            return;
        };
        if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
            return;
        }

        let Some(left_node) = arena.get(binary.left) else {
            return;
        };
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let Some(left_access) = arena.get_access_expr(left_node) else {
                return;
            };
            let direct_exports = arena
                .get_identifier_at(left_access.expression)
                .and_then(|ident| {
                    (ident.escaped_text == "exports").then(|| {
                        Self::commonjs_static_member_name_in_arena(
                            arena,
                            left_access.name_or_argument,
                        )
                        .map(|name| (name.clone(), Some(format!("exports.{name}"))))
                    })
                })
                .flatten();
            let module_exports = arena.get(left_access.expression).and_then(|target_node| {
                let target_access = arena.get_access_expr(target_node)?;
                let is_module_exports =
                    if target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                        arena
                            .get_identifier_at(target_access.expression)
                            .is_some_and(|ident| ident.escaped_text == "module")
                            && arena
                                .get_identifier_at(target_access.name_or_argument)
                                .is_some_and(|ident| ident.escaped_text == "exports")
                    } else if target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                        arena
                            .get_identifier_at(target_access.expression)
                            .is_some_and(|ident| ident.escaped_text == "module")
                            && Self::commonjs_static_member_name_in_arena(
                                arena,
                                target_access.name_or_argument,
                            )
                            .is_some_and(|name| name == "exports")
                    } else {
                        false
                    };
                is_module_exports.then(|| {
                    Self::commonjs_static_member_name_in_arena(arena, left_access.name_or_argument)
                        .map(|name| (name.clone(), Some(format!("module.exports.{name}"))))
                })?
            });

            // Also check if the expression is a known alias for exports/module.exports
            let alias_exports = if direct_exports.is_none() && module_exports.is_none() {
                arena
                    .get_identifier_at(left_access.expression)
                    .and_then(|ident| {
                        export_aliases
                            .contains(ident.escaped_text.as_str())
                            .then(|| {
                                Self::commonjs_static_member_name_in_arena(
                                    arena,
                                    left_access.name_or_argument,
                                )
                                .map(|name| (name, None))
                            })
                    })
                    .flatten()
            } else {
                None
            };

            if let Some((name_text, expando_root)) =
                direct_exports.or(module_exports).or(alias_exports)
            {
                if !pending_props.contains_key(&name_text) {
                    ordered_names.push(name_text.clone());
                }
                pending_props
                    .entry(name_text)
                    .or_default()
                    .push((binary.right, expando_root));
            }
        }

        self.collect_direct_commonjs_assignment_exports(
            arena,
            binary.right,
            pending_props,
            ordered_names,
            export_aliases,
        );
    }

    fn collect_late_bound_commonjs_assignment_candidate(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        property_name: &str,
        read_pos: u32,
        export_aliases: &FxHashSet<String>,
        best_match: &mut Option<(u32, NodeIndex, Option<String>)>,
    ) {
        let Some(node) = arena.get(expr_idx) else {
            return;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = arena.get_binary_expr(node) else {
            return;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return;
        }

        let direct_exports = arena
            .get(binary.left)
            .filter(|left_node| {
                left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .and_then(|left_node| arena.get_access_expr(left_node))
            .and_then(|left_access| {
                arena
                    .get_identifier_at(left_access.expression)
                    .and_then(|ident| {
                        (ident.escaped_text == "exports").then(|| {
                            Self::commonjs_static_member_name_in_arena(
                                arena,
                                left_access.name_or_argument,
                            )
                            .map(|name| (name, None))
                        })
                    })
            })
            .flatten();

        let module_exports = arena
            .get(binary.left)
            .filter(|left_node| {
                left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .and_then(|left_node| arena.get_access_expr(left_node))
            .and_then(|left_access| {
                arena
                    .get(left_access.expression)
                    .and_then(|container_node| {
                        (container_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
                            .then(|| arena.get_access_expr(container_node))
                            .flatten()
                            .and_then(|container_access| {
                                let base_is_module = arena
                                    .get_identifier_at(container_access.expression)
                                    .is_some_and(|ident| ident.escaped_text == "module");
                                let member_is_exports = Self::commonjs_static_member_name_in_arena(
                                    arena,
                                    container_access.name_or_argument,
                                )
                                .is_some_and(|name| name == "exports");
                                (base_is_module && member_is_exports).then(|| {
                                    let expando_root = arena
                                        .get_identifier_at(left_access.expression)
                                        .map(|ident| ident.escaped_text.clone());
                                    Self::commonjs_static_member_name_in_arena(
                                        arena,
                                        left_access.name_or_argument,
                                    )
                                    .map(|name| (name, expando_root))
                                })
                            })
                    })
            })
            .flatten();

        let alias_exports = if direct_exports.is_none() && module_exports.is_none() {
            arena
                .get(binary.left)
                .filter(|left_node| {
                    left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                })
                .and_then(|left_node| arena.get_access_expr(left_node))
                .and_then(|left_access| {
                    arena
                        .get_identifier_at(left_access.expression)
                        .and_then(|ident| {
                            export_aliases
                                .contains(ident.escaped_text.as_str())
                                .then(|| {
                                    Self::commonjs_static_member_name_in_arena(
                                        arena,
                                        left_access.name_or_argument,
                                    )
                                    .map(|name| (name, None))
                                })
                        })
                })
                .flatten()
        } else {
            None
        };

        if let Some((name_text, expando_root)) = direct_exports.or(module_exports).or(alias_exports)
            && name_text == property_name
            && node.pos > read_pos
            && best_match
                .as_ref()
                .is_none_or(|(best_pos, _, _)| node.pos >= *best_pos)
        {
            *best_match = Some((node.pos, binary.right, expando_root));
        }

        self.collect_late_bound_commonjs_assignment_candidate(
            arena,
            binary.right,
            property_name,
            read_pos,
            export_aliases,
            best_match,
        );
    }

    fn collect_future_commonjs_assignment_candidates(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        property_name: &str,
        read_pos: u32,
        export_aliases: &FxHashSet<String>,
        candidates: &mut Vec<(u32, NodeIndex, Option<String>)>,
    ) {
        let Some(node) = arena.get(expr_idx) else {
            return;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = arena.get_binary_expr(node) else {
            return;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return;
        }

        let direct_exports = arena
            .get(binary.left)
            .filter(|left_node| {
                left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .and_then(|left_node| arena.get_access_expr(left_node))
            .and_then(|left_access| {
                arena
                    .get_identifier_at(left_access.expression)
                    .and_then(|ident| {
                        (ident.escaped_text == "exports").then(|| {
                            Self::commonjs_static_member_name_in_arena(
                                arena,
                                left_access.name_or_argument,
                            )
                            .map(|name| (name, None))
                        })
                    })
            })
            .flatten();

        let module_exports = arena
            .get(binary.left)
            .filter(|left_node| {
                left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .and_then(|left_node| arena.get_access_expr(left_node))
            .and_then(|left_access| {
                arena
                    .get(left_access.expression)
                    .filter(|target_node| {
                        target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                            || target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    })
                    .and_then(|target_node| arena.get_access_expr(target_node))
                    .and_then(|target_access| {
                        let is_module_exports = if let Some(target_node) =
                            arena.get(left_access.expression)
                        {
                            if target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                                arena
                                    .get_identifier_at(target_access.expression)
                                    .is_some_and(|ident| ident.escaped_text == "module")
                                    && arena
                                        .get_identifier_at(target_access.name_or_argument)
                                        .is_some_and(|ident| ident.escaped_text == "exports")
                            } else if target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                            {
                                arena
                                    .get_identifier_at(target_access.expression)
                                    .is_some_and(|ident| ident.escaped_text == "module")
                                    && Self::commonjs_static_member_name_in_arena(
                                        arena,
                                        target_access.name_or_argument,
                                    )
                                    .is_some_and(|name| name == "exports")
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        is_module_exports.then(|| {
                            Self::commonjs_static_member_name_in_arena(
                                arena,
                                left_access.name_or_argument,
                            )
                            .map(|name| (name, None))
                        })?
                    })
            });

        let alias_exports = if direct_exports.is_none() && module_exports.is_none() {
            arena
                .get(binary.left)
                .filter(|left_node| {
                    left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                })
                .and_then(|left_node| arena.get_access_expr(left_node))
                .and_then(|left_access| {
                    arena
                        .get_identifier_at(left_access.expression)
                        .and_then(|ident| {
                            export_aliases
                                .contains(ident.escaped_text.as_str())
                                .then(|| {
                                    Self::commonjs_static_member_name_in_arena(
                                        arena,
                                        left_access.name_or_argument,
                                    )
                                    .map(|name| (name, None))
                                })
                        })
                })
                .flatten()
        } else {
            None
        };

        if let Some((name_text, expando_root)) = direct_exports.or(module_exports).or(alias_exports)
            && name_text == property_name
            && node.pos > read_pos
        {
            candidates.push((node.pos, binary.right, expando_root));
        }

        self.collect_future_commonjs_assignment_candidates(
            arena,
            binary.right,
            property_name,
            read_pos,
            export_aliases,
            candidates,
        );
    }

    fn collect_prior_commonjs_assignment_candidate(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        property_name: &str,
        read_pos: u32,
        export_aliases: &FxHashSet<String>,
        best_match: &mut Option<(u32, NodeIndex, Option<String>)>,
    ) {
        let Some(node) = arena.get(expr_idx) else {
            return;
        };
        if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = arena.get_binary_expr(node) else {
            return;
        };
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return;
        }

        let direct_exports = arena
            .get(binary.left)
            .filter(|left_node| {
                left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .and_then(|left_node| arena.get_access_expr(left_node))
            .and_then(|left_access| {
                arena
                    .get_identifier_at(left_access.expression)
                    .and_then(|ident| {
                        (ident.escaped_text == "exports").then(|| {
                            Self::commonjs_static_member_name_in_arena(
                                arena,
                                left_access.name_or_argument,
                            )
                            .map(|name| (name, None))
                        })
                    })
            })
            .flatten();

        let module_exports = arena
            .get(binary.left)
            .filter(|left_node| {
                left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            })
            .and_then(|left_node| arena.get_access_expr(left_node))
            .and_then(|left_access| {
                arena
                    .get(left_access.expression)
                    .filter(|target_node| {
                        target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                            || target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                    })
                    .and_then(|target_node| arena.get_access_expr(target_node))
                    .and_then(|target_access| {
                        let is_module_exports = if let Some(target_node) =
                            arena.get(left_access.expression)
                        {
                            if target_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                                arena
                                    .get_identifier_at(target_access.expression)
                                    .is_some_and(|ident| ident.escaped_text == "module")
                                    && arena
                                        .get_identifier_at(target_access.name_or_argument)
                                        .is_some_and(|ident| ident.escaped_text == "exports")
                            } else if target_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                            {
                                arena
                                    .get_identifier_at(target_access.expression)
                                    .is_some_and(|ident| ident.escaped_text == "module")
                                    && Self::commonjs_static_member_name_in_arena(
                                        arena,
                                        target_access.name_or_argument,
                                    )
                                    .is_some_and(|name| name == "exports")
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        is_module_exports.then(|| {
                            Self::commonjs_static_member_name_in_arena(
                                arena,
                                left_access.name_or_argument,
                            )
                            .map(|name| (name, None))
                        })?
                    })
            });

        let alias_exports = if direct_exports.is_none() && module_exports.is_none() {
            arena
                .get(binary.left)
                .filter(|left_node| {
                    left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || left_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                })
                .and_then(|left_node| arena.get_access_expr(left_node))
                .and_then(|left_access| {
                    arena
                        .get_identifier_at(left_access.expression)
                        .and_then(|ident| {
                            export_aliases
                                .contains(ident.escaped_text.as_str())
                                .then(|| {
                                    Self::commonjs_static_member_name_in_arena(
                                        arena,
                                        left_access.name_or_argument,
                                    )
                                    .map(|name| (name, None))
                                })
                        })
                })
                .flatten()
        } else {
            None
        };

        if let Some((name_text, expando_root)) = direct_exports.or(module_exports).or(alias_exports)
            && name_text == property_name
            && node.pos < read_pos
            && best_match
                .as_ref()
                .is_none_or(|(best_pos, _, _)| node.pos >= *best_pos)
        {
            *best_match = Some((node.pos, binary.right, expando_root));
        }

        self.collect_prior_commonjs_assignment_candidate(
            arena,
            binary.right,
            property_name,
            read_pos,
            export_aliases,
            best_match,
        );
    }

    pub(crate) fn infer_commonjs_export_rhs_type(
        &mut self,
        target_file_idx: usize,
        rhs_expr: NodeIndex,
        expando_root: Option<&str>,
    ) -> TypeId {
        if target_file_idx == self.ctx.current_file_idx {
            let mut ty = self
                .literal_type_from_initializer(rhs_expr)
                .or_else(|| self.commonjs_export_rhs_symbol_type(rhs_expr))
                .unwrap_or_else(|| self.get_type_of_node(rhs_expr));
            ty = self.upgrade_commonjs_export_constructor_type(rhs_expr, ty);
            ty = self.augment_commonjs_export_object_type_with_expandos(
                target_file_idx,
                expando_root,
                ty,
            );
            ty = self.augment_commonjs_export_callable_type_with_expandos(
                target_file_idx,
                expando_root,
                ty,
            );
            return crate::query_boundaries::common::widen_freshness(self.ctx.types, ty);
        }

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return TypeId::ANY;
        };
        let Some(all_binders) = self.ctx.all_binders.clone() else {
            return TypeId::ANY;
        };
        let Some(arena) = all_arenas.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(binder) = all_binders.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(source_file) = arena.source_files.first() else {
            return TypeId::ANY;
        };

        let mut checker = Box::new(CheckerState::with_parent_cache(
            arena.as_ref(),
            binder.as_ref(),
            self.ctx.types,
            source_file.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.current_file_idx = target_file_idx;
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);

        let mut ty = checker
            .literal_type_from_initializer(rhs_expr)
            .or_else(|| checker.commonjs_export_rhs_symbol_type(rhs_expr))
            .unwrap_or_else(|| checker.get_type_of_node(rhs_expr));
        ty = checker.upgrade_commonjs_export_constructor_type(rhs_expr, ty);
        ty = checker.augment_commonjs_export_object_type_with_expandos(
            target_file_idx,
            expando_root,
            ty,
        );
        ty = checker.augment_commonjs_export_callable_type_with_expandos(
            target_file_idx,
            expando_root,
            ty,
        );
        ty = crate::query_boundaries::common::widen_freshness(checker.ctx.types, ty);
        ty = if crate::query_boundaries::common::is_unique_symbol_type(checker.ctx.types, ty) {
            ty
        } else {
            crate::query_boundaries::common::widen_type(checker.ctx.types, ty)
        };
        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        ty
    }

    pub(crate) fn current_file_commonjs_late_bound_named_export_type(
        &mut self,
        property_name: &str,
        read_pos: u32,
    ) -> Option<TypeId> {
        if self
            .current_file_commonjs_prior_named_export_type(property_name, read_pos)
            .is_some_and(|prior_type| prior_type != TypeId::UNDEFINED)
        {
            return Some(TypeId::ANY);
        }

        let target_file_idx = self.ctx.current_file_idx;
        let target_arena = self.ctx.arena.clone();
        let source_file = target_arena.source_files.first()?;
        let export_aliases = Self::collect_commonjs_export_aliases_in_arena(&target_arena);
        let mut best_match: Option<(u32, NodeIndex, Option<String>)> = None;
        let mut candidates: Vec<(u32, NodeIndex, Option<String>)> = Vec::new();

        let mut all_stmts: Vec<NodeIndex> = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            all_stmts.push(stmt_idx);
            if let Some(stmt_node) = target_arena.get(stmt_idx)
                && stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(stmt) = target_arena.get_expression_statement(stmt_node)
                && let Some(iife_stmts) =
                    Self::get_iife_body_statements(&target_arena, stmt.expression)
            {
                all_stmts.extend_from_slice(iife_stmts);
            }
        }

        for stmt_idx in all_stmts {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            self.collect_future_commonjs_assignment_candidates(
                &target_arena,
                stmt.expression,
                property_name,
                read_pos,
                &export_aliases,
                &mut candidates,
            );
            self.collect_late_bound_commonjs_assignment_candidate(
                &target_arena,
                stmt.expression,
                property_name,
                read_pos,
                &export_aliases,
                &mut best_match,
            );
        }

        let (_, rhs_expr, expando_root) = best_match?;
        let rhs_type =
            self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr, expando_root.as_deref());
        if rhs_type == TypeId::UNDEFINED {
            return None;
        }

        let expected_widened = crate::query_boundaries::common::widen_literal_type(
            self.ctx.types,
            crate::query_boundaries::common::widen_freshness(self.ctx.types, rhs_type),
        );
        for (_, candidate_rhs, candidate_root) in candidates {
            let candidate_type = self.infer_commonjs_export_rhs_type(
                target_file_idx,
                candidate_rhs,
                candidate_root.as_deref(),
            );
            if candidate_type == TypeId::UNDEFINED {
                continue;
            }
            let candidate_widened = crate::query_boundaries::common::widen_literal_type(
                self.ctx.types,
                crate::query_boundaries::common::widen_freshness(self.ctx.types, candidate_type),
            );
            if candidate_widened != expected_widened {
                return Some(TypeId::ANY);
            }
        }

        Some(rhs_type)
    }

    pub(crate) fn current_file_commonjs_prior_named_export_type(
        &mut self,
        property_name: &str,
        read_pos: u32,
    ) -> Option<TypeId> {
        let target_file_idx = self.ctx.current_file_idx;
        let target_arena = self.ctx.arena.clone();
        let source_file = target_arena.source_files.first()?;
        let export_aliases = Self::collect_commonjs_export_aliases_in_arena(&target_arena);
        let mut best_match: Option<(u32, NodeIndex, Option<String>)> = None;

        let mut all_stmts: Vec<NodeIndex> = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            all_stmts.push(stmt_idx);
            if let Some(stmt_node) = target_arena.get(stmt_idx)
                && stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(stmt) = target_arena.get_expression_statement(stmt_node)
                && let Some(iife_stmts) =
                    Self::get_iife_body_statements(&target_arena, stmt.expression)
            {
                all_stmts.extend_from_slice(iife_stmts);
            }
        }

        for stmt_idx in all_stmts {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            self.collect_prior_commonjs_assignment_candidate(
                &target_arena,
                stmt.expression,
                property_name,
                read_pos,
                &export_aliases,
                &mut best_match,
            );
        }

        let (_, rhs_expr, expando_root) = best_match?;
        Some(self.infer_commonjs_export_rhs_type(
            target_file_idx,
            rhs_expr,
            expando_root.as_deref(),
        ))
    }

    pub(super) fn commonjs_string_literal_rhs_type(
        &mut self,
        target_file_idx: usize,
        rhs_expr: NodeIndex,
    ) -> Option<TypeId> {
        let literal = if target_file_idx == self.ctx.current_file_idx {
            self.literal_type_from_initializer(rhs_expr)
        } else {
            let all_arenas = self.ctx.all_arenas.clone()?;
            let all_binders = self.ctx.all_binders.clone()?;
            let arena = all_arenas.get(target_file_idx)?;
            let binder = all_binders.get(target_file_idx)?;
            let source_file = arena.source_files.first()?;

            let mut checker = Box::new(CheckerState::with_parent_cache(
                arena.as_ref(),
                binder.as_ref(),
                self.ctx.types,
                source_file.file_name.clone(),
                self.ctx.compiler_options.clone(),
                self,
            ));
            checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
            checker.ctx.copy_cross_file_state_from(&self.ctx);
            checker.ctx.current_file_idx = target_file_idx;
            self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);
            checker.literal_type_from_initializer(rhs_expr)
        }?;

        crate::query_boundaries::common::string_literal_value(self.ctx.types, literal)
            .is_some()
            .then_some(literal)
    }

    fn augment_commonjs_export_object_type_with_expandos(
        &mut self,
        target_file_idx: usize,
        expando_root: Option<&str>,
        base_type: TypeId,
    ) -> TypeId {
        use rustc_hash::FxHashMap;
        use tsz_solver::ObjectShape;

        let Some(root_name) = expando_root else {
            return base_type;
        };
        let expando_props =
            self.collect_commonjs_expando_property_types_for_root(target_file_idx, root_name);
        if expando_props.is_empty() {
            return base_type;
        }

        let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, base_type)
        else {
            return base_type;
        };

        let mut properties: FxHashMap<tsz_common::interner::Atom, PropertyInfo> = shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();
        let mut changed = false;

        for (prop_name, prop_type) in expando_props {
            let prop_type =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, prop_type);
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if properties.contains_key(&prop_atom) {
                continue;
            }

            properties.insert(
                prop_atom,
                PropertyInfo {
                    name: prop_atom,
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: properties.len() as u32,
                    is_string_named: false,
                },
            );
            changed = true;
        }

        if !changed {
            return base_type;
        }

        self.ctx.types.factory().object_with_index(ObjectShape {
            flags: shape.flags,
            properties: properties.into_values().collect(),
            string_index: shape.string_index,
            number_index: shape.number_index,
            symbol: shape.symbol,
        })
    }

    fn augment_commonjs_export_callable_type_with_expandos(
        &mut self,
        target_file_idx: usize,
        expando_root: Option<&str>,
        base_type: TypeId,
    ) -> TypeId {
        use rustc_hash::FxHashMap;
        use tsz_solver::{CallableShape, PropertyInfo};

        let Some(root_name) = expando_root else {
            return base_type;
        };
        let expando_props =
            self.collect_commonjs_expando_property_types_for_root(target_file_idx, root_name);
        if expando_props.is_empty() {
            return base_type;
        }

        let (mut callable_shape, mut property_count) = if let Some(shape) =
            crate::query_boundaries::common::callable_shape_for_type(self.ctx.types, base_type)
        {
            ((*shape).clone(), shape.properties.len())
        } else if let Some(function_shape) =
            crate::query_boundaries::type_computation::complex::get_function_shape(
                self.ctx.types,
                base_type,
            )
        {
            let signature = tsz_solver::CallSignature {
                type_params: function_shape.type_params.clone(),
                params: function_shape.params.clone(),
                this_type: function_shape.this_type,
                return_type: function_shape.return_type,
                type_predicate: function_shape.type_predicate,
                is_method: function_shape.is_method,
            };
            (
                CallableShape {
                    call_signatures: if function_shape.is_constructor {
                        Vec::new()
                    } else {
                        vec![signature.clone()]
                    },
                    construct_signatures: if function_shape.is_constructor {
                        vec![signature]
                    } else {
                        Vec::new()
                    },
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                },
                0,
            )
        } else {
            return base_type;
        };

        let mut properties: FxHashMap<tsz_common::interner::Atom, PropertyInfo> = callable_shape
            .properties
            .iter()
            .map(|prop| (prop.name, prop.clone()))
            .collect();
        let mut changed = false;

        for (prop_name, prop_type) in expando_props {
            let prop_type =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, prop_type);
            let prop_atom = self.ctx.types.intern_string(&prop_name);
            if let Some(existing) = properties.get_mut(&prop_atom) {
                let existing_is_placeholder = matches!(
                    existing.type_id,
                    TypeId::ANY | TypeId::UNKNOWN | TypeId::ERROR
                );
                if existing_is_placeholder && !matches!(prop_type, TypeId::ANY | TypeId::UNKNOWN) {
                    existing.type_id = prop_type;
                    existing.write_type = prop_type;
                    changed = true;
                }
                continue;
            }
            properties.insert(
                prop_atom,
                PropertyInfo {
                    name: prop_atom,
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: property_count as u32,
                    is_string_named: false,
                },
            );
            property_count += 1;
            changed = true;
        }

        if !changed {
            return base_type;
        }

        callable_shape.properties = properties.into_values().collect();
        self.ctx.types.factory().callable(callable_shape)
    }

    fn collect_commonjs_expando_property_types_for_root(
        &mut self,
        target_file_idx: usize,
        root_name: &str,
    ) -> Vec<(String, TypeId)> {
        use rustc_hash::FxHashMap;

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32).clone();
        let Some(source_file) = target_arena.source_files.first() else {
            return Vec::new();
        };

        let mut props: FxHashMap<String, TypeId> = FxHashMap::default();
        for &stmt_idx in &source_file.statements.nodes {
            self.collect_commonjs_expando_property_types_from_node(
                target_file_idx,
                &target_arena,
                stmt_idx,
                root_name,
                &mut props,
            );
        }

        props.into_iter().collect()
    }

    fn collect_commonjs_expando_property_types_from_node(
        &mut self,
        target_file_idx: usize,
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
        root_name: &str,
        props: &mut rustc_hash::FxHashMap<String, TypeId>,
    ) {
        let Some(node) = arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && let Some(access_key) =
                Self::commonjs_expando_assignment_access_key(arena, binary.left)
            && let Some(prop_name) = access_key.strip_prefix(root_name)
            && let Some(prop_name) = prop_name.strip_prefix('.')
            && !prop_name.is_empty()
            && !prop_name.contains('.')
        {
            let prop_type =
                self.infer_commonjs_export_rhs_type(target_file_idx, binary.right, None);
            props.insert(prop_name.to_string(), prop_type);
        }

        for child_idx in arena.get_children(idx) {
            self.collect_commonjs_expando_property_types_from_node(
                target_file_idx,
                arena,
                child_idx,
                root_name,
                props,
            );
        }
    }

    fn commonjs_expando_assignment_access_key(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.clone()),
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = arena.get_access_expr(node)?;
                let left = Self::commonjs_expando_assignment_access_key(arena, access.expression)?;
                let right = arena.get_identifier_at(access.name_or_argument)?;
                Some(format!("{left}.{}", right.escaped_text))
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = arena.get_access_expr(node)?;
                let left = Self::commonjs_expando_assignment_access_key(arena, access.expression)?;
                let right =
                    Self::commonjs_static_member_name_in_arena(arena, access.name_or_argument)?;
                Some(format!("{left}.{right}"))
            }
            _ => None,
        }
    }

    pub(super) fn infer_commonjs_descriptor_method_type(
        &mut self,
        target_file_idx: usize,
        method_idx: NodeIndex,
        contextual_type: Option<TypeId>,
    ) -> TypeId {
        if target_file_idx == self.ctx.current_file_idx {
            let request = TypingRequest::NONE.contextual_opt(contextual_type);
            return self.get_type_of_function_impl(method_idx, &request);
        }

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return TypeId::ANY;
        };
        let Some(all_binders) = self.ctx.all_binders.clone() else {
            return TypeId::ANY;
        };
        let Some(arena) = all_arenas.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(binder) = all_binders.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(source_file) = arena.source_files.first() else {
            return TypeId::ANY;
        };

        let mut checker = Box::new(CheckerState::with_parent_cache(
            arena.as_ref(),
            binder.as_ref(),
            self.ctx.types,
            source_file.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.current_file_idx = target_file_idx;
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);

        let request = TypingRequest::NONE.contextual_opt(contextual_type);
        let ty = checker.get_type_of_function_impl(method_idx, &request);
        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        ty
    }
}
