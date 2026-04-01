//! JSX runtime/factory handling: import source validation (TS2875),
//! factory-in-scope checks (TS2874), fragment factory (TS17016/TS2879),
//! pragma extraction, and factory symbol referencing.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

/// Extract the `@jsx` pragma factory name from a source file's leading comments.
///
/// Matches the tsc behavior: scans for `@jsx <identifier>` in block comments
/// (`/* ... */` or `/** ... */`). Returns the factory expression (e.g., `"h"`,
/// `"React.createElement"`). Only the first occurrence is used.
pub(crate) fn extract_jsx_pragma(source: &str) -> Option<String> {
    // Only scan leading comments (pragmas must appear before code).
    // We limit scanning to prevent searching entire large files.
    let scan_limit = source.len().min(4096);
    let text = &source[..scan_limit];

    let mut pos = 0;
    let bytes = text.as_bytes();
    while pos < bytes.len() {
        // Skip whitespace
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        // Look for block comments (/* ... */ or /** ... */)
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = text[comment_start..].find("*/") {
                let comment_body = &text[comment_start..comment_start + end_offset];
                // Search for @jsx within this comment
                if let Some(jsx_pos) = comment_body.find("@jsx ") {
                    let after_jsx = &comment_body[jsx_pos + 5..];
                    let factory: String = after_jsx
                        .trim_start()
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$' || *c == '.')
                        .collect();
                    if !factory.is_empty() {
                        return Some(factory);
                    }
                }
                pos = comment_start + end_offset + 2;
            } else {
                break; // Unterminated comment
            }
            continue;
        }
        // Look for line comments (// ...)
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
            // Line comments can't contain @jsx pragmas (tsc only uses block comments)
            if let Some(nl) = text[pos..].find('\n') {
                pos += nl + 1;
            } else {
                break;
            }
            continue;
        }
        // Hit non-comment code — stop scanning
        break;
    }
    None
}

impl<'a> CheckerState<'a> {
    fn should_prefer_jsx_import_source_anchor(
        &self,
        candidate_idx: NodeIndex,
        current_idx: NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let function_like_depth = |mut idx: NodeIndex| {
            let mut depth = 0usize;
            while idx.is_some() {
                let Some(node) = self.ctx.arena.get(idx) else {
                    break;
                };
                if matches!(
                    node.kind,
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION
                        || k == syntax_kind_ext::FUNCTION_EXPRESSION
                        || k == syntax_kind_ext::ARROW_FUNCTION
                        || k == syntax_kind_ext::METHOD_DECLARATION
                        || k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR
                        || k == syntax_kind_ext::CONSTRUCTOR
                ) {
                    depth += 1;
                }
                let Some(ext) = self.ctx.arena.get_extended(idx) else {
                    break;
                };
                if ext.parent.is_none() {
                    break;
                }
                idx = ext.parent;
            }
            depth
        };

        let candidate_depth = function_like_depth(candidate_idx);
        let current_depth = function_like_depth(current_idx);
        if candidate_depth != current_depth {
            return candidate_depth < current_depth;
        }

        let candidate_start = self
            .get_node_span(candidate_idx)
            .map(|(start, _)| start)
            .unwrap_or(u32::MAX);
        let current_start = self
            .get_node_span(current_idx)
            .map(|(start, _)| start)
            .unwrap_or(u32::MAX);
        candidate_start < current_start
    }

    /// Check that the JSX import source module can be resolved (TS2875).
    pub(crate) fn check_jsx_import_source(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        // Determine runtime suffix from mode or pragma.
        // When `@jsxImportSource` pragma is present, it overrides jsx mode
        // and forces react-jsx behavior even in preserve mode.
        let pragma_source = self.extract_jsx_import_source_pragma();
        let runtime_suffix = match self.ctx.compiler_options.jsx_mode {
            JsxMode::ReactJsx => "jsx-runtime",
            JsxMode::ReactJsxDev => "jsx-dev-runtime",
            _ if pragma_source.is_some() => "jsx-runtime",
            _ => return,
        };

        if self.ctx.jsx_import_source_checked {
            if let Some((current_idx, runtime_path)) =
                self.ctx.deferred_jsx_import_source_error.clone()
                && self.should_prefer_jsx_import_source_anchor(node_idx, current_idx)
            {
                self.ctx.deferred_jsx_import_source_error = Some((node_idx, runtime_path));
            }
            return;
        }
        self.ctx.jsx_import_source_checked = true;

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let source: String = if let Some(ref pragma) = pragma_source {
            pragma.clone()
        } else if self.ctx.compiler_options.jsx_import_source.is_empty() {
            "react".to_string()
        } else {
            self.ctx.compiler_options.jsx_import_source.clone()
        };

        let runtime_path = format!("{source}/{runtime_suffix}");
        if self.module_exists_cross_file(&runtime_path)
            || self.is_ambient_module_match(&runtime_path)
            || self.jsx_runtime_file_exists_on_disk(&source, runtime_suffix)
        {
            return;
        }

        // Defer the TS2875 diagnostic instead of emitting it here.
        // This function runs inside JSX element type resolution, which can be
        // inside a speculative call-checker context that truncates diagnostics.
        // The deferred diagnostic is emitted at the end of check_source_file.
        self.ctx.deferred_jsx_import_source_error = Some((node_idx, runtime_path));
    }

    /// Extract `@jsxImportSource <package>` pragma from the current file's
    /// leading comments. Returns the package name or None.
    pub(crate) fn extract_jsx_import_source_pragma(&self) -> Option<String> {
        let sf = self.ctx.arena.source_files.first()?;
        let text = &sf.text;
        let scan_limit = text.len().min(4096);
        let scan_text = &text[..scan_limit];
        let bytes = scan_text.as_bytes();
        let mut pos = 0;
        while pos < bytes.len() {
            if bytes[pos].is_ascii_whitespace() {
                pos += 1;
                continue;
            }
            if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
                let comment_start = pos + 2;
                if let Some(end_offset) = scan_text[comment_start..].find("*/") {
                    let comment_body = &scan_text[comment_start..comment_start + end_offset];
                    if let Some(idx) = comment_body.find("@jsxImportSource") {
                        let after = &comment_body[idx + "@jsxImportSource".len()..];
                        let pkg: String = after
                            .trim_start()
                            .chars()
                            .take_while(|c| {
                                c.is_alphanumeric()
                                    || *c == '_'
                                    || *c == '-'
                                    || *c == '/'
                                    || *c == '@'
                                    || *c == '.'
                            })
                            .collect();
                        if !pkg.is_empty() {
                            return Some(pkg);
                        }
                    }
                    pos = comment_start + end_offset + 2;
                } else {
                    break;
                }
                continue;
            }
            if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
                if let Some(nl) = scan_text[pos..].find('\n') {
                    pos += nl + 1;
                } else {
                    break;
                }
                continue;
            }
            break;
        }
        None
    }

    /// Check if the JSX runtime file exists on disk by walking up from the
    /// current file's directory.
    fn jsx_runtime_file_exists_on_disk(&self, source: &str, suffix: &str) -> bool {
        use std::path::Path;

        let current_file = &self.ctx.file_name;
        if current_file.is_empty() {
            return false;
        }

        let mut dir = Path::new(current_file.as_str());
        if let Some(parent) = dir.parent() {
            dir = parent;
        }

        loop {
            let node_modules = dir.join("node_modules");
            if node_modules.is_dir() {
                let pkg_dir = node_modules.join(source);
                // Check <pkg>/<suffix>.d.ts and <pkg>/<suffix>/index.d.ts
                if pkg_dir.join(format!("{suffix}.d.ts")).is_file()
                    || pkg_dir.join(suffix).join("index.d.ts").is_file()
                {
                    return true;
                }

                let types_dir = node_modules.join("@types").join(source);
                if types_dir.join(format!("{suffix}.d.ts")).is_file()
                    || types_dir.join(suffix).join("index.d.ts").is_file()
                {
                    return true;
                }
            }

            match dir.parent() {
                Some(parent) if parent != dir => dir = parent,
                _ => break,
            }
        }

        false
    }

    /// Check that JSX fragments have a valid fragment factory when jsxFactory is set (TS17016),
    /// and that the fragment factory is in scope (TS2879).
    pub(crate) fn check_jsx_fragment_factory(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        if self.ctx.compiler_options.jsx_mode != JsxMode::React {
            return;
        }

        // When @jsxImportSource pragma overrides react mode, skip fragment checks.
        if self.extract_jsx_import_source_pragma().is_some() {
            return;
        }

        if self.ctx.compiler_options.jsx_factory_from_config {
            // When jsxFragmentFactory is configured, mark it as referenced
            // so unused-import checking (TS6192) doesn't flag it.
            if self.ctx.compiler_options.jsx_fragment_factory_from_config {
                self.mark_jsx_name_as_referenced(
                    &self.ctx.compiler_options.jsx_fragment_factory.clone(),
                    node_idx,
                );
                return;
            }

            use crate::diagnostics::diagnostic_codes;
            self.error_at_node(
                node_idx,
                "The 'jsxFragmentFactory' compiler option must be provided to use JSX fragments with the 'jsxFactory' compiler option.",
                diagnostic_codes::THE_JSXFRAGMENTFACTORY_COMPILER_OPTION_MUST_BE_PROVIDED_TO_USE_JSX_FRAGMENTS_WIT,
            );
            return;
        }

        // TS2879: check that the fragment factory root identifier is in scope.
        // Default fragment factory is React.Fragment, so root is "React" (or
        // whatever reactNamespace is configured to).
        let factory = self.ctx.compiler_options.jsx_factory.clone();
        let root_ident_owned = factory.split('.').next().unwrap_or(&factory).to_string();
        if root_ident_owned.is_empty() {
            return;
        }

        let lib_binders = self.get_lib_binders();
        let found = self.ctx.binder.resolve_name_with_filter(
            &root_ident_owned,
            self.ctx.arena,
            node_idx,
            &lib_binders,
            |_| true,
        );
        if found.is_some() {
            return;
        }
        if self
            .resolve_global_value_symbol(&root_ident_owned)
            .is_some()
        {
            return;
        }

        use crate::diagnostics::diagnostic_codes;
        self.error_at_node_msg(
            node_idx,
            diagnostic_codes::USING_JSX_FRAGMENTS_REQUIRES_FRAGMENT_FACTORY_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE,
            &[&root_ident_owned],
        );
    }

    /// Mark a JSX factory or fragment factory name as referenced for
    /// unused-import checking (TS6192). The name may be dotted (e.g.,
    /// `React.createElement`); we resolve only the root identifier.
    pub(crate) fn mark_jsx_name_as_referenced(&mut self, name: &str, node_idx: NodeIndex) {
        let root_ident = name.split('.').next().unwrap_or(name);
        if root_ident.is_empty() {
            return;
        }
        let lib_binders = self.get_lib_binders();
        if let Some(sym_id) = self.ctx.binder.resolve_name_with_filter(
            root_ident,
            self.ctx.arena,
            node_idx,
            &lib_binders,
            |_| true,
        ) {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }
    }

    /// Check that the JSX factory is in scope (TS2874).
    ///
    /// tsc 6.0 behavior:
    /// - Only classic "react" mode requires the factory in scope.
    /// - When `jsxFactory` compiler option is explicitly set, tsc skips scope
    ///   checking (the option is a name hint, not a scope requirement).
    /// - When using default (`React.createElement`) or `reactNamespace`, tsc
    ///   checks the full scope chain (local, imports, namespace, global).
    pub(crate) fn check_jsx_factory_in_scope(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        // Only classic "react" mode requires the factory in scope
        if self.ctx.compiler_options.jsx_mode != JsxMode::React {
            return;
        }

        // When @jsxImportSource pragma is present, it overrides react mode
        // to react-jsx behavior, so the factory scope check doesn't apply.
        if self.extract_jsx_import_source_pragma().is_some() {
            return;
        }

        // tsc 6.0 skips scope checking when jsxFactory is explicitly set.
        // However, we still need to mark the factory symbol as referenced
        // so that unused-import checking (TS6192) doesn't flag it.
        if self.ctx.compiler_options.jsx_factory_from_config {
            self.mark_jsx_name_as_referenced(
                &self.ctx.compiler_options.jsx_factory.clone(),
                node_idx,
            );
            return;
        }

        // Check for per-file /** @jsx factory */ pragma
        let pragma_factory = self
            .ctx
            .arena
            .source_files
            .first()
            .and_then(|sf| extract_jsx_pragma(&sf.text));

        let factory =
            pragma_factory.unwrap_or_else(|| self.ctx.compiler_options.jsx_factory.clone());
        let root_ident = factory.split('.').next().unwrap_or(&factory);

        if root_ident.is_empty() {
            return;
        }

        let file_has_any_parse_diag =
            self.ctx.has_parse_errors || !self.ctx.all_parse_error_positions.is_empty();
        if file_has_any_parse_diag {
            return;
        }

        // Check full scope chain (accept-all filter to include class members)
        let lib_binders = self.get_lib_binders();
        let found = self.ctx.binder.resolve_name_with_filter(
            root_ident,
            self.ctx.arena,
            node_idx,
            &lib_binders,
            |_| true, // Accept any symbol, including class members
        );
        if found.is_some() {
            return;
        }

        // Also check global scope as fallback (for lib-loaded symbols)
        if self.resolve_global_value_symbol(root_ident).is_some() {
            return;
        }

        // If not found, emit TS2874 at the tag name (tsc points at the tag name, not `<`)
        let error_node = self
            .ctx
            .arena
            .get(node_idx)
            .and_then(|node| self.ctx.arena.get_jsx_opening(node))
            .map(|jsx| jsx.tag_name)
            .unwrap_or(node_idx);
        use crate::diagnostics::diagnostic_codes;
        self.error_at_node_msg(
            error_node,
            diagnostic_codes::THIS_JSX_TAG_REQUIRES_TO_BE_IN_SCOPE_BUT_IT_COULD_NOT_BE_FOUND,
            &[root_ident],
        );
    }

    /// Try to resolve JSX namespace from a custom jsxFactory's parent entity.
    ///
    /// For `@jsxFactory: X.jsx`, resolves `X` in file scope, then looks for `JSX`
    /// in its exports/members. Returns `None` if no custom factory or the namespace
    /// can't be found.
    pub(crate) fn resolve_jsx_namespace_from_factory(&mut self) -> Option<tsz_binder::SymbolId> {
        use tsz_binder::symbol_flags;

        // Get the effective factory name (pragma overrides config)
        let pragma_factory = self
            .ctx
            .arena
            .source_files
            .first()
            .and_then(|sf| extract_jsx_pragma(&sf.text));
        let factory = pragma_factory.or_else(|| {
            if self.ctx.compiler_options.jsx_factory_from_config {
                Some(self.ctx.compiler_options.jsx_factory.clone())
            } else {
                None
            }
        })?;

        // Factory-scoped JSX namespace resolution uses the factory root symbol.
        // This applies to both bare identifiers like `jsx` and dotted factories
        // like `React.createElement`, both of which should probe `<root>.JSX`.
        let root_name = factory.split('.').next()?;
        if root_name.is_empty() {
            return None;
        }

        // Resolve the root entity (e.g., "X") in file scope
        let root_sym = self.ctx.binder.file_locals.get(root_name)?;
        let lib_binders = self.get_lib_binders();
        let root_symbol = self
            .ctx
            .binder
            .get_symbol_with_libs(root_sym, &lib_binders)?;

        // Look for "JSX" in the root entity's exports (namespace members)
        if let Some(exports) = root_symbol.exports.as_ref()
            && let Some(sym_id) = exports.get("JSX")
        {
            return Some(sym_id);
        }

        if let Some(module_name) = root_symbol.import_module.as_deref() {
            let source_file_idx = self
                .ctx
                .resolve_symbol_file_index(root_sym)
                .unwrap_or(self.ctx.current_file_idx);
            if let Some(sym_id) =
                self.resolve_cross_file_export_from_file(module_name, "JSX", Some(source_file_idx))
            {
                return Some(sym_id);
            }
        }

        // Some binder states keep the namespace merge partner separate from the
        // value-side factory symbol (`const jsx` + `namespace jsx`).
        for &candidate_id in self.ctx.binder.get_symbols().find_all_by_name(root_name) {
            let Some(candidate_symbol) = self.ctx.binder.get_symbol(candidate_id) else {
                continue;
            };
            if (candidate_symbol.flags
                & (symbol_flags::MODULE
                    | symbol_flags::NAMESPACE_MODULE
                    | symbol_flags::VALUE_MODULE))
                == 0
            {
                continue;
            }
            if let Some(exports) = candidate_symbol.exports.as_ref()
                && let Some(sym_id) = exports.get("JSX")
            {
                return Some(sym_id);
            }
        }

        self.resolve_namespace_member_from_all_binders(root_name, "JSX")
    }
}
