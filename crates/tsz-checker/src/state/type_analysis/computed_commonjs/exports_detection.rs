use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::Value as JsonValue;
use std::collections::BTreeSet;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
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
        if (symbol.flags & symbol_flags::CLASS) == 0 {
            return None;
        }

        let symbol_type = self.get_type_of_symbol(sym_id);
        (symbol_type != TypeId::ERROR && symbol_type != TypeId::UNKNOWN).then_some(symbol_type)
    }

    pub(crate) fn check_commonjs_export_property_redeclarations(&mut self) {
        if !self.is_js_file() {
            return;
        }

        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return;
        };

        let mut rhs_expr = None;
        for (stmt_ordinal, &stmt_idx) in source_file.statements.nodes.iter().enumerate() {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            let candidate = if stmt_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                self.ctx
                    .arena
                    .get_expression_statement(stmt_node)
                    .and_then(|stmt| {
                        self.direct_commonjs_module_export_assignment_rhs(
                            self.ctx.arena,
                            stmt.expression,
                        )
                    })
            } else if stmt_node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
                self.direct_commonjs_module_export_rhs_from_variable_statement(
                    self.ctx.arena,
                    stmt_idx,
                )
            } else {
                None
            };
            if let Some(found_rhs) = candidate {
                let _ = stmt_ordinal;
                rhs_expr = Some(found_rhs);
            }
        }
        let Some(rhs_expr) = rhs_expr else {
            return;
        };

        let direct_export_root = self
            .ctx
            .arena
            .get(rhs_expr)
            .filter(|node| node.kind == SyntaxKind::Identifier as u16)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .map(|ident| ident.escaped_text.clone());
        let Some(direct_export_root) = direct_export_root else {
            return;
        };

        let mut explicit_exports: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();
        let mut root_exports: FxHashMap<String, Vec<NodeIndex>> = FxHashMap::default();

        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let Some(expr_node) = self.ctx.arena.get(stmt.expression) else {
                continue;
            };
            if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some((target, member_name)) =
                    self.commonjs_define_property_target_and_name(stmt.expression)
            {
                if self.is_current_file_commonjs_export_base(target) {
                    explicit_exports
                        .entry(member_name)
                        .or_default()
                        .push(stmt.expression);
                    continue;
                }

                if self
                    .ctx
                    .arena
                    .get_identifier_at(target)
                    .is_some_and(|ident| ident.escaped_text == direct_export_root)
                {
                    root_exports
                        .entry(member_name)
                        .or_default()
                        .push(stmt.expression);
                }
                continue;
            }
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }

            let Some(left_node) = self.ctx.arena.get(binary.left) else {
                continue;
            };
            if left_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && left_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                continue;
            }
            let Some(access) = self.ctx.arena.get_access_expr(left_node) else {
                continue;
            };
            let Some(member_name) =
                Self::commonjs_static_member_name_in_arena(self.ctx.arena, access.name_or_argument)
            else {
                continue;
            };

            if self.is_current_file_commonjs_export_base(access.expression) {
                explicit_exports
                    .entry(member_name)
                    .or_default()
                    .push(binary.left);
                continue;
            }

            if self
                .ctx
                .arena
                .get_identifier_at(access.expression)
                .is_some_and(|ident| ident.escaped_text == direct_export_root)
            {
                root_exports
                    .entry(member_name)
                    .or_default()
                    .push(binary.left);
            }
        }

        let all_names: FxHashSet<String> = explicit_exports
            .keys()
            .chain(root_exports.keys())
            .cloned()
            .collect();
        for name in all_names {
            let export_nodes = explicit_exports.get(&name);
            let root_nodes = root_exports.get(&name);
            let export_len = export_nodes.map_or(0, Vec::len);
            let root_len = root_nodes.map_or(0, Vec::len);
            if export_len + root_len < 2 {
                continue;
            }
            let message = format!("Cannot redeclare exported variable '{name}'.");
            let mut seen = FxHashSet::default();
            for &node in export_nodes
                .into_iter()
                .flatten()
                .chain(root_nodes.into_iter().flatten())
            {
                if seen.insert(node) {
                    self.error_at_node(
                        node,
                        &message,
                        crate::diagnostics::diagnostic_codes::CANNOT_REDECLARE_EXPORTED_VARIABLE,
                    );
                }
            }
        }
    }

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
                        is_string_named: false,
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
            is_string_named: false,
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
        self.current_file_commonjs_namespace_type_with_display_extension(false)
    }

    fn current_file_commonjs_namespace_type_with_display_extension(
        &mut self,
        preserve_js_extension: bool,
    ) -> TypeId {
        // Use the cached JsExportSurface for typed exports instead of
        // re-scanning the AST with augment_namespace_props_with_commonjs_exports_for_file.
        let current_file_idx = self.ctx.current_file_idx;
        let surface = self.resolve_js_export_surface(current_file_idx);
        let can_merge_named_exports = surface.direct_export_type.is_none_or(|direct_export_type| {
            crate::query_boundaries::js_exports::commonjs_direct_export_supports_named_props(
                self.ctx.types,
                direct_export_type,
            )
        });

        // Deep-scan the AST for export names that may be nested (in if-blocks, etc.)
        // and not captured by the surface's top-level + IIFE scan.
        let mut export_names = BTreeSet::new();
        for source_file in &self.ctx.arena.source_files {
            for &stmt_idx in &source_file.statements.nodes {
                self.collect_current_file_commonjs_export_names(stmt_idx, &mut export_names);
            }
        }

        // Start with the surface's typed named exports and any deep-scan names.
        let mut props = if can_merge_named_exports {
            surface.named_exports
        } else {
            Vec::new()
        };

        // Add ANY-typed entries for any deep-scan names not already in the surface
        for name in &export_names {
            if !can_merge_named_exports {
                break;
            }
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
                is_string_named: false,
            });
        }

        let has_named_props = !props.is_empty();
        crate::query_boundaries::js_exports::JsExportSurface {
            direct_export_type: surface.direct_export_type,
            named_exports: props,
            prototype_members: surface.prototype_members,
            has_commonjs_exports: surface.has_commonjs_exports || has_named_props,
        }
        .to_type_id_with_display_name(
            self,
            Some(self.current_file_commonjs_module_name(preserve_js_extension)),
        )
        .unwrap_or_else(|| {
            let empty_namespace = self.ctx.types.factory().object(Vec::new());
            self.ctx.namespace_module_names.insert(
                empty_namespace,
                self.current_file_commonjs_module_name(preserve_js_extension),
            );
            empty_namespace
        })
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
        if args.nodes.len() < 2 {
            return None;
        }

        Some((
            args.nodes[0],
            self.current_file_commonjs_static_member_name(args.nodes[1])?,
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

    pub(crate) fn is_current_file_commonjs_export_base(&self, idx: NodeIndex) -> bool {
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
        let module_is_unshadowed = !self
            .resolve_identifier_symbol_without_tracking(access.expression)
            .is_some_and(|sym_id| {
                self.ctx
                    .binder
                    .get_symbol(sym_id)
                    .is_some_and(|symbol| symbol.decl_file_idx == self.ctx.current_file_idx as u32)
            });
        self.ctx
            .arena
            .get_identifier_at(access.expression)
            .is_some_and(|ident| ident.escaped_text == "module" && module_is_unshadowed)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    fn current_file_commonjs_module_name(&self, preserve_js_extension: bool) -> String {
        if !preserve_js_extension
            && let Some(specifier) = self.current_file_explicit_js_module_specifier()
        {
            return specifier
                .rsplit(|ch| ['/', '\\'].contains(&ch))
                .next()
                .unwrap_or(specifier)
                .to_string();
        }

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

    fn current_file_explicit_js_module_specifier(&self) -> Option<&str> {
        let paths = self.ctx.resolved_module_paths.as_ref()?;
        paths.iter().find_map(|((_, specifier), &target_idx)| {
            (target_idx == self.ctx.current_file_idx
                && matches!(
                    specifier,
                    s if s.ends_with(".js")
                        || s.ends_with(".jsx")
                        || s.ends_with(".mjs")
                        || s.ends_with(".cjs")
                ))
            .then_some(specifier.as_str())
        })
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
}
