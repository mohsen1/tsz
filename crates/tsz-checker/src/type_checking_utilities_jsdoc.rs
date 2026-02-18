//! JSDoc type annotation utilities, class and enum helper methods for `CheckerState`.
//! Split from `type_checking_utilities.rs` for maintainability.

use crate::query_boundaries::type_checking_utilities as query;
use crate::state::{CheckerState, EnumKind, MemberAccessLevel};
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

#[derive(Clone)]
struct JsdocTypedefInfo {
    base_type: Option<String>,
    properties: Vec<(String, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimitiveOverlapKind {
    String,
    Number,
    BigInt,
    Boolean,
    Symbol,
}

#[derive(Clone, Copy, Debug)]
enum SimpleOverlapType {
    Primitive(PrimitiveOverlapKind),
    StringLiteral(tsz_common::interner::Atom),
    NumberLiteral(f64),
    BigIntLiteral(tsz_common::interner::Atom),
    BooleanLiteral(bool),
}

impl<'a> CheckerState<'a> {
    // ============================================================================
    // Section 57: JSDoc Type Annotation Utilities
    // ============================================================================

    /// Resolve a typeof type reference to its actual type.
    ///
    /// This function resolves `typeof X` type queries to the type of symbol X.
    /// It handles both direct typeof queries and typeof queries applied to
    /// type applications (generics).
    ///
    /// ## Parameters:
    /// - `type_id`: The type to resolve (may be a `TypeQuery` or Application)
    ///
    /// ## Returns:
    /// - The resolved type if `type_id` is a typeof query
    /// - The original `type_id` if it's not a typeof query
    ///
    /// ## Examples:
    /// ```typescript
    /// class C {}
    /// type T1 = typeof C;  // C (the class type)
    /// type T2 = typeof<C>;  // Same as above
    /// ```
    pub(crate) fn resolve_type_query_type(&mut self, type_id: TypeId) -> TypeId {
        use tsz_binder::SymbolId;
        use tsz_solver::SymbolRef;
        let factory = self.ctx.types.factory();

        match query::classify_type_query(self.ctx.types, type_id) {
            query::TypeQueryKind::TypeQuery(SymbolRef(sym_id)) => {
                // Check for cycle in typeof resolution (scoped borrow)
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    // Cycle detected - return ERROR to prevent infinite loop
                    return TypeId::ERROR;
                }

                // Mark as visiting (use try_borrow_mut to avoid panic on nested borrow)
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }

                // Resolve the symbol type
                let result = self.get_type_of_symbol(SymbolId(sym_id));

                // Unmark after resolution
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }

                result
            }
            query::TypeQueryKind::ApplicationWithTypeQuery {
                base_sym_ref: SymbolRef(sym_id),
                args,
            } => {
                // Check for cycle in typeof resolution (scoped borrow)
                let is_cycle = { self.ctx.typeof_resolution_stack.borrow().contains(&sym_id) };
                if is_cycle {
                    return TypeId::ERROR;
                }

                // Mark as visiting (use try_borrow_mut to avoid panic on nested borrow)
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.insert(sym_id);
                }

                // Resolve the base type
                let base = self.get_type_of_symbol(SymbolId(sym_id));

                // Unmark after resolution
                if let Ok(mut stack) = self.ctx.typeof_resolution_stack.try_borrow_mut() {
                    stack.remove(&sym_id);
                }

                factory.application(base, args)
            }
            query::TypeQueryKind::Application { .. } | query::TypeQueryKind::Other => type_id,
        }
    }

    /// Get `JSDoc` type annotation for a node.
    ///
    /// This function extracts and parses `JSDoc` `@type` annotations for a given node.
    /// It searches for the enclosing source file, extracts `JSDoc` comments,
    /// and parses the type annotation.
    ///
    /// ## Parameters:
    /// - `idx`: The node to get `JSDoc` type annotation for
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The parsed type from `JSDoc`
    /// - `None`: If no `JSDoc` type annotation exists
    ///
    /// ## Example:
    /// ```typescript
    /// /**
    ///  * @type {string} x - The parameter type
    ///  */
    /// function foo(x) {}
    /// // The JSDoc annotation can be used for type inference
    /// ```
    pub(crate) fn jsdoc_type_annotation_for_node(&mut self, idx: NodeIndex) -> Option<TypeId> {
        let is_js_file = self.ctx.file_name.ends_with(".js")
            || self.ctx.file_name.ends_with(".jsx")
            || self.ctx.file_name.ends_with(".mjs")
            || self.ctx.file_name.ends_with(".cjs");
        if is_js_file && !self.ctx.compiler_options.check_js {
            return None;
        }

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let node = self.ctx.arena.get(idx)?;
        let mut jsdoc = self.try_leading_jsdoc(comments, node.pos, source_text);
        if jsdoc.is_none() {
            let mut current = idx;
            for _ in 0..4 {
                let Some(ext) = self.ctx.arena.get_extended(current) else {
                    break;
                };
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                let Some(parent_node) = self.ctx.arena.get(parent) else {
                    break;
                };
                jsdoc = self.try_leading_jsdoc(comments, parent_node.pos, source_text);
                if jsdoc.is_some() {
                    break;
                }
                current = parent;
            }
        }
        let jsdoc = jsdoc?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let type_expr = type_expr.trim();

        self.jsdoc_type_from_expression(type_expr).or_else(|| {
            self.resolve_jsdoc_typedef_type(type_expr, idx, node.pos, comments, source_text)
                .or_else(|| {
                    if let Some((module_specifier, member_name)) =
                        Self::parse_jsdoc_import_type(type_expr)
                        && let Some(sym_id) =
                            self.resolve_cross_file_export(&module_specifier, &member_name)
                    {
                        let resolved = self.type_reference_symbol_type(sym_id);
                        if resolved != TypeId::ERROR {
                            return Some(resolved);
                        }
                    }
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(type_expr) {
                        let symbol = self.ctx.binder.get_symbol(sym_id)?;
                        if (symbol.flags & symbol_flags::TYPE_ALIAS) != 0
                            || (symbol.flags & symbol_flags::CLASS) != 0
                            || (symbol.flags & symbol_flags::INTERFACE) != 0
                            || (symbol.flags & symbol_flags::ENUM) != 0
                        {
                            let resolved = self.type_reference_symbol_type(sym_id);
                            if resolved != TypeId::ERROR {
                                return Some(resolved);
                            }
                        }
                    }
                    None
                })
        })
    }

    /// Resolve a direct leading JSDoc `@type` annotation for a node (no parent fallback).
    ///
    /// Unlike `jsdoc_type_annotation_for_node`, this only considers comments attached
    /// to `idx` itself and never climbs ancestors. Use this in sites where parent
    /// fallback can incorrectly pull unrelated JSDoc.
    pub(crate) fn jsdoc_type_annotation_for_node_direct(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        let is_js_file = self.ctx.file_name.ends_with(".js")
            || self.ctx.file_name.ends_with(".jsx")
            || self.ctx.file_name.ends_with(".mjs")
            || self.ctx.file_name.ends_with(".cjs");
        if is_js_file && !self.ctx.compiler_options.check_js {
            return None;
        }

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;
        let node = self.ctx.arena.get(idx)?;
        let jsdoc = self.try_leading_jsdoc(comments, node.pos, source_text)?;
        let type_expr = Self::extract_jsdoc_type_expression(&jsdoc)?;
        let type_expr = type_expr.trim();

        self.jsdoc_type_from_expression(type_expr).or_else(|| {
            self.resolve_jsdoc_typedef_type(type_expr, idx, node.pos, comments, source_text)
                .or_else(|| {
                    if let Some((module_specifier, member_name)) =
                        Self::parse_jsdoc_import_type(type_expr)
                        && let Some(sym_id) =
                            self.resolve_cross_file_export(&module_specifier, &member_name)
                    {
                        let resolved = self.type_reference_symbol_type(sym_id);
                        if resolved != TypeId::ERROR {
                            return Some(resolved);
                        }
                    }
                    if let Some(sym_id) = self.ctx.binder.file_locals.get(type_expr) {
                        let symbol = self.ctx.binder.get_symbol(sym_id)?;
                        if (symbol.flags & symbol_flags::TYPE_ALIAS) != 0
                            || (symbol.flags & symbol_flags::CLASS) != 0
                            || (symbol.flags & symbol_flags::INTERFACE) != 0
                            || (symbol.flags & symbol_flags::TYPE_PARAMETER) != 0
                            || (symbol.flags & symbol_flags::FUNCTION_SCOPED_VARIABLE) != 0
                            || (symbol.flags & symbol_flags::BLOCK_SCOPED_VARIABLE) != 0
                        {
                            let t = self.type_reference_symbol_type(sym_id);
                            if t != TypeId::ERROR {
                                return Some(t);
                            }
                        }
                    }
                    None
                })
        })
    }

    fn parse_jsdoc_import_type(type_expr: &str) -> Option<(String, String)> {
        let expr = type_expr.trim();
        let rest = expr.strip_prefix("import(")?;
        let mut rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' && quote != '`' {
            return None;
        }

        rest = &rest[quote.len_utf8()..];
        let close_quote = rest.find(quote)?;
        let module_specifier = rest[..close_quote].trim().to_string();
        let after_quote = rest[close_quote + quote.len_utf8()..].trim_start();
        let after_quote = after_quote.strip_prefix(')')?;
        let after_dot = after_quote.trim_start().strip_prefix('.')?;
        let after_dot = after_dot.trim_start();

        let mut end = 0usize;
        for (idx, ch) in after_dot.char_indices() {
            if idx == 0 {
                if !ch.is_ascii_alphabetic() && ch != '_' && ch != '$' {
                    return None;
                }
            } else if !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$' {
                break;
            }
            end = idx + ch.len_utf8();
        }
        if end == 0 {
            return None;
        }

        Some((module_specifier, after_dot[..end].to_string()))
    }

    /// Parse a JSDoc-style `@type` expression into a concrete type.
    ///
    /// Supports a constrained subset needed for conformance tests:
    /// primitives, type parameters, `keyof typeof`, type references,
    /// and fallback symbol resolution.
    fn jsdoc_type_from_expression(&mut self, type_expr: &str) -> Option<TypeId> {
        let type_expr = type_expr.trim();
        let factory = self.ctx.types.factory();

        match type_expr {
            "string" => Some(TypeId::STRING),
            "number" => Some(TypeId::NUMBER),
            "boolean" => Some(TypeId::BOOLEAN),
            "object" => Some(TypeId::OBJECT),
            "any" => Some(TypeId::ANY),
            "unknown" => Some(TypeId::UNKNOWN),
            "undefined" => Some(TypeId::UNDEFINED),
            "null" => Some(TypeId::NULL),
            "void" => Some(TypeId::VOID),
            "never" => Some(TypeId::NEVER),
            _ => {
                if let Some(tp) = self.ctx.type_parameter_scope.get(type_expr) {
                    return Some(*tp);
                }

                // Narrow support for conformance-critical pattern:
                //   @type {keyof typeof <identifier>}
                if let Some(rest) = type_expr.strip_prefix("keyof") {
                    let rest = rest.trim_start();
                    if let Some(name) = rest.strip_prefix("typeof") {
                        let name = name.trim();
                        if !name.is_empty()
                            && name
                                .chars()
                                .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                        {
                            let symbols = self.ctx.binder.get_symbols();
                            let candidates = symbols.find_all_by_name(name);
                            for sym_id in candidates {
                                let Some(sym) = symbols.get(sym_id) else {
                                    continue;
                                };
                                let value_mask = symbol_flags::FUNCTION_SCOPED_VARIABLE
                                    | symbol_flags::BLOCK_SCOPED_VARIABLE
                                    | symbol_flags::FUNCTION
                                    | symbol_flags::CLASS
                                    | symbol_flags::ENUM
                                    | symbol_flags::VALUE_MODULE;
                                if (sym.flags & value_mask) == 0 {
                                    continue;
                                }
                                let operand = self.get_type_of_symbol(sym_id);
                                if operand == TypeId::ERROR {
                                    continue;
                                }
                                let keyof = factory.keyof(operand);
                                return Some(self.judge_evaluate(keyof));
                            }
                        }
                    }
                }

                None
            }
        }
    }

    /// Resolve a typedef referenced by a `JSDoc` type annotation (e.g., `Foo`) from
    /// preceding `@typedef` declarations in the same file.
    fn resolve_jsdoc_typedef_type(
        &mut self,
        type_expr: &str,
        anchor_idx: NodeIndex,
        anchor_pos: u32,
        comments: &[tsz_common::comments::CommentRange],
        source_text: &str,
    ) -> Option<TypeId> {
        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let anchor_scopes = self.function_scope_ancestors(anchor_idx);
        let mut best_def: Option<(u32, JsdocTypedefInfo)> = None;

        for comment in comments {
            if comment.end > anchor_pos {
                continue;
            }
            if !is_jsdoc_comment(comment, source_text) {
                continue;
            }
            if let Some(comment_scope) = self.function_scope_for_position(comment.pos)
                && !anchor_scopes.contains(&comment_scope)
            {
                continue;
            }

            let content = get_jsdoc_content(comment, source_text);
            for (name, typedef_info) in Self::parse_jsdoc_typedefs(&content) {
                if name != type_expr {
                    continue;
                }
                best_def = Some((comment.pos, typedef_info));
            }
        }

        let (_, typedef_info) = best_def?;
        self.type_from_jsdoc_typedef(typedef_info)
    }

    fn function_scope_for_position(&self, pos: u32) -> Option<NodeIndex> {
        let mut best: Option<(u32, NodeIndex)> = None;
        for (idx, node) in self.ctx.arena.nodes.iter().enumerate() {
            if !node.is_function_like() {
                continue;
            }
            if node.pos <= pos
                && pos <= node.end
                && best
                    .as_ref()
                    .is_none_or(|(best_pos, _)| *best_pos < node.pos)
            {
                best = Some((node.pos, NodeIndex(idx as u32)));
            }
        }

        best.map(|(_, idx)| idx)
    }

    fn function_scope_ancestors(&self, anchor_idx: NodeIndex) -> Vec<NodeIndex> {
        let mut scopes = Vec::new();
        let mut current = anchor_idx;
        while !current.is_none() {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };

            if node.is_function_like() {
                scopes.push(current);
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            current = ext.parent;
        }

        scopes
    }

    fn type_from_jsdoc_typedef(&mut self, info: JsdocTypedefInfo) -> Option<TypeId> {
        let factory = self.ctx.types.factory();
        let mut prop_infos = Vec::with_capacity(info.properties.len());

        for (name, prop_type_expr) in info.properties {
            let prop_type = if prop_type_expr.trim().is_empty() {
                TypeId::ANY
            } else {
                self.jsdoc_type_from_expression(&prop_type_expr)
                    .unwrap_or(TypeId::ANY)
            };
            let name_atom = self.ctx.types.intern_string(&name);
            prop_infos.push(PropertyInfo {
                name: name_atom,
                type_id: prop_type,
                write_type: prop_type,
                optional: false,
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: None,
            });
        }

        if !prop_infos.is_empty() {
            return Some(factory.object(prop_infos));
        }

        if let Some(base_type_expr) = info.base_type {
            self.jsdoc_type_from_expression(&base_type_expr)
        } else {
            None
        }
    }

    fn parse_jsdoc_typedefs(jsdoc: &str) -> Vec<(String, JsdocTypedefInfo)> {
        let mut typedefs = Vec::new();
        let mut current_name: Option<String> = None;
        let mut current_info = JsdocTypedefInfo {
            base_type: None,
            properties: Vec::new(),
        };

        for raw_line in jsdoc.lines() {
            let line = raw_line.trim_start_matches('*').trim();
            if line.is_empty() || !line.starts_with('@') {
                continue;
            }

            if let Some(rest) = line.strip_prefix("@typedef") {
                if let Some((name, base_type)) = Self::parse_jsdoc_typedef_definition(rest) {
                    if let Some(previous_name) = current_name.take() {
                        typedefs.push((previous_name, current_info));
                        current_info = JsdocTypedefInfo {
                            base_type: None,
                            properties: Vec::new(),
                        };
                    }
                    current_name = Some(name);
                    current_info.base_type = base_type;
                    current_info.properties.clear();
                }
                continue;
            }

            if let Some((name, prop_type)) = Self::parse_jsdoc_property_type(line)
                && current_name.is_some()
            {
                current_info.properties.push((name, prop_type));
            }
        }

        if let Some(previous_name) = current_name.take() {
            typedefs.push((previous_name, current_info));
        }
        typedefs
    }

    fn parse_jsdoc_typedef_definition(line: &str) -> Option<(String, Option<String>)> {
        let mut rest = line.trim();
        if rest.is_empty() {
            return None;
        }

        let base_type = if rest.starts_with('{') {
            let (expr, after_expr) = Self::parse_jsdoc_curly_type_expr(rest)?;
            rest = after_expr.trim();
            Some(expr.trim().to_string())
        } else {
            None
        };

        let name = rest.split_whitespace().next()?;
        Some((name.to_string(), base_type))
    }

    fn parse_jsdoc_property_type(line: &str) -> Option<(String, String)> {
        let mut rest = line.trim();
        if !rest.starts_with("@property") {
            return None;
        }
        rest = &rest["@property".len()..];
        rest = rest.trim();

        let prop_type = if rest.starts_with('{') {
            let (expr, after_expr) = Self::parse_jsdoc_curly_type_expr(rest)?;
            rest = after_expr.trim();
            expr.trim().to_string()
        } else {
            "any".to_string()
        };

        let name = rest
            .split_whitespace()
            .next()
            .map(|name| {
                name.trim_end_matches(',')
                    .trim()
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split('=')
                    .next()
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .filter(|name| !name.is_empty())?;

        Some((name, prop_type))
    }

    fn parse_jsdoc_curly_type_expr(line: &str) -> Option<(&str, &str)> {
        if !line.starts_with('{') {
            return None;
        }
        let mut depth = 0usize;
        for (idx, ch) in line.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((&line[1..idx], &line[idx + 1..]));
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn extract_jsdoc_type_expression(jsdoc: &str) -> Option<&str> {
        let tag_pos = jsdoc.find("@type")?;
        let rest = &jsdoc[tag_pos + "@type".len()..];
        let open = rest.find('{')?;
        let after_open = &rest[open + 1..];
        let mut depth = 1usize;
        let mut end_idx = None;
        for (i, ch) in after_open.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end_idx = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end_idx = end_idx?;
        Some(after_open[..end_idx].trim())
    }

    // =========================================================================
    // JSDoc Helpers for Implicit Any Suppression
    // =========================================================================

    /// Get the `JSDoc` comment content for a function node.
    ///
    /// Walks up the parent chain from the function node to find the `JSDoc`
    /// comment. For variable-assigned functions (e.g., `const f = () => {}`),
    /// the `JSDoc` is on the variable statement, not the function itself.
    ///
    /// Returns the raw `JSDoc` content (without `/**` and `*/` delimiters).
    pub(crate) fn get_jsdoc_for_function(&self, func_idx: NodeIndex) -> Option<String> {
        let is_js_file = self.ctx.file_name.ends_with(".js")
            || self.ctx.file_name.ends_with(".jsx")
            || self.ctx.file_name.ends_with(".mjs")
            || self.ctx.file_name.ends_with(".cjs");
        if is_js_file && !self.ctx.compiler_options.check_js {
            return None;
        }

        use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};

        let sf = self.ctx.arena.source_files.first()?;
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        // Try the function node itself first
        let func_node = self.ctx.arena.get(func_idx)?;

        // For inline JSDoc (comment overlapping with node position)
        if let Some(comment) = comments
            .iter()
            .find(|c| c.pos <= func_node.pos && func_node.pos < c.end)
            && is_jsdoc_comment(comment, source_text)
        {
            return Some(get_jsdoc_content(comment, source_text));
        }

        // Try leading comments before the function node
        if let Some(content) = self.try_leading_jsdoc(comments, func_node.pos, source_text) {
            return Some(content);
        }

        // Walk up the parent chain: function -> variable declaration -> variable
        // declaration list -> variable statement, looking for JSDoc at each level.
        // This handles `const f = value => ...` where JSDoc is on the `const` line.
        let mut current = func_idx;
        for _ in 0..4 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                break;
            };
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                break;
            };
            if let Some(content) = self.try_leading_jsdoc(comments, parent_node.pos, source_text) {
                return Some(content);
            }
            current = parent;
        }

        None
    }

    /// Try to find a leading `JSDoc` comment before a given position.
    fn try_leading_jsdoc(
        &self,
        comments: &[tsz_common::comments::CommentRange],
        pos: u32,
        source_text: &str,
    ) -> Option<String> {
        use tsz_common::comments::{
            get_jsdoc_content, get_leading_comments_from_cache, is_jsdoc_comment,
        };

        let leading = get_leading_comments_from_cache(comments, pos, source_text);
        if let Some(comment) = leading.last() {
            let end = comment.end as usize;
            let check = pos as usize;
            if end <= check
                && source_text
                    .get(end..check)
                    .is_some_and(|gap| gap.chars().all(char::is_whitespace))
                && is_jsdoc_comment(comment, source_text)
            {
                return Some(get_jsdoc_content(comment, source_text));
            }
        }
        None
    }

    /// Check if a parameter node has an inline `/** @type {T} */` `JSDoc` annotation.
    ///
    /// In TypeScript, parameters can have inline `JSDoc` type annotations like:
    ///   `function foo(/** @type {string} */ msg, /** @type {number} */ count)`
    /// These annotations suppress TS7006 because the parameter type is provided via `JSDoc`.
    pub(crate) fn param_has_inline_jsdoc_type(&self, param_idx: NodeIndex) -> bool {
        let sf = match self.ctx.arena.source_files.first() {
            Some(sf) => sf,
            None => return false,
        };
        let source_text: &str = &sf.text;
        let comments = &sf.comments;

        let param_node = match self.ctx.arena.get(param_idx) {
            Some(n) => n,
            None => return false,
        };

        // Look for a JSDoc comment that ends right before or overlaps the parameter position
        if let Some(content) = self.try_leading_jsdoc(comments, param_node.pos, source_text) {
            // Check if the JSDoc contains @type {something}
            return content.contains("@type");
        }

        false
    }

    /// Check if a `JSDoc` comment has a `@param {type}` annotation for the given parameter name.
    ///
    /// Returns true if the `JSDoc` contains `@param {someType} paramName`.
    pub(crate) fn jsdoc_has_param_type(jsdoc: &str, param_name: &str) -> bool {
        // JSDoc @param may span multiple lines. Collect all text after each @param
        // and process them. We also need to handle nested braces in types like
        // @param {{ x: T, y: T}} obj
        let mut in_param = false;
        let mut param_text = String::new();

        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();

            // Skip backtick-quoted sections to find real @-tags.
            // Lines like `` `@param` @param {string} z `` have a real @param after backticks.
            let effective = Self::skip_backtick_quoted(trimmed);

            // Check if this line starts a new @tag
            if effective.starts_with('@') {
                // Process any accumulated @param text
                if in_param {
                    if Self::check_param_text(&param_text, param_name) {
                        return true;
                    }
                    param_text.clear();
                }
                if let Some(rest) = effective.strip_prefix("@param") {
                    in_param = true;
                    param_text = rest.to_string();
                } else {
                    in_param = false;
                }
            } else if in_param {
                // Continuation line for multi-line @param
                param_text.push(' ');
                param_text.push_str(trimmed);
            }
        }
        // Process the last @param if any
        if in_param && Self::check_param_text(&param_text, param_name) {
            return true;
        }
        false
    }

    /// Skip leading backtick-quoted sections in a `JSDoc` line.
    ///
    /// Lines like `` `@param` @param {string} z `` contain backtick-quoted text
    /// before the real `@param` tag. This function strips those leading quoted
    /// sections so the real tag can be detected.
    fn skip_backtick_quoted(s: &str) -> &str {
        let mut rest = s;
        loop {
            rest = rest.trim_start();
            if rest.starts_with('`') {
                // Find matching closing backtick
                if let Some(end) = rest[1..].find('`') {
                    rest = &rest[end + 2..];
                    continue;
                }
            }
            break;
        }
        rest
    }

    /// Helper to check if a @param text (after "@param") matches a parameter name.
    /// Handles nested braces in type expressions like `{{ x: T, y: T}}`.
    fn check_param_text(text: &str, param_name: &str) -> bool {
        let rest = text.trim();
        // Must have a type in braces: @param {type} name
        if !rest.starts_with('{') {
            return false;
        }
        // Find matching closing brace, handling nesting
        let mut depth = 0;
        let mut brace_end = None;
        for (i, ch) in rest.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        brace_end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(brace_end) = brace_end else {
            return false;
        };
        // Extract name after the type
        let after_type = rest[brace_end + 1..].trim();
        // The name is the first word (may be followed by description)
        let name = after_type.split_whitespace().next().unwrap_or("");
        // Handle [name] and [name=default] syntax
        let name = name.trim_start_matches('[');
        let name = name.split('=').next().unwrap_or(name);
        let name = name.trim_end_matches(']');
        // Handle backtick-quoted names like `args`
        let name = name.trim_matches('`');
        if name == param_name {
            return true;
        }
        false
    }

    /// Check if a `JSDoc` comment has any type annotations (`@param {type}`, `@returns {type}`,
    /// `@type {type}`, or `@template`).
    ///
    /// In tsc, when a function has `JSDoc` type annotations, implicit any errors (TS7010/TS7011)
    /// are suppressed even without explicit `@returns`, because the developer is providing
    /// type information through `JSDoc`.
    pub(crate) fn jsdoc_has_type_annotations(jsdoc: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim();
            // @param {type} name
            if let Some(rest) = trimmed.strip_prefix("@param")
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @returns {type} or @return {type}
            if let Some(rest) = trimmed
                .strip_prefix("@returns")
                .or_else(|| trimmed.strip_prefix("@return"))
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @type {type}
            if let Some(rest) = trimmed.strip_prefix("@type")
                && rest.trim().starts_with('{')
            {
                return true;
            }
            // @template T
            if trimmed.starts_with("@template") {
                return true;
            }
        }
        false
    }

    /// Check if a `JSDoc` comment has a `@type {expr}` tag.
    ///
    /// When `@type` declares a full function type (e.g., `@type {function((string)): string}`),
    /// all parameters are typed and TS7006 should be suppressed.
    pub(crate) fn jsdoc_has_type_tag(jsdoc: &str) -> bool {
        for line in jsdoc.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("@type")
                && rest.trim().starts_with('{')
            {
                return true;
            }
        }
        false
    }

    /// Extract `@template` type parameter names from a `JSDoc` comment.
    ///
    /// Supports simple forms like:
    /// - `@template T`
    /// - `@template T,U`
    /// - `@template T U`
    pub(crate) fn jsdoc_template_type_params(jsdoc: &str) -> Vec<String> {
        let mut out = Vec::new();
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed.strip_prefix("@template") else {
                continue;
            };
            for token in rest.split([',', ' ', '\t']) {
                let name = token.trim();
                if name.is_empty() {
                    continue;
                }
                if name
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
                    && !out.iter().any(|existing| existing == name)
                {
                    out.push(name.to_string());
                }
            }
        }
        out
    }

    /// Extract a simple identifier from `@returns {T}` / `@return {T}`.
    ///
    /// Returns `None` for complex type expressions.
    pub(crate) fn jsdoc_returns_type_name(jsdoc: &str) -> Option<String> {
        for line in jsdoc.lines() {
            let trimmed = line.trim().trim_start_matches('*').trim();
            let Some(rest) = trimmed
                .strip_prefix("@returns")
                .or_else(|| trimmed.strip_prefix("@return"))
            else {
                continue;
            };
            let rest = rest.trim_start();
            if !rest.starts_with('{') {
                continue;
            }
            let after_open = &rest[1..];
            let end = after_open.find('}')?;
            let type_expr = after_open[..end].trim();
            if !type_expr.is_empty()
                && type_expr
                    .chars()
                    .all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
            {
                return Some(type_expr.to_string());
            }
        }
        None
    }

    // =========================================================================
    // Class Helper Methods
    // =========================================================================

    /// Check if a class has a base class (extends clause).
    ///
    /// Returns true if the class has any heritage clause with `extends` keyword.
    pub(crate) fn class_has_base(&self, class: &tsz_parser::parser::node::ClassData) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                return true;
            }
        }

        false
    }

    /// Check whether a class extends `null`.
    pub(crate) fn class_extends_null(&self, class: &tsz_parser::parser::node::ClassData) -> bool {
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class.heritage_clauses else {
            return false;
        };

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

            let Some(&first_type_idx) = heritage.types.nodes.first() else {
                continue;
            };

            let expr_idx = if let Some(type_node) = self.ctx.arena.get(first_type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                expr_type_args.expression
            } else {
                first_type_idx
            };

            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };

            if expr_node.kind == SyntaxKind::NullKeyword as u16 {
                return true;
            }

            if expr_node.kind == SyntaxKind::Identifier as u16
                && self
                    .ctx
                    .arena
                    .get_identifier(expr_node)
                    .is_some_and(|id| id.escaped_text == "null")
            {
                return true;
            }
        }

        false
    }

    /// Check whether a class declaration merges with an interface declaration
    /// that has an extends clause.
    pub(crate) fn class_has_merged_interface_extends(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        if class.name.is_none() {
            return false;
        }

        let Some(name_node) = self.ctx.arena.get(class.name) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };

        let Some(sym_id) = self.ctx.binder.file_locals.get(&name_ident.escaped_text) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::INTERFACE_DECLARATION {
                continue;
            }
            let Some(iface) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };
            let Some(heritage_clauses) = &iface.heritage_clauses else {
                continue;
            };
            if !heritage_clauses.nodes.is_empty() {
                return true;
            }
        }

        false
    }

    /// Check whether a class requires a `super()` call in its constructor.
    pub(crate) fn class_requires_super_call(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        self.class_has_base(class) && !self.class_extends_null(class)
    }

    /// Check whether a class has features that require strict `super()` placement checks.
    ///
    /// Matches TypeScript diagnostics TS2376/TS2401 trigger conditions:
    /// initialized instance properties, constructor parameter properties,
    /// or private identifiers.
    pub(crate) fn class_has_super_call_position_sensitive_members(
        &self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };

                    if self.is_private_identifier_name(prop.name) {
                        return true;
                    }

                    if !self.has_static_modifier(&prop.modifiers) && !prop.initializer.is_none() {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.is_private_identifier_name(method.name) {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.is_private_identifier_name(accessor.name) {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };

                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };

                        if self.has_parameter_property_modifier(&param.modifiers) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Find the constructor body in a class member list.
    ///
    /// Returns the body node of the first constructor member that has a body.
    pub(crate) fn find_constructor_body(
        &self,
        members: &tsz_parser::parser::NodeList,
    ) -> Option<NodeIndex> {
        for &member_idx in &members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };
            if !ctor.body.is_none() {
                return Some(ctor.body);
            }
        }
        None
    }

    // =========================================================================
    // Enum Helper Functions
    // =========================================================================

    /// Get the enum symbol from a type reference.
    ///
    /// Returns the symbol ID if the type refers to an enum, None otherwise.
    pub(crate) fn enum_symbol_from_type(&self, type_id: TypeId) -> Option<SymbolId> {
        // Phase 4.2: Use resolve_type_to_symbol_id instead of get_ref_symbol
        let sym_id = self.ctx.resolve_type_to_symbol_id(type_id)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }
        Some(sym_id)
    }

    /// Determine the kind of enum (string, numeric, or mixed).
    ///
    /// Returns None if the symbol is not an enum or has no members.
    pub(crate) fn enum_kind(&self, sym_id: SymbolId) -> Option<EnumKind> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let enum_decl = self.ctx.arena.get_enum_at(decl_idx)?;

        let mut saw_string = false;
        let mut saw_numeric = false;

        for &member_idx in &enum_decl.members.nodes {
            let Some(member) = self.ctx.arena.get_enum_member_at(member_idx) else {
                continue;
            };

            if !member.initializer.is_none() {
                let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                    continue;
                };
                match init_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => saw_string = true,
                    k if k == SyntaxKind::NumericLiteral as u16 => saw_numeric = true,
                    _ => {}
                }
            } else {
                saw_numeric = true;
            }
        }

        if saw_string && saw_numeric {
            Some(EnumKind::Mixed)
        } else if saw_string {
            Some(EnumKind::String)
        } else {
            Some(EnumKind::Numeric)
        }
    }

    /// Get the literal type of an enum member from its initializer.
    ///
    /// Returns the literal type (e.g., Literal(0), Literal("a")) of the enum member.
    /// This is used to create `TypeData::Enum(member_def_id`, `literal_type`) for nominal typing.
    pub(crate) fn enum_member_type_from_decl(&self, member_decl: NodeIndex) -> TypeId {
        let factory = self.ctx.types.factory();
        // Get the member node
        let Some(member_node) = self.ctx.arena.get(member_decl) else {
            return TypeId::ERROR;
        };
        let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
            return TypeId::ERROR;
        };

        // Check if member has an explicit initializer
        if !member.initializer.is_none() {
            let Some(init_node) = self.ctx.arena.get(member.initializer) else {
                return TypeId::ERROR;
            };

            match init_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => {
                    // Get the string literal value
                    if let Some(lit) = self.ctx.arena.get_literal(init_node) {
                        return factory.literal_string(&lit.text);
                    }
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    // Get the numeric literal value
                    if let Some(lit) = self.ctx.arena.get_literal(init_node) {
                        // lit.value is Option<f64>, use it if available
                        if let Some(value) = lit.value {
                            return factory.literal_number(value);
                        }
                        // Fallback: parse from text
                        if let Ok(value) = lit.text.parse::<f64>() {
                            return factory.literal_number(value);
                        }
                    }
                }
                _ => {
                    // Try to evaluate constant expression
                    if let Some(value) = self.evaluate_constant_expression(member.initializer) {
                        return factory.literal_number(value);
                    }
                }
            }
        }

        // No explicit initializer or computed value
        // This could be an auto-incremented numeric member
        // Fall back to NUMBER type (not a specific literal)
        TypeId::NUMBER
    }

    /// Evaluate a constant numeric expression (for enum member initializers).
    ///
    /// Handles: numeric literals, unary +/-/~, binary +/-/*/ // /%/|/&/^/<</>>/>>>,
    /// and parenthesized expressions. Returns None if the expression cannot be
    /// evaluated at compile time.
    fn evaluate_constant_expression(&self, expr_idx: NodeIndex) -> Option<f64> {
        let node = self.ctx.arena.get(expr_idx)?;
        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                let lit = self.ctx.arena.get_literal(node)?;
                lit.value.or_else(|| lit.text.parse::<f64>().ok())
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = self.ctx.arena.get_unary_expr(node)?;
                let operand = self.evaluate_constant_expression(unary.operand)?;
                match unary.operator {
                    op if op == SyntaxKind::MinusToken as u16 => Some(-operand),
                    op if op == SyntaxKind::PlusToken as u16 => Some(operand),
                    op if op == SyntaxKind::TildeToken as u16 => Some(!(operand as i32) as f64),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                let bin = self.ctx.arena.get_binary_expr(node)?;
                let left = self.evaluate_constant_expression(bin.left)?;
                let right = self.evaluate_constant_expression(bin.right)?;
                match bin.operator_token {
                    op if op == SyntaxKind::PlusToken as u16 => Some(left + right),
                    op if op == SyntaxKind::MinusToken as u16 => Some(left - right),
                    op if op == SyntaxKind::AsteriskToken as u16 => Some(left * right),
                    op if op == SyntaxKind::SlashToken as u16 => {
                        if right == 0.0 {
                            None
                        } else {
                            Some(left / right)
                        }
                    }
                    op if op == SyntaxKind::PercentToken as u16 => {
                        if right == 0.0 {
                            None
                        } else {
                            Some(left % right)
                        }
                    }
                    op if op == SyntaxKind::BarToken as u16 => {
                        Some((left as i32 | right as i32) as f64)
                    }
                    op if op == SyntaxKind::AmpersandToken as u16 => {
                        Some((left as i32 & right as i32) as f64)
                    }
                    op if op == SyntaxKind::CaretToken as u16 => {
                        Some((left as i32 ^ right as i32) as f64)
                    }
                    op if op == SyntaxKind::LessThanLessThanToken as u16 => {
                        Some(((left as i32) << (right as u32 & 0x1f)) as f64)
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanToken as u16 => {
                        Some(((left as i32) >> (right as u32 & 0x1f)) as f64)
                    }
                    op if op == SyntaxKind::GreaterThanGreaterThanGreaterThanToken as u16 => {
                        Some(((left as u32) >> (right as u32 & 0x1f)) as f64)
                    }
                    op if op == SyntaxKind::AsteriskAsteriskToken as u16 => Some(left.powf(right)),
                    _ => None,
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                self.evaluate_constant_expression(paren.expression)
            }
            _ => None,
        }
    }

    // =========================================================================
    // Class Helper Functions
    // =========================================================================

    /// Get the class symbol from an expression node.
    ///
    /// Returns the symbol ID if the expression refers to a class, None otherwise.
    pub(crate) fn class_symbol_from_expression(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_identifier_symbol(expr_idx)?;
            let symbol = self.ctx.binder.get_symbol(sym_id)?;
            if symbol.flags & symbol_flags::CLASS != 0 {
                return Some(sym_id);
            }
        }
        None
    }

    /// Get the class symbol from a type annotation node.
    ///
    /// Handles type queries like `typeof MyClass`.
    pub(crate) fn class_symbol_from_type_annotation(
        &self,
        type_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let node = self.ctx.arena.get(type_idx)?;
        if node.kind != syntax_kind_ext::TYPE_QUERY {
            return None;
        }
        let query = self.ctx.arena.get_type_query(node)?;
        self.class_symbol_from_expression(query.expr_name)
    }

    /// Get the class symbol from an assignment target.
    ///
    /// Handles cases where the target is a variable with a class type annotation
    /// or initialized with a class expression.
    pub(crate) fn assignment_target_class_symbol(&self, left_idx: NodeIndex) -> Option<SymbolId> {
        let node = self.ctx.arena.get(left_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(left_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS != 0 {
            return Some(sym_id);
        }
        if symbol.flags
            & (symbol_flags::FUNCTION_SCOPED_VARIABLE | symbol_flags::BLOCK_SCOPED_VARIABLE)
            == 0
        {
            return None;
        }
        if symbol.value_declaration.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(symbol.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;
        if !var_decl.type_annotation.is_none()
            && let Some(class_sym) =
                self.class_symbol_from_type_annotation(var_decl.type_annotation)
        {
            return Some(class_sym);
        }
        if !var_decl.initializer.is_none()
            && let Some(class_sym) = self.class_symbol_from_expression(var_decl.initializer)
        {
            return Some(class_sym);
        }
        None
    }

    /// Get the access level of a class constructor.
    ///
    /// Returns `Some(MemberAccessLevel::Private)` or `Some(MemberAccessLevel::Protected)` if restricted.
    /// Returns None if public (the default) or if the symbol is not a class.
    ///
    /// Note: If a class has no explicit constructor, it inherits the access level
    /// from its base class's constructor.
    pub(crate) fn class_constructor_access_level(
        &self,
        sym_id: SymbolId,
    ) -> Option<MemberAccessLevel> {
        let mut visited = rustc_hash::FxHashSet::default();
        self.class_constructor_access_level_inner(sym_id, &mut visited)
    }

    fn class_constructor_access_level_inner(
        &self,
        sym_id: SymbolId,
        visited: &mut rustc_hash::FxHashSet<SymbolId>,
    ) -> Option<MemberAccessLevel> {
        // Cycle detection: bail out if we've already visited this symbol
        if !visited.insert(sym_id) {
            return None;
        }

        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS == 0 {
            return None;
        }
        let decl_idx = if !symbol.value_declaration.is_none() {
            symbol.value_declaration
        } else {
            *symbol.declarations.first()?
        };
        let class = self.ctx.arena.get_class_at(decl_idx)?;

        // First, check if this class has an explicit constructor
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            // Check modifiers for access level
            if self.has_private_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Private);
            }
            if self.has_protected_modifier(&ctor.modifiers) {
                return Some(MemberAccessLevel::Protected);
            }
            // Explicit public constructor - public default
            return None;
        }

        // No explicit constructor found - check base class if extends clause exists
        let Some(ref heritage_clauses) = class.heritage_clauses else {
            // No extends clause - public default
            return None;
        };

        // Find the extends clause and get the base class
        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (not implements)
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause
            let Some(&first_type_idx) = heritage.types.nodes.first() else {
                continue;
            };

            // Get the expression from ExpressionWithTypeArguments
            let expr_idx = if let Some(type_node) = self.ctx.arena.get(first_type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                expr_type_args.expression
            } else {
                first_type_idx
            };

            // Resolve the base class symbol
            let Some(base_sym) = self.resolve_heritage_symbol(expr_idx) else {
                continue;
            };

            // Recursively check the base class's constructor access level
            // This handles inherited private/protected constructors
            return self.class_constructor_access_level_inner(base_sym, visited);
        }

        // No extends clause or couldn't resolve base class - public default
        None
    }

    // =========================================================================
    // =========================================================================
    // Type Query Helper Functions
    // =========================================================================

    /// Check if a type cannot be used as an index type (TS2538).
    pub(crate) fn type_is_invalid_index_type(&self, type_id: TypeId) -> bool {
        query::is_invalid_index_type(self.ctx.types, type_id)
    }

    fn classify_simple_overlap_type(&self, type_id: TypeId) -> Option<SimpleOverlapType> {
        use query::LiteralTypeKind;

        let primitive = match type_id {
            TypeId::STRING => Some(PrimitiveOverlapKind::String),
            TypeId::NUMBER => Some(PrimitiveOverlapKind::Number),
            TypeId::BIGINT => Some(PrimitiveOverlapKind::BigInt),
            TypeId::BOOLEAN => Some(PrimitiveOverlapKind::Boolean),
            TypeId::SYMBOL => Some(PrimitiveOverlapKind::Symbol),
            _ => None,
        };
        if let Some(kind) = primitive {
            return Some(SimpleOverlapType::Primitive(kind));
        }

        match query::classify_literal_type(self.ctx.types, type_id) {
            LiteralTypeKind::String(atom) => Some(SimpleOverlapType::StringLiteral(atom)),
            LiteralTypeKind::Number(value) => Some(SimpleOverlapType::NumberLiteral(value)),
            LiteralTypeKind::BigInt(atom) => Some(SimpleOverlapType::BigIntLiteral(atom)),
            LiteralTypeKind::Boolean(value) => Some(SimpleOverlapType::BooleanLiteral(value)),
            LiteralTypeKind::NotLiteral => None,
        }
    }

    fn simple_overlap_types_overlap(
        &self,
        left: SimpleOverlapType,
        right: SimpleOverlapType,
    ) -> bool {
        use PrimitiveOverlapKind as P;
        use SimpleOverlapType as T;

        match (left, right) {
            (T::Primitive(a), T::Primitive(b)) => a == b,
            (T::Primitive(P::String), T::StringLiteral(_))
            | (T::StringLiteral(_), T::Primitive(P::String))
            | (T::Primitive(P::Number), T::NumberLiteral(_))
            | (T::NumberLiteral(_), T::Primitive(P::Number))
            | (T::Primitive(P::BigInt), T::BigIntLiteral(_))
            | (T::BigIntLiteral(_), T::Primitive(P::BigInt))
            | (T::Primitive(P::Boolean), T::BooleanLiteral(_))
            | (T::BooleanLiteral(_), T::Primitive(P::Boolean)) => true,
            (T::StringLiteral(a), T::StringLiteral(b))
            | (T::BigIntLiteral(a), T::BigIntLiteral(b)) => a == b,
            (T::NumberLiteral(a), T::NumberLiteral(b)) => a == b,
            (T::BooleanLiteral(a), T::BooleanLiteral(b)) => a == b,
            _ => false,
        }
    }

    fn union_overlap_fast_path(&self, members: &[TypeId], other: TypeId) -> Option<bool> {
        let other_simple = self.classify_simple_overlap_type(other)?;
        for &member in members {
            let member_simple = self.classify_simple_overlap_type(member)?;
            if self.simple_overlap_types_overlap(member_simple, other_simple) {
                return Some(true);
            }
        }
        Some(false)
    }

    fn simple_overlap_fast_path(&self, left: TypeId, right: TypeId) -> Option<bool> {
        let left_simple = self.classify_simple_overlap_type(left)?;
        let right_simple = self.classify_simple_overlap_type(right)?;
        Some(self.simple_overlap_types_overlap(left_simple, right_simple))
    }

    /// Check if two types have no overlap (for TS2367 validation).
    /// Returns true if the types can never be equal in a comparison.
    pub(crate) fn types_have_no_overlap(&mut self, left: TypeId, right: TypeId) -> bool {
        tracing::trace!(left = ?left, right = ?right, "types_have_no_overlap called");

        // any, unknown, error types can overlap with anything
        if left == TypeId::ANY || right == TypeId::ANY {
            tracing::trace!("has ANY");
            return false;
        }
        if left == TypeId::UNKNOWN || right == TypeId::UNKNOWN {
            tracing::trace!("has UNKNOWN");
            return false;
        }
        if left == TypeId::ERROR || right == TypeId::ERROR {
            tracing::trace!("has ERROR");
            return false;
        }

        // null/undefined are always comparable with any type (TSC's "comparable relation").
        // Even with strictNullChecks enabled, `null === x` and `undefined === x` should
        // never trigger TS2367.
        if left == TypeId::NULL
            || left == TypeId::UNDEFINED
            || right == TypeId::NULL
            || right == TypeId::UNDEFINED
        {
            return false;
        }

        // Same type always overlaps
        if left == right {
            tracing::trace!("same type");
            return false;
        }

        // For type parameters, check the constraint instead of the parameter itself
        let effective_left =
            match query::classify_for_type_parameter_constraint(self.ctx.types, left) {
                query::TypeParameterConstraintKind::TypeParameter {
                    constraint: Some(constraint),
                } => {
                    tracing::trace!(?constraint, "left is type param with constraint");
                    constraint
                }
                _ => left,
            };

        let effective_right =
            match query::classify_for_type_parameter_constraint(self.ctx.types, right) {
                query::TypeParameterConstraintKind::TypeParameter {
                    constraint: Some(constraint),
                } => {
                    tracing::trace!(?constraint, "right is type param with constraint");
                    constraint
                }
                _ => right,
            };

        tracing::trace!(
            ?effective_left,
            ?effective_right,
            "effective types for overlap check"
        );

        // Fast path for primitive/literal combinations without recursive relation checks.
        if let Some(has_overlap) = self.simple_overlap_fast_path(effective_left, effective_right) {
            return !has_overlap;
        }

        // Check union types: if any member of one union overlaps with the other, they overlap
        if let query::UnionMembersKind::Union(left_members) =
            query::classify_for_union_members(self.ctx.types, effective_left)
        {
            if let Some(has_overlap) = self.union_overlap_fast_path(&left_members, effective_right)
            {
                return !has_overlap;
            }

            tracing::trace!("effective_left is union");
            for &left_member in &left_members {
                tracing::trace!(?left_member, ?effective_right, "checking union member");
                if !self.types_have_no_overlap(left_member, effective_right) {
                    tracing::trace!("union member overlaps - union overlaps");
                    return false;
                }
            }
            tracing::trace!("no union members overlap - returning true");
            return true;
        }

        if let query::UnionMembersKind::Union(right_members) =
            query::classify_for_union_members(self.ctx.types, effective_right)
        {
            if let Some(has_overlap) = self.union_overlap_fast_path(&right_members, effective_left)
            {
                return !has_overlap;
            }

            tracing::trace!("effective_right is union");
            for &right_member in &right_members {
                if !self.types_have_no_overlap(effective_left, right_member) {
                    return false;
                }
            }
            return true;
        }

        // For intersection types (e.g., `string & { $Brand: any }`), check if
        // ANY member of the intersection overlaps with the other type. A branded
        // string type overlaps with a string literal since the `string` member does.
        if let Some(left_members) = query::get_intersection_members(self.ctx.types, effective_left)
        {
            for member in &left_members {
                if !self.types_have_no_overlap(*member, effective_right) {
                    return false;
                }
            }
        }
        if let Some(right_members) =
            query::get_intersection_members(self.ctx.types, effective_right)
        {
            for member in &right_members {
                if !self.types_have_no_overlap(effective_left, *member) {
                    return false;
                }
            }
        }

        // If either is assignable to the other, they overlap
        let trace_enabled = tracing::enabled!(tracing::Level::TRACE);
        let left_to_right = self.is_assignable_to(effective_left, effective_right);
        let right_to_left = if left_to_right {
            false
        } else {
            self.is_assignable_to(effective_right, effective_left)
        };

        if trace_enabled {
            let left_type_str = self.format_type(effective_left);
            let right_type_str = self.format_type(effective_right);
            tracing::trace!(
                ?effective_left,
                ?effective_right,
                %left_type_str,
                %right_type_str,
                left_to_right,
                right_to_left,
                "assignability check"
            );
        }
        if left_to_right || right_to_left {
            return false;
        }

        tracing::trace!("no overlap detected");
        // No other overlap detected
        true
    }

    /// Get display string for implicit any return type.
    ///
    /// Returns "any" for null/undefined only types, otherwise formats the type.
    pub(crate) fn implicit_any_return_display(&self, return_type: TypeId) -> String {
        if self.is_null_or_undefined_only(return_type) {
            return "any".to_string();
        }
        self.format_type(return_type)
    }

    /// Check if we should report implicit any return type.
    ///
    /// Only reports when return type is exactly 'any', not when it contains 'any' somewhere.
    /// For example, Promise<void> should not trigger TS7010 even if Promise's definition
    /// contains 'any' in its type structure.
    pub(crate) fn should_report_implicit_any_return(&self, return_type: TypeId) -> bool {
        // void is a valid inferred return type (functions with no return statements),
        // it should NOT trigger TS7010 "Function lacks ending return statement"
        if return_type == TypeId::VOID {
            return false;
        }
        // Under strictNullChecks, null and undefined are concrete types (not implicit any).
        // Only treat null/undefined returns as implicit any when strictNullChecks is OFF,
        // where they widen to `any`.
        if return_type == TypeId::ANY {
            return true;
        }
        !self.ctx.strict_null_checks() && self.is_null_or_undefined_only(return_type)
    }

    // =========================================================================
    // Type Refinement Helper Functions
    // =========================================================================

    /// Refine variable declaration type based on assignment.
    ///
    /// Returns the more specific type when `prev_type` is ANY and `current_type` is concrete.
    /// This implements type refinement for multiple assignments.
    pub(crate) const fn refine_var_decl_type(
        &self,
        prev_type: TypeId,
        current_type: TypeId,
    ) -> TypeId {
        if matches!(prev_type, TypeId::ANY | TypeId::ERROR)
            && !matches!(current_type, TypeId::ANY | TypeId::ERROR)
        {
            return current_type;
        }
        prev_type
    }

    // =========================================================================
    // Property Readonly Helper Functions
    // =========================================================================

    /// Check if a class property is readonly.
    ///
    /// Looks up the class by name, finds the property member declaration,
    /// and checks if it has a readonly modifier.
    pub(crate) fn is_class_property_readonly(&self, class_name: &str, prop_name: &str) -> bool {
        let Some(class_sym_id) = self.get_symbol_by_name(class_name) else {
            return false;
        };
        let Some(class_sym) = self.ctx.binder.get_symbol(class_sym_id) else {
            return false;
        };
        if class_sym.value_declaration.is_none() {
            return false;
        }
        let Some(class_node) = self.ctx.arena.get(class_sym.value_declaration) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(member_node) {
                let member_name = self.get_identifier_text_from_idx(prop_decl.name);
                if member_name.as_deref() == Some(prop_name) {
                    return self.has_readonly_modifier(&prop_decl.modifiers);
                }
            }
        }
        false
    }

    /// Check if an interface property is readonly by looking up the interface declaration in the AST.
    ///
    /// Given a type name (e.g., "I"), finds the interface declaration and checks
    /// if the named property has a readonly modifier.
    pub(crate) fn is_interface_property_readonly(&self, type_name: &str, prop_name: &str) -> bool {
        use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;

        let Some(sym_id) = self.get_symbol_by_name(type_name) else {
            return false;
        };
        let Some(sym) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        // Check all declarations (interfaces can be merged)
        for &decl_idx in &sym.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(iface_data) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };
            for &member_idx in &iface_data.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != PROPERTY_SIGNATURE {
                    continue;
                }
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let member_name = self.get_identifier_text_from_idx(sig.name);
                if member_name.as_deref() == Some(prop_name) {
                    return self.has_readonly_modifier(&sig.modifiers);
                }
            }
        }
        false
    }

    /// Get the declared type name from a variable expression.
    ///
    /// For `declare const obj: I`, given the expression node for `obj`,
    /// returns "I" (the type reference name from the variable's type annotation).
    pub(crate) fn get_declared_type_name_from_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;

        // Must be an identifier
        self.ctx.arena.get_identifier(node)?;

        // Resolve the variable's symbol
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let sym = self.ctx.binder.get_symbol(sym_id)?;

        // Get the variable's declaration
        if sym.value_declaration.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(sym.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        // Get the type annotation
        if var_decl.type_annotation.is_none() {
            return None;
        }
        let type_node = self.ctx.arena.get(var_decl.type_annotation)?;

        // If it's a type reference, get the name
        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
            return self.get_identifier_text_from_idx(type_ref.type_name);
        }

        None
    }

    /// Check if a property of a type is readonly.
    ///
    /// Delegates to the solver's comprehensive implementation which handles:
    /// - `ReadonlyType` wrappers (readonly arrays/tuples)
    /// - Object types with readonly properties
    /// - `ObjectWithIndex` types (readonly index signatures)
    /// - Union types (readonly if ANY member has readonly property)
    /// - Intersection types (readonly ONLY if ALL members have readonly property)
    pub(crate) fn is_property_readonly(&self, type_id: TypeId, prop_name: &str) -> bool {
        self.ctx.types.is_property_readonly(type_id, prop_name)
    }

    /// Get the class name from a variable declaration.
    ///
    /// Returns the class name if the variable is initialized with a class expression.
    pub(crate) fn get_class_name_from_var_decl(&self, decl_idx: NodeIndex) -> Option<String> {
        let var_decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;

        if var_decl.initializer.is_none() {
            return None;
        }

        let init_node = self.ctx.arena.get(var_decl.initializer)?;
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        let class = self.ctx.arena.get_class(init_node)?;
        if class.name.is_none() {
            return None;
        }

        let ident = self.ctx.arena.get_identifier_at(class.name)?;
        Some(ident.escaped_text.clone())
    }

    // =========================================================================
    // AST Navigation Helper Functions
    // =========================================================================

    /// Get class expression returned from a function body.
    ///
    /// Searches for return statements that return class expressions.
    pub(crate) fn returned_class_expression(&self, body_idx: NodeIndex) -> Option<NodeIndex> {
        if body_idx.is_none() {
            return None;
        }
        let node = self.ctx.arena.get(body_idx)?;
        if node.kind != syntax_kind_ext::BLOCK {
            return self.class_expression_from_expr(body_idx);
        }
        let block = self.ctx.arena.get_block(node)?;
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::RETURN_STATEMENT {
                continue;
            }
            let ret = self.ctx.arena.get_return_statement(stmt)?;
            if ret.expression.is_none() {
                continue;
            }
            if let Some(expr_idx) = self.class_expression_from_expr(ret.expression) {
                return Some(expr_idx);
            }
            let expr_node = self.ctx.arena.get(ret.expression)?;
            if let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                && let Some(class_idx) =
                    self.class_declaration_from_identifier_in_block(block, &ident.escaped_text)
            {
                return Some(class_idx);
            }
        }
        None
    }

    /// Find class declaration by identifier name in a block.
    ///
    /// Searches for class declarations with the given name.
    pub(crate) fn class_declaration_from_identifier_in_block(
        &self,
        block: &tsz_parser::parser::node::BlockData,
        name: &str,
    ) -> Option<NodeIndex> {
        for &stmt_idx in &block.statements.nodes {
            let stmt = self.ctx.arena.get(stmt_idx)?;
            if stmt.kind != syntax_kind_ext::CLASS_DECLARATION {
                continue;
            }
            let class = self.ctx.arena.get_class(stmt)?;
            if class.name.is_none() {
                continue;
            }
            let ident = self.ctx.arena.get_identifier_at(class.name)?;
            if ident.escaped_text == name {
                return Some(stmt_idx);
            }
        }
        None
    }

    /// Get class expression from any expression node.
    ///
    /// Unwraps parenthesized expressions and returns the class expression if found.
    pub(crate) fn class_expression_from_expr(&self, expr_idx: NodeIndex) -> Option<NodeIndex> {
        const MAX_TREE_WALK_ITERATIONS: usize = 1000;

        let mut current = expr_idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                let paren = self.ctx.arena.get_parenthesized(node)?;
                current = paren.expression;
                continue;
            }
            if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                return Some(current);
            }
            return None;
        }
    }

    /// Get function declaration from callee expression.
    ///
    /// Returns the function declaration if the callee is a function with a body.
    pub(crate) fn function_decl_from_callee(&self, callee_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(callee_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let sym_id = self.resolve_identifier_symbol(callee_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        for &decl_idx in &symbol.declarations {
            let func = self.ctx.arena.get_function_at(decl_idx)?;
            if !func.body.is_none() {
                return Some(decl_idx);
            }
        }

        if !symbol.value_declaration.is_none() {
            let decl_idx = symbol.value_declaration;
            let func = self.ctx.arena.get_function_at(decl_idx)?;
            if !func.body.is_none() {
                return Some(decl_idx);
            }
        }

        None
    }

    // ============================================================================
    // Section 58: Enum Type Utilities
    // ============================================================================

    /// Get enum member type by property name.
    ///
    /// This function resolves the type of an enum member accessed by name.
    /// It searches through all enum declarations for the symbol to find
    /// a matching member name and returns the enum type (not the primitive).
    ///
    /// ## Parameters:
    /// - `sym_id`: The enum symbol ID
    /// - `property_name`: The member property name to search for
    ///
    /// ## Returns:
    /// - `Some(TypeId)`: The enum type (as a Ref to the enum symbol)
    /// - `None`: If the symbol is not an enum or member not found
    ///
    /// ## Examples:
    /// ```typescript
    /// enum Color {
    ///   Red,
    ///   Green,
    ///   Blue
    /// }
    /// type T = Color["Red"];  // Returns the enum type Color
    /// ```
    ///
    /// Note: This returns the enum type itself, not STRING or NUMBER,
    /// which allows proper enum assignability checking.
    pub(crate) fn enum_member_type_for_name(
        &mut self,
        sym_id: SymbolId,
        property_name: &str,
    ) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::ENUM == 0 {
            return None;
        }

        // Check if the property exists in this enum
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(enum_decl) = self.ctx.arena.get_enum(node) else {
                continue;
            };
            for &member_idx in &enum_decl.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };
                if let Some(name) = self.get_property_name(member.name)
                    && name == property_name
                {
                    // Return the enum type itself by getting the computed type of the symbol
                    // This returns TypeData::Enum(def_id, structural_type) which allows proper
                    // enum assignability checking with nominal identity
                    return Some(self.get_type_of_symbol(sym_id));
                }
            }
        }

        None
    }
}
