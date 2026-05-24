use super::*;

impl<'a> NamespaceES5Transformer<'a> {
    pub(super) fn namespace_statement_erases_runtime(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return true;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => self
                .arena
                .get_export_decl(member_node)
                .is_none_or(|export_data| {
                    self.namespace_statement_erases_runtime(export_data.export_clause)
                }),
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::NAMED_EXPORTS =>
            {
                true
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => self
                .arena
                .get_import_decl_at(member_idx)
                .is_none_or(|import| {
                    self.import_equals_uses_external_module_ref(member_idx)
                        || !self.import_equals_target_has_runtime_value(
                            member_idx,
                            import.module_specifier,
                        )
                }),
            _ => false,
        }
    }

    pub(super) fn import_equals_uses_external_module_ref(&self, import_idx: NodeIndex) -> bool {
        let Some(import) = self.arena.get_import_decl_at(import_idx) else {
            return false;
        };
        self.arena.get(import.module_specifier).is_some_and(|node| {
            node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                || node.kind == SyntaxKind::StringLiteral as u16
        })
    }
    pub(super) fn transform_import_equals_in_namespace(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        if !self.import_equals_target_has_runtime_value(import_idx, import.module_specifier) {
            return None;
        }

        let alias = get_identifier_text(self.arena, import.import_clause)?;
        if !self.import_equals_alias_is_referenced_after_node(import_idx, import) {
            return None;
        }

        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);
        let is_exported = self
            .arena
            .has_modifier(&import.modifiers, SyntaxKind::ExportKeyword);

        if is_exported {
            Some(IRNode::NamespaceExport {
                namespace: ns_name.to_string().into(),
                name: alias.into(),
                value: Box::new(target_expr),
            })
        } else {
            Some(IRNode::VarDecl {
                name: alias.into(),
                initializer: Some(Box::new(target_expr)),
            })
        }
    }

    pub(super) fn transform_import_equals_exported(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        let alias = get_identifier_text(self.arena, import.import_clause)?;

        if !self.import_equals_target_has_runtime_value(import_idx, import.module_specifier) {
            return None;
        }

        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);

        Some(IRNode::NamespaceExport {
            namespace: ns_name.to_string().into(),
            name: alias.into(),
            value: Box::new(target_expr),
        })
    }

    fn import_equals_target_has_runtime_value(
        &self,
        import_idx: NodeIndex,
        target_idx: NodeIndex,
    ) -> bool {
        let Some(target_parts) = collect_qualified_name_parts(self.arena, target_idx) else {
            return true;
        };

        let namespace_parts = self.containing_namespace_parts(import_idx);
        if !namespace_parts.is_empty() {
            let mut relative_parts = namespace_parts;
            relative_parts.extend(target_parts.iter().cloned());
            if let Some(has_runtime) = entity_path_has_runtime_value(self.arena, &relative_parts) {
                return has_runtime;
            }
        }

        entity_path_has_runtime_value(self.arena, &target_parts).unwrap_or(true)
    }

    fn import_equals_alias_is_referenced_after_node(
        &self,
        import_idx: NodeIndex,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let Some(alias) = get_identifier_text(self.arena, import.import_clause) else {
            return true;
        };
        let Some(source_text) = self.source_text else {
            return true;
        };
        let Some(import_node) = self.arena.get(import_idx) else {
            return true;
        };
        let full_haystack = self.source_after_import_equals(import_node, import);
        let haystack = if let Some(scope_end) = self.namespace_import_scope_end(import_idx) {
            let full_start_in_source = source_text.len().saturating_sub(full_haystack.len());
            let scope_end = scope_end as usize;
            if scope_end <= full_start_in_source {
                ""
            } else {
                let end_in_full = scope_end - full_start_in_source;
                &full_haystack[..end_in_full.min(full_haystack.len())]
            }
        } else {
            full_haystack
        };
        let stripped = crate::import_usage::strip_type_only_content(haystack);
        crate::import_usage::contains_identifier_occurrence_before_shadow(&stripped, &alias)
    }

    fn source_after_import_equals(
        &self,
        import_node: &Node,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> &'a str {
        let Some(source_text) = self.source_text else {
            return "";
        };
        let mut start = self
            .arena
            .get(import.module_specifier)
            .map_or(import_node.end as usize, |module_node| {
                module_node.end as usize
            });
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        while start < bytes.len() {
            match bytes[start] {
                b'\n' => {
                    start += 1;
                    break;
                }
                b'\r' => {
                    start += 1;
                    if start < bytes.len() && bytes[start] == b'\n' {
                        start += 1;
                    }
                    break;
                }
                _ => start += 1,
            }
        }
        &source_text[start..]
    }

    fn namespace_import_scope_end(&self, import_idx: NodeIndex) -> Option<u32> {
        let block_idx = self.containing_module_block(import_idx)?;
        let block_node = self.arena.get(block_idx)?;
        let block = self.arena.get_module_block(block_node)?;
        let statements = block.statements.as_ref()?;
        let last_stmt = statements
            .nodes
            .last()
            .and_then(|last_idx| self.arena.get(*last_idx))?;
        Some(self.find_code_end_of_erased_stmt(last_stmt.pos, last_stmt.end))
    }

    fn containing_module_block(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = self.arena.parent_of(node_idx).unwrap_or(NodeIndex::NONE);
        while current != NodeIndex::NONE {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::MODULE_BLOCK {
                return Some(current);
            }
            current = self.arena.parent_of(current).unwrap_or(NodeIndex::NONE);
        }
        None
    }

    fn containing_namespace_parts(&self, node_idx: NodeIndex) -> Vec<String> {
        let mut groups = Vec::new();
        let mut current = self.arena.parent_of(node_idx).unwrap_or(NodeIndex::NONE);

        while current != NodeIndex::NONE {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.arena.get_module(node)
                && let Some(parts) = self.flatten_module_name(module.name)
            {
                groups.push(parts);
            }
            current = self.arena.parent_of(current).unwrap_or(NodeIndex::NONE);
        }

        groups.reverse();
        groups.into_iter().flatten().collect()
    }

    pub(super) fn rewrite_const_enum_accesses(
        &self,
        nodes: &mut [IRNode],
        namespace_path: &[String],
    ) {
        if self.const_enum_values.is_empty() {
            return;
        }

        for node in nodes {
            self.rewrite_const_enum_accesses_in_node(node, namespace_path);
        }
    }

    fn rewrite_const_enum_accesses_in_node(&self, node: &mut IRNode, namespace_path: &[String]) {
        if let Some(replacement) = self.const_enum_replacement(node, namespace_path) {
            *node = replacement;
            return;
        }

        match node {
            IRNode::BinaryExpr { left, right, .. }
            | IRNode::LogicalOr { left, right }
            | IRNode::LogicalAnd { left, right } => {
                self.rewrite_const_enum_accesses_in_node(left, namespace_path);
                self.rewrite_const_enum_accesses_in_node(right, namespace_path);
            }
            IRNode::PrefixUnaryExpr { operand, .. }
            | IRNode::PostfixUnaryExpr { operand, .. }
            | IRNode::Parenthesized(operand)
            | IRNode::SpreadElement(operand)
            | IRNode::ExpressionStatement(operand)
            | IRNode::ThrowStatement(operand)
            | IRNode::PrivateFieldGet {
                receiver: operand, ..
            }
            | IRNode::PrivateStaticFieldGet {
                receiver: operand, ..
            }
            | IRNode::PrivateFieldIn { obj: operand, .. } => {
                self.rewrite_const_enum_accesses_in_node(operand, namespace_path);
            }
            IRNode::CallExpr { callee, arguments }
            | IRNode::NewExpr {
                callee, arguments, ..
            } => {
                self.rewrite_const_enum_accesses_in_node(callee, namespace_path);
                for arg in arguments {
                    self.rewrite_const_enum_accesses_in_node(arg, namespace_path);
                }
            }
            IRNode::PropertyAccess { object, .. } => {
                self.rewrite_const_enum_accesses_in_node(object, namespace_path);
            }
            IRNode::ElementAccess { object, index } => {
                self.rewrite_const_enum_accesses_in_node(object, namespace_path);
                self.rewrite_const_enum_accesses_in_node(index, namespace_path);
            }
            IRNode::ConditionalExpr {
                condition,
                when_true,
                when_false,
            } => {
                self.rewrite_const_enum_accesses_in_node(condition, namespace_path);
                self.rewrite_const_enum_accesses_in_node(when_true, namespace_path);
                self.rewrite_const_enum_accesses_in_node(when_false, namespace_path);
            }
            IRNode::CommaExpr(items)
            | IRNode::CommaExprMultiline(items)
            | IRNode::CommaExprMultilineFlat(items)
            | IRNode::ArrayLiteral(items)
            | IRNode::VarDeclList(items)
            | IRNode::Block(items)
            | IRNode::Sequence(items)
            | IRNode::StaticBlockIIFE { statements: items } => {
                for item in items {
                    self.rewrite_const_enum_accesses_in_node(item, namespace_path);
                }
            }
            IRNode::ObjectLiteral { properties, .. } => {
                for property in properties {
                    if let IRPropertyKey::Computed(key) = &mut property.key {
                        self.rewrite_const_enum_accesses_in_node(key, namespace_path);
                    }
                    self.rewrite_const_enum_accesses_in_node(&mut property.value, namespace_path);
                }
            }
            IRNode::FunctionExpr {
                parameters, body, ..
            }
            | IRNode::FunctionDecl {
                parameters, body, ..
            } => {
                for param in parameters {
                    if let Some(default_value) = &mut param.default_value {
                        self.rewrite_const_enum_accesses_in_node(default_value, namespace_path);
                    }
                }
                for item in body {
                    self.rewrite_const_enum_accesses_in_node(item, namespace_path);
                }
            }
            IRNode::VarDecl { initializer, .. } => {
                if let Some(initializer) = initializer {
                    self.rewrite_const_enum_accesses_in_node(initializer, namespace_path);
                }
            }
            IRNode::ReturnStatement(expr) => {
                if let Some(expr) = expr {
                    self.rewrite_const_enum_accesses_in_node(expr, namespace_path);
                }
            }
            IRNode::IfStatement {
                condition,
                then_branch,
                else_branch,
            } => {
                self.rewrite_const_enum_accesses_in_node(condition, namespace_path);
                self.rewrite_const_enum_accesses_in_node(then_branch, namespace_path);
                if let Some(else_branch) = else_branch {
                    self.rewrite_const_enum_accesses_in_node(else_branch, namespace_path);
                }
            }
            IRNode::SwitchStatement { expression, cases } => {
                self.rewrite_const_enum_accesses_in_node(expression, namespace_path);
                for case in cases {
                    if let Some(test) = &mut case.test {
                        self.rewrite_const_enum_accesses_in_node(test, namespace_path);
                    }
                    for statement in &mut case.statements {
                        self.rewrite_const_enum_accesses_in_node(statement, namespace_path);
                    }
                }
            }
            IRNode::ForStatement {
                initializer,
                condition,
                incrementor,
                body,
            } => {
                if let Some(initializer) = initializer {
                    self.rewrite_const_enum_accesses_in_node(initializer, namespace_path);
                }
                if let Some(condition) = condition {
                    self.rewrite_const_enum_accesses_in_node(condition, namespace_path);
                }
                if let Some(incrementor) = incrementor {
                    self.rewrite_const_enum_accesses_in_node(incrementor, namespace_path);
                }
                self.rewrite_const_enum_accesses_in_node(body, namespace_path);
            }
            IRNode::ForInOfStatement {
                initializer,
                expression,
                body,
                ..
            } => {
                self.rewrite_const_enum_accesses_in_node(initializer, namespace_path);
                self.rewrite_const_enum_accesses_in_node(expression, namespace_path);
                self.rewrite_const_enum_accesses_in_node(body, namespace_path);
            }
            IRNode::WhileStatement { condition, body }
            | IRNode::DoWhileStatement { body, condition } => {
                self.rewrite_const_enum_accesses_in_node(condition, namespace_path);
                self.rewrite_const_enum_accesses_in_node(body, namespace_path);
            }
            IRNode::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            } => {
                self.rewrite_const_enum_accesses_in_node(try_block, namespace_path);
                if let Some(catch_clause) = catch_clause {
                    for statement in &mut catch_clause.body {
                        self.rewrite_const_enum_accesses_in_node(statement, namespace_path);
                    }
                }
                if let Some(finally_block) = finally_block {
                    self.rewrite_const_enum_accesses_in_node(finally_block, namespace_path);
                }
            }
            IRNode::LabeledStatement { statement, .. } => {
                self.rewrite_const_enum_accesses_in_node(statement, namespace_path);
            }
            IRNode::ES5ClassIIFE {
                base_class,
                body,
                computed_prop_temp_inits,
                deferred_static_blocks,
                ..
            }
            | IRNode::ES5ClassAssignment {
                base_class,
                body,
                computed_prop_temp_inits,
                deferred_static_blocks,
                ..
            } => {
                if let Some(base_class) = base_class {
                    self.rewrite_const_enum_accesses_in_node(base_class, namespace_path);
                }
                for item in body {
                    self.rewrite_const_enum_accesses_in_node(item, namespace_path);
                }
                for item in computed_prop_temp_inits {
                    self.rewrite_const_enum_accesses_in_node(item, namespace_path);
                }
                for item in deferred_static_blocks {
                    self.rewrite_const_enum_accesses_in_node(item, namespace_path);
                }
            }
            IRNode::ES5ClassApply {
                factory,
                base_class,
            } => {
                self.rewrite_const_enum_accesses_in_node(factory, namespace_path);
                self.rewrite_const_enum_accesses_in_node(base_class, namespace_path);
            }
            IRNode::PrototypeMethod {
                method_name,
                function,
                ..
            }
            | IRNode::StaticMethod {
                method_name,
                function,
                ..
            } => {
                if let crate::transforms::ir::IRMethodName::Computed(name) = method_name {
                    self.rewrite_const_enum_accesses_in_node(name, namespace_path);
                }
                self.rewrite_const_enum_accesses_in_node(function, namespace_path);
            }
            IRNode::DefineProperty {
                target,
                property_name,
                descriptor,
                ..
            } => {
                self.rewrite_const_enum_accesses_in_node(target, namespace_path);
                if let crate::transforms::ir::IRMethodName::Computed(name) = property_name {
                    self.rewrite_const_enum_accesses_in_node(name, namespace_path);
                }
                if let Some(get) = &mut descriptor.get {
                    self.rewrite_const_enum_accesses_in_node(get, namespace_path);
                }
                if let Some(set) = &mut descriptor.set {
                    self.rewrite_const_enum_accesses_in_node(set, namespace_path);
                }
                if let Some(value) = &mut descriptor.value {
                    self.rewrite_const_enum_accesses_in_node(value, namespace_path);
                }
            }
            IRNode::AwaiterCall {
                this_arg,
                generator_body,
                ..
            } => {
                self.rewrite_const_enum_accesses_in_node(this_arg, namespace_path);
                self.rewrite_const_enum_accesses_in_node(generator_body, namespace_path);
            }
            IRNode::GeneratorBody { cases, .. } => {
                for case in cases {
                    for statement in &mut case.statements {
                        self.rewrite_const_enum_accesses_in_node(statement, namespace_path);
                    }
                }
            }
            IRNode::GeneratorOp { value, .. } => {
                if let Some(value) = value {
                    self.rewrite_const_enum_accesses_in_node(value, namespace_path);
                }
            }
            IRNode::IfBreak { condition, .. } => {
                self.rewrite_const_enum_accesses_in_node(condition, namespace_path);
            }
            IRNode::PrivateFieldSet {
                receiver, value, ..
            } => {
                self.rewrite_const_enum_accesses_in_node(receiver, namespace_path);
                self.rewrite_const_enum_accesses_in_node(value, namespace_path);
            }
            IRNode::PrivateStaticFieldSet {
                receiver,
                state,
                value,
                ..
            } => {
                self.rewrite_const_enum_accesses_in_node(receiver, namespace_path);
                self.rewrite_const_enum_accesses_in_node(state, namespace_path);
                self.rewrite_const_enum_accesses_in_node(value, namespace_path);
            }
            IRNode::WeakMapSet { key, value, .. } => {
                self.rewrite_const_enum_accesses_in_node(key, namespace_path);
                self.rewrite_const_enum_accesses_in_node(value, namespace_path);
            }
            IRNode::EnumIIFE { members, .. } => {
                for member in members {
                    if let EnumMemberValue::Computed(expr) = &mut member.value {
                        self.rewrite_const_enum_accesses_in_node(expr, namespace_path);
                    }
                }
            }
            IRNode::NamespaceIIFE {
                body, name_parts, ..
            } => {
                let nested_namespace_path = name_parts
                    .iter()
                    .map(|part| part.as_ref().to_string())
                    .collect::<Vec<_>>();
                for item in body {
                    self.rewrite_const_enum_accesses_in_node(item, &nested_namespace_path);
                }
            }
            IRNode::NamespaceExport { value, .. } => {
                self.rewrite_const_enum_accesses_in_node(value, namespace_path);
            }
            _ => {}
        }
    }

    fn const_enum_replacement(&self, node: &IRNode, namespace_path: &[String]) -> Option<IRNode> {
        let IRNode::PropertyAccess { object, property } = node else {
            return None;
        };
        let enum_path = ir_access_path(object)?;
        let values = self.lookup_const_enum_values_for_ir(&enum_path, namespace_path)?;
        let value = values.get(property.as_ref())?;
        let literal = value.to_js_literal();
        if self.remove_comments {
            return Some(IRNode::Raw(literal.into()));
        }
        Some(IRNode::Raw(
            format!("{literal} /* {enum_path}.{property} */").into(),
        ))
    }

    fn lookup_const_enum_values_for_ir(
        &self,
        enum_path: &str,
        namespace_path: &[String],
    ) -> Option<&FxHashMap<String, EnumValue>> {
        if let Some(values) = self.lookup_const_enum_values_direct(enum_path) {
            return Some(values);
        }
        let namespace_prefix = namespace_path.join(".");
        if !namespace_prefix.is_empty() {
            let scoped_path = format!("{namespace_prefix}.{enum_path}");
            if let Some(values) = self.lookup_const_enum_values_direct(&scoped_path) {
                return Some(values);
            }
        }

        if let Some(dot_pos) = enum_path.find('.') {
            let first = &enum_path[..dot_pos];
            let rest = &enum_path[dot_pos + 1..];
            if !namespace_prefix.is_empty() {
                let scoped_alias = format!("{namespace_prefix}.{first}");
                if let Some(target) = self.const_enum_import_aliases.get(&scoped_alias) {
                    let resolved = format!("{target}.{rest}");
                    if let Some(values) = self.lookup_const_enum_values_direct(&resolved) {
                        return Some(values);
                    }
                }
            }
            if let Some(target) = self.const_enum_import_aliases.get(first) {
                let resolved = format!("{target}.{rest}");
                return self.lookup_const_enum_values_direct(&resolved);
            }
        } else {
            if !namespace_prefix.is_empty() {
                let scoped_alias = format!("{namespace_prefix}.{enum_path}");
                if let Some(target) = self.const_enum_import_aliases.get(&scoped_alias)
                    && let Some(values) = self.lookup_const_enum_values_direct(target)
                {
                    return Some(values);
                }
            }
            if let Some(target) = self.const_enum_import_aliases.get(enum_path) {
                return self.lookup_const_enum_values_direct(target);
            }
        }

        None
    }

    fn lookup_const_enum_values_direct(
        &self,
        enum_path: &str,
    ) -> Option<&FxHashMap<String, EnumValue>> {
        let entries = self.const_enum_values.get(enum_path)?;
        entries
            .iter()
            .find(|entry| entry.scope_start == 0 && entry.scope_end == u32::MAX)
            .or_else(|| entries.first())
            .map(|entry| &entry.values)
    }
}
