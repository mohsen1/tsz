use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    /// Check that the JSX import source module can be resolved (TS2875).
    pub(crate) fn check_jsx_import_source(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        let runtime_suffix = match self.ctx.compiler_options.jsx_mode {
            JsxMode::ReactJsx => "jsx-runtime",
            JsxMode::ReactJsxDev => "jsx-dev-runtime",
            _ => return,
        };

        if self.ctx.jsx_import_source_checked {
            return;
        }
        self.ctx.jsx_import_source_checked = true;

        if !self.ctx.report_unresolved_imports {
            return;
        }

        let source: String = if self.ctx.compiler_options.jsx_import_source.is_empty() {
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

        use crate::diagnostics::diagnostic_codes;
        self.error_at_node_msg(
            node_idx,
            diagnostic_codes::THIS_JSX_TAG_REQUIRES_THE_MODULE_PATH_TO_EXIST_BUT_NONE_COULD_BE_FOUND_MAKE_SURE,
            &[&runtime_path],
        );
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
                let candidate1 = node_modules.join(source).join(format!("{suffix}.d.ts"));
                if candidate1.is_file() {
                    return true;
                }

                let candidate2 = node_modules
                    .join("@types")
                    .join(source)
                    .join(format!("{suffix}.d.ts"));
                if candidate2.is_file() {
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

    /// Check that JSX fragments have a valid fragment factory when jsxFactory is set (TS17016).
    pub(crate) fn check_jsx_fragment_factory(&mut self, node_idx: NodeIndex) {
        use tsz_common::checker_options::JsxMode;

        if self.ctx.compiler_options.jsx_mode != JsxMode::React {
            return;
        }

        if !self.ctx.compiler_options.jsx_factory_from_config
            || self.ctx.compiler_options.jsx_fragment_factory_from_config
        {
            return;
        }

        use crate::diagnostics::diagnostic_codes;
        self.error_at_node(
            node_idx,
            "The 'jsxFragmentFactory' compiler option must be provided to use JSX fragments with the 'jsxFactory' compiler option.",
            diagnostic_codes::THE_JSXFRAGMENTFACTORY_COMPILER_OPTION_MUST_BE_PROVIDED_TO_USE_JSX_FRAGMENTS_WIT,
        );
    }
}
