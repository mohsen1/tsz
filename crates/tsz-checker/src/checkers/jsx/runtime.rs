//! JSX runtime/factory handling: import source validation (TS2875),
//! factory-in-scope checks (TS2874), fragment factory (TS17016/TS2879),
//! pragma extraction, and factory symbol referencing.

use crate::state::CheckerState;
use std::sync::Arc;
use tsz_binder::{BinderState, SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;

/// Returns true when the byte at `pos` (or end-of-string) is a JSDoc pragma
/// tag/value boundary — i.e. ASCII whitespace (matching tsc's `\s+` separator
/// between pragma name and value, and between value and end-of-comment).
///
/// Pragma tags such as `@jsxRuntime`, `@jsxImportSource`, and `@jsxFrag` are
/// only recognized when followed by such a boundary; otherwise comments like
/// `@jsxRuntimeautomatic` or `@jsxImportSourcex` would be misparsed as the
/// real pragma with arbitrary identifier suffixes attached.
fn is_pragma_boundary(body: &str, pos: usize) -> bool {
    let bytes = body.as_bytes();
    pos >= bytes.len() || (bytes[pos] as char).is_ascii_whitespace()
}

/// Find the first occurrence of the pragma tag `tag` in `body` such that the
/// character immediately after the tag is a pragma boundary (whitespace or
/// end-of-body). Returns the byte offset *after* the tag, or `None` if no
/// complete-tag occurrence exists. Iterates past prefix-only matches like
/// `@jsxRuntimeautomatic` so that a later valid `@jsxRuntime classic` in the
/// same comment is still recognized.
fn find_complete_pragma_tag(body: &str, tag: &str) -> Option<usize> {
    let mut start = 0;
    while let Some(rel) = body[start..].find(tag) {
        let abs = start + rel;
        let after = abs + tag.len();
        if is_pragma_boundary(body, after) {
            return Some(after);
        }
        // Advance past this incomplete match and keep scanning.
        start = abs + tag.len();
        if start >= body.len() {
            break;
        }
    }
    None
}

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

/// Extract the `@jsxFrag` pragma factory name from a source file's leading comments.
///
/// Mirrors `extract_jsx_pragma` behavior, but for fragment factory pragmas.
/// Returns values like `"React.Fragment"` or `"Fragment"`.
pub(crate) fn extract_jsx_frag_pragma(source: &str) -> Option<String> {
    let scan_limit = source.len().min(4096);
    let text = &source[..scan_limit];

    let mut pos = 0;
    let bytes = text.as_bytes();
    while pos < bytes.len() {
        if bytes[pos].is_ascii_whitespace() {
            pos += 1;
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = text[comment_start..].find("*/") {
                let comment_body = &text[comment_start..comment_start + end_offset];
                // tsc accepts `@jsxFrag`, `@jsxfrag`, and `@jsxFragment` as
                // synonyms for the fragment-factory pragma. Prefer the longer
                // `@jsxFragment` form when present so that a comment like
                // `@jsxFragment Foo` doesn't get parsed as `@jsxFrag` with an
                // `ment` suffix on the tag (which would fail the boundary
                // check below). Both forms must be followed by a pragma
                // boundary (whitespace / end-of-comment), so neither
                // `@jsxFragx Foo` nor `@jsxFragmentx Foo` matches.
                let lowered = comment_body.to_ascii_lowercase();
                let after_idx = find_complete_pragma_tag(&lowered, "@jsxfragment")
                    .or_else(|| find_complete_pragma_tag(&lowered, "@jsxfrag"));
                if let Some(after) = after_idx {
                    let factory: String = comment_body[after..]
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
                break;
            }
            continue;
        }
        if pos + 1 < bytes.len() && bytes[pos] == b'/' && bytes[pos + 1] == b'/' {
            if let Some(nl) = text[pos..].find('\n') {
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

/// Extract the `@jsxRuntime` pragma from a source file's comments.
///
/// Scans ALL block comments for `@jsxRuntime classic` or `@jsxRuntime automatic`.
/// The last occurrence wins (matching tsc behavior).
pub(crate) fn extract_jsx_runtime_pragma(source: &str) -> Option<&'static str> {
    let mut result = None;
    let bytes = source.as_bytes();
    let mut pos = 0;
    while pos + 1 < bytes.len() {
        if bytes[pos] == b'/' && bytes[pos + 1] == b'*' {
            let comment_start = pos + 2;
            if let Some(end_offset) = source[comment_start..].find("*/") {
                let comment_body = &source[comment_start..comment_start + end_offset];
                if let Some(after) = find_complete_pragma_tag(comment_body, "@jsxRuntime") {
                    // Skip leading whitespace, then read the value token and
                    // require it to be terminated by another pragma boundary.
                    // Together this rejects both `@jsxRuntimeautomatic`
                    // (no boundary after the tag) and `@jsxRuntime automaticx`
                    // (no boundary after the value).
                    let rest = comment_body[after..].trim_start();
                    let value_end = rest
                        .char_indices()
                        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_' || *c == '$'))
                        .map(|(i, _)| i)
                        .unwrap_or(rest.len());
                    let value = &rest[..value_end];
                    let value_terminated =
                        value_end == rest.len() || rest.as_bytes()[value_end].is_ascii_whitespace();
                    if value_terminated {
                        match value {
                            "classic" => result = Some("classic"),
                            "automatic" => result = Some("automatic"),
                            _ => {}
                        }
                    }
                }
                pos = comment_start + end_offset + 2;
            } else {
                break;
            }
            continue;
        }
        pos += 1;
    }
    result
}

impl<'a> CheckerState<'a> {
    pub(super) fn current_jsx_source_text(&self) -> Option<&str> {
        self.ctx
            .get_arena_for_file(self.ctx.current_file_idx as u32)
            .source_files
            .first()
            .or_else(|| self.ctx.arena.source_files.first())
            .map(|sf| sf.text.as_ref())
    }

    /// Return whether the current file contains any JSX construct
    /// (element, self-closing element, or fragment).
    ///
    /// Used to decide whether the JSX factory (e.g. `React`) is actually
    /// referenced by emit/checking. In files without JSX, an unused
    /// `import React from "react"` should still report TS6133, matching tsc.
    pub(crate) fn current_file_contains_jsx(&self) -> bool {
        use tsz_parser::parser::syntax_kind_ext::{
            JSX_ELEMENT, JSX_FRAGMENT, JSX_SELF_CLOSING_ELEMENT,
        };
        self.ctx.arena.nodes.iter().any(|node| {
            node.kind == JSX_ELEMENT
                || node.kind == JSX_FRAGMENT
                || node.kind == JSX_SELF_CLOSING_ELEMENT
        })
    }

    /// Return the effective JSX mode for the current file, taking the
    /// `@jsxRuntime` pragma into account.
    pub(crate) fn effective_jsx_mode(&self) -> tsz_common::checker_options::JsxMode {
        use tsz_common::checker_options::JsxMode;
        let pragma = self
            .current_jsx_source_text()
            .and_then(extract_jsx_runtime_pragma);
        match pragma {
            Some("classic") => JsxMode::React,
            Some("automatic") => {
                if self.ctx.compiler_options.jsx_mode == JsxMode::ReactJsxDev {
                    JsxMode::ReactJsxDev
                } else {
                    JsxMode::ReactJsx
                }
            }
            _ => self.ctx.compiler_options.jsx_mode,
        }
    }

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
        let effective_mode = self.effective_jsx_mode();
        let runtime_suffix = match effective_mode {
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
        // tsc treats jsxImportSource as a package specifier — an absolute path
        // (`/foo`) cannot be a valid package source even when a same-named
        // source file happens to exist in the project, so it always reports
        // TS2875. Skip the resolution checks when the source is absolute so
        // we match that behavior.
        let source_is_absolute = source.starts_with('/');
        if !source_is_absolute
            && (self.module_exists_cross_file(&runtime_path)
                || self.is_ambient_module_match(&runtime_path)
                || self.jsx_runtime_file_exists_on_disk(&source, runtime_suffix))
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
        let text = self.current_jsx_source_text()?;
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
                    // Only honor `@jsxImportSource` when followed by a pragma
                    // boundary. Without this, fake tags like
                    // `@jsxImportSourcex preact` would slip through and the
                    // package parser would extract `x` as the source.
                    if let Some(after) = find_complete_pragma_tag(comment_body, "@jsxImportSource")
                    {
                        let pkg: String = comment_body[after..]
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

    /// Check JSX fragment factory diagnostics:
    /// - TS17016 for config-only `jsxFactory` without `jsxFragmentFactory`
    /// - TS17017 for `@jsx` pragma without `@jsxFrag` pragma
    /// - TS2879 when fragment factory root identifier is missing from scope
    pub(crate) fn check_jsx_fragment_factory(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        if self.effective_jsx_mode() != JsxMode::React {
            return;
        }

        // When @jsxImportSource pragma overrides react mode, skip fragment checks.
        if self.extract_jsx_import_source_pragma().is_some() {
            return;
        }

        let (pragma_factory, pragma_fragment_factory) = self
            .current_jsx_source_text()
            .map(|source| (extract_jsx_pragma(source), extract_jsx_frag_pragma(source)))
            .unwrap_or_default();

        if pragma_factory.is_some() {
            if pragma_fragment_factory.is_none() {
                use crate::diagnostics::diagnostic_codes;
                self.error_at_node_msg(
                    node_idx,
                    diagnostic_codes::AN_JSXFRAG_PRAGMA_IS_REQUIRED_WHEN_USING_AN_JSX_PRAGMA_WITH_JSX_FRAGMENTS,
                    &[],
                );
            }
        } else if self.ctx.compiler_options.jsx_factory_from_config {
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
        // Fragment scope uses @jsxFrag if present, otherwise jsxFragmentFactory
        // (default React.Fragment).
        let fragment_factory = pragma_fragment_factory
            .unwrap_or_else(|| self.ctx.compiler_options.jsx_fragment_factory.clone());
        let root_ident_owned = fragment_factory
            .split('.')
            .next()
            .unwrap_or(&fragment_factory)
            .to_string();
        if root_ident_owned.is_empty() {
            return;
        }
        // tsc treats literal-keyword sentinels (`null`, `undefined`, `true`,
        // `false`) in `@jsxfrag`/`jsxFragmentFactory` as a user-driven opt-out
        // and does not emit TS2879 for them (other diagnostics already cover
        // the invalid-identifier case).
        if matches!(
            root_ident_owned.as_str(),
            "null" | "undefined" | "true" | "false"
        ) {
            return;
        }

        let found = self.resolve_jsx_factory_symbol_in_scope(&root_ident_owned, node_idx);
        if found.is_some() {
            // Mark the fragment factory's import as referenced so subsequent
            // unused-import checks (TS6133 / TS6192) don't flag it.
            self.mark_jsx_name_as_referenced(&fragment_factory, node_idx);
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
            |sym_id| self.is_jsx_factory_symbol_visible(sym_id, &lib_binders),
        ) {
            self.ctx.referenced_symbols.borrow_mut().insert(sym_id);
        }
    }

    fn resolve_jsx_factory_symbol_in_scope(
        &self,
        root_ident: &str,
        node_idx: NodeIndex,
    ) -> Option<SymbolId> {
        let lib_binders = self.get_lib_binders();
        self.ctx
            .binder
            .resolve_name_with_filter(
                root_ident,
                self.ctx.arena,
                node_idx,
                &lib_binders,
                |sym_id| self.is_jsx_factory_symbol_visible(sym_id, &lib_binders),
            )
            .or_else(|| {
                self.resolve_global_value_symbol(root_ident)
                    .filter(|sym_id| self.is_jsx_factory_symbol_visible(*sym_id, &lib_binders))
            })
    }

    fn is_jsx_factory_symbol_visible(
        &self,
        sym_id: SymbolId,
        lib_binders: &[Arc<BinderState>],
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol_with_libs(sym_id, lib_binders) else {
            return false;
        };
        if symbol.is_type_only || !symbol.has_any_flags(symbol_flags::VALUE | symbol_flags::ALIAS) {
            return false;
        }
        if symbol.decl_file_idx == u32::MAX
            || symbol.decl_file_idx == self.ctx.current_file_idx as u32
        {
            return true;
        }
        if symbol.is_umd_export {
            return true;
        }
        if symbol.is_exported || symbol.import_module.is_some() {
            return false;
        }
        let Some(owner_binder) = self.ctx.get_binder_for_file(symbol.decl_file_idx as usize) else {
            return false;
        };
        !owner_binder.is_external_module()
            || owner_binder
                .global_augmentations
                .contains_key(symbol.escaped_name.as_str())
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
        if self.effective_jsx_mode() != JsxMode::React {
            return;
        }

        // When @jsxImportSource pragma is present, it overrides react mode
        // to react-jsx behavior, so the factory scope check doesn't apply.
        if self.extract_jsx_import_source_pragma().is_some() {
            return;
        }

        // tsc 6.0 skips scope checking when jsxFactory / jsxFragmentFactory is
        // explicitly set: the option is a name hint rather than a scope
        // requirement, and other diagnostics (TS17016, TS17017, TS5024) cover
        // invalid configured names. We still mark the factory symbol referenced
        // so unused-import checking (TS6192) doesn't flag it.
        let is_fragment = self
            .ctx
            .arena
            .get(node_idx)
            .is_some_and(|n| n.kind == tsz_parser::parser::syntax_kind_ext::JSX_FRAGMENT);
        if !is_fragment && self.ctx.compiler_options.jsx_factory_from_config {
            self.mark_jsx_name_as_referenced(
                &self.ctx.compiler_options.jsx_factory.clone(),
                node_idx,
            );
            return;
        }
        // For fragments, skip TS2874 whenever EITHER jsxFactory OR
        // jsxFragmentFactory is configured: tsc treats those as a user-driven
        // factory regime and reports TS17016 / TS17017 / TS5024 instead of
        // the scope-of-default-factory message. Mark BOTH the factory and the
        // fragment factory as referenced — fragments compile to
        // `factory(fragmentFactory, ...)` so both names participate, and
        // unused-imports checks (TS6133/TS6192) must see them used.
        if is_fragment
            && (self.ctx.compiler_options.jsx_fragment_factory_from_config
                || self.ctx.compiler_options.jsx_factory_from_config)
        {
            if self.ctx.compiler_options.jsx_factory_from_config {
                self.mark_jsx_name_as_referenced(
                    &self.ctx.compiler_options.jsx_factory.clone(),
                    node_idx,
                );
            }
            self.mark_jsx_name_as_referenced(
                &self.ctx.compiler_options.jsx_fragment_factory.clone(),
                node_idx,
            );
            return;
        }

        // Check for per-file /** @jsx factory */ pragma
        let pragma_factory = self.current_jsx_source_text().and_then(extract_jsx_pragma);
        let pragma_fragment_factory = self
            .current_jsx_source_text()
            .and_then(extract_jsx_frag_pragma);

        let factory = if is_fragment {
            pragma_fragment_factory
                .unwrap_or_else(|| self.ctx.compiler_options.jsx_fragment_factory.clone())
        } else {
            pragma_factory
                .clone()
                .unwrap_or_else(|| self.ctx.compiler_options.jsx_factory.clone())
        };
        let root_ident = factory.split('.').next().unwrap_or(&factory);

        if root_ident.is_empty() {
            return;
        }
        // Literal-keyword sentinels (`null`, `undefined`, `true`, `false`) in
        // a `@jsxfrag`/`@jsx` pragma are user-driven opt-outs. tsc does not
        // emit TS2874 for them — other diagnostics (TS17016/TS17017/TS2879)
        // cover the invalid-identifier case when appropriate. The JSX factory
        // is still conceptually used (fragments compile to `factory(null, …)`),
        // so mark its import as referenced before returning.
        if is_fragment && matches!(root_ident, "null" | "undefined" | "true" | "false") {
            let jsx_factory =
                pragma_factory.unwrap_or_else(|| self.ctx.compiler_options.jsx_factory.clone());
            let jsx_root = jsx_factory.split('.').next().unwrap_or(&jsx_factory);
            if !jsx_root.is_empty() {
                let jsx_in_scope = self
                    .resolve_jsx_factory_symbol_in_scope(jsx_root, node_idx)
                    .is_some();
                if jsx_in_scope {
                    self.mark_jsx_name_as_referenced(&jsx_factory, node_idx);
                } else {
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
                        &[jsx_root],
                    );
                }
            }
            return;
        }

        let file_has_any_parse_diag =
            self.ctx.has_parse_errors || !self.ctx.all_parse_error_positions.is_empty();
        if file_has_any_parse_diag {
            return;
        }

        let resolved_in_scope = self
            .resolve_jsx_factory_symbol_in_scope(root_ident, node_idx)
            .is_some();

        if resolved_in_scope {
            // tsc treats a pragma-driven `@jsx` / `@jsxFrag` factory as a use
            // of the imported identifier — suppress TS6133 / TS6192 on the
            // import that brought the factory into scope.
            self.mark_jsx_name_as_referenced(&factory, node_idx);
            // Fragments compile to `<jsx-factory>(<fragment-factory>, …)`, so the
            // JSX factory itself is also conceptually used by every fragment.
            // Mark its import too (and emit TS2874 if it isn't in scope).
            if is_fragment {
                let jsx_factory =
                    pragma_factory.unwrap_or_else(|| self.ctx.compiler_options.jsx_factory.clone());
                let jsx_root = jsx_factory.split('.').next().unwrap_or(&jsx_factory);
                if !jsx_root.is_empty() {
                    let jsx_in_scope = self
                        .resolve_jsx_factory_symbol_in_scope(jsx_root, node_idx)
                        .is_some();
                    if jsx_in_scope {
                        self.mark_jsx_name_as_referenced(&jsx_factory, node_idx);
                    } else {
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
                            &[jsx_root],
                        );
                    }
                }
            }
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
        let pragma_factory = self.current_jsx_source_text().and_then(extract_jsx_pragma);
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

#[cfg(test)]
pub(crate) fn extract_jsx_import_source_pragma_text_only_for_test(text: &str) -> Option<String> {
    // Mirrors `CheckerState::extract_jsx_import_source_pragma` but operates on
    // a raw `&str`, so we can unit-test the boundary handling without
    // constructing a checker. Kept in sync with the real implementation.
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
                if let Some(after) = find_complete_pragma_tag(comment_body, "@jsxImportSource") {
                    let pkg: String = comment_body[after..]
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

#[cfg(test)]
mod pragma_boundary_tests {
    //! Regression coverage for issue #2942: JSDoc pragmas like `@jsxRuntime`,
    //! `@jsxImportSource`, and `@jsxFrag` must be parsed as complete tags
    //! followed by a whitespace boundary, not as raw substrings/prefixes.
    //!
    //! Each invalid case here was previously misrecognized by tsz and changed
    //! JSX checking even though tsc treats them as unrelated tags.
    use super::{extract_jsx_frag_pragma, extract_jsx_import_source_pragma_text_only_for_test};
    use super::{extract_jsx_pragma, extract_jsx_runtime_pragma};

    // ---- @jsxRuntime --------------------------------------------------------

    #[test]
    fn jsx_runtime_classic_recognized() {
        assert_eq!(
            extract_jsx_runtime_pragma("/* @jsxRuntime classic */\nconst x = 1;"),
            Some("classic")
        );
    }

    #[test]
    fn jsx_runtime_automatic_recognized() {
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime automatic */\n"),
            Some("automatic")
        );
    }

    #[test]
    fn jsx_runtime_prefix_tag_is_ignored() {
        // `@jsxRuntimeautomatic` is not the @jsxRuntime tag — it is some
        // unknown JSDoc tag. Must not switch to automatic mode.
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntimeautomatic */\n"),
            None
        );
        assert_eq!(
            extract_jsx_runtime_pragma("/* @jsxRuntimeclassic */\n"),
            None
        );
    }

    #[test]
    fn jsx_runtime_invalid_value_with_suffix_is_ignored() {
        // Tag boundary holds, but the value `automaticx` is not `automatic`.
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime automaticx */\n"),
            None
        );
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime classicx */\n"),
            None
        );
    }

    #[test]
    fn jsx_runtime_unknown_value_is_ignored() {
        assert_eq!(
            extract_jsx_runtime_pragma("/** @jsxRuntime hybrid */\n"),
            None
        );
    }

    #[test]
    fn jsx_runtime_later_valid_pragma_still_wins_after_invalid_prefix() {
        // tsc keeps the last valid occurrence; a junk `@jsxRuntimeautomatic`
        // earlier must not poison a later real `@jsxRuntime classic`.
        let src = "/** @jsxRuntimeautomatic */\n/** @jsxRuntime classic */\n";
        assert_eq!(extract_jsx_runtime_pragma(src), Some("classic"));
    }

    // ---- @jsxImportSource ---------------------------------------------------

    #[test]
    fn jsx_import_source_recognized() {
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test("/** @jsxImportSource preact */\n"),
            Some("preact".to_string())
        );
    }

    #[test]
    fn jsx_import_source_prefix_tag_is_ignored() {
        // `@jsxImportSourcex preact` is an unrelated tag — must not yield
        // package `x` (the previous bug) or `preact`.
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test(
                "/** @jsxImportSourcex preact */\n"
            ),
            None
        );
    }

    #[test]
    fn jsx_import_source_scoped_package_recognized() {
        assert_eq!(
            extract_jsx_import_source_pragma_text_only_for_test(
                "/* @jsxImportSource @emotion/react */\n"
            ),
            Some("@emotion/react".to_string())
        );
    }

    // ---- @jsxFrag / @jsxFragment --------------------------------------------

    #[test]
    fn jsx_frag_recognized() {
        assert_eq!(
            extract_jsx_frag_pragma("/** @jsxFrag Fragment */\n"),
            Some("Fragment".to_string())
        );
    }

    #[test]
    fn jsx_fragment_long_form_recognized() {
        // tsc accepts `@jsxFragment` as a synonym; previously the longer form
        // would be parsed as `@jsxFrag` plus an `ment` suffix, which now
        // (correctly) fails the boundary check — so the longer form must be
        // tried first.
        assert_eq!(
            extract_jsx_frag_pragma("/** @jsxFragment Foo */\n"),
            Some("Foo".to_string())
        );
    }

    #[test]
    fn jsx_frag_prefix_tag_is_ignored() {
        assert_eq!(extract_jsx_frag_pragma("/** @jsxFragx Fragment */\n"), None);
        assert_eq!(extract_jsx_frag_pragma("/** @jsxFragmentx Foo */\n"), None);
    }

    // ---- @jsx (control: existing behavior preserved) ------------------------

    #[test]
    fn jsx_factory_pragma_still_recognized() {
        assert_eq!(extract_jsx_pragma("/** @jsx h */\n"), Some("h".to_string()));
        assert_eq!(
            extract_jsx_pragma("/** @jsx React.createElement */\n"),
            Some("React.createElement".to_string())
        );
    }
}
