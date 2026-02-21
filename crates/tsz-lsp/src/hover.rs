//! Hover implementation for LSP.
//!
//! Displays type information and documentation for the symbol at the cursor.
//! Produces quickinfo output compatible with tsserver's expected format:
//! - `display_string`: The raw signature (e.g. `const x: number`, `function foo(): void`)
//! - `kind`: The symbol kind (e.g. `const`, `function`, `class`)
//! - `kind_modifiers`: Comma-separated modifier list (e.g. `export,declare`)
//! - `documentation`: Extracted `JSDoc` content

use crate::jsdoc::{jsdoc_for_node, parse_jsdoc};
use crate::resolver::{ScopeCache, ScopeCacheStats, ScopeWalker};
use crate::utils::{find_node_at_or_before_offset, is_symbol_query_node};
use tsz_checker::state::CheckerState;
use tsz_common::position::{Position, Range};
use tsz_parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;

/// A single `JSDoc` tag (e.g. `@param`, `@returns`, `@deprecated`).
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct JsDocTag {
    /// The tag name (e.g. "param", "returns", "deprecated")
    pub name: String,
    /// The tag text content
    pub text: String,
}

/// Information returned for a hover request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HoverInfo {
    /// The contents of the hover (usually Markdown)
    pub contents: Vec<String>,
    /// The range of the symbol being hovered
    pub range: Option<Range>,
    /// The raw display string for tsserver quickinfo (e.g. `const x: number`)
    pub display_string: String,
    /// The symbol kind string for tsserver (e.g. `const`, `function`, `class`)
    pub kind: String,
    /// Comma-separated kind modifiers for tsserver (e.g. `export,declare`)
    pub kind_modifiers: String,
    /// The documentation text extracted from `JSDoc`
    pub documentation: String,
    /// `JSDoc` tags (e.g. @param, @returns, @deprecated)
    pub tags: Vec<JsDocTag>,
}

define_lsp_provider!(full HoverProvider, "Hover provider.");

impl<'a> HoverProvider<'a> {
    /// Get hover information at the given position.
    ///
    /// # Arguments
    /// * `root` - The root node of the AST
    /// * `position` - The cursor position
    /// * `type_cache` - Mutable reference to the persistent type cache (for performance)
    pub fn get_hover(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, None, None)
    }

    pub fn get_hover_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        self.get_hover_internal(root, position, type_cache, Some(scope_cache), scope_stats)
    }

    fn get_hover_internal(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<HoverInfo> {
        // 1. Find node at position
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;
        let mut node_idx = find_node_at_or_before_offset(self.arena, offset, self.source_text);

        if node_idx.is_none()
            && let Some(adjusted) = self.find_symbol_query_node_at_or_before(offset)
        {
            node_idx = adjusted;
        }

        if node_idx.is_none() {
            return None;
        }

        if !is_symbol_query_node(self.arena, node_idx)
            && (self.is_comment_context(offset) || self.should_backtrack_to_previous_symbol(offset))
            && let Some(adjusted) = self.find_symbol_query_node_at_or_before(offset)
        {
            node_idx = adjusted;
        }

        if !is_symbol_query_node(self.arena, node_idx) {
            return None;
        }

        node_idx = self
            .remap_import_equals_rhs_to_alias(node_idx)
            .unwrap_or(node_idx);

        // 2. Resolve symbol using ScopeWalker
        let mut walker = ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, node_idx, scope_cache, scope_stats)?
        } else {
            walker.resolve_node(root, node_idx)?
        };
        let symbol = self.binder.symbols.get(symbol_id)?;

        // 3. Compute Type Information
        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = type_cache.take() {
            CheckerState::with_cache(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                compiler_options,
            )
        };

        let type_id = checker.get_type_of_symbol(symbol_id);
        let type_string = checker.format_type(type_id);

        // Extract and save the updated cache for future queries
        *type_cache = Some(checker.extract_cache());

        // 4. Get the declaration node for determining keyword and modifiers
        let decl_node_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first) = symbol.declarations.first() {
            first
        } else {
            NodeIndex::NONE
        };

        // 5. Determine the kind string (tsserver-compatible)
        let kind = self.get_tsserver_kind(symbol, decl_node_idx);

        // 6. Determine kind modifiers (export, declare, abstract, etc.)
        let kind_modifiers = self.get_kind_modifiers(symbol, decl_node_idx);

        // 7. Construct the display string matching tsserver format
        let display_string = self.build_display_string(symbol, &kind, &type_string, decl_node_idx);

        // 8. Extract Documentation (JSDoc)
        let raw_documentation = if decl_node_idx.is_some() {
            jsdoc_for_node(self.arena, root, decl_node_idx, self.source_text)
        } else {
            String::new()
        };
        let formatted_doc = self.format_jsdoc_for_hover(&raw_documentation);
        let documentation_text = self.extract_plain_documentation(&raw_documentation);

        // 9. Build response
        let mut contents = Vec::new();

        // Code block for the signature
        contents.push(format!("```typescript\n{display_string}\n```"));

        // Documentation paragraph
        if let Some(doc) = formatted_doc {
            contents.push(doc);
        }

        // Calculate range for the hovered identifier
        let node = self.arena.get(node_idx)?;
        let start = self.line_map.offset_to_position(node.pos, self.source_text);
        let end = self.line_map.offset_to_position(node.end, self.source_text);

        Some(HoverInfo {
            contents,
            range: Some(Range::new(start, end)),
            display_string,
            kind,
            kind_modifiers,
            documentation: documentation_text,
            tags: Vec::new(),
        })
    }

    fn find_symbol_query_node_at_or_before(&self, offset: u32) -> Option<NodeIndex> {
        let mut probe = offset.min(self.source_text.len() as u32);
        let bytes = self.source_text.as_bytes();
        let mut remaining = 256u32;

        while probe > 0 && remaining > 0 {
            probe -= 1;
            remaining -= 1;

            let candidate = find_node_at_or_before_offset(self.arena, probe, self.source_text);
            if candidate.is_some() && is_symbol_query_node(self.arena, candidate) {
                return Some(candidate);
            }

            let ch = bytes[probe as usize];
            if ch == b'\n' || ch == b'\r' {
                break;
            }
        }

        None
    }

    fn is_comment_context(&self, offset: u32) -> bool {
        let bytes = self.source_text.as_bytes();
        if bytes.is_empty() {
            return false;
        }
        let idx = (offset as usize).min(bytes.len());

        if idx > 0 {
            let prev = bytes[idx - 1];
            if prev == b'/' || prev == b'*' {
                return true;
            }
        }
        if idx < bytes.len() {
            let current = bytes[idx];
            if current == b'/' || current == b'*' {
                return true;
            }
        }

        let prefix = &self.source_text[..idx];
        if let Some(start) = prefix.rfind("/*")
            && prefix[start + 2..].rfind("*/").is_none()
        {
            return true;
        }

        false
    }

    fn should_backtrack_to_previous_symbol(&self, offset: u32) -> bool {
        let bytes = self.source_text.as_bytes();
        if bytes.is_empty() {
            return false;
        }

        let idx = (offset as usize).min(bytes.len());
        if idx == 0 {
            return false;
        }

        let prev = bytes[idx - 1];
        if !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$') {
            return false;
        }

        if idx >= bytes.len() {
            return true;
        }

        let current = bytes[idx];
        !(current.is_ascii_alphanumeric() || current == b'_' || current == b'$')
    }

    fn remap_import_equals_rhs_to_alias(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.arena.get(node_idx)?;
        if node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }
        let parent_idx = self.arena.get_extended(node_idx)?.parent;
        let parent = self.arena.get(parent_idx)?;
        if parent.kind != tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
            return None;
        }
        let import_decl = self.arena.get_import_decl(parent)?;
        if !self.is_descendant_of(node_idx, import_decl.module_specifier) {
            return None;
        }
        import_decl
            .import_clause
            .is_some()
            .then_some(import_decl.import_clause)
    }

    fn is_descendant_of(&self, mut node_idx: NodeIndex, ancestor: NodeIndex) -> bool {
        if !ancestor.is_some() {
            return false;
        }
        loop {
            if node_idx == ancestor {
                return true;
            }
            let Some(ext) = self.arena.get_extended(node_idx) else {
                return false;
            };
            if !ext.parent.is_some() {
                return false;
            }
            node_idx = ext.parent;
        }
    }

    /// Build the display string in tsserver quickinfo format.
    fn build_display_string(
        &self,
        symbol: &tsz_binder::Symbol,
        kind: &str,
        type_string: &str,
        decl_node_idx: NodeIndex,
    ) -> String {
        use tsz_binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::ALIAS != 0 {
            if decl_node_idx.is_some()
                && let Some(decl_node) = self.arena.get(decl_node_idx)
                && decl_node.kind == tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                && let Some(import_decl) = self.arena.get_import_decl(decl_node)
                && import_decl.module_specifier.is_some()
                && let Some(module_ref_node) = self.arena.get(import_decl.module_specifier)
                && module_ref_node.kind != tsz_scanner::SyntaxKind::StringLiteral as u16
            {
                let start = module_ref_node.pos as usize;
                let end = module_ref_node.end as usize;
                if end <= self.source_text.len() && start <= end {
                    let module_ref = self.source_text[start..end].trim();
                    if !module_ref.is_empty() {
                        return format!(
                            "namespace {module_ref}\nimport {} = {module_ref}",
                            symbol.escaped_name
                        );
                    }
                }
            }
            if let Some(module_name) = symbol.import_module.as_deref() {
                if decl_node_idx.is_some()
                    && let Some(decl_node) = self.arena.get(decl_node_idx)
                    && decl_node.kind == tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                {
                    return format!(
                        "(alias) module \"{module_name}\"\nimport {} = require(\"{module_name}\")",
                        symbol.escaped_name
                    );
                }
                return format!(
                    "(alias) module \"{module_name}\"\nimport {}",
                    symbol.escaped_name
                );
            }
            return format!("(alias) {}", symbol.escaped_name);
        }

        if f & symbol_flags::FUNCTION != 0 {
            let merged_with_namespace = self.symbol_has_namespace_merge(symbol);
            let sig = if merged_with_namespace {
                self.function_signature_from_symbol(symbol)
                    .unwrap_or_else(|| Self::arrow_to_colon(type_string))
            } else {
                // Convert arrow notation "(params) => ret" to "(params): ret"
                // for named function display
                Self::arrow_to_colon(type_string)
            };
            if merged_with_namespace {
                return format!(
                    "function {}{}\nnamespace {}",
                    symbol.escaped_name, sig, symbol.escaped_name
                );
            }
            return format!("function {}{}", symbol.escaped_name, sig);
        }
        if f & symbol_flags::CLASS != 0 {
            if self.symbol_has_namespace_merge(symbol) {
                return format!(
                    "class {}\nnamespace {}",
                    symbol.escaped_name, symbol.escaped_name
                );
            }
            return format!("class {}", symbol.escaped_name);
        }
        if f & symbol_flags::INTERFACE != 0 {
            return format!("interface {}", symbol.escaped_name);
        }
        if f & symbol_flags::ENUM != 0 {
            return format!("enum {}", symbol.escaped_name);
        }
        if f & symbol_flags::TYPE_ALIAS != 0 {
            return format!("type {} = {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::ENUM_MEMBER != 0 {
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!(
                    "(enum member) {}.{} = {}",
                    parent, symbol.escaped_name, type_string
                );
            }
            return format!("(enum member) {} = {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::PROPERTY != 0 {
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!(
                    "(property) {}.{}: {}",
                    parent, symbol.escaped_name, type_string
                );
            }
            return format!("(property) {}: {}", symbol.escaped_name, type_string);
        }
        if f & symbol_flags::METHOD != 0 {
            let sig = Self::arrow_to_colon(type_string);
            let parent_name = self.get_parent_name(decl_node_idx);
            if let Some(parent) = parent_name {
                return format!("(method) {}.{}{}", parent, symbol.escaped_name, sig);
            }
            return format!("(method) {}{}", symbol.escaped_name, sig);
        }
        if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            if let Some(module_ref) = self.find_import_equals_module_ref_text(symbol) {
                return format!(
                    "namespace {}\nimport {} = {}",
                    symbol.escaped_name, symbol.escaped_name, module_ref
                );
            }
            return format!("namespace {}", symbol.escaped_name);
        }
        if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            let mut type_string = self
                .merged_function_initializer_display_type(decl_node_idx)
                .unwrap_or_else(|| type_string.to_string());
            if type_string == "error"
                && let Some(array_type) =
                    self.array_constructor_initializer_display_type(decl_node_idx)
            {
                type_string = array_type;
            }
            let keyword = self.get_variable_keyword(decl_node_idx);
            if self.is_local_variable(decl_node_idx) {
                return format!(
                    "(local {}) {}: {}",
                    keyword, symbol.escaped_name, type_string
                );
            }
            return format!("{} {}: {}", keyword, symbol.escaped_name, type_string);
        }
        if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            let mut type_string = self
                .merged_function_initializer_display_type(decl_node_idx)
                .unwrap_or_else(|| type_string.to_string());
            if type_string == "error"
                && let Some(array_type) =
                    self.array_constructor_initializer_display_type(decl_node_idx)
            {
                type_string = array_type;
            }
            if self.is_parameter_declaration(decl_node_idx) {
                return format!("(parameter) {}: {}", symbol.escaped_name, type_string);
            }
            if self.is_local_variable(decl_node_idx) {
                return format!("(local var) {}: {}", symbol.escaped_name, type_string);
            }
            return format!("var {}: {}", symbol.escaped_name, type_string);
        }

        format!("({}) {}: {}", kind, symbol.escaped_name, type_string)
    }

    fn find_import_equals_module_ref_text(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        for &decl_idx in &symbol.declarations {
            let decl_node = self.arena.get(decl_idx)?;
            if decl_node.kind != tsz_parser::syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }
            let import_decl = self.arena.get_import_decl(decl_node)?;
            if !import_decl.module_specifier.is_some() {
                continue;
            }
            let module_ref_node = self.arena.get(import_decl.module_specifier)?;
            if module_ref_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16 {
                continue;
            }
            let start = module_ref_node.pos as usize;
            let end = module_ref_node.end as usize;
            if end <= self.source_text.len() && start <= end {
                let module_ref = self.source_text[start..end].trim();
                if !module_ref.is_empty() {
                    return Some(module_ref.to_string());
                }
            }
        }
        None
    }

    fn symbol_has_namespace_merge(&self, symbol: &tsz_binder::Symbol) -> bool {
        use tsz_binder::symbol_flags;
        if symbol.flags & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            return true;
        }
        symbol.declarations.iter().any(|&decl_idx| {
            self.arena
                .get(decl_idx)
                .is_some_and(|node| node.kind == tsz_parser::syntax_kind_ext::MODULE_DECLARATION)
        })
    }

    fn function_signature_from_symbol(&self, symbol: &tsz_binder::Symbol) -> Option<String> {
        for &decl_idx in &symbol.declarations {
            let Some(node) = self.arena.get(decl_idx) else {
                continue;
            };
            let Some(func) = self.arena.get_function(node) else {
                continue;
            };
            let Some(name_node) = self.arena.get(func.name) else {
                continue;
            };
            let Some(name_ident) = self.arena.get_identifier(name_node) else {
                continue;
            };
            let name = self.arena.resolve_identifier_text(name_ident);
            if name != symbol.escaped_name.as_str() {
                continue;
            }

            let start = node.pos as usize;
            let end = node.end.min(self.source_text.len() as u32) as usize;
            if start >= end {
                continue;
            }
            let text = &self.source_text[start..end];
            let Some(open) = text.find('(') else {
                continue;
            };
            let mut depth = 0i32;
            let mut close = None;
            for (i, ch) in text[open..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(open + i);
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let Some(close_pos) = close else {
                continue;
            };
            let params = &text[open..=close_pos];
            let after = text[close_pos + 1..].trim_start();
            if let Some(rest) = after.strip_prefix(':') {
                let ret = rest
                    .trim_start()
                    .split(['{', ';', '\n'])
                    .next()
                    .unwrap_or("")
                    .trim();
                if !ret.is_empty() {
                    return Some(format!("{params}: {ret}"));
                }
            }
            return Some(format!("{params}: void"));
        }
        None
    }

    fn merged_function_initializer_display_type(&self, decl_node_idx: NodeIndex) -> Option<String> {
        use tsz_binder::symbol_flags;

        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        if decl_node.kind != tsz_parser::syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        if !var_decl.initializer.is_some() {
            return None;
        }
        let init_node = self.arena.get(var_decl.initializer)?;
        if init_node.kind != tsz_scanner::SyntaxKind::Identifier as u16 {
            return None;
        }

        let init_sym_id = self
            .binder
            .node_symbols
            .get(&var_decl.initializer.0)
            .copied()
            .or_else(|| {
                self.binder
                    .resolve_identifier(self.arena, var_decl.initializer)
            })?;
        let init_symbol = self.binder.get_symbol(init_sym_id)?;
        if (init_symbol.flags & symbol_flags::FUNCTION) == 0
            || !self.symbol_has_namespace_merge(init_symbol)
        {
            return None;
        }

        if self.namespace_has_value_exports(init_symbol) {
            Some(format!("typeof {}", init_symbol.escaped_name))
        } else {
            self.function_signature_from_symbol(init_symbol)
                .map(|sig| Self::colon_to_arrow_signature(&sig))
        }
    }

    fn namespace_has_value_exports(&self, symbol: &tsz_binder::Symbol) -> bool {
        use tsz_binder::symbol_flags;

        symbol.exports.as_ref().is_some_and(|exports| {
            exports.iter().any(|(_, sym_id)| {
                self.binder
                    .get_symbol(*sym_id)
                    .is_some_and(|export_symbol| (export_symbol.flags & symbol_flags::VALUE) != 0)
            })
        })
    }

    fn array_constructor_initializer_display_type(
        &self,
        decl_node_idx: NodeIndex,
    ) -> Option<String> {
        if !decl_node_idx.is_some() {
            return None;
        }
        let decl_node = self.arena.get(decl_node_idx)?;
        if decl_node.kind != tsz_parser::syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let var_decl = self.arena.get_variable_declaration(decl_node)?;
        if !var_decl.initializer.is_some() {
            return None;
        }
        let init_node = self.arena.get(var_decl.initializer)?;
        let call = self.arena.get_call_expr(init_node)?;
        let callee = self.arena.get_identifier_text(call.expression)?;
        if callee != "Array" {
            return None;
        }

        if let Some(type_args) = call.type_arguments.as_ref()
            && let Some(&first_type_arg) = type_args.nodes.first()
            && let Some(type_node) = self.arena.get(first_type_arg)
        {
            let start = type_node.pos as usize;
            let end = type_node.end.min(self.source_text.len() as u32) as usize;
            if start < end {
                let mut elem = self.source_text[start..end].trim().to_string();
                while elem.ends_with('>') {
                    let opens = elem.chars().filter(|&c| c == '<').count();
                    let closes = elem.chars().filter(|&c| c == '>').count();
                    if closes > opens {
                        elem.pop();
                        elem = elem.trim_end().to_string();
                    } else {
                        break;
                    }
                }
                while elem.ends_with(',') {
                    elem.pop();
                    elem = elem.trim_end().to_string();
                }
                if !elem.is_empty() {
                    return Some(format!("{elem}[]"));
                }
            }
        }

        if let Some(args) = call.arguments.as_ref()
            && let Some(&first_arg) = args.nodes.first()
            && let Some(first_node) = self.arena.get(first_arg)
            && first_node.kind == tsz_scanner::SyntaxKind::StringLiteral as u16
        {
            return Some("string[]".to_string());
        }

        Some("any[]".to_string())
    }

    fn colon_to_arrow_signature(signature: &str) -> String {
        let trimmed = signature.trim();
        if !trimmed.starts_with('(') {
            return trimmed.to_string();
        }
        let bytes = trimmed.as_bytes();
        let mut depth = 0i32;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let after = trimmed[i + 1..].trim_start();
                        if let Some(rest) = after.strip_prefix(':') {
                            return format!("{} => {}", &trimmed[..=i], rest.trim_start());
                        }
                        break;
                    }
                }
                _ => {}
            }
        }
        trimmed.to_string()
    }

    /// Convert arrow notation `(params) => ret` to colon notation `(params): ret`.
    /// Used when displaying named functions/methods where TypeScript uses `:` for
    /// the return type, not `=>`.
    fn arrow_to_colon(type_string: &str) -> String {
        // Find the last `) => ` at paren depth 0 and replace with `): `
        let bytes = type_string.as_bytes();
        let mut depth = 0i32;
        let mut last_close = None;
        for (i, &b) in bytes.iter().enumerate() {
            match b {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        last_close = Some(i);
                    }
                }
                _ => {}
            }
        }
        if let Some(close_idx) = last_close {
            let after = &type_string[close_idx + 1..];
            if let Some(arrow_pos) = after.find(" => ") {
                let before = &type_string[..close_idx + 1];
                let ret = &after[arrow_pos + 4..];
                return format!("{before}: {ret}");
            }
        }
        type_string.to_string()
    }

    /// Get the tsserver-compatible kind string for the symbol.
    fn get_tsserver_kind(&self, symbol: &tsz_binder::Symbol, decl_node_idx: NodeIndex) -> String {
        use tsz_binder::symbol_flags;
        let f = symbol.flags;

        if f & symbol_flags::ALIAS != 0 {
            return "alias".to_string();
        }
        if f & symbol_flags::FUNCTION != 0 {
            return "function".to_string();
        }
        if f & symbol_flags::CLASS != 0 {
            return "class".to_string();
        }
        if f & symbol_flags::INTERFACE != 0 {
            return "interface".to_string();
        }
        if f & symbol_flags::ENUM != 0 {
            return "enum".to_string();
        }
        if f & symbol_flags::TYPE_ALIAS != 0 {
            return "type".to_string();
        }
        if f & symbol_flags::ENUM_MEMBER != 0 {
            return "enum member".to_string();
        }
        if f & (symbol_flags::VALUE_MODULE | symbol_flags::NAMESPACE_MODULE) != 0 {
            return "module".to_string();
        }
        if f & symbol_flags::METHOD != 0 {
            return "method".to_string();
        }
        if f & symbol_flags::CONSTRUCTOR != 0 {
            return "constructor".to_string();
        }
        if f & symbol_flags::PROPERTY != 0 {
            return "property".to_string();
        }
        if f & symbol_flags::TYPE_PARAMETER != 0 {
            return "type parameter".to_string();
        }
        if f & symbol_flags::GET_ACCESSOR != 0 {
            return "getter".to_string();
        }
        if f & symbol_flags::SET_ACCESSOR != 0 {
            return "setter".to_string();
        }
        if f & symbol_flags::BLOCK_SCOPED_VARIABLE != 0 {
            return self.get_variable_keyword(decl_node_idx).to_string();
        }
        if f & symbol_flags::FUNCTION_SCOPED_VARIABLE != 0 {
            if self.is_parameter_declaration(decl_node_idx) {
                return "parameter".to_string();
            }
            return "var".to_string();
        }
        "var".to_string()
    }

    /// Get comma-separated kind modifiers string for tsserver.
    fn get_kind_modifiers(&self, symbol: &tsz_binder::Symbol, decl_node_idx: NodeIndex) -> String {
        use tsz_binder::symbol_flags as sf;
        use tsz_parser::modifier_flags as mf;

        let mut modifiers = Vec::new();

        if symbol.is_exported || symbol.flags & sf::EXPORT_VALUE != 0 {
            modifiers.push("export");
        }
        if symbol.flags & sf::ABSTRACT != 0 {
            modifiers.push("abstract");
        }
        if symbol.flags & sf::STATIC != 0 {
            modifiers.push("static");
        }
        if symbol.flags & sf::PRIVATE != 0 {
            modifiers.push("private");
        }
        if symbol.flags & sf::PROTECTED != 0 {
            modifiers.push("protected");
        }

        if decl_node_idx.is_some()
            && let Some(ext) = self.arena.get_extended(decl_node_idx)
        {
            let mflags = ext.modifier_flags;
            if mflags & mf::AMBIENT != 0 {
                modifiers.push("declare");
            }
            if mflags & mf::ASYNC != 0 {
                modifiers.push("async");
            }
            if mflags & mf::READONLY != 0 {
                modifiers.push("readonly");
            }
            if !modifiers.contains(&"export") && mflags & mf::EXPORT != 0 {
                modifiers.push("export");
            }
            if !modifiers.contains(&"abstract") && mflags & mf::ABSTRACT != 0 {
                modifiers.push("abstract");
            }
        }

        modifiers.join(",")
    }

    /// Determine the variable keyword (const, let, or var) from the declaration node.
    fn get_variable_keyword(&self, decl_node_idx: NodeIndex) -> &'static str {
        use tsz_parser::parser::flags::node_flags;
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return "let";
        }

        let node = match self.arena.get(decl_node_idx) {
            Some(n) => n,
            None => return "let",
        };

        let list_idx = if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
            if let Some(ext) = self.arena.get_extended(decl_node_idx) {
                ext.parent
            } else {
                return "let";
            }
        } else if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST {
            decl_node_idx
        } else {
            let flags = node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
            return "var";
        };

        if let Some(list_node) = self.arena.get(list_idx) {
            let flags = list_node.flags as u32;
            if flags & node_flags::CONST != 0 {
                return "const";
            }
            if flags & node_flags::LET != 0 {
                return "let";
            }
        }

        "let"
    }

    /// Check if a variable declaration is local (inside a function/method body).
    /// TypeScript uses `(local var)`, `(local const)`, `(local let)` for variables
    /// declared inside function bodies, as opposed to module-level declarations.
    fn is_local_variable(&self, decl_node_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return false;
        }

        // Walk up the parent chain looking for a function-like container
        let mut current = decl_node_idx;
        loop {
            let ext = match self.arena.get_extended(current) {
                Some(e) => e,
                None => return false,
            };
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let parent_node = match self.arena.get(parent_idx) {
                Some(n) => n,
                None => return false,
            };
            match parent_node.kind {
                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::ARROW_FUNCTION
                | syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::CONSTRUCTOR
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR => {
                    return true;
                }
                syntax_kind_ext::SOURCE_FILE
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::MODULE_BLOCK => {
                    return false;
                }
                _ => {
                    current = parent_idx;
                }
            }
        }
    }

    /// Check if a declaration node is a parameter.
    fn is_parameter_declaration(&self, decl_node_idx: NodeIndex) -> bool {
        use tsz_parser::syntax_kind_ext;

        if decl_node_idx.is_none() {
            return false;
        }
        if let Some(node) = self.arena.get(decl_node_idx) {
            return node.kind == syntax_kind_ext::PARAMETER;
        }
        false
    }

    /// Get the parent symbol name (for enum members, properties, methods).
    fn get_parent_name(&self, decl_node_idx: NodeIndex) -> Option<String> {
        if decl_node_idx.is_none() {
            return None;
        }
        let ext = self.arena.get_extended(decl_node_idx)?;
        let parent_idx = ext.parent;
        if parent_idx.is_none() {
            return None;
        }
        let parent_node = self.arena.get(parent_idx)?;
        if let Some(data) = self.arena.get_identifier(parent_node) {
            return Some(self.arena.resolve_identifier_text(data).to_string());
        }
        if let Some(data) = self.arena.get_class(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        if let Some(data) = self.arena.get_enum(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        if let Some(data) = self.arena.get_interface(parent_node)
            && let Some(name_node) = self.arena.get(data.name)
            && let Some(id) = self.arena.get_identifier(name_node)
        {
            return Some(self.arena.resolve_identifier_text(id).to_string());
        }
        None
    }

    /// Extract plain documentation text from `JSDoc` (without markdown formatting).
    fn extract_plain_documentation(&self, doc: &str) -> String {
        if doc.is_empty() {
            return String::new();
        }
        let parsed = parse_jsdoc(doc);
        if let Some(summary) = parsed.summary.as_ref() {
            summary.clone()
        } else {
            doc.to_string()
        }
    }

    fn format_jsdoc_for_hover(&self, doc: &str) -> Option<String> {
        if doc.is_empty() {
            return None;
        }

        let parsed = parse_jsdoc(doc);
        if parsed.is_empty() {
            return Some(doc.to_string());
        }

        let mut sections = Vec::new();
        if let Some(summary) = parsed.summary.as_ref()
            && !summary.is_empty()
        {
            sections.push(summary.clone());
        }

        if !parsed.params.is_empty() {
            let mut names: Vec<&String> = parsed.params.keys().collect();
            names.sort();
            let mut lines = Vec::new();
            lines.push("Parameters:".to_string());
            for name in names {
                let desc = parsed.params.get(name).map_or("", |s| s.as_str());
                if desc.is_empty() {
                    lines.push(format!("- `{name}`"));
                } else {
                    lines.push(format!("- `{name}` {desc}"));
                }
            }
            sections.push(lines.join("\n"));
        }

        let formatted = sections.join("\n\n");
        if formatted.is_empty() {
            Some(doc.to_string())
        } else {
            Some(formatted)
        }
    }
}

#[cfg(test)]
#[path = "../tests/hover_tests.rs"]
mod hover_tests;
