use crate::context::TypingRequest;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommonJsExportTargetRoot {
    Exports,
    ModuleExports,
    Alias,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommonJsExportAssignmentTarget {
    member_name: String,
    root: CommonJsExportTargetRoot,
}

impl CommonJsExportAssignmentTarget {
    fn direct_assignment_expando_root(&self) -> Option<String> {
        match self.root {
            CommonJsExportTargetRoot::Exports => Some(format!("exports.{}", self.member_name)),
            CommonJsExportTargetRoot::ModuleExports => {
                Some(format!("module.exports.{}", self.member_name))
            }
            CommonJsExportTargetRoot::Alias => None,
        }
    }
}

impl<'a> CheckerState<'a> {
    pub(super) fn collect_direct_commonjs_assignment_exports(
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

        if let Some(target) =
            Self::commonjs_export_assignment_target(arena, binary.left, export_aliases)
        {
            let expando_root = target.direct_assignment_expando_root();
            let member_name = target.member_name;
            if !pending_props.contains_key(&member_name) {
                ordered_names.push(member_name.clone());
            }
            pending_props
                .entry(member_name)
                .or_default()
                .push((binary.right, expando_root));
        }

        Self::collect_direct_commonjs_assignment_exports(
            arena,
            binary.right,
            pending_props,
            ordered_names,
            export_aliases,
        );
    }

    fn collect_late_bound_commonjs_assignment_candidate(
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

        if let Some(target) =
            Self::commonjs_export_assignment_target(arena, binary.left, export_aliases)
            && target.member_name == property_name
            && node.pos > read_pos
            && best_match
                .as_ref()
                .is_none_or(|(best_pos, _, _)| node.pos >= *best_pos)
        {
            *best_match = Some((node.pos, binary.right, None));
        }

        Self::collect_late_bound_commonjs_assignment_candidate(
            arena,
            binary.right,
            property_name,
            read_pos,
            export_aliases,
            best_match,
        );
    }

    /// Classify a binary-expression LHS as a CommonJS export target.
    ///
    /// Returns a target descriptor when the LHS matches:
    /// - `exports.<name>` / `exports["<name>"]`
    /// - `module.exports.<name>` / `module["exports"].<name>`
    /// - `<alias>.<name>` where `alias` is in `export_aliases`
    fn commonjs_export_assignment_target(
        arena: &tsz_parser::parser::NodeArena,
        binary_left: NodeIndex,
        export_aliases: &FxHashSet<String>,
    ) -> Option<CommonJsExportAssignmentTarget> {
        let left_node = arena.get(binary_left).filter(|n| {
            n.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || n.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        })?;
        let left_access = arena.get_access_expr(left_node)?;
        let expr_idx = left_access.expression;
        let name_or_arg = left_access.name_or_argument;

        // exports.<name> / exports["<name>"]
        if let Some(ident) = arena.get_identifier_at(expr_idx)
            && ident.escaped_text == "exports"
        {
            return Self::commonjs_static_member_name_in_arena(arena, name_or_arg).map(|name| {
                CommonJsExportAssignmentTarget {
                    member_name: name,
                    root: CommonJsExportTargetRoot::Exports,
                }
            });
        }

        // module.exports.<name> / module["exports"].<name>
        if let Some(container_node) = arena.get(expr_idx)
            && (container_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || container_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(container_access) = arena.get_access_expr(container_node)
        {
            let is_module_exports =
                if container_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    arena
                        .get_identifier_at(container_access.expression)
                        .is_some_and(|i| i.escaped_text == "module")
                        && arena
                            .get_identifier_at(container_access.name_or_argument)
                            .is_some_and(|i| i.escaped_text == "exports")
                } else {
                    arena
                        .get_identifier_at(container_access.expression)
                        .is_some_and(|i| i.escaped_text == "module")
                        && Self::commonjs_static_member_name_in_arena(
                            arena,
                            container_access.name_or_argument,
                        )
                        .is_some_and(|n| n == "exports")
                };
            if is_module_exports {
                return Self::commonjs_static_member_name_in_arena(arena, name_or_arg).map(
                    |name| CommonJsExportAssignmentTarget {
                        member_name: name,
                        root: CommonJsExportTargetRoot::ModuleExports,
                    },
                );
            }
        }

        // <alias>.<name> where alias is in export_aliases
        arena
            .get_identifier_at(expr_idx)
            .and_then(|ident| {
                export_aliases
                    .contains(ident.escaped_text.as_str())
                    .then(|| {
                        Self::commonjs_static_member_name_in_arena(arena, name_or_arg).map(|name| {
                            CommonJsExportAssignmentTarget {
                                member_name: name,
                                root: CommonJsExportTargetRoot::Alias,
                            }
                        })
                    })
            })
            .flatten()
    }

    fn collect_future_commonjs_assignment_candidates(
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

        if let Some(target) =
            Self::commonjs_export_assignment_target(arena, binary.left, export_aliases)
            && target.member_name == property_name
            && node.pos > read_pos
        {
            candidates.push((node.pos, binary.right, None));
        }

        Self::collect_future_commonjs_assignment_candidates(
            arena,
            binary.right,
            property_name,
            read_pos,
            export_aliases,
            candidates,
        );
    }

    fn collect_prior_commonjs_assignment_candidate(
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

        if let Some(target) =
            Self::commonjs_export_assignment_target(arena, binary.left, export_aliases)
            && target.member_name == property_name
            && node.pos < read_pos
            && best_match
                .as_ref()
                .is_none_or(|(best_pos, _, _)| node.pos >= *best_pos)
        {
            *best_match = Some((node.pos, binary.right, None));
        }

        Self::collect_prior_commonjs_assignment_candidate(
            arena,
            binary.right,
            property_name,
            read_pos,
            export_aliases,
            best_match,
        );
    }

    /// Follow an assignment chain to the value actually exported.
    ///
    /// `exports = module.exports = C` assigns through an intermediate target.
    /// An assignment expression takes its target's declared type, so an ambient
    /// `declare var module: { exports: any }` — what node typings and
    /// hand-written shims provide — makes the chain resolve to an error type
    /// rather than to `C`. tsc collects the assigned value, keeping the real
    /// export type, so every `exports.<name>` is still checked against it.
    fn commonjs_export_rhs_through_assignment_chain(&self, rhs_expr: NodeIndex) -> NodeIndex {
        let mut current = rhs_expr;
        for _ in 0..8 {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                break;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                break;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                break;
            }
            current = binary.right;
        }
        current
    }

    /// Whether `property_name` has at least one direct `exports.NAME` /
    /// `module.exports.NAME` assignment in this file whose RHS is not an
    /// "aliasable expression" (an entity-name expression `a`/`a.b.c`, or a
    /// class expression).
    ///
    /// tsc's binder (`isAliasableExpression`, `bindExportsPropertyAssignment`)
    /// gives an aliasable-RHS export assignment `SymbolFlags.Alias`; alias
    /// reads resolve to the aliased declaration's own widened type and never
    /// go through flow-sensitive "used before assigned" analysis, so they are
    /// unordered regardless of textual position — this is the existing
    /// `commonjs_exports_is_not_ordered` behavior. Any other RHS shape
    /// (function expression, arrow function, object literal, ...) is instead
    /// bound as a real `Property` declaration with its own flow-assignment
    /// node, so a read that precedes it is ordered and reports TS2565, e.g.
    /// `module.exports.jj = module.exports.j; module.exports.j = function
    /// j() {};` (oracle-verified, `tsc` 6.0.2).
    pub(crate) fn commonjs_export_property_has_non_aliasable_assignment(
        &self,
        property_name: &str,
    ) -> bool {
        let arena = self.ctx.arena;
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let export_aliases = Self::collect_commonjs_export_aliases_in_arena(arena);
        let mut pending_props: FxHashMap<String, Vec<(NodeIndex, Option<String>)>> =
            FxHashMap::default();
        let mut ordered_names: Vec<String> = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = arena.get_expression_statement(stmt_node) else {
                continue;
            };
            Self::collect_direct_commonjs_assignment_exports(
                arena,
                stmt.expression,
                &mut pending_props,
                &mut ordered_names,
                &export_aliases,
            );
        }
        pending_props.get(property_name).is_some_and(|assignments| {
            assignments
                .iter()
                .any(|(rhs_idx, _)| !self.commonjs_export_assignment_rhs_is_aliasable(*rhs_idx))
        })
    }

    fn commonjs_export_assignment_rhs_is_aliasable(&self, rhs_idx: NodeIndex) -> bool {
        self.is_entity_name_expression(rhs_idx)
            || self
                .ctx
                .arena
                .get(rhs_idx)
                .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_EXPRESSION)
    }

    /// The declared type from a JSDoc `@type` tag leading the `module.exports
    /// = X` / `exports = X` statement that owns `rhs_expr`, if any.
    ///
    /// tsc treats `@type` on the assignment statement as the export's
    /// declared type (like a `: T` annotation on a variable declaration) —
    /// the RHS is contextually typed against it and the *declared* type
    /// becomes the type of later `module.exports` reads, not a structural
    /// re-inference of the initializer. `assignment_ops.rs` already applies
    /// this for the assignment statement's own excess-property checks; this
    /// mirrors it for the export surface other statements read back.
    pub(crate) fn commonjs_export_rhs_jsdoc_declared_type(
        &mut self,
        rhs_expr: NodeIndex,
    ) -> Option<TypeId> {
        let stmt_idx = self.enclosing_expression_statement(rhs_expr)?;
        let declared_type = self.js_statement_declared_type(stmt_idx)?;
        (declared_type != TypeId::ERROR).then_some(declared_type)
    }

    pub(crate) fn infer_commonjs_export_rhs_type(
        &mut self,
        target_file_idx: usize,
        rhs_expr: NodeIndex,
        expando_root: Option<&str>,
    ) -> TypeId {
        if target_file_idx == self.ctx.current_file_idx {
            let mut ty = self
                .commonjs_export_rhs_jsdoc_declared_type(rhs_expr)
                .or_else(|| self.literal_type_from_initializer(rhs_expr))
                .or_else(|| self.commonjs_export_rhs_symbol_type(rhs_expr))
                .unwrap_or_else(|| self.get_type_of_node(rhs_expr));
            // An error type here means the chain's intermediate target carried
            // an ambient declaration, not that the module exports nothing
            // usable — the error renders as `any`, which then accepts every
            // property access silently. Recover the assigned value's type.
            // Deliberately re-typing the node rather than consulting
            // `commonjs_export_rhs_symbol_type`: for `exports = module.exports
            // = C` the latter answers with C's *instance* type, while tsc
            // reports the callable `() => void`.
            // The degenerate value differs by path: the chain types as an error
            // when only the ambient declaration is in play, and as `any` once
            // the surrounding method bodies are checked. Both mean the same
            // thing here — the intermediate target's declaration won, not the
            // module's real export — so rescue either.
            if ty == TypeId::ERROR
                || crate::query_boundaries::assignability::is_any_type(self.ctx.types, ty)
            {
                let inner = self.commonjs_export_rhs_through_assignment_chain(rhs_expr);
                if inner != rhs_expr {
                    let inner_ty = self.get_type_of_node(inner);
                    if inner_ty != TypeId::ERROR {
                        ty = inner_ty;
                    }
                }
            }
            ty = self.augment_commonjs_export_type_with_expandos(target_file_idx, expando_root, ty);
            ty = self.widen_fresh_object_literal_properties_for_display(ty);
            return crate::query_boundaries::common::widen_freshness(self.ctx.types, ty);
        }

        self.with_commonjs_child_checker_for_file(target_file_idx, |checker| {
            let mut ty = checker
                .commonjs_export_rhs_jsdoc_declared_type(rhs_expr)
                .or_else(|| checker.literal_type_from_initializer(rhs_expr))
                .or_else(|| checker.commonjs_export_rhs_symbol_type(rhs_expr))
                .unwrap_or_else(|| checker.get_type_of_node(rhs_expr));
            ty = checker.augment_commonjs_export_type_with_expandos(
                target_file_idx,
                expando_root,
                ty,
            );
            ty = checker.widen_fresh_object_literal_properties_for_display(ty);
            ty = crate::query_boundaries::common::widen_freshness(checker.ctx.types, ty);
            if crate::query_boundaries::common::is_unique_symbol_type(checker.ctx.types, ty) {
                ty
            } else {
                crate::query_boundaries::common::widen_type(checker.ctx.types, ty)
            }
        })
        .unwrap_or(TypeId::ANY)
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
            Self::collect_future_commonjs_assignment_candidates(
                &target_arena,
                stmt.expression,
                property_name,
                read_pos,
                &export_aliases,
                &mut candidates,
            );
            Self::collect_late_bound_commonjs_assignment_candidate(
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
            let mut candidate_type: Option<TypeId> = None;
            for (_, candidate_rhs, candidate_root) in candidates {
                let ty = self.infer_commonjs_export_rhs_type(
                    target_file_idx,
                    candidate_rhs,
                    candidate_root.as_deref(),
                );
                if ty == TypeId::UNDEFINED {
                    continue;
                }
                candidate_type = match candidate_type {
                    None => Some(ty),
                    Some(existing) if existing == ty => Some(existing),
                    Some(_) => Some(TypeId::ANY),
                };
            }
            return candidate_type;
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

    /// Declaration-level type of a CommonJS named export in the current file.
    ///
    /// `tsc` types `exports.x` from every assignment in the module, not only
    /// those textually preceding the use. `exports.f = undefined; exports.f();
    /// … exports.f = fn` reports nothing, because the declared type is `fn`, and
    /// a call written before any assignment is likewise fine. Selecting with an
    /// unbounded read position therefore takes the last assignment in the file
    /// rather than the last one above the use.
    pub(crate) fn current_file_commonjs_named_export_type(
        &mut self,
        property_name: &str,
    ) -> Option<TypeId> {
        self.current_file_commonjs_prior_named_export_type(property_name, u32::MAX)
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
            Self::collect_prior_commonjs_assignment_candidate(
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
            self.with_commonjs_child_checker_for_file_without_merge(target_file_idx, |checker| {
                checker.literal_type_from_initializer(rhs_expr)
            })?
        }?;

        crate::query_boundaries::common::string_literal_value(self.ctx.types, literal)
            .is_some()
            .then_some(literal)
    }

    fn augment_commonjs_export_type_with_expandos(
        &mut self,
        target_file_idx: usize,
        expando_root: Option<&str>,
        base_type: TypeId,
    ) -> TypeId {
        let Some(root_name) = expando_root else {
            return base_type;
        };
        let expando_members =
            self.collect_commonjs_expando_property_types_for_root(target_file_idx, root_name);
        if expando_members.is_empty() {
            return base_type;
        }

        crate::query_boundaries::js_exports::commonjs_export_type_with_expando_members(
            self.ctx.types,
            base_type,
            &expando_members,
        )
    }

    fn collect_commonjs_expando_property_types_for_root(
        &mut self,
        target_file_idx: usize,
        root_name: &str,
    ) -> Vec<crate::query_boundaries::js_exports::CommonJsExpandoMember> {
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

        props
            .into_iter()
            .map(
                |(name, type_id)| crate::query_boundaries::js_exports::CommonJsExpandoMember {
                    name,
                    type_id,
                },
            )
            .collect()
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
                .map(|ident| ident.escaped_text.to_string()),
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

        self.with_commonjs_child_checker_for_file(target_file_idx, |checker| {
            let request = TypingRequest::NONE.contextual_opt(contextual_type);
            checker.get_type_of_function_impl(method_idx, &request)
        })
        .unwrap_or(TypeId::ANY)
    }
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashSet;
    use tsz_parser::parser::{NodeArena, NodeIndex, ParserState};

    use super::{CheckerState, CommonJsExportTargetRoot};

    fn binary_lhs_of(source: &str) -> (NodeArena, NodeIndex) {
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        parser.parse_source_file();
        let arena = parser.get_arena().clone();
        let sf = arena.source_files.first().expect("source file");
        let stmt_idx = sf.statements.nodes[0];
        let stmt_node = arena.get(stmt_idx).expect("stmt");
        let stmt = arena
            .get_expression_statement(stmt_node)
            .expect("expr stmt");
        let bin_node = arena.get(stmt.expression).expect("binary node");
        let left = arena.get_binary_expr(bin_node).expect("binary").left;
        (arena, left)
    }

    fn no_aliases() -> FxHashSet<String> {
        FxHashSet::default()
    }

    fn aliases(names: &[&str]) -> FxHashSet<String> {
        names.iter().map(|&s| s.to_string()).collect()
    }

    fn assert_target(
        source: &str,
        aliases: &FxHashSet<String>,
        name: &str,
        root: CommonJsExportTargetRoot,
    ) {
        let (arena, left) = binary_lhs_of(source);
        let result = CheckerState::commonjs_export_assignment_target(&arena, left, aliases)
            .expect("export assignment target");
        assert_eq!(result.member_name, name);
        assert_eq!(result.root, root);
    }

    #[test]
    fn classify_exports_dot_prop() {
        assert_target(
            "exports.foo = 1;",
            &no_aliases(),
            "foo",
            CommonJsExportTargetRoot::Exports,
        );
    }

    #[test]
    fn classify_exports_bracket_prop() {
        assert_target(
            r#"exports["bar"] = 2;"#,
            &no_aliases(),
            "bar",
            CommonJsExportTargetRoot::Exports,
        );
    }

    #[test]
    fn classify_module_exports_dot_prop() {
        assert_target(
            "module.exports.baz = 3;",
            &no_aliases(),
            "baz",
            CommonJsExportTargetRoot::ModuleExports,
        );
    }

    #[test]
    fn classify_module_bracket_exports_dot_prop() {
        assert_target(
            r#"module["exports"].qux = 4;"#,
            &no_aliases(),
            "qux",
            CommonJsExportTargetRoot::ModuleExports,
        );
    }

    #[test]
    fn classify_alias_prop() {
        assert_target(
            "e.thing = 5;",
            &aliases(&["e"]),
            "thing",
            CommonJsExportTargetRoot::Alias,
        );
    }

    #[test]
    fn classify_non_export_returns_none() {
        let (arena, left) = binary_lhs_of("obj.prop = 6;");
        let result = CheckerState::commonjs_export_assignment_target(&arena, left, &no_aliases());
        assert_eq!(result, None);
    }

    #[test]
    fn classify_alias_not_in_set_returns_none() {
        let (arena, left) = binary_lhs_of("e.thing = 7;");
        let result = CheckerState::commonjs_export_assignment_target(&arena, left, &no_aliases());
        assert_eq!(result, None);
    }
}
