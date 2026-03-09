//! Import type resolution helpers (`import("./module").Foo`).

use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Format a generic type name with its type parameter names for TS2314 messages.
    /// e.g., "Foo" + [T, U] → "Foo<T, U>"
    pub(crate) fn format_generic_display_name_with_interner(
        name: &str,
        type_params: &[tsz_solver::TypeParamInfo],
        types: &dyn tsz_solver::QueryDatabase,
    ) -> String {
        if type_params.is_empty() {
            return name.to_string();
        }
        let param_names: Vec<String> = type_params
            .iter()
            .map(|p| types.resolve_atom(p.name))
            .collect();
        format!("{}<{}>", name, param_names.join(", "))
    }

    /// Walk the left chain of nested qualified names to find a root import call.
    /// For `import("./m").A.B`, the AST is:
    ///   QualifiedName(left: QualifiedName(left: CallExpr(import("./m")), right: A), right: B)
    /// Returns the `CALL_EXPRESSION` `NodeIndex` if the leftmost node is an `import()` call.
    pub(crate) fn find_leftmost_import_call(&self, mut idx: NodeIndex) -> Option<NodeIndex> {
        const MAX_DEPTH: usize = 64;
        for _ in 0..MAX_DEPTH {
            let node = self.ctx.arena.get(idx)?;
            if node.kind == syntax_kind_ext::QUALIFIED_NAME {
                let qn = self.ctx.arena.get_qualified_name(node)?;
                idx = qn.left;
            } else if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                // Check if it's import(...)
                let call = self.ctx.arena.get_call_expr(node)?;
                let expr_node = self.ctx.arena.get(call.expression)?;
                if expr_node.kind == SyntaxKind::ImportKeyword as u16 {
                    return Some(idx);
                }
                return None;
            } else {
                return None;
            }
        }
        None
    }

    /// Extract the module specifier string from an `import()` call expression.
    pub(crate) fn get_import_type_module_specifier(
        &self,
        call_idx: NodeIndex,
    ) -> Option<(String, NodeIndex)> {
        let node = self.ctx.arena.get(call_idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        let args = call.arguments.as_ref()?;
        let &first_arg = args.nodes.first()?;
        let arg_node = self.ctx.arena.get(first_arg)?;
        let literal = self.ctx.arena.get_literal(arg_node)?;
        Some((literal.text.clone(), first_arg))
    }

    /// Check an import type expression for module resolution and emit TS2307 if needed.
    /// Returns the resolved type or `TypeId::ERROR`.
    pub(crate) fn check_import_type_and_resolve(
        &mut self,
        call_idx: NodeIndex,
        _type_name_idx: NodeIndex,
        _type_ref_idx: NodeIndex,
    ) -> TypeId {
        let Some((module_name, specifier_node)) = self.get_import_type_module_specifier(call_idx)
        else {
            return TypeId::ERROR;
        };

        if !self.ctx.report_unresolved_imports {
            return TypeId::ERROR;
        }

        // Check if the module resolves through any of the known resolution paths.
        // Import type specifiers may not have been collected by the CLI driver's
        // module specifier scanner (which only scans import/export declarations),
        // so resolved_modules may not have an entry. We check multiple sources:

        // 1. Driver-resolved modules (from import/export declarations)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(&module_name)
        {
            return TypeId::ERROR; // Module exists — return ERROR (lowering can't resolve it yet)
        }

        // 2. Binder module_exports (cross-file)
        if self.ctx.binder.module_exports.contains_key(&module_name) {
            return TypeId::ERROR;
        }

        // 3. Shorthand ambient modules (declare module "foo")
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(&module_name)
        {
            return TypeId::ERROR;
        }

        // 4. Declared modules (ambient modules with body)
        if self.ctx.binder.declared_modules.contains(&module_name) {
            return TypeId::ERROR;
        }

        // 5. Check if the driver has a resolution error for this specifier
        //    (positive evidence of failed resolution)
        if let Some(error) = self.ctx.get_resolution_error(&module_name) {
            let error_code = error.code;
            let error_message = error.message.clone();
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                self.error_at_node(specifier_node, &error_message, error_code);
            }
            return TypeId::ERROR;
        }

        // 6. For non-relative specifiers (no ./ or ../ prefix), if not found in
        //    declared/ambient modules, emit TS2307. Non-relative specifiers target
        //    packages or ambient modules — the binder has complete information.
        let is_relative = module_name.starts_with("./") || module_name.starts_with("../");
        if !is_relative {
            let module_key = module_name.to_string();
            if !self.ctx.modules_with_ts2307_emitted.contains(&module_key) {
                self.ctx.modules_with_ts2307_emitted.insert(module_key);
                let (message, code) = self.module_not_found_diagnostic(&module_name);
                self.error_at_node(specifier_node, &message, code);
            }
            return TypeId::ERROR;
        }

        // 7. For relative specifiers without resolution data, we can't determine
        //    if the module exists (import type specifiers aren't collected by the
        //    driver's module scanner). Check resolved_module_paths for cross-file
        //    resolution.
        if let Some(ref paths) = self.ctx.resolved_module_paths {
            // If there's no entry for this (file_idx, specifier), the specifier
            // was never resolved. Check if any project file matches.
            let key = (self.ctx.current_file_idx, module_name);
            if paths.contains_key(&key) {
                return TypeId::ERROR; // Module resolved to a project file
            }
        }

        // Relative specifier with no resolution data — we can't confirm it doesn't
        // exist. Return ERROR without emitting TS2307 to avoid false positives.
        TypeId::ERROR
    }
}
