use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
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
}
