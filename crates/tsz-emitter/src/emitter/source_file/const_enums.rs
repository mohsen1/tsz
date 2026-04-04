use super::super::Printer;
use crate::enums::evaluator::EnumEvaluator;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::NodeList;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Pre-pass: scan all statements (recursively) for const enum declarations
    /// and evaluate their member values. The results are stored in
    /// `const_enum_values` so that property/element access expressions
    /// referencing const enum members can be inlined during emit.
    pub(in crate::emitter) fn collect_const_enum_values(&mut self, statements: &NodeList) {
        self.const_enum_values.clear();
        self.const_enum_import_aliases.clear();
        let mut evaluator = EnumEvaluator::new(self.arena);
        self.collect_const_enums_recursive(&mut evaluator, statements, 0, u32::MAX, "");
        self.collect_const_enum_import_aliases(statements);
    }

    /// Recursively scan a statement list for const enum declarations,
    /// descending into function bodies, blocks, namespaces, etc.
    /// `scope_start`/`scope_end` track the enclosing function's position range
    /// (or `0..u32::MAX` for file-level) so that const enums are scoped correctly.
    fn collect_const_enums_recursive(
        &mut self,
        evaluator: &mut EnumEvaluator,
        statements: &NodeList,
        scope_start: u32,
        scope_end: u32,
        ns_prefix: &str,
    ) {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };

            // Direct const enum declarations
            if stmt_node.kind == syntax_kind_ext::ENUM_DECLARATION {
                self.try_register_const_enum(
                    evaluator,
                    stmt_idx,
                    scope_start,
                    scope_end,
                    ns_prefix,
                );
                continue;
            }

            // `export enum` / `export const enum` / `export namespace` / `export function`
            // — the declaration is inside an ExportDeclaration wrapper
            if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_data) = self.arena.get_export_decl(stmt_node)
                && export_data.export_clause.is_some()
            {
                let clause_idx = export_data.export_clause;
                if let Some(clause_node) = self.arena.get(clause_idx) {
                    if clause_node.kind == syntax_kind_ext::ENUM_DECLARATION {
                        self.try_register_const_enum(
                            evaluator,
                            clause_idx,
                            scope_start,
                            scope_end,
                            ns_prefix,
                        );
                    }
                    // Recurse into exported namespace/module bodies
                    if let Some(module_data) = self.arena.get_module(clause_node) {
                        let child_prefix = self.build_ns_prefix(ns_prefix, module_data.name);
                        self.recurse_into_module_body(
                            evaluator,
                            module_data.body,
                            scope_start,
                            scope_end,
                            &child_prefix,
                        );
                    }
                    // Recurse into exported function bodies
                    if let Some(func) = self.arena.get_function(clause_node)
                        && let Some(body_node) = self.arena.get(func.body)
                        && let Some(block) = self.arena.get_block(body_node)
                    {
                        // Entering a new function scope — use the function's range
                        let fn_start = clause_node.pos;
                        let fn_end = clause_node.end;
                        self.collect_const_enums_recursive(
                            evaluator,
                            &block.statements,
                            fn_start,
                            fn_end,
                            ns_prefix,
                        );
                    }
                }
                continue;
            }

            // Recurse into function/method/constructor bodies
            if let Some(func) = self.arena.get_function(stmt_node) {
                if let Some(body_node) = self.arena.get(func.body)
                    && let Some(block) = self.arena.get_block(body_node)
                {
                    // Entering a new function scope — use the function's range
                    let fn_start = stmt_node.pos;
                    let fn_end = stmt_node.end;
                    self.collect_const_enums_recursive(
                        evaluator,
                        &block.statements,
                        fn_start,
                        fn_end,
                        ns_prefix,
                    );
                }
                continue;
            }

            // Recurse into blocks (if/else/try/catch/while/for bodies)
            if let Some(block) = self.arena.get_block(stmt_node) {
                self.collect_const_enums_recursive(
                    evaluator,
                    &block.statements,
                    scope_start,
                    scope_end,
                    ns_prefix,
                );
                continue;
            }

            // Recurse into namespace/module bodies
            if let Some(module_data) = self.arena.get_module(stmt_node) {
                let child_prefix = self.build_ns_prefix(ns_prefix, module_data.name);
                self.recurse_into_module_body(
                    evaluator,
                    module_data.body,
                    scope_start,
                    scope_end,
                    &child_prefix,
                );
                continue;
            }

            // Recurse into if statement branches
            if let Some(if_data) = self.arena.get_if_statement(stmt_node) {
                if let Some(then_node) = self.arena.get(if_data.then_statement)
                    && let Some(block) = self.arena.get_block(then_node)
                {
                    self.collect_const_enums_recursive(
                        evaluator,
                        &block.statements,
                        scope_start,
                        scope_end,
                        ns_prefix,
                    );
                }
                if let Some(else_node) = self.arena.get(if_data.else_statement)
                    && let Some(block) = self.arena.get_block(else_node)
                {
                    self.collect_const_enums_recursive(
                        evaluator,
                        &block.statements,
                        scope_start,
                        scope_end,
                        ns_prefix,
                    );
                }
            }
        }
    }

    /// Register a single enum declaration if it is a const enum.
    fn try_register_const_enum(
        &mut self,
        evaluator: &mut EnumEvaluator,
        enum_idx: NodeIndex,
        scope_start: u32,
        scope_end: u32,
        ns_prefix: &str,
    ) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        // Only process const enums (not regular enums)
        if !self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
        {
            return;
        }

        // Skip ambient (declare) enums — they may reference values from other files
        if self
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return;
        }

        // Get enum name
        let simple_name = self.get_identifier_text_idx(enum_data.name);
        if simple_name.is_empty() {
            return;
        }
        let qualified_key = if ns_prefix.is_empty() {
            simple_name
        } else {
            format!("{ns_prefix}.{simple_name}")
        };
        let values = evaluator.evaluate_enum(enum_idx);
        if !values.is_empty() {
            use crate::emitter::core::ScopedConstEnum;
            let entry = ScopedConstEnum {
                scope_start,
                scope_end,
                values,
            };
            self.const_enum_values
                .entry(qualified_key)
                .or_default()
                .push(entry);
        }
    }

    /// Helper: recurse into a module/namespace body for const enum collection.
    /// Handles both `Block` and `ModuleBlock` body nodes.
    fn build_ns_prefix(&self, current_prefix: &str, name_idx: NodeIndex) -> String {
        let name = self.get_identifier_text_idx(name_idx);
        if name.is_empty() {
            return current_prefix.to_string();
        }
        if current_prefix.is_empty() {
            name
        } else {
            format!("{current_prefix}.{name}")
        }
    }

    fn recurse_into_module_body(
        &mut self,
        evaluator: &mut EnumEvaluator,
        body_idx: NodeIndex,
        scope_start: u32,
        scope_end: u32,
        ns_prefix: &str,
    ) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };
        // Try regular Block first
        if let Some(block) = self.arena.get_block(body_node) {
            self.collect_const_enums_recursive(
                evaluator,
                &block.statements,
                scope_start,
                scope_end,
                ns_prefix,
            );
            return;
        }
        // Try ModuleBlock (namespace bodies use this)
        if let Some(module_block) = self.arena.get_module_block(body_node)
            && let Some(statements) = &module_block.statements
        {
            self.collect_const_enums_recursive(
                evaluator,
                statements,
                scope_start,
                scope_end,
                ns_prefix,
            );
        }
    }

    fn collect_const_enum_import_aliases(&mut self, statements: &NodeList) {
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_EQUALS_DECLARATION {
                continue;
            }
            let Some(import_data) = self.arena.get_import_decl(stmt_node) else {
                continue;
            };
            if import_data.is_type_only {
                continue;
            }
            let alias_name = self.get_identifier_text_idx(import_data.import_clause);
            if alias_name.is_empty() {
                continue;
            }
            let target = self.qualified_name_to_string(import_data.module_specifier);
            if !target.is_empty() {
                self.const_enum_import_aliases.insert(alias_name, target);
            }
        }
    }

    fn qualified_name_to_string(&self, idx: NodeIndex) -> String {
        let Some(node) = self.arena.get(idx) else {
            return String::new();
        };
        if node.kind == SyntaxKind::Identifier as u16 {
            return self.get_identifier_text_idx(idx);
        }
        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            if let Some(qn) = self.arena.get_qualified_name(node) {
                let left = self.qualified_name_to_string(qn.left);
                let right = self.get_identifier_text_idx(qn.right);
                if left.is_empty() {
                    return right;
                }
                if right.is_empty() {
                    return left;
                }
                return format!("{left}.{right}");
            }
        }
        String::new()
    }

    /// Pre-scan `export { x, y }` clauses (without module specifier) to collect
    /// local names that need inline `exports.X = X;` after their declarations.
    pub(in crate::emitter) fn collect_cjs_deferred_export_names(
        &self,
        statements: &tsz_parser::parser::NodeList,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export.module_specifier.is_some() {
                continue;
            }
            // Skip `export type { ... }` — type-only exports have no runtime effect
            if export.is_type_only {
                continue;
            }
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            // Skip clauses that mix same-name and renamed exports
            // (e.g., `export { as, as as return }`). When mixed, let the
            // clause handle ALL exports together to preserve source order.
            let has_renamed = named.elements.nodes.iter().any(|&spec_idx| {
                self.arena
                    .get(spec_idx)
                    .and_then(|n| self.arena.get_specifier(n))
                    .is_some_and(|s| s.property_name.is_some())
            });
            if has_renamed {
                continue;
            }
            for &spec_idx in &named.elements.nodes {
                if let Some(spec_node) = self.arena.get(spec_idx)
                    && let Some(spec) = self.arena.get_specifier(spec_node)
                {
                    if spec.is_type_only {
                        continue;
                    }
                    let local = self.get_identifier_text_idx(spec.name);
                    if !local.is_empty() {
                        names.insert(local);
                    }
                }
            }
        }
        // Remove function names — handled by preamble
        for (name, _) in &self.ctx.module_state.hoisted_func_exports {
            names.remove(name.as_str());
        }
        names
    }

    pub(in crate::emitter) fn collect_cjs_deferred_export_bindings(
        &self,
        statements: &tsz_parser::parser::NodeList,
    ) -> rustc_hash::FxHashMap<String, String> {
        let mut bindings = rustc_hash::FxHashMap::default();
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export) = self.arena.get_export_decl(stmt_node) else {
                continue;
            };
            if export.module_specifier.is_some() || export.is_type_only {
                continue;
            }
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            for &spec_idx in &named.elements.nodes {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    continue;
                };
                if spec.is_type_only {
                    continue;
                }
                let Some(export_name) = self.get_specifier_name_text(spec.name) else {
                    continue;
                };
                let local_name = if spec.property_name.is_some() {
                    self.get_specifier_name_text(spec.property_name)
                        .unwrap_or_else(|| export_name.clone())
                } else {
                    export_name.clone()
                };
                bindings.entry(local_name).or_insert(export_name);
            }
        }
        for (_, local_name) in &self.ctx.module_state.hoisted_func_exports {
            bindings.remove(local_name.as_str());
        }
        bindings
    }

    /// Get names declared by a statement for inline CJS export.
    /// Only returns names that have initializers — declarations without initializers
    /// are already covered by the preamble `exports.X = void 0;`.
    pub(in crate::emitter) fn get_declaration_export_names(
        &self,
        node: &tsz_parser::parser::node::Node,
    ) -> Vec<String> {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.arena.get_variable(node) {
                    return self.collect_variable_names_with_initializers(&var_stmt.declarations);
                }
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class) = self.arena.get_class(node) {
                    let name = self.get_identifier_text_idx(class.name);
                    if !name.is_empty() {
                        return vec![name];
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Collect variable names from declarations that HAVE initializers.
    fn collect_variable_names_with_initializers(
        &self,
        declarations: &tsz_parser::parser::NodeList,
    ) -> Vec<String> {
        let mut names = Vec::new();
        for &decl_idx in &declarations.nodes {
            if let Some(decl_node) = self.arena.get(decl_idx) {
                if let Some(var_decl_list) = self.arena.get_variable(decl_node) {
                    for &inner_idx in &var_decl_list.declarations.nodes {
                        if let Some(inner_node) = self.arena.get(inner_idx)
                            && let Some(decl) = self.arena.get_variable_declaration(inner_node)
                            && decl.initializer.is_some()
                            && let Some(name_node) = self.arena.get(decl.name)
                            && let Some(ident) = self.arena.get_identifier(name_node)
                        {
                            names.push(ident.escaped_text.clone());
                        }
                    }
                } else if let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    && decl.initializer.is_some()
                    && let Some(name_node) = self.arena.get(decl.name)
                    && let Some(ident) = self.arena.get_identifier(name_node)
                {
                    names.push(ident.escaped_text.clone());
                }
            }
        }
        names
    }

    pub(in crate::emitter) fn should_defer_for_of_comments(&self, node: &Node) -> bool {
        let for_of = match self.arena.get_for_in_of(node) {
            Some(for_of) => for_of,
            None => return false,
        };

        if for_of.await_modifier {
            return !self.ctx.options.target.supports_es2018();
        }

        self.ctx.target_es5 && self.ctx.options.downlevel_iteration
    }
}
