impl<'a> CheckerState<'a> {
    /// TS4094: For each `export default <expr>` statement, check whether the
    /// expression's type is an anonymous class constructor.  If so, report TS4094 for
    /// each private/protected member of its instance type.
    ///
    /// This covers patterns like `export default mix(AnonymousClass)` where the call
    /// returns the same anonymous class constructor type that was passed in.
    fn check_ts4094_in_export_assignments(&mut self, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            // TSZ represents `export default <expr>` as an EXPORT_DECLARATION node with
            // `is_default_export: true`. The TypeScript AST uses ExportAssignment for this,
            // but TSZ's parser collapses both into EXPORT_DECLARATION.
            if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.ctx.arena.get_export_decl(node).cloned() else {
                continue;
            };
            // Only care about `export default <expr>`, not `export { ... }` or re-exports.
            if !export_decl.is_default_export {
                continue;
            }
            let expr_idx = export_decl.export_clause;
            if expr_idx == tsz_parser::parser::NodeIndex::NONE {
                continue;
            }
            // Skip class/function declarations — they are handled by the class/function
            // checker paths which already emit TS4094 for anonymous class members.
            let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_EXPRESSION
                || expr_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
            {
                continue;
            }
            // Resolve the expression to an instance type. For patterns like
            // `export default mix(DisposableMixin)` where mix<T>(x:T):T returns the
            // constructor as-is, this yields the anonymous class's instance type.
            let Some(instance_type) = self.base_instance_type_from_expression(expr_idx, None)
            else {
                continue;
            };
            if self.instance_type_is_from_anonymous_class(instance_type) {
                self.report_instance_type_private_members_as_ts4094(stmt_idx, instance_type);
            }
        }
    }

    fn recheck_checked_js_import_diagnostics(&mut self, statements: &[NodeIndex]) {
        for &stmt_idx in statements {
            let Some(node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                continue;
            }

            let Some(import) = self.ctx.arena.get_import_decl(node).cloned() else {
                continue;
            };
            let Some(spec_node) = self.ctx.arena.get(import.module_specifier) else {
                continue;
            };
            let Some(literal) = self.ctx.arena.get_literal(spec_node) else {
                continue;
            };
            let resolved_target = self
                .ctx
                .resolve_import_target_from_file(self.ctx.current_file_idx, &literal.text)
                .or_else(|| self.ctx.resolve_import_target(&literal.text));
            if resolved_target.is_none() && !self.module_exists_cross_file(&literal.text) {
                continue;
            }

            self.check_imported_members(&import, &literal.text);
        }
    }
}
