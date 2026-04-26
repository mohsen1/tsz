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
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

/// `(tag_name, Some((pos, len)))` for orphaned `@extends`/`@augments` tags.
/// `None` means fully-dangling (no attached statement); `Some` gives the
/// statement's source position and length for diagnostic anchoring.
type OrphanedExtendsTag = (&'static str, Option<(u32, u32)>);

impl<'a> CheckerState<'a> {
    fn global_source_file_idx_for_name(&self, file_name: &str) -> Option<usize> {
        if self.ctx.file_name == file_name {
            return Some(self.ctx.current_file_idx);
        }

        self.ctx.all_arenas.as_ref().and_then(|arenas| {
            arenas.iter().enumerate().find_map(|(file_idx, arena)| {
                arena.source_files.first().and_then(|source_file| {
                    (source_file.file_name == file_name).then_some(file_idx)
                })
            })
        })
    }

    fn current_arena_source_file_idx_for_node(&self, idx: NodeIndex) -> Option<usize> {
        let mut current = idx;
        while current.is_some() {
            let node = self.ctx.arena.get(current)?;
            if let Some(source_file) = self.ctx.arena.get_source_file(node) {
                return self
                    .global_source_file_idx_for_name(&source_file.file_name)
                    .or(Some(self.ctx.current_file_idx));
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                break;
            }
            current = ext.parent;
        }
        None
    }

    fn symbol_file_idx_for_jsdoc_node(&self, idx: NodeIndex) -> Option<usize> {
        let direct_sym = self.ctx.binder.get_node_symbol(idx);
        if let Some(sym_id) = direct_sym
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && symbol.decl_file_idx != u32::MAX
        {
            return Some(symbol.decl_file_idx as usize);
        }

        let node = self.ctx.arena.get(idx)?;
        if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            let var_decl = self.ctx.arena.get_variable_declaration(node)?;
            let sym_id = self.ctx.binder.get_node_symbol(var_decl.name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.decl_file_idx != u32::MAX {
                return Some(symbol.decl_file_idx as usize);
            }
        }

        if node.kind == syntax_kind_ext::PARAMETER {
            let param = self.ctx.arena.get_parameter(node)?;
            let sym_id = self.ctx.binder.get_node_symbol(param.name)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.decl_file_idx != u32::MAX {
                return Some(symbol.decl_file_idx as usize);
            }
        }

        None
    }

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
        let current_file_idx = self.ctx.current_file_idx;
        let mut current_arena_sources: Vec<_> = self
            .ctx
            .arena
            .source_files
            .iter()
            .enumerate()
            .map(|(source_file_idx, source_file)| {
                (
                    source_file_idx,
                    source_file.file_name.clone(),
                    source_file.comments.clone(),
                    source_file.text.to_string(),
                )
            })
            .collect();
        current_arena_sources.sort_by_key(|(source_file_idx, file_name, _, _)| {
            if *source_file_idx == current_file_idx || *file_name == current_file_name {
                0usize
            } else {
                1usize
            }
        });

        for (source_file_idx, source_file_name, comments, source_text) in current_arena_sources {
            let prev_file_name = self.ctx.file_name.clone();
            let prev_file_idx = self.ctx.current_file_idx;
            self.ctx.file_name = source_file_name;
            if let Some(global_file_idx) = self.global_source_file_idx_for_name(&self.ctx.file_name)
            {
                self.ctx.current_file_idx = global_file_idx;
            } else if source_file_idx == prev_file_idx || self.ctx.file_name == prev_file_name {
                self.ctx.current_file_idx = prev_file_idx;
            }
            let info = self.resolve_jsdoc_typedef_info(name, &comments, &source_text);
            self.ctx.file_name = prev_file_name;
            self.ctx.current_file_idx = prev_file_idx;

            if let Some(info) = info {
                return Some(info);
            }
        }

        let all_arenas = self.ctx.all_arenas.clone()?;
        let all_binders = self.ctx.all_binders.clone()?;

        for (file_idx, (arena, binder)) in all_arenas.iter().zip(all_binders.iter()).enumerate() {
            if file_idx == current_file_idx {
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
                    && value_decl.is_some_and(|decl| decl.is_some());
                let result = if self.is_merged_interface_value_symbol(sym)
                    || ((flags & symbol_flags::CLASS) != 0
                        && value_decl.is_some_and(|decl| decl.is_some()))
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
        let idx = self.normalized_jsdoc_lookup_node(idx);
        let sf = self.source_file_data_for_node(idx)?;
        if sf.comments.is_empty() {
            return None;
        }
        // JSDoc requires multi-line comments (/** ... */).
        if !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }
        let source_file_name = sf.file_name.clone();
        let source_file_idx = self
            .symbol_file_idx_for_jsdoc_node(idx)
            .or_else(|| self.current_arena_source_file_idx_for_node(idx));
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
        self.validate_jsdoc_qualified_value_receiver_at_node(
            idx,
            node.pos,
            type_expr,
            &comments,
            &source_text,
        );
        // Set the anchor position for typedef scoping.
        let prev_anchor = self.ctx.jsdoc_typedef_anchor_pos.get();
        let prev_file_name = self.ctx.file_name.clone();
        let prev_file_idx = self.ctx.current_file_idx;
        self.ctx.file_name = source_file_name;
        if let Some(source_file_idx) = source_file_idx {
            self.ctx.current_file_idx = source_file_idx;
        }
        self.ctx.jsdoc_typedef_anchor_pos.set(node.pos);
        // Use the authoritative resolution kernel — no fallback chain needed.
        let result = self.resolve_jsdoc_reference(type_expr);
        self.ctx.jsdoc_typedef_anchor_pos.set(prev_anchor);
        self.ctx.file_name = prev_file_name;
        self.ctx.current_file_idx = prev_file_idx;
        result
    }

    pub(crate) fn jsdoc_type_expression_span_for_node(&self, idx: NodeIndex) -> Option<(u32, u32)> {
        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let idx = self.normalized_jsdoc_lookup_node(idx);
        let sf = self.source_file_data_for_node(idx)?;
        if sf.comments.is_empty() || !sf.comments.iter().any(|c| c.is_multi_line) {
            return None;
        }
        let source_text: String = sf.text.to_string();
        let comments = sf.comments.clone();
        let (jsdoc, comment_pos) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, &comments, &source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?.trim();
        if type_expr.is_empty() {
            return None;
        }
        let tag_pos = jsdoc.find("@type")?;
        let after_tag = tag_pos + "@type".len();
        let rest = &jsdoc[after_tag..];
        let rest_ws = rest.len() - rest.trim_start().len();
        let rest_trimmed = rest.trim_start();
        let expr_offset = if let Some(after_open) = rest_trimmed.strip_prefix('{') {
            let brace_ws = after_open.len() - after_open.trim_start().len();
            after_tag + rest_ws + 1 + brace_ws
        } else {
            after_tag + rest_ws
        };
        Some((comment_pos + expr_offset as u32 + 4, type_expr.len() as u32))
    }

    /// Locate the source span of the return type inside a JSDoc
    /// `@type {function(...): ReturnType}` annotation attached to `func_idx`.
    ///
    /// Used by TS2355/TS2366 emission so the diagnostic underlines the JSDoc
    /// return type (matching tsc) instead of the function name. Returns
    /// `None` if the function has no JSDoc `@type` annotation, or the
    /// annotation is not a `function(...)` form with an explicit return type.
    pub(crate) fn jsdoc_function_return_type_span_for_function(
        &self,
        func_idx: NodeIndex,
    ) -> Option<(u32, u32)> {
        use tsz_common::comments::is_jsdoc_comment;

        if !self.ctx.should_resolve_jsdoc() {
            return None;
        }
        let sf = self.source_file_data_for_node(func_idx)?;
        if sf.comments.is_empty() {
            return None;
        }
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let func_node = self.ctx.arena.get(func_idx)?;

        // Look for a JSDoc comment immediately preceding the function node.
        for comment in comments.iter().rev() {
            if comment.end <= func_node.pos
                && is_jsdoc_comment(comment, source_text)
                && let Some(span) = Self::jsdoc_type_tag_function_return_type_span_in_source(
                    source_text,
                    comment.pos,
                )
            {
                return Some(span);
            }
            if comment.end <= func_node.pos {
                break;
            }
        }

        // Walk up the parent chain (e.g. `const f = function () {}`).
        let mut current = func_idx;
        for _ in 0..4 {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            // Stop before checking statement-level containers whose "leading
            // JSDoc" belongs to their first child statement, not to the node
            // we started the walk from. Without this guard, walking from a
            // function expression up through its enclosing SourceFile/Block
            // could pick up `@type {function(): T}` from an unrelated
            // preceding declaration. Mirrors the guard in
            // `try_jsdoc_with_ancestor_walk` (params.rs).
            use tsz_parser::parser::syntax_kind_ext as sk;
            if matches!(
                parent_node.kind,
                sk::SOURCE_FILE
                    | sk::BLOCK
                    | sk::MODULE_BLOCK
                    | sk::CASE_CLAUSE
                    | sk::DEFAULT_CLAUSE
            ) {
                break;
            }
            for comment in comments.iter().rev() {
                if comment.end <= parent_node.pos
                    && is_jsdoc_comment(comment, source_text)
                    && let Some(span) = Self::jsdoc_type_tag_function_return_type_span_in_source(
                        source_text,
                        comment.pos,
                    )
                {
                    return Some(span);
                }
                // Stop at the first preceding comment to avoid scanning
                // earlier (unrelated) JSDoc that may match the function
                // shape but belong to a different declaration. Mirrors the
                // early break in the loop at lines 430-443 above.
                if comment.end <= parent_node.pos {
                    break;
                }
            }
            current = parent;
        }
        None
    }

    /// Emit TS2694 for JSDoc qualified type names `A.B` whose root `A` is
    /// a plain value (not a namespace/module/type container). tsc's JSDoc
    /// checker treats this as "Namespace 'A' has no exported member 'B'";
    /// without this pass we silently accept the unknown member.
    fn validate_jsdoc_qualified_value_receiver_at_node(
        &mut self,
        idx: NodeIndex,
        anchor_pos: u32,
        type_expr: &str,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) {
        use tsz_binder::symbol_flags;
        // Only handle the simple `A.B` shape. Deeper chains, generics, unions,
        // imports, or `(...)` groupings go through richer resolution elsewhere.
        let trimmed = type_expr.trim();
        if trimmed.contains('<')
            || trimmed.contains('|')
            || trimmed.contains('&')
            || trimmed.contains('(')
            || trimmed.contains('[')
            || trimmed.contains(' ')
            || trimmed.contains('"')
        {
            return;
        }
        let mut parts = trimmed.split('.');
        let Some(root) = parts.next().filter(|s| !s.is_empty()) else {
            return;
        };
        let Some(member) = parts.next().filter(|s| !s.is_empty()) else {
            return;
        };
        // Only single-dot A.B; deeper A.B.C handled elsewhere (and varies more).
        if parts.next().is_some() {
            return;
        }

        let Some(sym_id) = self.ctx.binder.file_locals.get(root) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };
        // JS salsa mode merges expando property assignments from every file
        // that touches a given identifier (e.g. `Ns.One = ...` in one file,
        // `/** @type {Ns.One} */` in another). Our per-file
        // `expando_properties` table doesn't see those cross-file
        // assignments, so skip the check when the declaring file isn't the
        // one we're currently processing.
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx as usize != self.ctx.current_file_idx
        {
            return;
        }
        // If the root has any "can-hold-members" flags — namespace/module,
        // type alias, interface, class, enum, or import alias — let the
        // normal resolution path run. We only want to flag the case where
        // `A` is a pure runtime value (e.g. `var a = foo();`) and someone
        // writes `{A.B}` in JSDoc.
        let member_holder_flags = symbol_flags::MODULE
            | symbol_flags::NAMESPACE_MODULE
            | symbol_flags::VALUE_MODULE
            | symbol_flags::TYPE_ALIAS
            | symbol_flags::INTERFACE
            | symbol_flags::CLASS
            | symbol_flags::ENUM
            | symbol_flags::ALIAS;
        if symbol.flags & member_holder_flags != 0 {
            return;
        }
        // Must actually be a value (variable, function, etc.).
        if !symbol.has_any_flags(symbol_flags::VALUE) {
            return;
        }
        // CommonJS `var mod = require("./x")` binds `mod` to the module's
        // exports. Qualified references like `{mod.Foo}` resolve through the
        // imported module and must not be flagged here.
        if symbol.import_module.is_some() {
            return;
        }
        // Detect `var mod = require("./x")` in JS salsa mode, which is
        // handled by the type checker rather than the binder's
        // `import_module` field.
        if symbol.declarations.iter().any(|&decl_idx| {
            if !decl_idx.is_some() {
                return false;
            }
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                return false;
            };
            var_decl.initializer.is_some()
                && self
                    .get_require_module_specifier(var_decl.initializer)
                    .is_some()
        }) {
            return;
        }
        // In JS salsa mode, plain values grow namespace-like expando members
        // via assignments such as `Workspace.Project = ...`. If the symbol has
        // any such member/export children, the qualified form `A.B` may be a
        // legitimate expando type reference — let the resolver decide.
        let has_members = symbol
            .members
            .as_ref()
            .is_some_and(|table| !table.is_empty());
        let has_exports = symbol
            .exports
            .as_ref()
            .is_some_and(|table| !table.is_empty());
        if has_members || has_exports {
            return;
        }
        // The binder also tracks `X.prop = value` expando assignments in a
        // side table keyed by the receiver name. If any expando property was
        // registered for this root, treat it as namespace-like and skip.
        if self
            .ctx
            .binder
            .expando_properties
            .get(root)
            .is_some_and(|props| props.contains(member) || !props.is_empty())
        {
            return;
        }

        let Some((_, comment_pos)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
        else {
            return;
        };
        let raw_comment = &source_text[comment_pos as usize..anchor_pos as usize];
        let Some(type_expr_offset) = raw_comment.find(trimmed) else {
            return;
        };
        // `member` sits at `root.len() + 1` bytes past the start of the type expression
        // (the +1 skips the `.`).
        let member_offset = type_expr_offset + root.len() + 1;

        let message = format_message(
            diagnostic_messages::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
            &[root, member],
        );
        self.error_at_position(
            comment_pos + member_offset as u32,
            member.len() as u32,
            &message,
            diagnostic_codes::NAMESPACE_HAS_NO_EXPORTED_MEMBER,
        );
    }

    fn normalized_jsdoc_lookup_node(&self, idx: NodeIndex) -> NodeIndex {
        let Some(node) = self.ctx.arena.get(idx) else {
            return idx;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return idx;
        }

        let Some(sym_id) = self.ctx.binder.get_node_symbol(idx) else {
            return idx;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return idx;
        };
        let value_decl = symbol.value_declaration;
        if value_decl.is_none() {
            return idx;
        }
        if value_decl == idx {
            return idx;
        }

        let Some(value_node) = self.ctx.arena.get(value_decl) else {
            return idx;
        };
        if value_node.kind != SyntaxKind::Identifier as u16 {
            return idx;
        }

        let Some(ext) = self.ctx.arena.get_extended(value_decl) else {
            return idx;
        };
        let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
            return idx;
        };
        if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            || parent_node.kind == syntax_kind_ext::PARAMETER
        {
            return ext.parent;
        }

        idx
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
        // Handle both cases: with type args (e.g., `Array<number>`) and without (e.g., `Array`).
        let (base_name, arg_strs, angle_idx) =
            if let Some(angle_idx) = Self::find_top_level_char(type_expr, '<') {
                if !type_expr.ends_with('>') {
                    return;
                }
                let base = type_expr[..angle_idx].trim();
                let args_str = &type_expr[angle_idx + 1..type_expr.len() - 1];
                let args = Self::split_type_args_respecting_nesting(args_str);
                if args.is_empty() {
                    return;
                }
                (base, args, Some(angle_idx))
            } else {
                // No angle brackets: zero type arguments provided.
                // In JSDoc, bare `Array`/`Promise`/`Object` resolve to `X<any>`.
                // TSC only reports TS2314 when noImplicitAny is enabled.
                if !self.ctx.compiler_options.no_implicit_any {
                    return;
                }
                let base = type_expr.trim();
                // Only check simple identifiers (skip union, intersection, array, etc.)
                if base.is_empty()
                    || base.contains('|')
                    || base.contains('&')
                    || base.contains('[')
                    || base.contains('(')
                    || base.contains('.')
                    || base.contains(' ')
                {
                    return;
                }
                (base, Vec::new(), None)
            };
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
        // Track whether the resolved params came from a JSDoc `@typedef` alias.
        // tsc formats TS2314 without a `<T, U>` suffix for JSDoc-defined aliases
        // (`Generic type 'Everything' requires 5 type argument(s).`) but *does*
        // include it for real TS declarations (`Generic type 'Array<T>' requires
        // 1 type argument(s).`). Preserve that distinction.
        let (type_params, is_jsdoc_typedef) =
            if let Some((_, type_params)) = self.resolve_global_jsdoc_typedef_info(base_name) {
                (type_params, true)
            } else if !symbol_constraints.is_empty() {
                (symbol_constraints, false)
            } else if base_name.starts_with("import(") {
                // Handle import type base names: import('./module').Foo
                if let Some((module_specifier, Some(member_name))) =
                    Self::parse_jsdoc_import_type(base_name)
                {
                    (
                        self.resolve_jsdoc_import_member(&module_specifier, &member_name)
                            .map(|sym_id| self.type_reference_symbol_type_with_params(sym_id).1)
                            .unwrap_or_default(),
                        false,
                    )
                } else {
                    (Vec::new(), false)
                }
            } else {
                (Vec::new(), false)
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
            // tsc renders the name with its type parameters in TS2314/TS2707
            // for real TS declarations — e.g. `Generic type 'Array<T>' requires
            // 1 type argument(s).` — but *without* the suffix for JSDoc
            // `@typedef` aliases (`Generic type 'Everything' requires 5 type
            // argument(s).`). Preserve that asymmetry.
            let display_name = if is_jsdoc_typedef {
                base_name.to_string()
            } else {
                Self::format_generic_display_name_with_interner(
                    base_name,
                    &type_params,
                    self.ctx.types,
                )
            };
            let message = if min_required < max_expected {
                format_message(
                    diagnostic_messages::GENERIC_TYPE_REQUIRES_BETWEEN_AND_TYPE_ARGUMENTS,
                    &[
                        &display_name,
                        &min_required.to_string(),
                        &max_expected.to_string(),
                    ],
                )
            } else {
                format_message(
                    diagnostic_messages::GENERIC_TYPE_REQUIRES_TYPE_ARGUMENT_S,
                    &[&display_name, &max_expected.to_string()],
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
        let Some(angle_idx) = angle_idx else {
            return;
        };
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
    /// Like `jsdoc_type_annotation_for_node_direct`, but resolves JSDoc `@type`
    /// annotations even when `checkJs` is not set. This is needed for type inference
    /// of JS class properties (`this.p = value` in constructors) when `allowJs` is
    /// enabled: tsc always reads `@type` annotations for inference even without `checkJs`.
    pub(crate) fn jsdoc_type_annotation_for_node_inference(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        // Only applicable to JS files with allowJs
        if !self.ctx.is_js_file() || !self.ctx.compiler_options.allow_js {
            return None;
        }
        let sf = self.source_file_data_for_node(idx)?;
        if sf.comments.is_empty() {
            return None;
        }
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

    /// Check if a node has any JSDoc accessibility modifier (`@public`, `@private`, `@protected`).
    ///
    /// Used for TS18010 detection in JS files where accessibility comes from JSDoc
    /// tags rather than AST modifiers.
    pub(crate) fn has_jsdoc_accessibility_modifier(&self, idx: NodeIndex) -> bool {
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
        Self::jsdoc_contains_tag(&jsdoc, "public")
            || Self::jsdoc_contains_tag(&jsdoc, "private")
            || Self::jsdoc_contains_tag(&jsdoc, "protected")
    }

    /// Scan statements for `@extends`/`@augments` not on class declarations (TS8022).
    ///
    /// Each result is `(tag, Some((pos, len)))` when the orphan is the leading
    /// JSDoc of a non-class statement (tsc reports these at the statement's
    /// position), or `(tag, None)` when it is a fully-dangling JSDoc comment
    /// not attached to any statement (tsc reports these at program level with
    /// no file/position).
    #[allow(clippy::type_complexity)]
    pub(crate) fn find_orphaned_extends_tags_for_statements(
        &self,
        statements: &[NodeIndex],
    ) -> Vec<OrphanedExtendsTag> {
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
                // Always mark class declarations as handled so that any
                // preceding `@extends`/`@augments` comment — even one
                // separated from the class by blank lines or intermediate
                // JSDoc comments — is not reported as orphaned.  tsc
                // considers `@extends` on a class valid (possibly redundant)
                // and never emits TS8022 for it.
                handled_comment_positions.push(node.pos);
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
            results.push((tag, Some((pos, len))));
        }
        // Phase 2: Check for dangling JSDoc comments not attached to any statement
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            // Note: we intentionally do NOT skip comments simply because they
            // appear before a handled class — tsc reports `@extends`/`@augments`
            // as orphaned when another JSDoc comment is interposed between the
            // tag and the class declaration (see extendsTag2). The
            // is_leading_of_any_stmt check below is the sole gate.
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
            // Dangling JSDoc comment. tsc distinguishes two cases:
            //   * If there is any statement after the comment (even separated
            //     by intervening JSDoc), tsc reports at program level with no
            //     file/position — see `extendsTag2.ts`.
            //   * If the comment is the last meaningful thing in the file, tsc
            //     anchors the diagnostic at the position just after the
            //     comment's closing `*/` — see `extendsTag4.ts`.
            let any_stmt_after = statements.iter().any(|&stmt_idx| {
                self.ctx
                    .arena
                    .get(stmt_idx)
                    .is_some_and(|n| n.pos >= comment.end)
            });
            if any_stmt_after {
                results.push((tag, None));
            } else {
                results.push((tag, Some((comment.end, 0))));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_parser::parser::node::NodeAccess;
    use tsz_solver::TypeInterner;

    fn enclosing_expression_statement(parser: &ParserState, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        for _ in 0..6 {
            let ext = parser.get_arena().get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = parser.get_arena().get(parent)?;
            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    #[test]
    fn jsdoc_direct_lookup_sees_prototype_property_statement_type() {
        let source = r#"
function C() { this.x = false; };
/** @type {number} */
C.prototype.x;
new C().x;
"#;
        let options = crate::context::CheckerOptions {
            allow_js: true,
            check_js: true,
            strict: true,
            ..crate::context::CheckerOptions::default()
        };
        let mut parser = ParserState::new("test.js".to_string(), source.to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);
        let types = TypeInterner::new();
        let mut checker = CheckerState::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.js".to_string(),
            options,
        );
        checker.ctx.set_lib_contexts(Vec::new());
        checker.check_source_file(root);

        let access_idx = parser
            .get_arena()
            .nodes
            .iter()
            .enumerate()
            .find_map(|(idx, node)| {
                if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let access = parser.get_arena().get_access_expr(node)?;
                let name = parser
                    .get_arena()
                    .get_identifier_text(access.name_or_argument)?;
                if name != "x" {
                    return None;
                }
                let base = parser.get_arena().get(access.expression)?;
                if base.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let base_access = parser.get_arena().get_access_expr(base)?;
                let base_name = parser
                    .get_arena()
                    .get_identifier_text(base_access.name_or_argument)?;
                (base_name == "prototype").then_some(NodeIndex(idx as u32))
            })
            .expect("missing prototype property access");
        let stmt_idx = enclosing_expression_statement(&parser, access_idx)
            .expect("missing enclosing statement for prototype property access");
        let sf = checker
            .source_file_data_for_node(stmt_idx)
            .expect("missing source file data");
        let raw_leading = checker.try_leading_jsdoc(
            &sf.comments,
            parser
                .get_arena()
                .get(stmt_idx)
                .expect("stmt_idx node must exist")
                .pos,
            &sf.text,
        );
        assert!(
            raw_leading.is_some(),
            "expected raw leading JSDoc for prototype statement"
        );
        let ancestor = checker.jsdoc_type_annotation_for_node(stmt_idx);
        let direct = checker.jsdoc_type_annotation_for_node_direct(stmt_idx);
        assert_eq!(
            ancestor.map(|ty| checker.format_type(ty)),
            Some("number".to_string())
        );
        assert_eq!(
            direct.map(|ty| checker.format_type(ty)),
            Some("number".to_string())
        );
    }
}
