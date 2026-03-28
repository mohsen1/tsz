//! JSDoc annotation lookup and AST-level orchestration.
//!
//! This module owns functions that find JSDoc annotations on AST nodes
//! and extract type information from them. It delegates the actual type
//! resolution to `resolution.rs` and pure text parsing to `parsing.rs`.
//!
//! - Callable type annotation lookup (`jsdoc_callable_type_annotation_for_node`)
//! - Type annotation lookup (`jsdoc_type_annotation_for_node`)
//! - Satisfies annotation lookup (`jsdoc_satisfies_annotation_with_pos`)
//! - Global typedef resolution across files (`resolve_global_jsdoc_typedef_type`)
//! - Source file data lookup (`source_file_data_for_node`)
//! - Type query resolution (`resolve_type_query_type`)
//! - Generic constraint validation (`validate_jsdoc_generic_constraints_at_node`)
//! - Metadata queries (`jsdoc_has_readonly_tag`, `jsdoc_access_level`)
//! - Orphaned extends/augments detection (`find_orphaned_extends_tags_for_statements`)
//! - Scoping helpers (`is_in_different_function_scope`, `find_function_body_end`)

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
use crate::query_boundaries::type_checking_utilities as query;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn jsdoc_callable_type_annotation_for_node(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }

        let sf = self.source_file_data_for_node(idx)?;
        if sf.comments.is_empty() {
            return None;
        }
        // JSDoc requires multi-line comments (/** ... */).
        if !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }

        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let node = self.ctx.arena.get(idx)?;
        let jsdoc = self.try_jsdoc_with_ancestor_walk(idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;

        self.jsdoc_concrete_callable_type_from_expr(type_expr, node.pos, &comments, &source_text)
    }

    pub(crate) fn jsdoc_callable_type_annotation_for_node_direct(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }

        let sf = self.source_file_data_for_node(idx)?;
        if sf.comments.is_empty() {
            return None;
        }
        // JSDoc requires multi-line comments (/** ... */).
        if !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }

        let source_text = sf.text.to_string();
        let comments = sf.comments.clone();
        let node = self.ctx.arena.get(idx)?;
        let jsdoc = self.try_leading_jsdoc(&comments, node.pos, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;

        self.jsdoc_concrete_callable_type_from_expr(type_expr, node.pos, &comments, &source_text)
    }

    pub(crate) fn resolve_global_jsdoc_typedef_info(
        &mut self,
        name: &str,
    ) -> Option<(TypeId, Vec<tsz_solver::TypeParamInfo>)> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }

        let current_file_name = self.ctx.file_name.clone();
        let mut current_arena_sources: Vec<_> = self
            .ctx
            .arena
            .source_files
            .iter()
            .map(|source_file| {
                (
                    source_file.file_name.clone(),
                    source_file.comments.clone(),
                    source_file.text.to_string(),
                )
            })
            .collect();
        current_arena_sources.sort_by_key(|(file_name, _, _)| {
            if *file_name == current_file_name {
                0usize
            } else {
                1usize
            }
        });

        for (_file_name, comments, source_text) in current_arena_sources {
            if let Some(info) = self.resolve_jsdoc_typedef_info(name, &comments, &source_text) {
                return Some(info);
            }
        }

        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;

        for (file_idx, (arena, binder)) in all_arenas.iter().zip(all_binders.iter()).enumerate() {
            if file_idx == self.ctx.current_file_idx {
                continue;
            }

            for source_file in &arena.source_files {
                let comments = source_file.comments.clone();
                let source_text = source_file.text.to_string();
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
                checker.ctx.current_file_idx = file_idx;
                self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);

                if let Some(info) =
                    checker.resolve_jsdoc_typedef_info(name, &comments, &source_text)
                {
                    self.ctx.merge_symbol_file_targets_from(&checker.ctx);
                    return Some(info);
                }
            }
        }

        None
    }

    pub(crate) fn source_file_data_for_node(
        &self,
        idx: NodeIndex,
    ) -> Option<&tsz_parser::parser::node::SourceFileData> {
        let mut current = idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if let Some(source_file) = self.ctx.arena.get_source_file(node) {
                return Some(source_file);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        None
    }
    /// Resolve `typeof X` type queries to the type of symbol X.
    pub(crate) fn resolve_type_query_type(&mut self, type_id: TypeId) -> TypeId {
        use tsz_binder::SymbolId;
        use tsz_binder::symbol_flags;
        use tsz_solver::SymbolRef;
        match query::classify_type_query(self.ctx.types, type_id) {
            query::TypeQueryKind::TypeQuery(SymbolRef(sym_id)) => {
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    return TypeId::ERROR;
                }
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }
                let sym = SymbolId(sym_id);
                let value_decl = self.get_cross_file_symbol(sym).map_or_else(
                    || self.ctx.binder.get_symbol(sym).map(|s| s.value_declaration),
                    |s| Some(s.value_declaration),
                );
                let flags = self
                    .get_cross_file_symbol(sym)
                    .map(|s| s.flags)
                    .or_else(|| self.ctx.binder.get_symbol(sym).map(|s| s.flags))
                    .unwrap_or(0);
                let is_merged_type_alias_value = (flags & symbol_flags::TYPE_ALIAS) != 0
                    && (flags & symbol_flags::VARIABLE) != 0
                    && value_decl.is_some_and(|decl| !decl.is_none());
                let result = if self.is_merged_interface_value_symbol(sym)
                    || ((flags & symbol_flags::CLASS) != 0
                        && value_decl.is_some_and(|decl| !decl.is_none()))
                    || is_merged_type_alias_value
                {
                    self.type_of_value_declaration_for_symbol(
                        sym,
                        value_decl.unwrap_or(NodeIndex::NONE),
                    )
                } else {
                    self.get_type_of_symbol(sym)
                };
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }
                result
            }
            query::TypeQueryKind::ApplicationWithTypeQuery {
                base_sym_ref: SymbolRef(sym_id),
                args,
            } => {
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    return TypeId::ERROR;
                }
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }
                let base = self.ctx.create_lazy_type_ref(SymbolId(sym_id));
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }
                self.ctx.types.application(base, args)
            }
            query::TypeQueryKind::Application { .. } | query::TypeQueryKind::Other => type_id,
        }
    }
    /// Extract and parse a JSDoc `@type` annotation for the given node.
    pub(crate) fn jsdoc_type_annotation_for_node(&mut self, idx: NodeIndex) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let sf = self.source_file_data_for_node(idx)?;
        if sf.comments.is_empty() {
            return None;
        }
        // JSDoc requires multi-line comments (/** ... */).
        if !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let node = self.ctx.arena.get(idx)?;
        let jsdoc = self.try_jsdoc_with_ancestor_walk(idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let type_expr = type_expr.trim();
        self.validate_jsdoc_generic_constraints_at_node(
            idx,
            node.pos,
            type_expr,
            &jsdoc,
            &comments,
            &source_text,
        );
        // Set the anchor position for typedef scoping.
        let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
        self.ctx.jsdoc_typedef_anchor_pos.set(node.pos);
        // Use the authoritative resolution kernel — no fallback chain needed.
        let result = self.resolve_jsdoc_reference(type_expr);
        self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
        result
    }
    fn validate_jsdoc_generic_constraints_at_node(
        &mut self,
        idx: NodeIndex,
        anchor_pos: u32,
        type_expr: &str,
        _jsdoc: &str,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) {
        let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') else {
            return;
        };
        if !type_expr.ends_with('>') {
            return;
        }
        let base_name = type_expr[..angle_idx].trim();
        let args_str = &type_expr[angle_idx + 1..type_expr.len() - 1];
        let arg_strs = Self::split_type_args_respecting_nesting(args_str);
        if arg_strs.is_empty() {
            return;
        }
        let symbol_constraints = self
            .ctx
            .binder
            .file_locals
            .get(base_name)
            .or_else(|| {
                self.ctx
                    .binder
                    .get_symbols()
                    .find_all_by_name(base_name)
                    .iter()
                    .copied()
                    .find(|&sym_id| {
                        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                            (symbol.flags
                                & (symbol_flags::TYPE_ALIAS
                                    | symbol_flags::CLASS
                                    | symbol_flags::INTERFACE
                                    | symbol_flags::ENUM))
                                != 0
                        })
                    })
            })
            .map(|sym_id| self.type_reference_symbol_type_with_params(sym_id).1)
            .unwrap_or_default();
        let type_params =
            if let Some((_, type_params)) = self.resolve_global_jsdoc_typedef_info(base_name) {
                type_params
            } else if !symbol_constraints.is_empty() {
                symbol_constraints
            } else {
                Vec::new()
            };
        if type_params.is_empty() {
            return;
        }
        let Some((_, comment_pos)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
        else {
            return;
        };
        let raw_comment = &source_text[comment_pos as usize..anchor_pos as usize];
        let Some(type_expr_offset) = raw_comment.find(type_expr) else {
            return;
        };
        let got = arg_strs.len();
        let max_expected = type_params.len();
        let min_required = type_params
            .iter()
            .filter(|param| param.default.is_none())
            .count();
        if got < min_required || got > max_expected {
            let message = if min_required < max_expected {
                format_message(
                    diagnostic_messages::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                    &[
                        base_name,
                        &min_required.to_string(),
                        &max_expected.to_string(),
                    ],
                )
            } else {
                format_message(
                    diagnostic_messages::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                    &[base_name, &max_expected.to_string()],
                )
            };
            self.error_at_position(
                comment_pos + type_expr_offset as u32,
                base_name.len() as u32,
                &message,
                if min_required < max_expected {
                    diagnostic_codes::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS
                } else {
                    diagnostic_codes::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S
                },
            );
            return;
        }
        let mut arg_search_offset = angle_idx + 1;
        for (arg_str, param) in arg_strs.iter().zip(type_params.iter()) {
            let Some(constraint) = param.constraint else {
                arg_search_offset += arg_str.len() + 1;
                continue;
            };
            let Some(type_arg) = self.resolve_jsdoc_type_str(arg_str.trim()) else {
                arg_search_offset += arg_str.len() + 1;
                continue;
            };
            if self.is_assignable_to(type_arg, constraint) {
                arg_search_offset += arg_str.len() + 1;
                continue;
            }
            let widened_arg =
                crate::query_boundaries::common::widen_literal_type(self.ctx.types, type_arg);
            let message = format_message(
                diagnostic_messages::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
                &[
                    &self.format_type_diagnostic(widened_arg),
                    &self.format_type_diagnostic(constraint),
                ],
            );
            let Some(arg_rel_in_expr) = type_expr[arg_search_offset..].find(arg_str.trim()) else {
                arg_search_offset += arg_str.len() + 1;
                continue;
            };
            let arg_pos =
                comment_pos as usize + type_expr_offset + arg_search_offset + arg_rel_in_expr;
            self.ctx.error(
                arg_pos as u32,
                arg_str.trim().len() as u32,
                message,
                diagnostic_codes::TYPE_DOES_NOT_SATISFY_THE_CONSTRAINT,
            );
            return;
        }
    }
    /// Resolve a direct leading JSDoc `@type` annotation (no parent fallback).
    pub(crate) fn jsdoc_type_annotation_for_node_direct(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let sf = self.source_file_data_for_node(idx)?;
        // Fast path: no comments in file means no JSDoc possible.
        // Avoids expensive Arc<str>::to_string() + Vec::clone() per call.
        if sf.comments.is_empty() {
            return None;
        }
        // Fast path: JSDoc annotations require multi-line comments (/** ... */).
        // If the file has only single-line comments (//), skip the expensive
        // source text copy. This eliminates ~47GB of memmove on expression-heavy
        // files with only // comments (e.g., optional-chain benchmarks).
        if !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let jsdoc = self.try_leading_jsdoc(
            &comments,
            self.effective_jsdoc_pos_for_node(idx, &comments, &source_text)?,
            &source_text,
        )?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let type_expr = type_expr.trim();
        // Use the authoritative resolution kernel — no fallback chain needed.
        self.resolve_jsdoc_reference(type_expr)
    }
    /// Extract `@satisfies` annotation and its keyword position.
    pub(crate) fn jsdoc_satisfies_annotation_with_pos(
        &mut self,
        idx: NodeIndex,
    ) -> Option<(TypeId, u32)> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let sf = self.source_file_data_for_node(idx)?;
        if sf.comments.is_empty() {
            return None;
        }
        // Fast path: @satisfies requires multi-line comments (/** ... */).
        if !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let (jsdoc, jsdoc_start) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_satisfies_expression(&jsdoc)?;
        let type_expr = type_expr.trim();
        let raw_comment = source_text.get(jsdoc_start as usize..)?;
        let tag_offset = raw_comment.find("@satisfies")? as u32;
        let keyword_pos = jsdoc_start + tag_offset + 1;
        let resolved = self.resolve_jsdoc_type_str(type_expr)?;
        Some((self.judge_evaluate(resolved), keyword_pos))
    }

    /// Check if a node has a JSDoc `@readonly` tag.
    pub(crate) fn jsdoc_has_readonly_tag(&self, idx: NodeIndex) -> bool {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let Some(jsdoc) = self.try_leading_jsdoc(
            comments,
            self.ctx.arena.get(idx).map_or(0, |n| n.pos),
            source_text,
        ) else {
            return false;
        };
        Self::jsdoc_contains_tag(&jsdoc, "readonly")
    }
    /// Get the access level from JSDoc `@private` / `@protected` / `@public` tags.
    pub(crate) fn jsdoc_access_level(
        &self,
        idx: NodeIndex,
    ) -> Option<crate::state::MemberAccessLevel> {
        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let jsdoc = self.try_leading_jsdoc(
            comments,
            self.ctx.arena.get(idx).map_or(0, |n| n.pos),
            source_text,
        )?;
        if Self::jsdoc_contains_tag(&jsdoc, "private") {
            Some(crate::state::MemberAccessLevel::Private)
        } else if Self::jsdoc_contains_tag(&jsdoc, "protected") {
            Some(crate::state::MemberAccessLevel::Protected)
        } else {
            None
        }
    }
    /// Scan statements for `@extends`/`@augments` not on class declarations (TS8022).
    pub(crate) fn find_orphaned_extends_tags_for_statements(
        &self,
        statements: &[NodeIndex],
    ) -> Vec<(&'static str, u32, u32)> {
        use tsz_parser::parser::syntax_kind_ext;
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return Vec::new();
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let mut results = Vec::new();
        let mut handled_comment_positions = Vec::new();
        // Phase 1: Check each top-level statement's leading JSDoc
        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                if let Some(jsdoc) = self.try_leading_jsdoc(comments, node.pos, source_text)
                    && (Self::jsdoc_contains_tag(&jsdoc, "augments")
                        || Self::jsdoc_contains_tag(&jsdoc, "extends"))
                {
                    handled_comment_positions.push(node.pos);
                }
                continue;
            }
            let Some(jsdoc) = self.try_leading_jsdoc(comments, node.pos, source_text) else {
                continue;
            };
            let tag = if Self::jsdoc_contains_tag(&jsdoc, "augments") {
                "augments"
            } else if Self::jsdoc_contains_tag(&jsdoc, "extends") {
                "extends"
            } else {
                continue;
            };
            handled_comment_positions.push(node.pos);
            let (pos, len) = if node.kind == syntax_kind_ext::FUNCTION_DECLARATION {
                if let Some(func) = self.ctx.arena.get_function(node)
                    && let Some(name_node) = self.ctx.arena.get(func.name)
                {
                    (name_node.pos, name_node.end - name_node.pos)
                } else {
                    (node.pos, node.end - node.pos)
                }
            } else {
                (node.pos, node.end - node.pos)
            };
            results.push((tag, pos, len));
        }
        // Phase 2: Check for dangling JSDoc comments not attached to any statement
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            if handled_comment_positions
                .iter()
                .any(|&stmt_pos| comment.end <= stmt_pos)
            {
                continue;
            }
            let content = get_jsdoc_content(comment, source_text);
            let tag = if Self::jsdoc_contains_tag(&content, "augments") {
                "augments"
            } else if Self::jsdoc_contains_tag(&content, "extends") {
                "extends"
            } else {
                continue;
            };
            let is_leading_of_any_stmt = statements.iter().any(|&stmt_idx| {
                if let Some(n) = self.ctx.arena.get(stmt_idx)
                    && let Some((_, leading_pos)) =
                        self.try_leading_jsdoc_with_pos(comments, n.pos, source_text)
                {
                    return leading_pos == comment.pos;
                }
                false
            });
            if is_leading_of_any_stmt {
                continue;
            }
            let needle = format!("@{tag}");
            let (pos, len) = if let Some(offset) = source_text
                .get(comment.pos as usize..comment.end as usize)
                .and_then(|s| s.find(&needle))
            {
                let tag_pos = comment.pos + offset as u32;
                (tag_pos, needle.len() as u32)
            } else {
                (comment.pos, comment.end - comment.pos)
            };
            results.push((tag, pos, len));
        }
        results
    }

    /// Check if two source positions are in different function scopes.
    /// Used for JSDoc typedef scoping — a typedef defined inside a function
    /// should not be visible outside that function.
    #[allow(dead_code)]
    pub(crate) fn is_in_different_function_scope(&self, comment_pos: u32, anchor_pos: u32) -> bool {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let source_text = sf.text.to_string();
        // Walk from anchor_pos backward to see if we cross a function boundary
        // before reaching comment_pos. If comment_pos is inside a function body
        // and anchor_pos is outside it, they're in different scopes.
        let text = &source_text[..anchor_pos as usize];
        let mut depth: i32 = 0;
        for ch in text[comment_pos as usize..].chars() {
            match ch {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        // If depth != 0, the comment is inside a nested scope relative to anchor
        depth != 0
    }

    /// Find the end position of a function body by scanning for the matching '}'.
    pub(crate) fn find_function_body_end(node_pos: u32, node_end: u32, source_text: &str) -> u32 {
        let start = node_pos as usize;
        let end = node_end as usize;
        if end > source_text.len() {
            return node_end;
        }
        let slice = &source_text[start..end];
        let mut depth = 0i32;
        let mut last_close = node_end;
        for (i, ch) in slice.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        last_close = (start + i + 1) as u32;
                        break;
                    }
                }
                _ => {}
            }
        }
        last_close
    }
}
