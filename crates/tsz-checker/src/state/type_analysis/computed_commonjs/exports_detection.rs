use crate::query_boundaries::js_exports as js_exports_query;
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Whether the current file gives `exports` / `module.exports` a value
    /// meaning by assigning to them.
    ///
    /// `tsc` reports `/** @type {exports} */` as TS2749 (a value used as a
    /// type) once such an assignment exists, and as TS2304 (cannot find name)
    /// when it does not — the CommonJS globals only become values in a module
    /// that actually exports.
    pub(crate) fn current_file_has_commonjs_export_assignment(&self) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let arena = &self.ctx.arena;
        let names_exports_root = |idx: tsz_parser::parser::NodeIndex| -> bool {
            arena.get_identifier_at(idx).is_some_and(|ident| {
                ident.escaped_text == "exports" || ident.escaped_text == "module"
            })
        };
        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                return false;
            }
            let Some(stmt) = arena.get_expression_statement(stmt_node) else {
                return false;
            };
            let Some(expr_node) = arena.get(stmt.expression) else {
                return false;
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                return false;
            }
            let Some(binary) = arena.get_binary_expr(expr_node) else {
                return false;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                return false;
            }
            // `exports = …`, `module.exports = …`, `exports.X = …`,
            // `module.exports.X = …` — walk the assignment target to its root.
            let mut root = binary.left;
            for _ in 0..4 {
                let Some(node) = arena.get(root) else {
                    return false;
                };
                if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    break;
                }
                let Some(access) = arena.get_access_expr(node) else {
                    return false;
                };
                root = access.expression;
            }
            names_exports_root(root)
        })
    }

    pub(crate) fn current_source_file_has_esm_syntax(&self) -> bool {
        self.source_file_idx_has_esm_syntax(self.ctx.current_file_idx)
    }

    pub(crate) fn source_file_idx_has_esm_syntax(&self, file_idx: usize) -> bool {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        Self::source_file_arena_has_esm_syntax(arena)
    }

    fn source_file_arena_has_esm_syntax(arena: &tsz_parser::parser::NodeArena) -> bool {
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        for &stmt_idx in &source_file.statements.nodes {
            if stmt_idx.is_none() {
                continue;
            }
            let Some(stmt) = arena.get(stmt_idx) else {
                continue;
            };
            match stmt.kind {
                syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::IMPORT_EQUALS_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                _ => {}
            }
        }
        false
    }

    pub(super) fn commonjs_static_member_name_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                arena.get_literal(node).map(|lit| lit.text.clone())
            }
            k if k == SyntaxKind::Identifier as u16 => arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string()),
            _ => None,
        }
    }

    pub(super) fn commonjs_export_rhs_symbol_type(
        &mut self,
        rhs_expr: NodeIndex,
    ) -> Option<TypeId> {
        let rhs_node = self.ctx.arena.get(rhs_expr)?;
        if rhs_node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol_without_tracking(rhs_expr)?;
        let symbol = self.get_symbol_globally(sym_id)?;
        if !symbol.has_any_flags(symbol_flags::CLASS) {
            return None;
        }

        let symbol_type = self.get_type_of_symbol(sym_id);
        (symbol_type != TypeId::ERROR && symbol_type != TypeId::UNKNOWN).then_some(symbol_type)
    }
    pub(crate) fn json_module_type_for_module(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        if !self.ctx.compiler_options.resolve_json_module {
            return None;
        }

        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))?;

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let source_file = target_arena.source_files.first()?;
        if !source_file.file_name.ends_with(".json") {
            return None;
        }

        let source_text = source_file.text.trim();
        if source_text.is_empty() {
            return Some(js_exports_query::commonjs_empty_namespace_type(
                self.ctx.types,
            ));
        }

        let parsed = serde_json::from_str::<JsonValue>(source_text).ok()?;
        Some(js_exports_query::json_module_value_type(
            self.ctx.types,
            &parsed,
        ))
    }

    pub(crate) fn json_module_namespace_type_for_module(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let json_type = self.json_module_type_for_module(module_name, source_file_idx)?;
        if !self.json_namespace_import_uses_default_export(source_file_idx) {
            return Some(js_exports_query::commonjs_json_namespace_type(
                self.ctx.types,
                json_type,
            ));
        }

        Some(js_exports_query::json_esm_namespace_type(
            self.ctx.types,
            json_type,
        ))
    }

    fn json_namespace_import_uses_default_export(&self, source_file_idx: Option<usize>) -> bool {
        if !self.ctx.compiler_options.module.is_node_module() {
            return false;
        }

        let file_idx = source_file_idx.unwrap_or(self.ctx.current_file_idx);
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };
        let file_name = source_file.file_name.as_str();
        if file_name.ends_with(".mts") || file_name.ends_with(".mjs") {
            return true;
        }
        if file_name.ends_with(".cts") || file_name.ends_with(".cjs") {
            return false;
        }

        self.lookup_file_is_esm(file_name).unwrap_or(false)
    }

    fn is_undefined_like_commonjs_rhs(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "undefined");
        }

        if node.kind != syntax_kind_ext::VOID_EXPRESSION
            && node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        {
            return false;
        }

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return false;
        };
        if unary.operator != SyntaxKind::VoidKeyword as u16 {
            return false;
        }
        let Some(expr) = self.ctx.arena.get(unary.operand) else {
            return false;
        };

        matches!(expr.kind, k if k == SyntaxKind::NumericLiteral as u16)
            && self
                .ctx
                .arena
                .get_literal(expr)
                .is_some_and(|lit| lit.text == "0")
    }

    /// The current file's CommonJS `module.exports`/`exports` namespace type,
    /// tagged with its `typeof import("...")` display name.
    ///
    /// `tsc` (7.0.2) always strips the source file's extension from this
    /// display name, whether the diagnostic is reported in the current file
    /// (a `module.exports[...] = ...` write) or in a file that `require`s it
    /// — the tsc-cache oracle has zero `typeof import("....js")` fingerprints
    /// anywhere in the conformance corpus.
    pub(crate) fn current_file_commonjs_namespace_type(&mut self) -> TypeId {
        if self.current_source_file_has_esm_syntax() {
            let empty_namespace = js_exports_query::commonjs_empty_namespace_type(self.ctx.types);
            self.ctx
                .namespace_module_names
                .insert(empty_namespace, self.current_file_commonjs_module_name());
            return empty_namespace;
        }

        // Use the cached JsExportSurface for typed exports instead of
        // re-scanning the AST with augment_namespace_props_with_commonjs_exports_for_file.
        let current_file_idx = self.ctx.current_file_idx;
        // While this file's own export surface is still being computed,
        // `resolve_js_export_surface` hands back a re-entrancy placeholder
        // rather than the file's surface. That placeholder reports no
        // `module.exports = X`, which makes the merge test below vacuously
        // true and lets the deep scan synthesize a namespace containing every
        // `module.exports.<name>` in the file. A property write typed inside
        // that window then resolves against a namespace that *has* the member,
        // so its missing-property diagnostic is lost — while a sibling write
        // typed after the window resolves against the real export type and
        // reports correctly. Suppress the merge inside the window so both
        // resolve against the same thing.
        let reentrant_state = self
            .ctx
            .js_export_surface_resolution_set
            .get(&current_file_idx)
            .copied();
        let surface_is_reentrant_placeholder = reentrant_state.is_some();
        // A read typed inside the resolution window of a file with a bare
        // `module.exports = X` resolves to `X` directly (published by step 1
        // of the surface computation): tsc types every same-file
        // `module.exports` reference as the export= target, including reads
        // inside a sibling `module.exports.p = function () { ... }` RHS whose
        // inference is exactly what re-entered here. The placeholder namespace
        // below would type — and node-cache — the read as `{}`, producing a
        // spurious TS2349 on `module.exports(...)`.
        if let Some(Some(direct_export_type)) = reentrant_state {
            return direct_export_type;
        }
        let surface = self.resolve_js_export_surface(current_file_idx);
        let can_merge_named_exports = !surface_is_reentrant_placeholder
            && js_exports_query::commonjs_export_surface_can_merge_named_exports(
                self.ctx.types,
                &surface,
            );

        // Deep-scan the AST for export names that may be nested (in if-blocks, etc.)
        // and not captured by the surface's top-level + IIFE scan.
        let mut export_names = BTreeSet::new();
        if can_merge_named_exports {
            for source_file in &self.ctx.arena.source_files {
                for &stmt_idx in &source_file.statements.nodes {
                    self.collect_current_file_commonjs_export_names(stmt_idx, &mut export_names);
                }
            }
        }

        let display_name = self.current_file_commonjs_module_name();
        js_exports_query::current_file_commonjs_namespace_type(
            self,
            surface,
            export_names,
            display_name,
        )
    }

    fn collect_current_file_commonjs_export_names(
        &self,
        root: NodeIndex,
        names: &mut BTreeSet<String>,
    ) {
        let mut stack = vec![root];

        while let Some(idx) = stack.pop() {
            if idx.is_none() {
                continue;
            }
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };

            if self.is_commonjs_scope_boundary(node.kind) {
                continue;
            }

            if node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(node)
                && binary.operator_token == SyntaxKind::EqualsToken as u16
                && !self.is_undefined_like_commonjs_rhs(binary.right)
                && let Some(name) =
                    self.current_file_commonjs_export_target_member_name(binary.left)
            {
                names.insert(name);
            }

            if node.kind == syntax_kind_ext::CALL_EXPRESSION {
                if let Some(name) = self.current_file_commonjs_define_property_export_name(idx) {
                    names.insert(name);
                }
                // If this call is an IIFE, scan its body for export assignments.
                // IIFEs don't create a new module scope — `exports` still refers to
                // the module's exports object inside `(function() { ... })()`.
                if let Some(iife_stmts) = Self::get_iife_body_statements(self.ctx.arena, idx) {
                    for &stmt_idx in iife_stmts {
                        stack.push(stmt_idx);
                    }
                }
            }

            for child_idx in self.ctx.arena.get_children(idx) {
                stack.push(child_idx);
            }
        }
    }

    const fn is_commonjs_scope_boundary(&self, kind: u16) -> bool {
        matches!(
            kind,
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR
                || k == syntax_kind_ext::CONSTRUCTOR
                || k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
                || k == syntax_kind_ext::MODULE_DECLARATION
        )
    }

    fn current_file_commonjs_export_target_member_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base(access.expression) {
                    return None;
                }
                self.ctx
                    .arena
                    .get_identifier_at(access.name_or_argument)
                    .map(|ident| ident.escaped_text.to_string())
            }
            syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                let access = self.ctx.arena.get_access_expr(node)?;
                if !self.is_current_file_commonjs_export_base(access.expression) {
                    return None;
                }
                self.current_file_commonjs_static_member_name(access.name_or_argument)
            }
            _ => None,
        }
    }

    fn commonjs_define_property_target_and_name(
        &self,
        idx: NodeIndex,
    ) -> Option<(NodeIndex, String)> {
        let node = self.ctx.arena.get(idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        let callee_node = self.ctx.arena.get(call.expression)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let callee = self.ctx.arena.get_access_expr(callee_node)?;
        let is_object_define_property = self
            .identifier_resolves_to_unshadowed_global(callee.expression, "Object")
            && self
                .ctx
                .arena
                .get_identifier_at(callee.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "defineProperty");
        if !is_object_define_property {
            return None;
        }

        let args = call.arguments.as_ref()?;
        if args.nodes.len() < 2 {
            return None;
        }

        Some((
            args.nodes[0],
            self.constant_define_property_name_in_file(
                self.ctx.current_file_idx,
                self.ctx.arena,
                args.nodes[1],
            )?,
        ))
    }

    fn current_file_commonjs_define_property_export_name(&self, idx: NodeIndex) -> Option<String> {
        let (target, name) = self.commonjs_define_property_target_and_name(idx)?;
        self.is_current_file_commonjs_export_base(target)
            .then_some(name)
    }

    pub(crate) fn current_file_commonjs_static_member_name(
        &self,
        idx: NodeIndex,
    ) -> Option<String> {
        Self::static_member_name_in_arena(self.ctx.arena, idx)
    }

    fn static_member_name_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> Option<String> {
        let node = arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                arena.get_literal(node).map(|lit| lit.text.clone())
            }
            _ => None,
        }
    }

    /// Check if a node in an arena is literally `exports` (unbound) or `module.exports`.
    /// Does not follow variable aliases. Works on any arena.
    fn is_literal_exports_or_module_exports_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports");
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            if node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
                return false;
            }
            let Some(access) = arena.get_access_expr(node) else {
                return false;
            };
            return arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "module")
                && Self::commonjs_static_member_name_in_arena(arena, access.name_or_argument)
                    .is_some_and(|name| name == "exports");
        }

        let Some(access) = arena.get_access_expr(node) else {
            return false;
        };
        arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "module")
            && arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    /// Check if a node is `exports`, `module.exports`, or a chain assignment
    /// (e.g., `exports = module.exports` or `module.exports = exports = {}`).
    /// Returns true if any part of the assignment chain is exports/module.exports.
    fn is_exports_or_module_exports_or_chain_in_arena(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };

        if Self::is_literal_exports_or_module_exports_in_arena(arena, idx) {
            return true;
        }

        // Chain assignment: `exports = module.exports` or `module.exports = exports = {}`
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            return Self::is_exports_or_module_exports_or_chain_in_arena(arena, binary.left)
                || Self::is_exports_or_module_exports_or_chain_in_arena(arena, binary.right);
        }

        false
    }

    /// Collect names of variables that alias `exports` or `module.exports`.
    /// Scans top-level variable declarations looking for patterns like:
    /// - `var x = exports`
    /// - `var x = module.exports`
    /// - `var x = exports = module.exports`
    /// - `var x = module.exports = exports = {}`
    pub(super) fn collect_commonjs_export_aliases_in_arena(
        arena: &tsz_parser::parser::NodeArena,
    ) -> FxHashSet<String> {
        let mut aliases = FxHashSet::default();
        let Some(source_file) = arena.source_files.first() else {
            return aliases;
        };

        for &stmt_idx in &source_file.statements.nodes {
            Self::collect_export_aliases_from_statement(arena, stmt_idx, &mut aliases);
        }

        aliases
    }

    /// Recursively scan a statement (and its children) for variable declarations
    /// that alias exports/module.exports.
    fn collect_export_aliases_from_statement(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
        aliases: &mut FxHashSet<String>,
    ) {
        let Some(node) = arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            // VariableStatement → declarations contains VariableDeclarationList nodes
            if let Some(var_stmt) = arena.get_variable(node) {
                for &decl_list_idx in &var_stmt.declarations.nodes {
                    let Some(decl_list_node) = arena.get(decl_list_idx) else {
                        continue;
                    };
                    // VariableDeclarationList → declarations contains VariableDeclaration nodes
                    if let Some(decl_list) = arena.get_variable(decl_list_node) {
                        for &decl_idx in &decl_list.declarations.nodes {
                            Self::check_var_decl_for_export_alias(arena, decl_idx, aliases);
                        }
                    } else {
                        // Fallback: try as direct VariableDeclaration
                        Self::check_var_decl_for_export_alias(arena, decl_list_idx, aliases);
                    }
                }
            }
        }

        // Also scan children for nested variable declarations (but not function/class boundaries)
        for child_idx in arena.get_children(idx) {
            let Some(child_node) = arena.get(child_idx) else {
                continue;
            };
            // Don't cross function/class boundaries
            if child_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || child_node.is_function_expression_or_arrow()
                || child_node.is_class_like()
            {
                continue;
            }
            Self::collect_export_aliases_from_statement(arena, child_idx, aliases);
        }
    }

    fn check_var_decl_for_export_alias(
        arena: &tsz_parser::parser::NodeArena,
        decl_idx: NodeIndex,
        aliases: &mut FxHashSet<String>,
    ) {
        let Some(decl_node) = arena.get(decl_idx) else {
            return;
        };
        if let Some(var_decl) = arena.get_variable_declaration(decl_node)
            && var_decl.initializer.is_some()
            && Self::is_exports_or_module_exports_or_chain_in_arena(arena, var_decl.initializer)
            && let Some(name_ident) = arena.get_identifier_at(var_decl.name)
        {
            aliases.insert(name_ident.escaped_text.to_string());
        }
    }

    pub(crate) fn is_current_file_commonjs_export_base(&self, idx: NodeIndex) -> bool {
        if self.current_source_file_has_esm_syntax() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            // Direct `exports` identifier (not user-declared)
            if self.is_unshadowed_commonjs_exports_identifier(idx) {
                return true;
            }

            // Check if the identifier is a variable alias for exports/module.exports
            if let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                && (symbol.flags & tsz_binder::symbol_flags::VARIABLE) != 0
            {
                let decl_idx = symbol.value_declaration;
                if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                    && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
                    && var_decl.initializer.is_some()
                    && Self::is_exports_or_module_exports_or_chain_in_arena(
                        self.ctx.arena,
                        var_decl.initializer,
                    )
                {
                    return true;
                }
            }
            return false;
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.is_unshadowed_commonjs_module_identifier(access.expression)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn current_file_commonjs_module_name(&self) -> String {
        if let Some(specifier) = self.current_file_explicit_js_module_specifier() {
            let basename = specifier
                .rsplit(|ch| ['/', '\\'].contains(&ch))
                .next()
                .unwrap_or(specifier);
            return tsz_common::file_extensions::strip_known_extension(basename).to_string();
        }

        let file_name = self
            .ctx
            .arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
            .unwrap_or(self.ctx.file_name.as_str());
        let stripped = tsz_common::file_extensions::strip_known_extension(file_name);
        stripped
            .rsplit(|ch| ['/', '\\'].contains(&ch))
            .next()
            .unwrap_or(stripped)
            .to_string()
    }

    fn current_file_explicit_js_module_specifier(&self) -> Option<&str> {
        let paths = self.ctx.resolved_module_paths.as_ref()?;
        paths.iter().find_map(|((_, specifier), &target_idx)| {
            let ends_with_js_ext = tsz_common::file_extensions::JS_FAMILY_EXTENSIONS
                .iter()
                .any(|ext| specifier.ends_with(ext));
            (target_idx == self.ctx.current_file_idx && ends_with_js_ext)
                .then_some(specifier.as_str())
        })
    }
}
