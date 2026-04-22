//! Module resolution and query helpers for `DeclarationChecker`.

use std::path::{Component, Path, PathBuf};
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

use crate::declarations::DeclarationChecker;

impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    /// Check if the current file is a declaration file (.d.ts, .d.mts, .d.cts).
    pub(crate) fn is_declaration_file(&self) -> bool {
        self.ctx.is_declaration_file()
    }

    /// Check if the current file is an external module (has import/export statements).
    /// Script files (global scope) don't have imports/exports.
    pub(crate) fn is_external_module(&self) -> bool {
        // Check the per-file cache first (set by CLI driver for multi-file mode)
        // This preserves the correct is_external_module value across sequential file bindings
        if let Some(ref map) = self.ctx.is_external_module_by_file
            && let Some(&is_ext) = map.get(&self.ctx.file_name)
        {
            return is_ext;
        }
        // Fallback to binder (for single-file mode or tests)
        self.ctx.binder.is_external_module()
    }

    /// Check if a module exists (for TS2664 check).
    /// Returns true if the module is in `resolved_modules`, `module_exports`,
    /// `declared_modules`, or `shorthand_ambient_modules`.
    pub(crate) fn module_exists(&self, module_name: &str) -> bool {
        if self.ctx.resolve_import_target(module_name).is_some() {
            return true;
        }

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return true;
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self
            .ctx
            .module_exports_contains_module(self.ctx.binder, module_name)
        {
            return true;
        }

        if let Some(target_idx) = self.ctx.resolve_import_target(module_name)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            if let Some(target_file_name) = self
                .ctx
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                && self
                    .ctx
                    .module_exports_contains_module(target_binder, target_file_name)
            {
                return true;
            }
            if self
                .ctx
                .module_exports_contains_module(target_binder, module_name)
            {
                return true;
            }
        }

        // Check ambient module declarations (`declare module "X" { ... }`)
        if self.ctx.binder.declared_modules.contains(module_name) {
            return true;
        }

        // Check shorthand ambient modules (`declare module "X";`)
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return true;
        }

        // Check wildcard patterns in declared/shorthand ambient modules and module_exports
        if self.matches_ambient_module_pattern(module_name) {
            return true;
        }

        false
    }

    /// Check if a module name matches any wildcard ambient module pattern.
    pub(crate) fn matches_ambient_module_pattern(&self, module_name: &str) -> bool {
        let module_name = module_name.trim().trim_matches('"').trim_matches('\'');

        for patterns in [
            &self.ctx.binder.declared_modules,
            &self.ctx.binder.shorthand_ambient_modules,
        ] {
            for pattern in patterns {
                let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
                if pattern.contains('*')
                    && let Ok(glob) = globset::GlobBuilder::new(pattern)
                        .literal_separator(false)
                        .build()
                    && glob.compile_matcher().is_match(module_name)
                {
                    return true;
                }
            }
        }

        // Also check module_exports keys for wildcard patterns
        for pattern in self.ctx.binder.module_exports.keys() {
            let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
            if pattern.contains('*')
                && let Ok(glob) = globset::GlobBuilder::new(pattern)
                    .literal_separator(false)
                    .build()
                && glob.compile_matcher().is_match(module_name)
            {
                return true;
            }
        }

        false
    }

    /// Check if a module name is relative (starts with ./ or ../)
    pub(crate) fn is_relative_module_name(&self, name: &str) -> bool {
        if name.starts_with("./")
            || name.starts_with(".\\")
            || name.starts_with("../")
            || name.starts_with("..\\")
            || name == "."
            || name == ".."
            || name.starts_with('/')
        {
            return true;
        }

        // Treat rooted drive-specifier paths (e.g. "c:/x", "c:\\x") as invalid
        // for ambient module declarations as tsc does.
        let bytes = name.as_bytes();
        bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\')
    }

    pub(crate) fn module_augmentation_has_value_exports(&self, module_body: NodeIndex) -> bool {
        if module_body.is_none() {
            return false;
        }

        let Some(body_node) = self.ctx.arena.get(module_body) else {
            return false;
        };
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return false;
        }
        let Some(block) = self.ctx.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(stmts) = block.statements.as_ref() else {
            return false;
        };

        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                syntax_kind_ext::VARIABLE_STATEMENT
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) {
                        if export_decl.is_default_export
                            || export_decl.module_specifier.is_some()
                            || export_decl.export_clause.is_none()
                        {
                            return true;
                        }
                        if let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) {
                            match clause_node.kind {
                                syntax_kind_ext::VARIABLE_STATEMENT
                                | syntax_kind_ext::FUNCTION_DECLARATION
                                | syntax_kind_ext::CLASS_DECLARATION
                                | syntax_kind_ext::ENUM_DECLARATION => return true,
                                _ => {}
                            }
                        }
                    } else {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Normalize module augmentation keys for relative specifiers.
    pub(crate) fn normalize_module_augmentation_key(&self, name: &str) -> String {
        if let Some(target_idx) = self.ctx.resolve_import_target(name) {
            return format!("file_idx:{target_idx}");
        }
        if self.is_relative_module_name(name)
            && let Some(parent) = Path::new(&self.ctx.file_name).parent()
        {
            let joined = parent.join(name);
            let normalized = Self::normalize_path(&joined);
            return normalized.to_string_lossy().to_string();
        }
        name.to_string()
    }

    pub(crate) fn normalize_path(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                Component::RootDir => normalized.push(component.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::Normal(part) => normalized.push(part),
            }
        }
        normalized
    }

    /// Check if a node is inside a namespace/module declaration.
    /// This is used for TS2435 (ambient modules cannot be nested).
    /// Returns true only if a parent is an identifier-named namespace/module
    /// (e.g., `namespace M { }`). String-named ambient external modules
    /// (e.g., `declare module "foo" { }`) do NOT count — nested string-named
    /// modules inside them are module augmentations, which are valid.
    pub(crate) fn is_inside_namespace(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;

        // Skip the first iteration (the node itself)
        if let Some(ext) = self.ctx.arena.get_extended(current) {
            current = ext.parent;
        } else {
            return false;
        }

        while current.is_some() {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };

            if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Check if this parent module has an identifier name (namespace)
                // vs a string literal name (ambient external module).
                // Only identifier-named parents constitute a nesting violation.
                if let Some(module_data) = self.ctx.arena.get_module(node)
                    && let Some(name_node) = self.ctx.arena.get(module_data.name)
                    && name_node.kind == SyntaxKind::StringLiteral as u16
                {
                    // Parent is an ambient external module — skip it,
                    // nested string-named modules are module augmentations (valid)
                } else {
                    return true;
                }
            }

            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }

        false
    }

    /// Check if a node is inside an ambient context (declare namespace/module or .d.ts file).
    pub(crate) fn is_in_ambient_context(&self, node_idx: NodeIndex) -> bool {
        self.ctx.is_declaration_file() || self.ctx.arena.is_in_ambient_context(node_idx)
    }
}
