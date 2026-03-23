use crate::query_boundaries::common::{callable_shape_for_type, function_shape_for_type};
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
    fn json_value_type(&mut self, value: &JsonValue) -> TypeId {
        let factory = self.ctx.types.factory();
        match value {
            JsonValue::Null => TypeId::NULL,
            JsonValue::Bool(_) => TypeId::BOOLEAN,
            JsonValue::Number(_) => TypeId::NUMBER,
            JsonValue::String(_) => TypeId::STRING,
            JsonValue::Array(elements) => {
                let element_types: Vec<TypeId> = elements
                    .iter()
                    .map(|element| self.json_value_type(element))
                    .collect();
                let element_type = match element_types.as_slice() {
                    [] => TypeId::NEVER,
                    [single] => *single,
                    _ => factory.union(element_types),
                };
                factory.array(element_type)
            }
            JsonValue::Object(entries) => {
                let mut props = Vec::with_capacity(entries.len());
                for (declaration_order, (name, entry_value)) in entries.iter().enumerate() {
                    let prop_type = self.json_value_type(entry_value);
                    props.push(PropertyInfo {
                        name: self.ctx.types.intern_string(name),
                        type_id: prop_type,
                        write_type: prop_type,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: declaration_order as u32,
                    });
                }
                factory.object(props)
            }
        }
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
            return Some(self.ctx.types.factory().object(Vec::new()));
        }

        let parsed = serde_json::from_str::<JsonValue>(source_text).ok()?;
        Some(self.json_value_type(&parsed))
    }

    pub(crate) fn json_module_namespace_type_for_module(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let json_type = self.json_module_type_for_module(module_name, source_file_idx)?;
        let namespace_type = self.ctx.types.factory().object(vec![PropertyInfo {
            name: self.ctx.types.intern_string("default"),
            type_id: json_type,
            write_type: json_type,
            optional: false,
            readonly: false,
            is_method: false,
            is_class_prototype: false,
            visibility: Visibility::Public,
            parent_id: None,
            declaration_order: 0,
        }]);
        self.ctx.namespace_module_names.insert(
            namespace_type,
            self.imported_namespace_display_module_name(module_name),
        );
        Some(namespace_type)
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

    pub(crate) fn current_file_commonjs_namespace_type(&mut self) -> TypeId {
        self.current_file_commonjs_namespace_type_with_display_extension(false)
    }

    pub(crate) fn current_file_commonjs_module_exports_namespace_type(&mut self) -> TypeId {
        self.current_file_commonjs_namespace_type_with_display_extension(true)
    }

    fn current_file_commonjs_namespace_type_with_display_extension(
        &mut self,
        preserve_js_extension: bool,
    ) -> TypeId {
        // Use the cached JsExportSurface for typed exports instead of
        // re-scanning the AST with augment_namespace_props_with_commonjs_exports_for_file.
        let current_file_idx = self.ctx.current_file_idx;
        let surface = self.resolve_js_export_surface(current_file_idx);

        // Deep-scan the AST for export names that may be nested (in if-blocks, etc.)
        // and not captured by the surface's top-level + IIFE scan.
        let mut export_names = BTreeSet::new();
        for source_file in &self.ctx.arena.source_files {
            for &stmt_idx in &source_file.statements.nodes {
                self.collect_current_file_commonjs_export_names(stmt_idx, &mut export_names);
            }
        }

        // Start with the surface's typed named exports
        let mut props = surface.named_exports;

        // Add ANY-typed entries for any deep-scan names not already in the surface
        for name in &export_names {
            let name_atom = self.ctx.types.intern_string(name);
            if props.iter().any(|p| p.name == name_atom) {
                continue;
            }
            props.push(PropertyInfo {
                name: name_atom,
                type_id: TypeId::ANY,
                write_type: TypeId::ANY,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: props.len() as u32,
            });
        }

        let namespace_type = self.ctx.types.factory().object(props);
        self.ctx.namespace_module_names.insert(
            namespace_type,
            self.current_file_commonjs_module_name(preserve_js_extension),
        );
        namespace_type
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
                    .map(|ident| ident.escaped_text.clone())
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

    fn current_file_commonjs_define_property_export_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        let call = self.ctx.arena.get_call_expr(node)?;
        let callee_node = self.ctx.arena.get(call.expression)?;
        if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let callee = self.ctx.arena.get_access_expr(callee_node)?;
        let is_object_define_property = self
            .ctx
            .arena
            .get_identifier_at(callee.expression)
            .is_some_and(|ident| ident.escaped_text == "Object")
            && self
                .ctx
                .arena
                .get_identifier_at(callee.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "defineProperty");
        if !is_object_define_property {
            return None;
        }

        let args = call.arguments.as_ref()?;
        if args.nodes.len() < 2 || !self.is_current_file_commonjs_export_base(args.nodes[0]) {
            return None;
        }

        self.current_file_commonjs_static_member_name(args.nodes[1])
    }

    fn current_file_commonjs_static_member_name(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx.arena.get_literal(node).map(|lit| lit.text.clone())
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
            return false;
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
    fn collect_commonjs_export_aliases_in_arena(
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
                || child_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || child_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || child_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || child_node.kind == syntax_kind_ext::CLASS_EXPRESSION
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
            aliases.insert(name_ident.escaped_text.clone());
        }
    }

    fn is_current_file_commonjs_export_base(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.ctx.arena.get_identifier(node) {
                // Direct `exports` identifier (not user-declared)
                if ident.escaped_text == "exports"
                    && self
                        .resolve_identifier_symbol_without_tracking(idx)
                        .is_none()
                {
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
            }
            return false;
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.ctx
            .arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| {
                ident.escaped_text == "module"
                    && self
                        .resolve_identifier_symbol_without_tracking(access.expression)
                        .is_none()
            })
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn current_file_commonjs_module_name(&self, preserve_js_extension: bool) -> String {
        let file_name = self
            .ctx
            .arena
            .source_files
            .first()
            .map(|sf| sf.file_name.as_str())
            .unwrap_or(self.ctx.file_name.as_str());
        let stripped = if preserve_js_extension {
            Self::strip_typescript_module_extension(file_name)
        } else {
            Self::strip_known_module_extension(file_name)
        };
        stripped
            .rsplit(|ch| ['/', '\\'].contains(&ch))
            .next()
            .unwrap_or(stripped)
            .to_string()
    }

    fn strip_known_module_extension(path: &str) -> &str {
        for ext in &[
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts", ".js", ".jsx",
            ".mjs", ".cjs",
        ] {
            if let Some(stripped) = path.strip_suffix(ext) {
                return stripped;
            }
        }
        path
    }

    fn strip_typescript_module_extension(path: &str) -> &str {
        for ext in &[
            ".d.ts", ".d.tsx", ".d.mts", ".d.cts", ".ts", ".tsx", ".mts", ".cts",
        ] {
            if let Some(stripped) = path.strip_suffix(ext) {
                return stripped;
            }
        }
        path
    }

    fn collect_direct_commonjs_assignment_exports(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        pending_props: &mut FxHashMap<String, NodeIndex>,
        ordered_names: &mut Vec<String>,
        export_aliases: &FxHashSet<String>,
    ) {
        let Some(expr_node) = arena.get(expr_idx) else {
            return;
        };
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return;
        }
        let Some(binary) = arena.get_binary_expr(expr_node) else {
            return;
        };
        if binary.operator_token != tsz_scanner::SyntaxKind::EqualsToken as u16 {
            return;
        }

        let Some(left_node) = arena.get(binary.left) else {
            return;
        };
        if left_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(left_access) = arena.get_access_expr(left_node)
        {
            let direct_exports_name = arena
                .get_identifier_at(left_access.expression)
                .and_then(|ident| {
                    (ident.escaped_text == "exports").then(|| {
                        arena
                            .get_identifier_at(left_access.name_or_argument)
                            .map(|name| name.escaped_text.to_string())
                    })
                })
                .flatten();
            let module_exports_name = arena.get(left_access.expression).and_then(|target_node| {
                if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                    return None;
                }
                let target_access = arena.get_access_expr(target_node)?;
                let is_module_exports = arena
                    .get_identifier_at(target_access.expression)
                    .is_some_and(|ident| ident.escaped_text == "module")
                    && arena
                        .get_identifier_at(target_access.name_or_argument)
                        .is_some_and(|ident| ident.escaped_text == "exports");
                is_module_exports.then(|| {
                    arena
                        .get_identifier_at(left_access.name_or_argument)
                        .map(|name| name.escaped_text.to_string())
                })?
            });

            // Also check if the expression is a known alias for exports/module.exports
            let alias_exports_name =
                if direct_exports_name.is_none() && module_exports_name.is_none() {
                    arena
                        .get_identifier_at(left_access.expression)
                        .and_then(|ident| {
                            export_aliases
                                .contains(ident.escaped_text.as_str())
                                .then(|| {
                                    arena
                                        .get_identifier_at(left_access.name_or_argument)
                                        .map(|name| name.escaped_text.to_string())
                                })
                        })
                        .flatten()
                } else {
                    None
                };

            if let Some(name_text) = direct_exports_name
                .or(module_exports_name)
                .or(alias_exports_name)
            {
                if !pending_props.contains_key(&name_text) {
                    ordered_names.push(name_text.clone());
                }
                pending_props.insert(name_text, binary.right);
            }
        }

        self.collect_direct_commonjs_assignment_exports(
            arena,
            binary.right,
            pending_props,
            ordered_names,
            export_aliases,
        );
    }

    pub(crate) fn infer_commonjs_export_rhs_type(
        &mut self,
        target_file_idx: usize,
        rhs_expr: NodeIndex,
    ) -> TypeId {
        if target_file_idx == self.ctx.current_file_idx {
            if let Some(literal_type) = self.literal_type_from_initializer(rhs_expr) {
                return literal_type;
            }
            return self.get_type_of_node(rhs_expr);
        }

        let Some(all_arenas) = self.ctx.all_arenas.clone() else {
            return TypeId::ANY;
        };
        let Some(all_binders) = self.ctx.all_binders.clone() else {
            return TypeId::ANY;
        };
        let Some(arena) = all_arenas.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(binder) = all_binders.get(target_file_idx) else {
            return TypeId::ANY;
        };
        let Some(source_file) = arena.source_files.first() else {
            return TypeId::ANY;
        };

        let mut checker = Box::new(CheckerState::with_parent_cache(
            arena.as_ref(),
            binder.as_ref(),
            self.ctx.types,
            source_file.file_name.clone(),
            self.ctx.compiler_options.clone(),
            self,
        ));
        checker.ctx.lib_contexts = self.ctx.lib_contexts.clone();
        checker.ctx.copy_cross_file_state_from(&self.ctx);
        checker.ctx.current_file_idx = target_file_idx;
        self.ctx.copy_symbol_file_targets_to(&mut checker.ctx);

        let mut ty = checker
            .literal_type_from_initializer(rhs_expr)
            .unwrap_or_else(|| checker.get_type_of_node(rhs_expr));
        ty = checker.upgrade_commonjs_export_constructor_type(rhs_expr, ty);
        ty = tsz_solver::relations::freshness::widen_freshness(checker.ctx.types, ty);
        self.ctx.merge_symbol_file_targets_from(&checker.ctx);
        ty
    }

    fn upgrade_commonjs_export_constructor_type(
        &mut self,
        rhs_expr: NodeIndex,
        rhs_type: TypeId,
    ) -> TypeId {
        let Some(instance_type) =
            self.synthesize_js_constructor_instance_type(rhs_expr, rhs_type, &[])
        else {
            return rhs_type;
        };

        if let Some(func) = function_shape_for_type(self.ctx.types, rhs_type) {
            if func.is_constructor {
                return rhs_type;
            }

            let call_sig = tsz_solver::CallSignature {
                type_params: func.type_params.clone(),
                params: func.params.clone(),
                this_type: func.this_type,
                return_type: func.return_type,
                type_predicate: func.type_predicate.clone(),
                is_method: func.is_method,
            };
            let construct_sig = tsz_solver::CallSignature {
                return_type: instance_type,
                ..call_sig.clone()
            };
            let symbol = self.resolve_identifier_symbol_without_tracking(rhs_expr);
            return self
                .ctx
                .types
                .factory()
                .callable(tsz_solver::CallableShape {
                    call_signatures: vec![call_sig],
                    construct_signatures: vec![construct_sig],
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol,
                    is_abstract: false,
                });
        }

        let Some(shape) = callable_shape_for_type(self.ctx.types, rhs_type) else {
            return rhs_type;
        };
        if !shape.construct_signatures.is_empty() || shape.call_signatures.is_empty() {
            return rhs_type;
        }

        let mut new_shape = shape.as_ref().clone();
        let mut construct_sig = new_shape.call_signatures[0].clone();
        construct_sig.return_type = instance_type;
        new_shape.construct_signatures.push(construct_sig);
        if new_shape.symbol.is_none() {
            new_shape.symbol = self.resolve_identifier_symbol_without_tracking(rhs_expr);
        }
        self.ctx.types.factory().callable(new_shape)
    }

    pub(crate) fn direct_commonjs_module_export_assignment_rhs(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = arena.skip_parenthesized(expr_idx);
        let expr_node = arena.get(expr_idx)?;
        if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = arena.get_binary_expr(expr_node)?;
        if binary.operator_token != SyntaxKind::EqualsToken as u16 {
            return None;
        }
        let left_idx = arena.skip_parenthesized(binary.left);
        let left_node = arena.get(left_idx)?;

        if left_node.kind == SyntaxKind::Identifier as u16
            && arena
                .get_identifier(left_node)
                .is_some_and(|ident| ident.escaped_text == "exports")
        {
            return Some(binary.right);
        }

        if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let left_access = arena.get_access_expr(left_node)?;
        let target_idx = arena.skip_parenthesized(left_access.expression);
        arena
            .get_identifier_at(target_idx)
            .is_some_and(|ident| ident.escaped_text == "module")
            .then_some(())
            .and_then(|_| {
                arena
                    .get_identifier_at(left_access.name_or_argument)
                    .filter(|ident| ident.escaped_text == "exports")
            })?;
        Some(binary.right)
    }

    /// Extract the body statements of an IIFE (Immediately Invoked Function Expression).
    /// Recognizes patterns like `(function() { ... })()` and `(function() { ... }.call(this))`.
    fn get_iife_body_statements(
        arena: &tsz_parser::parser::NodeArena,
        call_idx: NodeIndex,
    ) -> Option<&[NodeIndex]> {
        let call_node = arena.get(call_idx)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = arena.get_call_expr(call_node)?;
        // Unwrap parentheses: `(function() { ... })()`
        let callee_idx = arena.skip_parenthesized(call.expression);
        let callee_node = arena.get(callee_idx)?;
        if callee_node.kind != syntax_kind_ext::FUNCTION_EXPRESSION {
            return None;
        }
        let func = arena.get_function(callee_node)?;
        let body_node = arena.get(func.body)?;
        let block = arena.get_block(body_node)?;
        Some(&block.statements.nodes)
    }

    fn augment_namespace_props_with_direct_assignment_exports_for_file(
        &mut self,
        target_file_idx: usize,
        props: &mut Vec<tsz_solver::PropertyInfo>,
    ) {
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };
        let export_aliases = Self::collect_commonjs_export_aliases_in_arena(target_arena);
        let mut pending_props: FxHashMap<String, NodeIndex> = FxHashMap::default();
        let mut ordered_names: Vec<String> = Vec::new();

        // Collect statements to scan: top-level statements + IIFE body statements.
        // In CommonJS files, `exports.X = value` inside IIFEs like `(function() { ... })()`
        // are valid export declarations (tsc recognizes them regardless of scope).
        let mut all_stmts: Vec<NodeIndex> = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            all_stmts.push(stmt_idx);
            // Check if this statement is an IIFE and extract its body statements
            if let Some(stmt_node) = target_arena.get(stmt_idx)
                && stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                && let Some(stmt) = target_arena.get_expression_statement(stmt_node)
                && let Some(iife_stmts) =
                    Self::get_iife_body_statements(target_arena, stmt.expression)
            {
                all_stmts.extend_from_slice(iife_stmts);
            }
        }

        for stmt_idx in &all_stmts {
            let Some(stmt_node) = target_arena.get(*stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            self.collect_direct_commonjs_assignment_exports(
                target_arena,
                stmt.expression,
                &mut pending_props,
                &mut ordered_names,
                &export_aliases,
            );
        }

        for name_text in ordered_names {
            let name_atom = self.ctx.types.intern_string(&name_text);
            let Some(rhs_expr) = pending_props.get(&name_text).copied() else {
                continue;
            };
            let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr);
            if rhs_type == TypeId::UNDEFINED {
                continue;
            }
            if let Some(existing) = props.iter_mut().find(|prop| prop.name == name_atom) {
                existing.type_id = rhs_type;
                existing.write_type = rhs_type;
                continue;
            }
            props.push(tsz_solver::PropertyInfo {
                name: name_atom,
                type_id: rhs_type,
                write_type: rhs_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: props.len() as u32,
            });
        }
    }

    pub(super) fn resolve_define_property_descriptor_object_literal(
        &self,
        target_file_idx: usize,
        arena: &tsz_parser::parser::NodeArena,
        descriptor_expr: NodeIndex,
    ) -> Option<tsz_parser::parser::node::LiteralExprData> {
        let descriptor_node = arena.get(descriptor_expr)?;
        if descriptor_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return arena.get_literal_expr(descriptor_node).cloned();
        }

        let ident = arena.get_identifier(descriptor_node)?;
        let binder = self.ctx.get_binder_for_file(target_file_idx)?;
        let sym_id = binder.file_locals.get(&ident.escaped_text)?;
        let symbol = binder.get_symbol(sym_id)?;
        let decl_idx = symbol.value_declaration;
        let decl_node = arena.get(decl_idx)?;
        let var_decl = arena.get_variable_declaration(decl_node)?;
        let init_node = arena.get(var_decl.initializer)?;
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        arena.get_literal_expr(init_node).cloned()
    }

    pub(super) fn augment_namespace_props_with_define_property_exports(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
        props: &mut Vec<tsz_solver::PropertyInfo>,
    ) {
        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name));
        let Some(target_file_idx) = target_file_idx else {
            return;
        };

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = target_arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = target_arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(call_node) = target_arena.get(stmt.expression) else {
                continue;
            };
            if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
                continue;
            }
            let Some(call) = target_arena.get_call_expr(call_node) else {
                continue;
            };
            let Some(callee_node) = target_arena.get(call.expression) else {
                continue;
            };
            if callee_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                continue;
            }
            let Some(access) = target_arena.get_access_expr(callee_node) else {
                continue;
            };
            let is_object_define_property = target_arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == "Object")
                && target_arena
                    .get_identifier_at(access.name_or_argument)
                    .is_some_and(|ident| ident.escaped_text == "defineProperty");
            if !is_object_define_property {
                continue;
            }

            let Some(args) = &call.arguments else {
                continue;
            };
            if args.nodes.len() < 3 {
                continue;
            }

            let target_expr = args.nodes[0];
            let name_expr = args.nodes[1];
            let descriptor_expr = args.nodes[2];

            let is_exports_target = target_arena
                .get_identifier_at(target_expr)
                .is_some_and(|ident| ident.escaped_text == "exports");
            let is_module_exports_target =
                target_arena.get(target_expr).is_some_and(|target_node| {
                    if target_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                        return false;
                    }
                    let Some(target_access) = target_arena.get_access_expr(target_node) else {
                        return false;
                    };
                    target_arena
                        .get_identifier_at(target_access.expression)
                        .is_some_and(|ident| ident.escaped_text == "module")
                        && target_arena
                            .get_identifier_at(target_access.name_or_argument)
                            .is_some_and(|ident| ident.escaped_text == "exports")
                });
            if !is_exports_target && !is_module_exports_target {
                continue;
            }

            let Some(name_node) = target_arena.get(name_expr) else {
                continue;
            };
            if name_node.kind != tsz_scanner::SyntaxKind::StringLiteral as u16 {
                continue;
            }
            let Some(name_lit) = target_arena.get_literal(name_node) else {
                continue;
            };
            let name_atom = self.ctx.types.intern_string(&name_lit.text);
            if props.iter().any(|prop| prop.name == name_atom) {
                continue;
            }

            let Some(descriptor) = self.resolve_define_property_descriptor_object_literal(
                target_file_idx,
                target_arena,
                descriptor_expr,
            ) else {
                continue;
            };

            let mut has_value = false;
            let mut writable_true = false;
            let mut has_getter = false;
            let mut has_setter = false;

            for &element_idx in &descriptor.elements.nodes {
                let Some(element_node) = target_arena.get(element_idx) else {
                    continue;
                };
                match element_node.kind {
                    syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                        let Some(prop) = target_arena.get_property_assignment(element_node) else {
                            continue;
                        };
                        let Some(prop_name) =
                            crate::types_domain::queries::core::get_literal_property_name(
                                target_arena,
                                prop.name,
                            )
                        else {
                            continue;
                        };

                        match prop_name.as_str() {
                            "value" => {
                                has_value = true;
                            }
                            "writable" => {
                                writable_true =
                                    target_arena.get(prop.initializer).is_some_and(|init| {
                                        init.kind == tsz_scanner::SyntaxKind::TrueKeyword as u16
                                    });
                            }
                            _ => {}
                        }
                    }
                    syntax_kind_ext::GET_ACCESSOR => {
                        has_getter = true;
                    }
                    syntax_kind_ext::SET_ACCESSOR => {
                        has_setter = true;
                    }
                    _ => {}
                }
            }

            let readonly = !has_value || !writable_true || has_getter || has_setter;
            props.push(tsz_solver::PropertyInfo {
                name: name_atom,
                type_id: TypeId::ANY,
                write_type: TypeId::ANY,
                optional: false,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: props.len() as u32,
            });
        }
    }

    pub(crate) fn augment_namespace_props_with_commonjs_exports_for_file(
        &mut self,
        target_file_idx: usize,
        props: &mut Vec<tsz_solver::PropertyInfo>,
    ) {
        self.augment_namespace_props_with_direct_assignment_exports_for_file(
            target_file_idx,
            props,
        );
        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let Some(source_file) = target_arena.source_files.first() else {
            return;
        };
        let module_name = source_file.file_name.clone();
        self.augment_namespace_props_with_define_property_exports(
            &module_name,
            Some(target_file_idx),
            props,
        );
    }

    pub(crate) fn commonjs_module_value_type(
        &mut self,
        module_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let factory = self.ctx.types.factory();

        if let Some(json_type) = self.json_module_type_for_module(module_name, source_file_idx) {
            return Some(json_type);
        }

        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name));

        let exports_table = self
            .resolve_effective_module_exports_from_file(module_name, source_file_idx)
            .or_else(|| {
                let target_file_idx = target_file_idx?;
                let target_file_name = self
                    .ctx
                    .get_arena_for_file(target_file_idx as u32)
                    .source_files
                    .first()
                    .map(|source_file| source_file.file_name.clone())?;
                self.resolve_effective_module_exports(&target_file_name)
            });

        if let Some(exports_table) = exports_table {
            let module_is_non_module_entity =
                self.ctx.module_resolves_to_non_module_entity(module_name);
            for (name, &sym_id) in exports_table.iter() {
                self.record_cross_file_symbol_if_needed(sym_id, name, module_name);
            }

            let export_equals_type = exports_table
                .get("export=")
                .map(|export_equals_sym| self.get_type_of_symbol(export_equals_sym));
            let mut props: Vec<PropertyInfo> = Vec::new();
            for (name, &sym_id) in exports_table.iter() {
                if name == "export="
                    || self.is_type_only_export_symbol(sym_id)
                    || self.is_export_from_type_only_wildcard(module_name, name)
                    || self.export_symbol_has_no_value(sym_id)
                    || self.is_export_type_only_from_file(module_name, name, source_file_idx)
                {
                    continue;
                }

                let mut prop_type = self.get_type_of_symbol(sym_id);
                prop_type = self.apply_module_augmentations(module_name, name, prop_type);
                let name_atom = self.ctx.types.intern_string(name);
                props.push(PropertyInfo {
                    name: name_atom,
                    type_id: prop_type,
                    write_type: prop_type,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: None,
                    declaration_order: props.len() as u32,
                });
            }

            if !module_is_non_module_entity
                && let Some(augmentations) = self.ctx.binder.module_augmentations.get(module_name)
            {
                for aug in augmentations {
                    let name_atom = self.ctx.types.intern_string(&aug.name);
                    if props.iter().any(|p| p.name == name_atom) {
                        continue;
                    }
                    props.push(PropertyInfo {
                        name: name_atom,
                        type_id: TypeId::ANY,
                        write_type: TypeId::ANY,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: false,
                        visibility: Visibility::Public,
                        parent_id: None,
                        declaration_order: props.len() as u32,
                    });
                }
            }

            // Use the cached JsExportSurface to merge CommonJS property-assignment
            // exports (exports.foo = ..., module.exports.foo = ..., Object.defineProperty)
            // instead of re-scanning the target file AST.
            let augment_target = source_file_idx
                .and_then(|src_idx| {
                    self.ctx
                        .resolve_import_target_from_file(src_idx, module_name)
                })
                .or(target_file_idx);
            if let Some(target_idx) = augment_target {
                let surface = self.resolve_js_export_surface(target_idx);
                for surface_prop in &surface.named_exports {
                    if let Some(existing) = props.iter_mut().find(|p| p.name == surface_prop.name) {
                        // Surface has inferred types; upgrade ANY-typed ESM stubs
                        existing.type_id = surface_prop.type_id;
                        existing.write_type = surface_prop.write_type;
                    } else {
                        let mut prop = surface_prop.clone();
                        prop.declaration_order = props.len() as u32;
                        props.push(prop);
                    }
                }
            }

            let namespace_type = factory.object(props);
            let display_module_name =
                self.resolve_namespace_display_module_name(&exports_table, module_name);
            self.ctx
                .namespace_module_names
                .insert(namespace_type, display_module_name);

            if let Some(export_equals_type) = export_equals_type {
                if module_is_non_module_entity {
                    return Some(export_equals_type);
                }
                return Some(factory.intersection2(export_equals_type, namespace_type));
            }

            return Some(namespace_type);
        }

        // Use the unified JS export surface for the no-export-table fallback.
        // This synthesizes module.exports, exports.foo, Object.defineProperty,
        // and prototype assignments through one authority.
        if let Some(surface) =
            self.resolve_js_export_surface_for_module(module_name, source_file_idx)
            && surface.has_commonjs_exports
        {
            let display_name = self.imported_namespace_display_module_name(module_name);
            return surface.to_type_id_with_display_name(self, Some(display_name));
        }

        None
    }
}
