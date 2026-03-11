use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{PropertyInfo, TypeId, Visibility};

impl<'a> CheckerState<'a> {
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
        let mut export_names = BTreeSet::new();
        for source_file in &self.ctx.arena.source_files {
            for &stmt_idx in &source_file.statements.nodes {
                self.collect_current_file_commonjs_export_names(stmt_idx, &mut export_names);
            }
        }

        let mut props = Vec::with_capacity(export_names.len());
        for (declaration_order, name) in export_names.into_iter().enumerate() {
            let name_atom = self.ctx.types.intern_string(&name);
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
                declaration_order: declaration_order as u32,
            });
        }

        self.augment_namespace_props_with_commonjs_exports_for_file(
            self.ctx.current_file_idx,
            &mut props,
        );

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

            if node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some(name) = self.current_file_commonjs_define_property_export_name(idx)
            {
                names.insert(name);
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

    fn is_current_file_commonjs_export_base(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "exports")
                && self
                    .resolve_identifier_symbol_without_tracking(idx)
                    .is_none();
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
            if let Some(name_text) = direct_exports_name.or(module_exports_name) {
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
        );
    }

    fn infer_commonjs_export_rhs_type(
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
        checker.ctx.all_arenas = Some(all_arenas.clone());
        checker.ctx.all_binders = Some(all_binders.clone());
        checker.ctx.resolved_module_paths = self.ctx.resolved_module_paths.clone();
        checker.ctx.module_specifiers = self.ctx.module_specifiers.clone();
        checker.ctx.current_file_idx = target_file_idx;
        if !self.ctx.cross_file_symbol_targets.borrow().is_empty() {
            *checker.ctx.cross_file_symbol_targets.borrow_mut() =
                self.ctx.cross_file_symbol_targets.borrow().clone();
        }

        let ty = checker
            .literal_type_from_initializer(rhs_expr)
            .unwrap_or_else(|| checker.get_type_of_node(rhs_expr));
        for (&sym_id, &target_idx) in checker.ctx.cross_file_symbol_targets.borrow().iter() {
            self.ctx
                .cross_file_symbol_targets
                .borrow_mut()
                .insert(sym_id, target_idx);
        }
        ty
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
        let mut pending_props: FxHashMap<String, NodeIndex> = FxHashMap::default();
        let mut ordered_names: Vec<String> = Vec::new();

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
            self.collect_direct_commonjs_assignment_exports(
                target_arena,
                stmt.expression,
                &mut pending_props,
                &mut ordered_names,
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

    pub(crate) fn resolve_direct_commonjs_assignment_export_type(
        &mut self,
        module_name: &str,
        export_name: &str,
        source_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let target_file_idx = source_file_idx
            .and_then(|file_idx| {
                self.ctx
                    .resolve_import_target_from_file(file_idx, module_name)
            })
            .or_else(|| self.ctx.resolve_import_target(module_name))?;

        let target_arena = self.ctx.get_arena_for_file(target_file_idx as u32);
        let source_file = target_arena.source_files.first()?;
        let mut pending_props: FxHashMap<String, NodeIndex> = FxHashMap::default();
        let mut ordered_names: Vec<String> = Vec::new();

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
            self.collect_direct_commonjs_assignment_exports(
                target_arena,
                stmt.expression,
                &mut pending_props,
                &mut ordered_names,
            );
        }

        let rhs_expr = pending_props.get(export_name).copied()?;
        let rhs_type = self.infer_commonjs_export_rhs_type(target_file_idx, rhs_expr);
        (rhs_type != TypeId::UNDEFINED).then_some(rhs_type)
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
}
