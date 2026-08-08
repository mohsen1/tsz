use crate::emitter::{JsxEmit, ModuleKind, Printer};
use crate::transforms::emit_utils;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Check if the file contains an export assignment (export =) with a runtime value.
    pub(in crate::emitter) fn has_export_assignment(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                && !self.export_assignment_identifier_is_type_only(node, statements)
            {
                return true;
            }
        }
        false
    }

    pub(in crate::emitter) fn export_assignment_identifier_is_type_only(
        &self,
        export_assignment_node: &Node,
        statements: &NodeList,
    ) -> bool {
        // With --verbatimModuleSyntax, type-only exports are NOT elided.
        // tsc preserves `export = I` → `module.exports = I;` even for interfaces.
        if self.ctx.options.verbatim_module_syntax {
            return false;
        }

        let Some(export_assign) = self.arena.get_export_assignment(export_assignment_node) else {
            return false;
        };
        let Some(assigned_name) = self.get_module_root_name(export_assign.expression) else {
            return false;
        };

        let mut matched_type = false;
        let mut matched_runtime = false;

        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION
                    && self.arena.get_interface(stmt_node).is_some_and(|iface| {
                        self.get_identifier_text_idx(iface.name) == assigned_name
                    }) =>
                {
                    matched_type = true;
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    && self.arena.get_type_alias(stmt_node).is_some_and(|alias| {
                        self.get_identifier_text_idx(alias.name) == assigned_name
                    }) =>
                {
                    matched_type = true;
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION
                    && self.arena.get_class(stmt_node).is_some_and(|class_decl| {
                        self.get_identifier_text_idx(class_decl.name) == assigned_name
                    })
                    && !self.arena.get_class(stmt_node).is_some_and(|class_decl| {
                        self.arena
                            .has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword)
                    }) =>
                {
                    matched_runtime = true;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    && self.arena.get_function(stmt_node).is_some_and(|func| {
                        self.get_identifier_text_idx(func.name) == assigned_name
                    })
                    && self.arena.get_function(stmt_node).is_some_and(|func| {
                        func.body.is_some()
                            && !self
                                .arena
                                .has_modifier(&func.modifiers, SyntaxKind::DeclareKeyword)
                    }) =>
                {
                    matched_runtime = true;
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    if let Some(enum_decl) = self.arena.get_enum(stmt_node)
                        && self.get_identifier_text_idx(enum_decl.name) == assigned_name
                    {
                        let is_declare = self
                            .arena
                            .has_modifier(&enum_decl.modifiers, SyntaxKind::DeclareKeyword);
                        let is_const = self
                            .arena
                            .has_modifier(&enum_decl.modifiers, SyntaxKind::ConstKeyword);
                        if !is_declare && !is_const {
                            matched_runtime = true;
                        } else if is_const && !self.ctx.options.preserve_const_enums {
                            // `const enum` without `preserveConstEnums` is erased —
                            // treat `export = E` as type-only so the assignment is
                            // elided and the __esModule marker is emitted instead.
                            matched_type = true;
                        }
                    }
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION
                    && self.arena.get_module(stmt_node).is_some_and(|module_decl| {
                        self.get_identifier_text_idx(module_decl.name) == assigned_name
                    }) =>
                {
                    // Namespace `X` matches the export-equals identifier.
                    // Distinguish runtime vs type-only by inspecting the body:
                    // - `namespace X { ...values... }` is runtime.
                    // - `declare namespace X { var a; function b(); }` emits no
                    //   namespace body, but still describes a runtime value.
                    //   `export = X` must therefore lower to `module.exports = X`.
                    //   A declare namespace with only type members remains type-only.
                    // - `namespace X { ...types only... }` and empty namespaces
                    //   are type-only.
                    if let Some(module_decl) = self.arena.get_module(stmt_node) {
                        if self.is_instantiated_module(module_decl.body) {
                            matched_runtime = true;
                        } else {
                            matched_type = true;
                        }
                    }
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    && self
                        .collect_variable_names_from_node(stmt_node)
                        .iter()
                        .any(|n| n == &assigned_name) =>
                {
                    // `var x` declares a runtime binding regardless of the
                    // `declare` modifier. `declare var server` in a module
                    // file says "the name `server` is a runtime value" — so
                    // `export = server` must lower to
                    // `module.exports = server`. Previously the `!is_declare`
                    // gate elided the assignment for ambient bindings.
                    matched_runtime = true;
                }
                k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                    && self
                        .arena
                        .get_import_decl(stmt_node)
                        .is_some_and(|import_decl| {
                            self.get_identifier_text_idx(import_decl.import_clause) == assigned_name
                                && self.import_decl_has_runtime_value(import_decl)
                        }) =>
                {
                    matched_runtime = true;
                }
                k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_decl) = self.arena.get_export_decl(stmt_node)
                        && let Some(inner) = self.arena.get(export_decl.export_clause)
                    {
                        if inner.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                            && let Some(import_decl) = self.arena.get_import_decl(inner)
                        {
                            let alias_name =
                                self.get_identifier_text_idx(import_decl.import_clause);
                            let target_root =
                                self.get_module_root_name(import_decl.module_specifier);
                            if alias_name == assigned_name
                                || target_root.as_deref() == Some(assigned_name.as_str())
                            {
                                matched_runtime = true;
                            }
                        }
                        let matches_exported_type = (inner.kind
                            == syntax_kind_ext::INTERFACE_DECLARATION
                            && self.arena.get_interface(inner).is_some_and(|iface| {
                                self.get_identifier_text_idx(iface.name) == assigned_name
                            }))
                            || (inner.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                && self.arena.get_type_alias(inner).is_some_and(|alias| {
                                    self.get_identifier_text_idx(alias.name) == assigned_name
                                }));
                        if matches_exported_type {
                            matched_type = true;
                        }
                    }
                }
                _ => {}
            }
        }

        matched_type && !matched_runtime
    }

    /// Check whether a statement node carries an `export` modifier.
    /// Covers all declaration kinds that can be exported: variable, function,
    /// class, enum, module/namespace, interface, and type alias.
    pub(in crate::emitter) fn statement_has_export_modifier(&self, node: &Node) -> bool {
        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.arena.get_variable(node).is_some_and(|v| {
                    self.arena
                        .has_modifier(&v.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.arena.get_function(node).is_some_and(|f| {
                    self.arena
                        .has_modifier(&f.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.arena.get_class(node).is_some_and(|c| {
                    self.arena
                        .has_modifier(&c.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.arena.get_enum(node).is_some_and(|e| {
                    self.arena
                        .has_modifier(&e.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.arena.get_module(node).is_some_and(|m| {
                    self.arena
                        .has_modifier(&m.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.arena.get_interface(node).is_some_and(|i| {
                    self.arena
                        .has_modifier(&i.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.arena.get_type_alias(node).is_some_and(|t| {
                    self.arena
                        .has_modifier(&t.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.arena.get_import_decl(node).is_some_and(|i| {
                    self.arena
                        .has_modifier(&i.modifiers, SyntaxKind::ExportKeyword)
                })
            }
            _ => false,
        }
    }

    /// Check if a file is a module (has any import/export syntax).
    /// TypeScript considers a file a module if it has ANY import/export syntax,
    /// including type-only imports/exports, declared exports, and exported
    /// interfaces/type aliases.
    pub(in crate::emitter) fn file_is_module(&self, statements: &NodeList) -> bool {
        // moduleDetection=force: treat all non-declaration files as modules
        if self.ctx.options.module_detection_force {
            return true;
        }
        if self.jsx_automatic_runtime_makes_module() {
            return true;
        }
        // Node16/NodeNext resolved to ESM: file is definitively a module based on
        // file extension (.mts) or package.json "type":"module", regardless of content
        if self.ctx.options.resolved_node_module_to_esm {
            return true;
        }
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                    {
                        return true;
                    }
                    // import equals: `import x = require(...)` makes this a module
                    // (even with non-string specifier), but NOT `import x = M.A`
                    // (namespace alias, not a module indicator)
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        if let Some(import_data) = self.arena.get_import_decl(node) {
                            if self
                                .arena
                                .has_modifier(&import_data.modifiers, SyntaxKind::ExportKeyword)
                            {
                                return true;
                            }
                            if import_data.module_specifier.is_none() {
                                // require(nonStringLiteral) — specifier failed to parse
                                // as string literal, but the `import` keyword still
                                // makes this a module
                                return true;
                            }
                            if let Some(spec_node) = self.arena.get(import_data.module_specifier)
                                && spec_node.kind == SyntaxKind::StringLiteral as u16
                            {
                                return true;
                            }
                        }
                    }
                    _ => {
                        if self.statement_has_export_modifier(node) {
                            return true;
                        }
                    }
                }
            }
        }
        // `import.meta` usage makes the file a module (ESM-only syntax).
        if self.contains_import_meta(statements) {
            return true;
        }
        // AMD/UMD/System lower dynamic import through the module wrapper runtime.
        // A file that only contains `import(expr)` still needs that wrapper so
        // the factory `require`/System context is in scope. CommonJS scripts do
        // not become external modules solely because of dynamic import.
        if matches!(
            self.ctx.options.module,
            ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System
        ) && self.source_has_dynamic_import_call(statements)
        {
            return true;
        }
        false
    }

    fn jsx_automatic_runtime_makes_module(&self) -> bool {
        crate::core::module_facts::jsx_automatic_runtime_makes_module(self.arena, &self.ctx.options)
    }

    pub(in crate::emitter) fn collect_module_dependencies(
        &self,
        statements: &[NodeIndex],
    ) -> Vec<String> {
        let mut deps = Vec::new();
        for &stmt_idx in statements {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    if !self.import_decl_should_schedule_wrapped_dependency(node, import_decl) {
                        continue;
                    }
                    if let Some(text) =
                        emit_utils::module_specifier_text(self.arena, import_decl.module_specifier)
                        && !deps.contains(&text)
                    {
                        deps.push(text);
                    }
                }
                continue;
            }

            if node.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_decl) = self.arena.get_export_decl(node)
            {
                if !self.export_decl_has_runtime_value(export_decl) {
                    continue;
                }
                if let Some(text) =
                    emit_utils::module_specifier_text(self.arena, export_decl.module_specifier)
                    && !deps.contains(&text)
                {
                    deps.push(text);
                }
            }
        }

        if self.jsx_automatic_runtime_makes_module() {
            let source = self
                .ctx
                .options
                .jsx_import_source
                .as_deref()
                .unwrap_or("react");
            let runtime = if matches!(self.ctx.options.jsx, JsxEmit::ReactJsxDev) {
                format!("{source}/jsx-dev-runtime")
            } else {
                format!("{source}/jsx-runtime")
            };
            if !deps.contains(&runtime) {
                deps.push(runtime);
            }
        }

        deps
    }

    pub(in crate::emitter) fn import_decl_has_runtime_value(
        &self,
        import_decl: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        if import_decl.import_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(import_decl.import_clause) else {
            return true;
        };

        if clause_node.kind != syntax_kind_ext::IMPORT_CLAUSE {
            // For `import X = require("module")`, check if it has an external module.
            // For `import X = Y` (identifier/qualified name), only emit when the
            // target resolves to a runtime value (TypeScript elides type-only aliases).
            if let Some(spec_node) = self.arena.get(import_decl.module_specifier) {
                return match spec_node.kind {
                    k if k == SyntaxKind::StringLiteral as u16 => true,
                    k if k == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE => true,
                    k if k == SyntaxKind::Identifier as u16
                        || k == syntax_kind_ext::QUALIFIED_NAME =>
                    {
                        if self.ctx.options.verbatim_module_syntax {
                            return true;
                        }
                        self.namespace_alias_target_has_runtime_value(
                            import_decl.module_specifier,
                            None,
                        )
                    }
                    _ => false,
                };
            }
            return false;
        }

        let Some(clause) = self.arena.get_import_clause(clause_node) else {
            return true;
        };

        if clause.is_type_only {
            return false;
        }

        if clause.name.is_some() {
            return true;
        }

        if clause.named_bindings.is_none() {
            return false;
        }

        let Some(bindings_node) = self.arena.get(clause.named_bindings) else {
            return false;
        };

        let Some(named) = self.arena.get_named_imports(bindings_node) else {
            return true;
        };

        if named.name.is_some() {
            return true;
        }

        if named.elements.nodes.is_empty() {
            return true;
        }

        for &spec_idx in &named.elements.nodes {
            let Some(spec_node) = self.arena.get(spec_idx) else {
                continue;
            };
            if let Some(spec) = self.arena.get_specifier(spec_node)
                && !spec.is_type_only
            {
                return true;
            }
        }

        false
    }

    pub(in crate::emitter) fn export_decl_has_runtime_value(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        crate::transforms::emit_utils::export_decl_has_runtime_value(
            self.arena,
            export_decl,
            self.ctx.options.preserve_const_enums,
        )
    }

    /// Returns true when `target_idx` is a simple identifier that resolves
    /// at the source-file top level to an `interface` or `type` alias
    /// declaration. Used by the script-mode `import x = T` preservation
    /// rule: tsc emits `var x = T;` (broken at runtime) for these cases
    /// while still eliding alias targets that resolve to non-instantiated
    /// namespaces or qualified-name chains.
    pub(in crate::emitter) fn identifier_target_is_interface_or_type_alias(
        &self,
        target_idx: NodeIndex,
    ) -> bool {
        let Some(target_node) = self.arena.get(target_idx) else {
            return false;
        };
        if !target_node.is_identifier() {
            return false;
        }
        let name = self.get_identifier_text_idx(target_idx);
        if name.is_empty() {
            return false;
        }
        for stmt_idx in self.scope_statements_for_runtime_lookup(None) {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let inner = if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                self.arena
                    .get_export_decl(stmt_node)
                    .and_then(|export| self.arena.get(export.export_clause))
            } else {
                Some(stmt_node)
            };
            let Some(inner) = inner else {
                continue;
            };
            match inner.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    if let Some(decl) = self.arena.get_interface(inner)
                        && self.get_identifier_text_idx(decl.name) == name
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    if let Some(decl) = self.arena.get_type_alias(inner)
                        && self.get_identifier_text_idx(decl.name) == name
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Check if we should emit the __esModule marker.
    /// Returns true if the file contains any ES6 module syntax (import/export),
    /// excluding `export =` which is legacy `CommonJS`.
    /// TypeScript emits __esModule for ANY module syntax, including type-only
    /// imports/exports, declared exports, exported interfaces/type aliases,
    /// and `import.meta` usage (which makes the file a module per spec).
    ///
    /// Mirrors tsc's `shouldEmitUnderscoreUnderscoreESModule`:
    /// - JS files with CJS patterns (module.exports, exports.foo) and no real ESM syntax
    ///   do NOT get __esModule, even when moduleDetection=force.
    /// - Files with `export =` do NOT get __esModule.
    /// - All other module files get __esModule.
    pub(in crate::emitter) fn should_emit_es_module_marker(&self, statements: &NodeList) -> bool {
        // If file has a runtime `export =`, do not emit __esModule.
        // Type-only `export =` aliases (e.g. interface) are filtered out.
        if self.has_export_assignment(statements) {
            return false;
        }

        // Check if the file has real ESM syntax (import/export statements)
        let has_esm_syntax = self.has_esm_module_syntax(statements);

        // tsc's shouldEmitUnderscoreUnderscoreESModule:
        // For JS files (.js/.cjs/.mjs) with CJS patterns (module.exports, exports.foo)
        // and no real ESM import/export syntax, skip __esModule.
        // This matches: `hasJSFileExtension(file) && file.commonJsModuleIndicator &&
        //   (!file.externalModuleIndicator || file.externalModuleIndicator === true)`
        if self.is_current_root_js_source
            && self.has_commonjs_module_indicator(statements)
            && !has_esm_syntax
        {
            return false;
        }

        // If file has real ESM syntax, emit __esModule
        if has_esm_syntax {
            return true;
        }

        // moduleDetection=force: treat all non-declaration files as modules
        if self.ctx.options.module_detection_force {
            return true;
        }

        false
    }

    /// Check if the file has any ESM module syntax (import/export statements,
    /// import.meta, export modifiers).
    fn has_esm_module_syntax(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                        return true;
                    }
                    // import equals: `import x = require(...)` is module syntax
                    // (even with non-string specifier), but NOT `import x = M.A`
                    k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                        if let Some(import_data) = self.arena.get_import_decl(node) {
                            if import_data.module_specifier.is_none() {
                                // require(nonStringLiteral) — still a module
                                return true;
                            }
                            if let Some(spec_node) = self.arena.get(import_data.module_specifier)
                                && spec_node.kind == SyntaxKind::StringLiteral as u16
                            {
                                return true;
                            }
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                        return true;
                    }
                    // Type-only `export =` still marks the file as a module and
                    // TypeScript emits `__esModule` for it.
                    k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                        if self.export_assignment_identifier_is_type_only(node, statements) {
                            return true;
                        }
                    }
                    // Check for export modifier on any declaration type
                    // (including declare and type-only declarations)
                    _ => {
                        if self.statement_has_export_modifier(node) {
                            return true;
                        }
                    }
                }
            }
        }

        // `import.meta` usage makes the file a module (ESM-only syntax).
        if self.contains_import_meta(statements) {
            return true;
        }

        false
    }

    /// Check if the file has CJS module patterns like `module.exports = ...`,
    /// `exports.foo = ...`, or `require("...")`.
    /// This is a lightweight emitter-level check that approximates tsc's
    /// binder-level `commonJsModuleIndicator`.
    fn has_commonjs_module_indicator(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if self.statement_has_cjs_pattern(stmt_idx) {
                return true;
            }
        }
        false
    }

    /// Check if a statement subtree contains CJS module patterns.
    /// Looks for:
    /// - `module.exports = ...` (`BinaryExpression` with `PropertyAccessExpression`)
    /// - `exports.foo = ...` (`BinaryExpression` with `PropertyAccessExpression`)
    /// - `require("...")` calls
    fn statement_has_cjs_pattern(&self, node_idx: NodeIndex) -> bool {
        let mut stack = Vec::from([node_idx]);
        while let Some(idx) = stack.pop() {
            let Some(node) = self.arena.get(idx) else {
                continue;
            };
            if self.expression_is_cjs_pattern(node) {
                return true;
            }
            for child in self.arena.get_children(idx) {
                stack.push(child);
            }
        }
        false
    }

    /// Get the identifier text from a node, if it is an identifier.
    fn identifier_text_of(&self, node: &Node) -> Option<&str> {
        self.arena
            .get_identifier(node)
            .map(|id| self.arena.resolve_identifier_text(id))
    }

    /// Check if an expression is a CJS module pattern.
    fn expression_is_cjs_pattern(&self, node: &Node) -> bool {
        // Binary expression: `module.exports = X` or `exports.foo = X`
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin) = self.arena.get_binary_expr(node)
            && let Some(left) = self.arena.get(bin.left)
        {
            // Check for `module.exports` or `exports.foo`
            if left.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(access) = self.arena.get_access_expr(left)
                && let Some(expr) = self.arena.get(access.expression)
            {
                let expr_text = self.identifier_text_of(expr);
                // `module.exports = ...`
                if expr_text == Some("module")
                    && let Some(name) = self.arena.get(access.name_or_argument)
                    && self.identifier_text_of(name) == Some("exports")
                {
                    return true;
                }
                // `exports.foo = ...`
                if expr_text == Some("exports") {
                    return true;
                }
            }
        }
        // Call expression: `require("...")`
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = self.arena.get_call_expr(node)
            && let Some(callee) = self.arena.get(call.expression)
            && self.identifier_text_of(callee) == Some("require")
        {
            return true;
        }
        false
    }

    /// Check if any statement contains an `import.meta` expression.
    fn contains_import_meta(&self, statements: &NodeList) -> bool {
        crate::core::module_facts::contains_import_meta(self.arena, statements)
    }

    pub(in crate::emitter) fn source_has_dynamic_import_call(&self, statements: &NodeList) -> bool {
        crate::core::module_facts::source_has_dynamic_import_call(self.arena, statements)
    }

    /// Collect every bare `export { local [as exportName] }` (no `from`) whose
    /// exported name binds an import-bound local, keyed by local name in
    /// specifier source order. tsc's CommonJS transform emits these bindings
    /// right after the import's own `require(...)` line, not at the `export { }`
    /// statement's position, so `emit_import_declaration_commonjs` drains this
    /// map at each import site instead of leaving it for the export statement.
    /// Whether a given local actually turns out to be import-bound (vs. a plain
    /// local declaration) is decided later, when the import populates
    /// `commonjs_named_import_substitutions` — this pass only records candidates.
    pub(in crate::emitter) fn collect_bare_reexported_import_locals(
        &self,
        statements: &NodeList,
    ) -> rustc_hash::FxHashMap<String, Vec<String>> {
        let mut result: rustc_hash::FxHashMap<String, Vec<String>> =
            rustc_hash::FxHashMap::default();
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
            if export.is_type_only || export.module_specifier.is_some() {
                continue;
            }
            let Some(clause_node) = self.arena.get(export.export_clause) else {
                continue;
            };
            if clause_node.kind != syntax_kind_ext::NAMED_EXPORTS {
                continue;
            }
            let Some(named_exports) = self.arena.get_named_imports(clause_node) else {
                continue;
            };
            for spec_idx in self.collect_local_export_value_specifiers(&named_exports.elements) {
                let Some(spec_node) = self.arena.get(spec_idx) else {
                    continue;
                };
                let Some(spec) = self.arena.get_specifier(spec_node) else {
                    continue;
                };
                let Some(export_name) = self.get_specifier_name_text(spec.name) else {
                    continue;
                };
                let local_name = if spec.property_name.is_some() {
                    self.get_specifier_name_text(spec.property_name)
                        .unwrap_or_else(|| export_name.clone())
                } else {
                    export_name.clone()
                };
                result.entry(local_name).or_default().push(export_name);
            }
        }
        result
    }
}
