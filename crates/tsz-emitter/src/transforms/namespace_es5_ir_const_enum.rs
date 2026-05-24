use super::*;

impl<'a> NamespaceES5Transformer<'a> {
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
