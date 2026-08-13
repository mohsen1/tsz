//! JSDoc diagnostic validation helpers for `CheckerState`.
//!
//! This module owns all JSDoc-specific diagnostic emission:
//! - TS8033 duplicate `@type` in `@typedef` checking
//! - TS8021 missing type annotation in `@typedef` checking
//! - TS2304 base type validation for `@typedef` declarations
//! - TS2300 duplicate `@import` tag detection
//! - TS1109 malformed `@import` tag detection
//! - `@satisfies` malformed/duplicate tag detection

use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Clone)]
struct JsdocNamedDecl {
    name: String,
    pos: u32,
    len: u32,
    file_idx: usize,
    is_global_script_decl: bool,
}

// =============================================================================
// TS8033: Duplicate @type in @typedef
// =============================================================================

impl<'a> CheckerState<'a> {
    /// TS2300: Check for duplicate identifier collisions between JSDoc typedefs and
    /// type-capable value/export declarations (classes and CommonJS exported constructors).
    pub(crate) fn check_jsdoc_typedef_name_conflicts(&mut self) {
        use crate::diagnostics::{diagnostic_codes, format_message};
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let mut typedefs_by_name: FxHashMap<String, Vec<JsdocNamedDecl>> = FxHashMap::default();
        let mut type_values_by_name: FxHashMap<String, Vec<JsdocNamedDecl>> = FxHashMap::default();

        let all_arenas = self.ctx.all_arenas.clone().unwrap_or_else(|| {
            std::sync::Arc::new(vec![std::sync::Arc::new(self.ctx.arena.clone())])
        });

        for (file_idx, arena) in all_arenas.iter().enumerate() {
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };

            for comment in &source_file.comments {
                if !is_jsdoc_comment(comment, &source_file.text) {
                    continue;
                }
                let content = get_jsdoc_content(comment, &source_file.text);
                let comment_text = comment.get_text(&source_file.text);
                let is_global_script_file = self.jsdoc_file_is_global_script(file_idx);
                for (name, info) in Self::parse_jsdoc_typedefs(&content) {
                    let Some(offset) = Self::find_jsdoc_typedef_name_offset(comment_text, &name)
                    else {
                        continue;
                    };
                    typedefs_by_name
                        .entry(name.clone())
                        .or_default()
                        .push(JsdocNamedDecl {
                            name,
                            pos: comment.pos + offset as u32,
                            len: 0,
                            file_idx,
                            is_global_script_decl: is_global_script_file
                                && !Self::jsdoc_typedef_is_import_alias(&info),
                        });
                }
            }

            for decl in self.collect_jsdoc_type_capable_value_declarations(file_idx, arena.as_ref())
            {
                type_values_by_name
                    .entry(decl.name.clone())
                    .or_default()
                    .push(decl);
            }
        }

        let current_file_idx = self.ctx.current_file_idx;
        let mut emitted = FxHashSet::default();

        for decls in typedefs_by_name.values() {
            for decl in decls
                .iter()
                .filter(|decl| decl.file_idx == current_file_idx)
            {
                let local_conflict = type_values_by_name.get(&decl.name).is_some_and(|others| {
                    others.iter().any(|other| {
                        other.file_idx == current_file_idx
                            || (decl.is_global_script_decl && other.is_global_script_decl)
                    })
                });
                // Issue #3133: a JSDoc `@typedef` in a global-script JS file
                // collides with lib globals like `Object`, `Array`, `Promise`.
                // tsc surfaces TS2300 for those collisions; tsz historically
                // only checked local class/CommonJS value declarations.
                let lib_conflict =
                    decl.is_global_script_decl && self.ctx.has_name_in_lib(&decl.name);
                let has_conflict = local_conflict || lib_conflict;
                if !has_conflict {
                    continue;
                }

                let key = (decl.pos, decl.len, diagnostic_codes::DUPLICATE_IDENTIFIER);
                if emitted.insert(key) {
                    let message = format_message("Duplicate identifier '{0}'.", &[&decl.name]);
                    self.error_at_position(
                        decl.pos,
                        decl.len.max(decl.name.len() as u32),
                        &message,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            }
        }

        for decls in type_values_by_name.values() {
            for decl in decls
                .iter()
                .filter(|decl| decl.file_idx == current_file_idx)
            {
                let has_conflict = typedefs_by_name.get(&decl.name).is_some_and(|others| {
                    others.iter().any(|other| {
                        other.file_idx == current_file_idx
                            || (decl.is_global_script_decl && other.is_global_script_decl)
                    })
                });
                if !has_conflict {
                    continue;
                }

                let key = (decl.pos, decl.len, diagnostic_codes::DUPLICATE_IDENTIFIER);
                if emitted.insert(key) {
                    let message = format_message("Duplicate identifier '{0}'.", &[&decl.name]);
                    self.error_at_position(
                        decl.pos,
                        decl.len.max(decl.name.len() as u32),
                        &message,
                        diagnostic_codes::DUPLICATE_IDENTIFIER,
                    );
                }
            }
        }
    }

    fn collect_jsdoc_type_capable_value_declarations(
        &mut self,
        target_file_idx: usize,
        arena: &tsz_parser::parser::NodeArena,
    ) -> Vec<JsdocNamedDecl> {
        let Some(source_file) = arena.source_files.first() else {
            return Vec::new();
        };

        let export_object_roots = Self::collect_commonjs_export_object_roots(arena);
        let mut decls = Vec::new();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                continue;
            };

            if stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                && let Some(class_decl) = arena.get_class(stmt_node)
                && let Some(name_node) = arena.get(class_decl.name)
                && let Some(ident) = arena.get_identifier(name_node)
            {
                decls.push(JsdocNamedDecl {
                    name: ident.escaped_text.to_string(),
                    pos: name_node.pos,
                    len: name_node.end.saturating_sub(name_node.pos),
                    file_idx: target_file_idx,
                    is_global_script_decl: self.jsdoc_file_is_global_script(target_file_idx),
                });
            }

            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = arena.get_expression_statement(stmt_node) else {
                continue;
            };
            self.collect_commonjs_type_capable_exports_from_expression(
                target_file_idx,
                arena,
                stmt.expression,
                &export_object_roots,
                &mut decls,
            );
        }

        decls
    }

    fn collect_commonjs_export_object_roots(
        arena: &tsz_parser::parser::NodeArena,
    ) -> FxHashSet<String> {
        let Some(source_file) = arena.source_files.first() else {
            return FxHashSet::default();
        };

        let mut roots = FxHashSet::default();
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
            Self::collect_commonjs_export_object_roots_from_expression(
                arena,
                stmt.expression,
                &mut roots,
            );
        }
        roots
    }

    fn collect_commonjs_export_object_roots_from_expression(
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        roots: &mut FxHashSet<String>,
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
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return;
        }

        if Self::is_module_exports_target_in_arena(arena, binary.left)
            && let Some(rhs_node) = arena.get(binary.right)
            && rhs_node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = arena.get_identifier(rhs_node)
        {
            roots.insert(ident.escaped_text.to_string());
        }

        Self::collect_commonjs_export_object_roots_from_expression(arena, binary.right, roots);
    }

    fn collect_commonjs_type_capable_exports_from_expression(
        &mut self,
        target_file_idx: usize,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        export_object_roots: &FxHashSet<String>,
        decls: &mut Vec<JsdocNamedDecl>,
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
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return;
        }

        if let Some((name, pos, len)) =
            Self::commonjs_named_export_target_in_arena(arena, binary.left, export_object_roots)
            && self.expression_introduces_type_name(target_file_idx, binary.right)
        {
            decls.push(JsdocNamedDecl {
                name,
                pos,
                len,
                file_idx: target_file_idx,
                is_global_script_decl: self.jsdoc_file_is_global_script(target_file_idx),
            });
        }

        // TS7: members of a `module.exports = { X }` object literal carry only
        // value meaning. A bare/import-type reference to such a member is the
        // TS2749 (require destructure) / TS2694 (import-type) value-used-as-type
        // error, so they are not registered as type-capable exports. Direct
        // `exports.X = class`/`module.exports = Class` forms still export types
        // through the sibling collection paths.

        self.collect_commonjs_type_capable_exports_from_expression(
            target_file_idx,
            arena,
            binary.right,
            export_object_roots,
            decls,
        );
    }

    fn commonjs_named_export_target_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        left_idx: NodeIndex,
        export_object_roots: &FxHashSet<String>,
    ) -> Option<(String, u32, u32)> {
        let left_node = arena.get(left_idx)?;
        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = arena.get_access_expr(left_node)?;
        let name_node = arena.get(access.name_or_argument)?;
        let name_ident = arena.get_identifier_at(access.name_or_argument)?;
        let base_is_export_root = arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| export_object_roots.contains(ident.escaped_text.as_str()));
        base_is_export_root.then(|| {
            (
                name_ident.escaped_text.to_string(),
                name_node.pos,
                name_node.end.saturating_sub(name_node.pos),
            )
        })
    }

    pub(crate) fn is_module_exports_target_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = arena.get_access_expr(node) else {
            return false;
        };
        arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "module")
            && arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn expression_introduces_type_name(
        &mut self,
        target_file_idx: usize,
        expr_idx: NodeIndex,
    ) -> bool {
        let ty = self.infer_commonjs_export_rhs_type(target_file_idx, expr_idx, None);
        crate::query_boundaries::common::is_constructor_like_type(self.ctx.types, ty)
    }

    fn find_jsdoc_typedef_name_offset(comment_text: &str, name: &str) -> Option<usize> {
        Self::jsdoc_typedef_tag_spans(comment_text)
            .into_iter()
            .find_map(|(typedef_idx, segment_end)| {
                let after_typedef = typedef_idx + "@typedef".len();
                let rest = &comment_text[after_typedef..segment_end];
                rest.find(name)
                    .map(|name_offset| after_typedef + name_offset)
            })
    }

    fn jsdoc_typedef_tag_spans(comment_text: &str) -> Vec<(usize, usize)> {
        let typedef_offsets = Self::jsdoc_tag_offsets(comment_text, "typedef");
        typedef_offsets
            .iter()
            .enumerate()
            .map(|(idx, &start)| {
                let end = typedef_offsets
                    .get(idx + 1)
                    .copied()
                    .unwrap_or(comment_text.len());
                (start, end)
            })
            .collect()
    }

    fn jsdoc_type_tag_duplicate_anchor(comment_text: &str, type_tag_pos: usize) -> (usize, u32) {
        let after = type_tag_pos + "@type".len();
        let tag_text = &comment_text[after..];
        let anchor_offset = if let Some(brace_rel) = tag_text.find('{') {
            let mut depth = 0i32;
            let mut end = None;
            for (i, ch) in tag_text[brace_rel..].char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(brace_rel + i + 1);
                        break;
                    }
                }
            }
            end.map(|e| after + e)
        } else {
            None
        };
        anchor_offset.map_or((type_tag_pos, "@type".len() as u32), |offset| (offset, 0))
    }

    fn jsdoc_typedef_is_import_alias(info: &crate::jsdoc::types::JsdocTypedefInfo) -> bool {
        info.base_type
            .as_deref()
            .is_some_and(|base_type| base_type.trim().starts_with("import("))
    }

    pub(crate) fn jsdoc_file_is_global_script(&self, file_idx: usize) -> bool {
        let Some(all_arenas) = self.ctx.all_arenas.as_ref() else {
            return !self.ctx.binder.is_external_module();
        };
        let Some(arena) = all_arenas.get(file_idx) else {
            return false;
        };
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };

        if let Some(is_external_module_by_file) = self.ctx.is_external_module_by_file.as_ref()
            && let Some(is_external_module) = crate::context::lookup_is_external_module_in_map(
                is_external_module_by_file,
                &source_file.file_name,
            )
        {
            return !is_external_module;
        }

        if self
            .ctx
            .all_binders
            .as_ref()
            .and_then(|binders| binders.get(file_idx))
            .is_some_and(|binder| binder.is_external_module())
        {
            return false;
        }

        !source_file.statements.nodes.iter().any(|&stmt_idx| {
            arena.get(stmt_idx).is_some_and(|stmt| {
                stmt.kind == syntax_kind_ext::IMPORT_DECLARATION
                    || stmt.kind == syntax_kind_ext::EXPORT_DECLARATION
                    || stmt.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    || stmt.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            })
        })
    }

    /// TS8033: Check all JSDoc comments for `@typedef` with multiple `@type` tags.
    ///
    /// A `@typedef` JSDoc comment should have at most one `@type` tag.
    /// If multiple `@type` tags are found, emit TS8033 at the second occurrence.
    pub(crate) fn check_typedef_duplicate_type_tags(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        for comment in comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);

            for (segment_start, segment_end) in Self::jsdoc_typedef_tag_spans(comment_text) {
                let segment_text = &comment_text[segment_start..segment_end];
                let type_tag_offsets = Self::jsdoc_tag_offsets(segment_text, "type");
                for type_tag_offset in type_tag_offsets.into_iter().skip(1) {
                    // tsc anchors TS8033 at the current token *after* parsing
                    // the duplicate `@type {...}` argument, i.e. just past
                    // the closing `}` of that tag.
                    let type_tag_pos = segment_start + type_tag_offset;
                    let (anchor_offset, error_len) =
                        Self::jsdoc_type_tag_duplicate_anchor(comment_text, type_tag_pos);
                    self.ctx.error(
                        comment.pos + anchor_offset as u32,
                        error_len,
                        diagnostic_messages::A_JSDOC_TYPEDEF_COMMENT_MAY_NOT_CONTAIN_MULTIPLE_TYPE_TAGS
                            .to_string(),
                        diagnostic_codes::A_JSDOC_TYPEDEF_COMMENT_MAY_NOT_CONTAIN_MULTIPLE_TYPE_TAGS,
                    );
                }
            }
        }
    }

    /// Check for JSDoc `@param` tags whose name slot starts with `*`.
    /// TypeScript reports TS1003 at the `*` token for these malformed names.
    pub(crate) fn check_jsdoc_param_invalid_names(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        fn param_tag_len(line: &str) -> Option<usize> {
            let after_tag = line.strip_prefix("@param")?;
            let next = after_tag.chars().next().unwrap_or('\0');
            (next == '\0' || next.is_whitespace() || next == '{').then_some("@param".len())
        }

        fn skip_curly_type_expr(text: &str) -> Option<usize> {
            if !text.starts_with('{') {
                return None;
            }
            let mut depth = 0usize;
            for (idx, ch) in text.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(idx + 1);
                        }
                    }
                    _ => {}
                }
            }
            None
        }

        fn normalize_param_tag_body(raw_body: &str) -> (String, Vec<usize>) {
            let mut normalized = String::new();
            let mut mapping = Vec::new();
            let mut raw_offset = 0usize;

            for (line_idx, segment) in raw_body.split_inclusive('\n').enumerate() {
                let raw_line = segment.trim_end_matches(['\r', '\n']);
                let mut content_start = 0usize;

                if line_idx > 0 {
                    content_start = raw_line.len() - raw_line.trim_start().len();
                    let after_ws = &raw_line[content_start..];
                    if let Some(after_star) = after_ws.strip_prefix('*') {
                        content_start += 1;
                        content_start += after_star.len() - after_star.trim_start().len();
                    }
                }

                if !normalized.is_empty() && content_start < raw_line.len() {
                    normalized.push(' ');
                    mapping.push(raw_offset + content_start);
                }

                for (idx, ch) in raw_line[content_start..].char_indices() {
                    normalized.push(ch);
                    mapping.push(raw_offset + content_start + idx);
                }

                raw_offset += segment.len();
            }

            (normalized, mapping)
        }

        fn find_invalid_param_name_offset(raw_body: &str) -> Option<usize> {
            let (normalized, mapping) = normalize_param_tag_body(raw_body);
            let mut rest = normalized.as_str();
            let mut logical_offset = 0usize;

            let trimmed = rest.trim_start();
            logical_offset += rest.len() - trimmed.len();
            rest = trimmed;

            if rest.starts_with('{') {
                let type_len = skip_curly_type_expr(rest)?;
                logical_offset += type_len;
                rest = &rest[type_len..];

                let trimmed = rest.trim_start();
                logical_offset += rest.len() - trimmed.len();
                rest = trimmed;
            }

            rest.starts_with('*')
                .then(|| mapping.get(logical_offset).copied())
                .flatten()
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);
            let mut comment_offset = 0usize;
            let mut current_param_offset = None;
            let mut current_param_body = String::new();

            for segment in comment_text.split_inclusive('\n') {
                let raw_line = segment.trim_end_matches(['\r', '\n']);
                let mut content_start = raw_line.len() - raw_line.trim_start().len();
                let mut content = &raw_line[content_start..];

                if let Some(after_open) = content.strip_prefix("/**") {
                    content_start += 3;
                    let ws_after_open = after_open.len() - after_open.trim_start().len();
                    content_start += ws_after_open;
                    content = &raw_line[content_start..];
                } else if let Some(after_open) = content.strip_prefix("/*") {
                    content_start += 2;
                    let ws_after_open = after_open.len() - after_open.trim_start().len();
                    content_start += ws_after_open;
                    content = &raw_line[content_start..];
                }

                if let Some(after_star) = content.strip_prefix('*') {
                    content_start += 1;
                    let ws_after_star = after_star.len() - after_star.trim_start().len();
                    content_start += ws_after_star;
                    content = &raw_line[content_start..];
                }

                if let Some(tag_len) = param_tag_len(content) {
                    if let Some(param_offset) = current_param_offset.take() {
                        if let Some(invalid_offset) =
                            find_invalid_param_name_offset(&current_param_body)
                        {
                            self.ctx.error(
                                (comment.pos as usize + param_offset + invalid_offset) as u32,
                                1,
                                diagnostic_messages::IDENTIFIER_EXPECTED.to_string(),
                                diagnostic_codes::IDENTIFIER_EXPECTED,
                            );
                        }
                        current_param_body.clear();
                    }

                    current_param_offset = Some(comment_offset + content_start + tag_len);
                    current_param_body.push_str(&segment[content_start + tag_len..]);
                } else if current_param_offset.is_some() && content.starts_with('@') {
                    if let Some(param_offset) = current_param_offset.take() {
                        if let Some(invalid_offset) =
                            find_invalid_param_name_offset(&current_param_body)
                        {
                            self.ctx.error(
                                (comment.pos as usize + param_offset + invalid_offset) as u32,
                                1,
                                diagnostic_messages::IDENTIFIER_EXPECTED.to_string(),
                                diagnostic_codes::IDENTIFIER_EXPECTED,
                            );
                        }
                        current_param_body.clear();
                    }
                } else if current_param_offset.is_some() {
                    current_param_body.push_str(segment);
                }

                comment_offset += segment.len();
            }

            if let Some(param_offset) = current_param_offset
                && let Some(invalid_offset) = find_invalid_param_name_offset(&current_param_body)
            {
                self.ctx.error(
                    (comment.pos as usize + param_offset + invalid_offset) as u32,
                    1,
                    diagnostic_messages::IDENTIFIER_EXPECTED.to_string(),
                    diagnostic_codes::IDENTIFIER_EXPECTED,
                );
            }
        }
    }

    /// Check for JSDoc `@property`/`@prop`/`@member` tags that use private
    /// names like `#id`. TypeScript reports TS1003 at the `#` token because
    /// JSDoc property names must be identifiers, dotted names, or quoted names.
    pub(crate) fn check_jsdoc_property_private_names(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        fn jsdoc_tag_len(line: &str) -> Option<usize> {
            for tag in ["@property", "@prop", "@member"] {
                if let Some(after_tag) = line.strip_prefix(tag) {
                    let next = after_tag.chars().next().unwrap_or('\0');
                    if next == '\0' || next.is_whitespace() || next == '{' {
                        return Some(tag.len());
                    }
                }
            }
            None
        }

        fn skip_curly_type_expr(text: &str) -> Option<usize> {
            if !text.starts_with('{') {
                return None;
            }
            let mut depth = 0usize;
            for (idx, ch) in text.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            return Some(idx + 1);
                        }
                    }
                    _ => {}
                }
            }
            None
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);
            let mut comment_offset = 0usize;

            for segment in comment_text.split_inclusive('\n') {
                let raw_line = segment.trim_end_matches(['\r', '\n']);
                let mut content_start = raw_line.len() - raw_line.trim_start().len();
                let mut content = &raw_line[content_start..];

                if let Some(after_open) = content.strip_prefix("/**") {
                    content_start += 3;
                    let ws_after_open = after_open.len() - after_open.trim_start().len();
                    content_start += ws_after_open;
                    content = &raw_line[content_start..];
                } else if let Some(after_open) = content.strip_prefix("/*") {
                    content_start += 2;
                    let ws_after_open = after_open.len() - after_open.trim_start().len();
                    content_start += ws_after_open;
                    content = &raw_line[content_start..];
                }

                if let Some(after_star) = content.strip_prefix('*') {
                    content_start += 1;
                    let ws_after_star = after_star.len() - after_star.trim_start().len();
                    content_start += ws_after_star;
                    content = &raw_line[content_start..];
                }

                let Some(tag_len) = jsdoc_tag_len(content) else {
                    comment_offset += segment.len();
                    continue;
                };

                let after_tag = &content[tag_len..];
                let ws_after_tag = after_tag.len() - after_tag.trim_start().len();
                let rest = after_tag.trim_start();
                let rest_offset = content_start + tag_len + ws_after_tag;

                let private_name_offset = if rest.starts_with('{') {
                    skip_curly_type_expr(rest).and_then(|type_end| {
                        let after_type = &rest[type_end..];
                        let ws_after_type = after_type.len() - after_type.trim_start().len();
                        after_type
                            .trim_start()
                            .starts_with('#')
                            .then_some(type_end + ws_after_type)
                    })
                } else {
                    rest.starts_with('#').then_some(0)
                };

                if let Some(private_name_offset) = private_name_offset {
                    self.ctx.error(
                        comment.pos + (comment_offset + rest_offset + private_name_offset) as u32,
                        1,
                        diagnostic_messages::IDENTIFIER_EXPECTED.to_string(),
                        diagnostic_codes::IDENTIFIER_EXPECTED,
                    );
                }

                comment_offset += segment.len();
            }
        }
    }

    /// Check for malformed JSDoc function types like `function(@foo)`.
    ///
    /// TypeScript reports:
    /// - TS7014 on the whole function type when it lacks a return annotation
    /// - TS1110 at the `@`
    /// - TS2304 at the following identifier
    pub(crate) fn check_malformed_jsdoc_function_type_params(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);

            for (function_offset, _) in comment_text.match_indices("function(") {
                let after_function = &comment_text[function_offset + "function(".len()..];
                let Some(close_paren_offset) = after_function.find(')') else {
                    continue;
                };

                let params_text = &after_function[..close_paren_offset];
                let is_constructor_type = params_text.trim_start().starts_with("new:");
                let has_return_annotation = after_function[close_paren_offset + 1..]
                    .trim_start()
                    .starts_with(':');
                let function_len = "function(".len() + close_paren_offset + 1;
                let function_pos = comment.pos + function_offset as u32;
                let mut reported_missing_return = false;
                let mut search_offset = 0usize;

                while let Some(at_offset) = params_text[search_offset..].find('@') {
                    let at_offset = search_offset + at_offset;
                    let ident_start = at_offset + 1;
                    let ident = params_text[ident_start..]
                        .chars()
                        .take_while(|ch| *ch == '_' || *ch == '$' || ch.is_ascii_alphanumeric())
                        .collect::<String>();

                    if ident.is_empty() {
                        search_offset = ident_start;
                        continue;
                    }

                    if !reported_missing_return
                        && !is_constructor_type
                        && !has_return_annotation
                        && self.ctx.no_implicit_any()
                    {
                        self.ctx.error(
                            function_pos,
                            function_len as u32,
                            diagnostic_messages::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE
                                .replace("{0}", "any"),
                            diagnostic_codes::FUNCTION_TYPE_WHICH_LACKS_RETURN_TYPE_ANNOTATION_IMPLICITLY_HAS_AN_RETURN_TYPE,
                        );
                        reported_missing_return = true;
                    }

                    let at_pos = function_pos + "function(".len() as u32 + at_offset as u32;
                    self.ctx.error(
                        at_pos,
                        1,
                        diagnostic_messages::TYPE_EXPECTED.to_string(),
                        diagnostic_codes::TYPE_EXPECTED,
                    );
                    self.ctx.error(
                        at_pos + 1,
                        ident.len() as u32,
                        format!("Cannot find name '{ident}'."),
                        diagnostic_codes::CANNOT_FIND_NAME,
                    );

                    search_offset = ident_start + ident.len();
                }
            }
        }
    }

    /// Check unsupported multiline `@typedef {{ ... }}` wrappers in JSDoc comments
    /// that do not use leading `*` comment lines.
    ///
    /// TypeScript reports TS1110 at the first wrapped value line and again at the
    /// closing `}}` line for this malformed comment shape.
    ///
    /// Operates only on the current file's arena: positions are relative to the
    /// current source text, and `error_at_position` attaches them to the current
    /// file name. Walking other arenas here would mis-attribute mod7-style
    /// errors onto mod1-style files (positions resolve against the wrong text).
    pub(crate) fn check_jsdoc_unwrapped_multiline_typedefs(&mut self) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::comments::is_jsdoc_comment;

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;

        for comment in &sf.comments {
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }

            let comment_text = comment.get_text(source_text);
            let mut in_unwrapped_typedef = false;
            let mut awaiting_wrapped_value = false;
            let mut first_type_expected = None;
            let mut closing_type_expected = None;
            let mut line_offset = 0usize;

            for segment in comment_text.split_inclusive('\n') {
                let line = segment.trim_end_matches(['\r', '\n']);
                let trimmed = line.trim_start();
                let leading_ws = line.len().saturating_sub(trimmed.len());
                let has_comment_star = !in_unwrapped_typedef && trimmed.starts_with('*');
                let content = if has_comment_star {
                    trimmed[1..].trim_start()
                } else {
                    trimmed
                };

                if !in_unwrapped_typedef {
                    if !has_comment_star && content.starts_with("@typedef {{") {
                        in_unwrapped_typedef = true;
                    }
                } else if content.starts_with("}}") {
                    closing_type_expected =
                        Some(comment.pos + line_offset as u32 + leading_ws as u32);
                    break;
                } else if awaiting_wrapped_value
                    && !content.is_empty()
                    && first_type_expected.is_none()
                {
                    let mut pos = comment.pos + line_offset as u32 + leading_ws as u32;
                    if content.starts_with('*') {
                        pos += 1;
                    }
                    first_type_expected = Some(pos);
                }

                if in_unwrapped_typedef {
                    awaiting_wrapped_value = content.ends_with(':');
                }
                line_offset += segment.len();
            }

            if let Some(pos) = first_type_expected {
                self.error_at_position(
                    pos,
                    1,
                    diagnostic_messages::TYPE_EXPECTED,
                    diagnostic_codes::TYPE_EXPECTED,
                );
            }
            if let Some(pos) = closing_type_expected {
                self.error_at_position(
                    pos,
                    1,
                    diagnostic_messages::TYPE_EXPECTED,
                    diagnostic_codes::TYPE_EXPECTED,
                );
            }
        }
    }
}

// =============================================================================
// @satisfies tag validation
// =============================================================================

impl<'a> CheckerState<'a> {
    pub(crate) fn report_malformed_jsdoc_satisfies_tags(&mut self, idx: NodeIndex) {
        use tsz_common::comments::is_jsdoc_comment;

        if !self.ctx.should_resolve_jsdoc() {
            return;
        }

        let Some(sf) = self.ctx.arena.source_files.first() else {
            return;
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        if let Some((_jsdoc, jsdoc_start)) =
            self.try_jsdoc_with_ancestor_walk_and_pos(idx, comments, source_text)
            && let Some(comment) = comments.iter().find(|c| c.pos == jsdoc_start)
        {
            self.emit_malformed_jsdoc_satisfies_diagnostics(source_text, comment.pos, comment.end);
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(node) else {
            return;
        };
        if var_decl.initializer.is_none() {
            return;
        }

        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return;
        };
        if let Some((_, pos)) =
            self.try_leading_jsdoc_with_pos(comments, init_node.pos, source_text)
            && let Some(comment) = comments
                .iter()
                .find(|c| c.pos == pos)
                .filter(|c| is_jsdoc_comment(c, source_text))
        {
            self.emit_malformed_jsdoc_satisfies_diagnostics(source_text, comment.pos, comment.end);
        }
    }

    fn emit_malformed_jsdoc_satisfies_diagnostics(
        &mut self,
        source_text: &str,
        comment_pos: u32,
        comment_end: u32,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        for (name_pos, name_len, name) in
            Self::malformed_jsdoc_satisfies_unexpected_names(source_text, comment_pos, comment_end)
        {
            if self.resolve_jsdoc_type_str(&name).is_some() {
                continue;
            }
            self.ctx.error(
                name_pos,
                name_len,
                format_message(diagnostic_messages::CANNOT_FIND_NAME, &[&name]),
                diagnostic_codes::CANNOT_FIND_NAME,
            );
        }
        for open_pos in
            Self::malformed_jsdoc_satisfies_positions(source_text, comment_pos, comment_end)
        {
            self.ctx.error(
                open_pos,
                0,
                format_message(diagnostic_messages::EXPECTED, &["{"]),
                diagnostic_codes::EXPECTED,
            );
        }
    }

    /// Extract `@param {BareType}` and `@return {BareType}` / `@returns {BareType}` type
    /// expressions from a raw JSDoc comment block (the full `/**...*/` text slice).
    ///
    /// Returns `(type_expr_str, byte_offset_from_comment_start)` for each match where
    /// the type expression is a simple identifier (no `<`, `|`, `&`, `[`, `(`, `.`, spaces).
    /// Only bare names are returned; already-parameterized types like `Array<T>` are excluded.
    pub(crate) fn jsdoc_bare_param_return_type_spans(comment_text: &str) -> Vec<(String, usize)> {
        let mut results = Vec::new();
        let mut line_offset = 0usize;
        for raw_line in comment_text.split_inclusive('\n') {
            let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
            let trimmed = line.trim().trim_start_matches('*').trim();
            let is_param_or_return = Self::strip_jsdoc_tag_prefix(trimmed, "param").is_some()
                || Self::strip_jsdoc_return_tag_prefix(trimmed).is_some();
            if is_param_or_return && let Some(open_pos_in_line) = line.find('{') {
                let after_open = &line[open_pos_in_line + 1..];
                if let Some(close_rel) = after_open.find('}') {
                    let raw_type = &after_open[..close_rel];
                    let type_expr = raw_type.trim();
                    if !type_expr.is_empty()
                        && !type_expr.contains('<')
                        && !type_expr.contains('|')
                        && !type_expr.contains('&')
                        && !type_expr.contains('[')
                        && !type_expr.contains('(')
                        && !type_expr.contains('.')
                        && !type_expr.contains(' ')
                        && !type_expr.contains('\t')
                    {
                        let ws_before = raw_type.len().saturating_sub(raw_type.trim_start().len());
                        let type_expr_offset = line_offset + open_pos_in_line + 1 + ws_before;
                        results.push((type_expr.to_string(), type_expr_offset));
                    }
                }
            }
            line_offset += raw_line.len();
        }
        results
    }

    pub(crate) fn jsdoc_param_return_type_spans(comment_text: &str) -> Vec<(String, usize)> {
        let mut results = Vec::new();
        let mut line_offset = 0usize;
        for raw_line in comment_text.split_inclusive('\n') {
            let line = raw_line.trim_end_matches('\n').trim_end_matches('\r');
            // Strip both single-line (`/** … */`) and multi-line (`* …`)
            // JSDoc framing so single-line JSDoc tags like
            // `/** @param {T} y */` are recognised. Without the `/**` /
            // `*/` strip, the per-line `@param` detection below missed
            // every single-line JSDoc, leaking diagnostics like #3506.
            let trimmed = line
                .trim()
                .trim_start_matches("/**")
                .trim_start_matches('*')
                .trim_end_matches("*/")
                .trim();
            let is_param_or_return = trimmed.starts_with("@param ")
                || trimmed.starts_with("@param\t")
                || trimmed.starts_with("@param{")
                || trimmed.starts_with("@returns ")
                || trimmed.starts_with("@returns\t")
                || trimmed.starts_with("@returns{")
                || trimmed.starts_with("@return ")
                || trimmed.starts_with("@return\t")
                || trimmed.starts_with("@return{");
            if is_param_or_return && let Some(open_pos_in_line) = line.find('{') {
                let after_open = &line[open_pos_in_line + 1..];
                // Balance nested braces so `@param {{ a: T }} obj` extracts
                // the full `{ a: T }` body, not just `{ a: T`.
                let mut depth = 1usize;
                let mut close_rel = None;
                for (i, ch) in after_open.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                close_rel = Some(i);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(close_rel) = close_rel {
                    let raw_type = &after_open[..close_rel];
                    let type_expr = raw_type.trim();
                    if !type_expr.is_empty() {
                        let ws_before = raw_type.len().saturating_sub(raw_type.trim_start().len());
                        let type_expr_offset = line_offset + open_pos_in_line + 1 + ws_before;
                        results.push((type_expr.to_string(), type_expr_offset));
                    }
                }
            }
            line_offset += raw_line.len();
        }
        results
    }

    /// Byte positions where a `@satisfies` tag is missing its required `{TypeExpression}`
    /// braces entirely (no `{` follows the tag at all). `tsc`'s JSDoc tag parser never enters
    /// brace-parsing mode in this shape, so it reports a single `'{' expected` per occurrence —
    /// there is no opened brace to pair with a `'}' expected` companion.
    fn malformed_jsdoc_satisfies_positions(
        source_text: &str,
        comment_pos: u32,
        comment_end: u32,
    ) -> Vec<u32> {
        let raw = &source_text[comment_pos as usize..comment_end as usize];
        let mut result = Vec::new();
        for tag_start in Self::jsdoc_tag_offsets(raw, "satisfies") {
            let after_tag = tag_start + "@satisfies".len();
            let ws_trimmed = raw[after_tag..].trim_start_matches(char::is_whitespace);
            let skipped = raw[after_tag..].len() - ws_trimmed.len();
            if !ws_trimmed.starts_with('{') {
                let open_pos = comment_pos + (after_tag + skipped) as u32;
                result.push(open_pos);
            }
        }
        result
    }

    fn malformed_jsdoc_satisfies_unexpected_names(
        source_text: &str,
        comment_pos: u32,
        comment_end: u32,
    ) -> Vec<(u32, u32, String)> {
        let raw = &source_text[comment_pos as usize..comment_end as usize];
        let mut result = Vec::new();
        for tag_start in Self::jsdoc_tag_offsets(raw, "satisfies") {
            let after_tag = tag_start + "@satisfies".len();
            let ws_trimmed = raw[after_tag..].trim_start_matches(char::is_whitespace);
            let skipped = raw[after_tag..].len() - ws_trimmed.len();
            if ws_trimmed.starts_with('{') {
                continue;
            }

            let name_start = after_tag + skipped;
            let mut name_end = name_start;
            for (offset, ch) in raw[name_start..].char_indices() {
                let is_first = offset == 0;
                let is_name_char = if is_first {
                    ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
                } else {
                    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
                };
                if !is_name_char {
                    break;
                }
                name_end = name_start + offset + ch.len_utf8();
            }
            if name_end > name_start {
                result.push((
                    comment_pos + name_start as u32,
                    (name_end - name_start) as u32,
                    raw[name_start..name_end].to_string(),
                ));
            }
        }
        result
    }
}
