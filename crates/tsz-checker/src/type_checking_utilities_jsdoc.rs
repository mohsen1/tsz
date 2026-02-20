//! JSDoc type annotation utilities, class and enum helper methods for `CheckerState`.

use crate::query_boundaries::type_checking_utilities as query;
use crate::state::CheckerState;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::{IndexSignature, ObjectFlags, ObjectShape, PropertyInfo, TypeId, Visibility};

#[derive(Clone)]
struct JsdocTypedefInfo {
    base_type: Option<String>,
    properties: Vec<(String, String)>,
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
                //   @type {Object.<K, V>} or @type {Object<K, V>}
                let obj_map_inner = type_expr
                    .strip_prefix("Object.<")
                    .and_then(|rest| rest.strip_suffix(">"))
                    .or_else(|| {
                        type_expr
                            .strip_prefix("Object<")
                            .and_then(|rest| rest.strip_suffix(">"))
                    });

                if let Some(inner) = obj_map_inner {
                    let mut parts = inner.split(',');
                    let key_str = parts.next().unwrap_or("").trim();
                    let value_str = parts.next().unwrap_or("").trim();
                    if let (Some(key_type), Some(value_type)) = (
                        self.jsdoc_type_from_expression(key_str),
                        self.jsdoc_type_from_expression(value_str),
                    ) {
                        let mut shape = ObjectShape {
                            flags: ObjectFlags::empty(),
                            properties: Vec::new(),
                            string_index: None,
                            number_index: None,
                            symbol: None,
                        };
                        if key_type == TypeId::STRING {
                            shape.string_index = Some(IndexSignature {
                                key_type,
                                value_type,
                                readonly: false,
                            });
                            return Some(factory.object_with_index(shape));
                        } else if key_type == TypeId::NUMBER {
                            shape.number_index = Some(IndexSignature {
                                key_type,
                                value_type,
                                readonly: false,
                            });
                            return Some(factory.object_with_index(shape));
                        }
                    }
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
        while current.is_some() {
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

        let base_type = if let Some(base_type_expr) = &info.base_type {
            let expr = base_type_expr.trim();
            // If base type is explicitly provided and is NOT generic "Object"/"object",
            // TypeScript ignores all @property tags and uses the base type directly.
            if expr != "Object" && expr != "object" {
                return self.jsdoc_type_from_expression(expr);
            }
            Some(self.jsdoc_type_from_expression(expr).unwrap_or(TypeId::ANY))
        } else {
            None
        };

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

        let object_type = if !prop_infos.is_empty() {
            Some(factory.object(prop_infos))
        } else {
            None
        };

        match (object_type, base_type) {
            (Some(obj), Some(base)) => Some(factory.intersection(vec![obj, base])),
            (Some(obj), None) => Some(obj),
            (None, Some(base)) => Some(base),
            (None, None) => None,
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
        let typedef_pos = jsdoc.find("@typedef");
        let mut tag_pos = jsdoc.find("@type");

        while let Some(pos) = tag_pos {
            let next_char = jsdoc[pos + "@type".len()..].chars().next();
            if next_char.is_none() || !next_char.unwrap().is_alphabetic() {
                // If there's a @typedef before this @type, check if it absorbs it.
                if let Some(td_pos) = typedef_pos
                    && td_pos < pos {
                        let typedef_rest = &jsdoc[td_pos + "@typedef".len()..pos];
                        let mut has_non_object_base = false;
                        if let Some(open) = typedef_rest.find('{')
                            && let Some(close) = typedef_rest[open..].find('}') {
                                let base = typedef_rest[open + 1..open + close].trim();
                                if base != "Object" && base != "object" && !base.is_empty() {
                                    has_non_object_base = true;
                                }
                            }
                        if !has_non_object_base {
                            return None; // The @type is absorbed by the @typedef
                        }
                    }
                break;
            }
            tag_pos = jsdoc[pos + 1..].find("@type").map(|p| p + pos + 1);
        }
        let tag_pos = tag_pos?;
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
    /// Also handles alternate `@param name {type}` syntax (name before type).
    fn check_param_text(text: &str, param_name: &str) -> bool {
        let rest = text.trim();

        // Handle alternate syntax: @param `name` {type} or @param name {type}
        // where the name comes before the type
        if !rest.starts_with('{') {
            // Could be `name` {type} or name {type}
            let name_part = rest.split_whitespace().next().unwrap_or("");
            let name_part = name_part.trim_matches('`');
            let decoded = Self::decode_unicode_escapes(name_part);
            return decoded == param_name;
        }

        // Standard syntax: @param {type} name
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
        // Decode unicode escapes in JSDoc param names (e.g. \u0061 -> a)
        let decoded = Self::decode_unicode_escapes(name);
        decoded == param_name
    }

    /// Decode unicode escapes (`\uXXXX` and `\u{XXXX}`) in a string.
    fn decode_unicode_escapes(s: &str) -> String {
        if !s.contains("\\u") {
            return s.to_string();
        }
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '\\' && chars.peek() == Some(&'u') {
                chars.next(); // consume 'u'
                if chars.peek() == Some(&'{') {
                    // \u{XXXX} form
                    chars.next(); // consume '{'
                    let mut hex = String::new();
                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next();
                            break;
                        }
                        hex.push(c);
                        chars.next();
                    }
                    if let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        result.push(decoded);
                        continue;
                    }
                    // Fallback: push original
                    result.push_str("\\u{");
                    result.push_str(&hex);
                    result.push('}');
                } else {
                    // \uXXXX form (exactly 4 hex digits)
                    let mut hex = String::new();
                    for _ in 0..4 {
                        if let Some(&c) = chars.peek() {
                            if c.is_ascii_hexdigit() {
                                hex.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    if hex.len() == 4
                        && let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(decoded) = char::from_u32(code)
                    {
                        result.push(decoded);
                        continue;
                    }
                    // Fallback: push original
                    result.push_str("\\u");
                    result.push_str(&hex);
                }
            } else {
                result.push(ch);
            }
        }
        result
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

                    if !self.has_static_modifier(&prop.modifiers) && prop.initializer.is_some() {
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
            if ctor.body.is_some() {
                return Some(ctor.body);
            }
        }
        None
    }
}
