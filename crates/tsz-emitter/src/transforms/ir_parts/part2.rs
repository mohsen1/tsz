impl IRNode {
    /// Return whether this `IR` subtree references `name` as an identifier.
    pub fn contains_identifier(&self, name: &str) -> bool {
        match self {
            Self::Identifier(ident) => ident.as_ref() == name,
            Self::This { captured } => *captured && name == "_this",
            Self::BinaryExpr { left, right, .. }
            | Self::LogicalOr { left, right }
            | Self::LogicalAnd { left, right } => {
                left.contains_identifier(name) || right.contains_identifier(name)
            }
            Self::PrefixUnaryExpr { operand, .. }
            | Self::PostfixUnaryExpr { operand, .. }
            | Self::Parenthesized(operand)
            | Self::SpreadElement(operand)
            | Self::ExpressionStatement(operand)
            | Self::ThrowStatement(operand)
            | Self::PrivateFieldGet {
                receiver: operand, ..
            }
            | Self::PrivateStaticFieldGet {
                receiver: operand, ..
            }
            | Self::PrivateFieldIn { obj: operand, .. } => operand.contains_identifier(name),
            Self::CallExpr { callee, arguments }
            | Self::NewExpr {
                callee, arguments, ..
            } => {
                callee.contains_identifier(name)
                    || arguments.iter().any(|arg| arg.contains_identifier(name))
            }
            Self::PropertyAccess { object, .. } => object.contains_identifier(name),
            Self::ElementAccess { object, index } => {
                object.contains_identifier(name) || index.contains_identifier(name)
            }
            Self::ConditionalExpr {
                condition,
                when_true,
                when_false,
            } => {
                condition.contains_identifier(name)
                    || when_true.contains_identifier(name)
                    || when_false.contains_identifier(name)
            }
            Self::LeadingCommentExpr { expression, .. } => expression.contains_identifier(name),
            Self::CommaExpr(nodes)
            | Self::CommaExprMultiline(nodes)
            | Self::CommaExprMultilineFlat(nodes)
            | Self::ArrayLiteral(nodes)
            | Self::VarDeclList(nodes)
            | Self::Block(nodes)
            | Self::Sequence(nodes)
            | Self::StaticBlockIIFE { statements: nodes } => {
                nodes.iter().any(|node| node.contains_identifier(name))
            }
            Self::NewTargetCapture { initializer } => initializer.contains_identifier(name),
            Self::ObjectLiteral { properties, .. } => properties
                .iter()
                .any(|property| property.contains_identifier(name)),
            Self::FunctionExpr {
                parameters, body, ..
            }
            | Self::FunctionDecl {
                parameters, body, ..
            } => {
                if parameters
                    .iter()
                    .any(|param| param.contains_identifier(name))
                {
                    return true;
                }
                if function_body_declares_var(body, name) {
                    return false;
                }
                body.iter().any(|node| node.contains_identifier(name))
            }
            Self::VarDecl {
                name: var_name,
                initializer,
            } => {
                var_name.as_ref() == name
                    || initializer
                        .as_ref()
                        .is_some_and(|init| init.contains_identifier(name))
            }
            Self::ReturnStatement(expr) => expr
                .as_ref()
                .is_some_and(|expr| expr.contains_identifier(name)),
            Self::IfStatement {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.contains_identifier(name)
                    || then_branch.contains_identifier(name)
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| branch.contains_identifier(name))
            }
            Self::SwitchStatement { expression, cases } => {
                expression.contains_identifier(name)
                    || cases.iter().any(|case| case.contains_identifier(name))
            }
            Self::ForStatement {
                initializer,
                condition,
                incrementor,
                body,
            } => {
                initializer
                    .as_ref()
                    .is_some_and(|init| init.contains_identifier(name))
                    || condition
                        .as_ref()
                        .is_some_and(|condition| condition.contains_identifier(name))
                    || incrementor
                        .as_ref()
                        .is_some_and(|incrementor| incrementor.contains_identifier(name))
                    || body.contains_identifier(name)
            }
            Self::ForInOfStatement {
                initializer,
                expression,
                body,
                ..
            } => {
                initializer.contains_identifier(name)
                    || expression.contains_identifier(name)
                    || body.contains_identifier(name)
            }
            Self::WhileStatement { condition, body }
            | Self::DoWhileStatement { body, condition } => {
                condition.contains_identifier(name) || body.contains_identifier(name)
            }
            Self::TryStatement {
                try_block,
                catch_clause,
                finally_block,
            } => {
                try_block.contains_identifier(name)
                    || catch_clause
                        .as_ref()
                        .is_some_and(|catch| catch.contains_identifier(name))
                    || finally_block
                        .as_ref()
                        .is_some_and(|finally_block| finally_block.contains_identifier(name))
            }
            Self::LabeledStatement { statement, .. } => statement.contains_identifier(name),
            Self::ES5ClassIIFE {
                base_class,
                body,
                computed_prop_temp_inits,
                deferred_static_blocks,
                ..
            }
            | Self::ES5ClassAssignment {
                base_class,
                body,
                computed_prop_temp_inits,
                deferred_static_blocks,
                ..
            } => {
                base_class
                    .as_ref()
                    .is_some_and(|base| base.contains_identifier(name))
                    || body.iter().any(|node| node.contains_identifier(name))
                    || computed_prop_temp_inits
                        .iter()
                        .any(|node| node.contains_identifier(name))
                    || deferred_static_blocks
                        .iter()
                        .any(|node| node.contains_identifier(name))
            }
            Self::ExtendsHelper {
                class_name,
                super_name,
            } => class_name.as_ref() == name || super_name.as_ref() == name,
            Self::ES5ClassApply {
                factory,
                base_class,
            } => factory.contains_identifier(name) || base_class.contains_identifier(name),
            Self::PrototypeMethod {
                class_name,
                method_name,
                function,
                ..
            }
            | Self::StaticMethod {
                class_name,
                method_name,
                function,
                ..
            } => {
                class_name.as_ref() == name
                    || method_name.contains_identifier(name)
                    || function.contains_identifier(name)
            }
            Self::DefineProperty {
                target,
                property_name,
                descriptor,
                ..
            } => {
                target.contains_identifier(name)
                    || property_name.contains_identifier(name)
                    || descriptor.contains_identifier(name)
            }
            Self::AwaiterCall {
                this_arg,
                generator_body,
                ..
            } => this_arg.contains_identifier(name) || generator_body.contains_identifier(name),
            Self::GeneratorBody { cases, .. } => {
                cases.iter().any(|case| case.contains_identifier(name))
            }
            Self::GeneratorOp { value, .. } => value
                .as_ref()
                .is_some_and(|value| value.contains_identifier(name)),
            Self::IfBreak { condition, .. } => condition.contains_identifier(name),
            Self::PrivateFieldSet {
                receiver, value, ..
            } => receiver.contains_identifier(name) || value.contains_identifier(name),
            Self::PrivateStaticFieldSet {
                receiver,
                state,
                value,
                ..
            } => {
                receiver.contains_identifier(name)
                    || state.contains_identifier(name)
                    || value.contains_identifier(name)
            }
            Self::WeakMapSet { key, value, .. } => {
                key.contains_identifier(name) || value.contains_identifier(name)
            }
            Self::NamedImport { var_name, .. }
            | Self::NamespaceImport { var_name, .. }
            | Self::DefaultImport { var_name, .. }
            | Self::RequireStatement { var_name, .. }
            | Self::ExportInit { name: var_name }
            | Self::ExportAssignment { name: var_name } => var_name.as_ref() == name,
            Self::ReExportProperty {
                export_name,
                module_var,
                import_name,
            } => {
                export_name.as_ref() == name
                    || module_var.as_ref() == name
                    || import_name.as_ref() == name
            }
            Self::EnumIIFE {
                name: enum_name,
                members,
                namespace_export,
                ..
            } => {
                enum_name.as_ref() == name
                    || namespace_export
                        .as_ref()
                        .is_some_and(|ns| ns.as_ref() == name)
                    || members
                        .iter()
                        .any(|member| member.contains_identifier(name))
            }
            Self::NamespaceIIFE {
                name: namespace_name,
                body,
                parent_name,
                param_name,
                ..
            } => {
                namespace_name.as_ref() == name
                    || parent_name
                        .as_ref()
                        .is_some_and(|parent| parent.as_ref() == name)
                    || param_name
                        .as_ref()
                        .is_some_and(|param| param.as_ref() == name)
                    || body.iter().any(|node| node.contains_identifier(name))
            }
            Self::NamespaceExport {
                namespace,
                name: export_name,
                value,
            } => {
                namespace.as_ref() == name
                    || export_name.as_ref() == name
                    || value.contains_identifier(name)
            }
            Self::WithStatement { expression, body } => {
                expression.contains_identifier(name) || body.contains_identifier(name)
            }
            Self::NumericLiteral(_)
            | Self::StringLiteral(_)
            | Self::RawStringLiteral(_)
            | Self::BooleanLiteral(_)
            | Self::NullLiteral
            | Self::Undefined
            | Self::RuntimeHelper(_)
            | Self::Super
            | Self::ImportMeta
            | Self::EmptyStatement
            | Self::HoistedVarGroupBreak
            | Self::BreakStatement(_)
            | Self::ContinueStatement(_)
            | Self::GeneratorSent
            | Self::GeneratorLabel
            | Self::GeneratorTryPush { .. }
            | Self::GeneratorTryPushFinally { .. }
            | Self::GeneratorTryPushCatch { .. }
            | Self::Raw(_)
            | Self::Comment { .. }
            | Self::TrailingComment(_)
            | Self::ASTRef(_)
            | Self::ASTRefWithGeneratorThis { .. }
            | Self::ASTRefWithCapturedClassHeritageThis(_)
            | Self::ASTRefWithInheritedComputedNameSuper { .. }
            | Self::ASTRefWithInheritedComputedNameThis { .. }
            | Self::ASTRefRange(..)
            | Self::UseStrict
            | Self::EsesModuleMarker => false,
        }
    }

    /// Return whether this `IR` subtree contains a generated captured-this
    /// reference. User identifiers named `_this` are intentionally ignored.
    pub fn contains_captured_this_reference(&self) -> bool {
        match self {
            Self::This { captured } => *captured,
            Self::BinaryExpr { left, right, .. }
            | Self::LogicalOr { left, right }
            | Self::LogicalAnd { left, right } => {
                left.contains_captured_this_reference() || right.contains_captured_this_reference()
            }
            Self::PrefixUnaryExpr { operand, .. }
            | Self::PostfixUnaryExpr { operand, .. }
            | Self::Parenthesized(operand)
            | Self::SpreadElement(operand)
            | Self::ExpressionStatement(operand)
            | Self::ThrowStatement(operand)
            | Self::PrivateFieldGet {
                receiver: operand, ..
            }
            | Self::PrivateStaticFieldGet {
                receiver: operand, ..
            }
            | Self::PrivateFieldIn { obj: operand, .. } => {
                operand.contains_captured_this_reference()
            }
            Self::CallExpr { callee, arguments }
            | Self::NewExpr {
                callee, arguments, ..
            } => {
                callee.contains_captured_this_reference()
                    || arguments.iter().any(Self::contains_captured_this_reference)
            }
            Self::PropertyAccess { object, .. } => object.contains_captured_this_reference(),
            Self::ElementAccess { object, index } => {
                object.contains_captured_this_reference()
                    || index.contains_captured_this_reference()
            }
            Self::ConditionalExpr {
                condition,
                when_true,
                when_false,
            } => {
                condition.contains_captured_this_reference()
                    || when_true.contains_captured_this_reference()
                    || when_false.contains_captured_this_reference()
            }
            Self::LeadingCommentExpr { expression, .. } => {
                expression.contains_captured_this_reference()
            }
            Self::CommaExpr(nodes)
            | Self::CommaExprMultiline(nodes)
            | Self::CommaExprMultilineFlat(nodes)
            | Self::ArrayLiteral(nodes)
            | Self::VarDeclList(nodes)
            | Self::Block(nodes)
            | Self::Sequence(nodes)
            | Self::StaticBlockIIFE { statements: nodes } => {
                nodes.iter().any(Self::contains_captured_this_reference)
            }
            Self::NewTargetCapture { initializer } => {
                initializer.contains_captured_this_reference()
            }
            Self::ObjectLiteral { properties, .. } => properties
                .iter()
                .any(IRProperty::contains_captured_this_reference),
            Self::FunctionExpr {
                parameters, body, ..
            }
            | Self::FunctionDecl {
                parameters, body, ..
            } => {
                !function_body_declares_var(body, "_this")
                    && (parameters
                        .iter()
                        .any(IRParam::contains_captured_this_reference)
                        || body.iter().any(Self::contains_captured_this_reference))
            }
            Self::VarDecl { initializer, .. } => initializer
                .as_ref()
                .is_some_and(|init| init.contains_captured_this_reference()),
            Self::ReturnStatement(expr) => expr
                .as_ref()
                .is_some_and(|expr| expr.contains_captured_this_reference()),
            Self::IfStatement {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.contains_captured_this_reference()
                    || then_branch.contains_captured_this_reference()
                    || else_branch
                        .as_ref()
                        .is_some_and(|branch| branch.contains_captured_this_reference())
            }
            Self::GeneratorBody { cases, .. } => cases
                .iter()
                .any(IRGeneratorCase::contains_captured_this_reference),
            Self::GeneratorOp { value, .. } => value
                .as_ref()
                .is_some_and(|value| value.contains_captured_this_reference()),
            Self::AwaiterCall {
                this_arg,
                generator_body,
                ..
            } => {
                this_arg.contains_captured_this_reference()
                    || generator_body.contains_captured_this_reference()
            }
            Self::PrivateFieldSet {
                receiver, value, ..
            } => {
                receiver.contains_captured_this_reference()
                    || value.contains_captured_this_reference()
            }
            Self::PrivateStaticFieldSet {
                receiver,
                state,
                value,
                ..
            } => {
                receiver.contains_captured_this_reference()
                    || state.contains_captured_this_reference()
                    || value.contains_captured_this_reference()
            }
            Self::WeakMapSet { key, value, .. } => {
                key.contains_captured_this_reference() || value.contains_captured_this_reference()
            }
            _ => false,
        }
    }

    /// Create an identifier node
    pub fn id(name: impl Into<Cow<'static, str>>) -> Self {
        Self::Identifier(name.into())
    }

    /// Create a string literal
    pub fn string(s: impl Into<Cow<'static, str>>) -> Self {
        Self::StringLiteral(s.into())
    }

    /// Create a numeric literal
    pub fn number(n: impl Into<Cow<'static, str>>) -> Self {
        Self::NumericLiteral(n.into())
    }

    /// Create a call expression
    pub fn call(callee: Self, args: Vec<Self>) -> Self {
        Self::CallExpr {
            callee: Box::new(callee),
            arguments: args,
        }
    }

    /// Create a property access
    pub fn prop(object: Self, property: impl Into<Cow<'static, str>>) -> Self {
        Self::PropertyAccess {
            object: Box::new(object),
            property: property.into(),
        }
    }

    /// Create an element access
    pub fn elem(object: Self, index: Self) -> Self {
        Self::ElementAccess {
            object: Box::new(object),
            index: Box::new(index),
        }
    }

    /// Create a binary expression
    pub fn binary(left: Self, op: impl Into<Cow<'static, str>>, right: Self) -> Self {
        Self::BinaryExpr {
            left: Box::new(left),
            operator: op.into(),
            right: Box::new(right),
        }
    }

    /// Create an assignment expression
    pub fn assign(target: Self, value: Self) -> Self {
        Self::BinaryExpr {
            left: Box::new(target),
            operator: Cow::Borrowed("="),
            right: Box::new(value),
        }
    }

    /// Create an object-literal comma expression with accessor-aware layout.
    pub fn object_literal_comma_expr(parts: Vec<Self>) -> Self {
        if parts
            .iter()
            .any(|part| matches!(part, Self::CallExpr { .. }))
        {
            Self::CommaExprMultiline(parts)
        } else {
            Self::CommaExpr(parts)
        }
    }

    /// Create the `_a.trys.push([...])` IR for a state-machine try region.
    /// Picks the variant that matches the sparse-slot shape expected by tsc:
    /// `[s, c, f, e]`, `[s, c, , e]`, or `[s, , f, e]`.
    pub fn generator_try_push(
        start_label: u32,
        catch_label: Option<u32>,
        finally_label: Option<u32>,
        end_label: u32,
    ) -> Self {
        match (catch_label, finally_label) {
            (Some(catch_label), Some(finally_label)) => Self::GeneratorTryPush {
                start_label,
                catch_label,
                finally_label,
                end_label,
            },
            (Some(catch_label), None) => Self::GeneratorTryPushCatch {
                start_label,
                catch_label,
                end_label,
            },
            (None, Some(finally_label)) => Self::GeneratorTryPushFinally {
                start_label,
                finally_label,
                end_label,
            },
            (None, None) => panic!(
                "generator_try_push requires at least one handler (catch or finally); \
                 a handler-less try has no runtime entry"
            ),
        }
    }

    /// Create a var declaration
    pub fn var_decl(name: impl Into<Cow<'static, str>>, init: Option<Self>) -> Self {
        Self::VarDecl {
            name: name.into(),
            initializer: init.map(Box::new),
        }
    }

    /// Create a return statement
    pub fn ret(expr: Option<Self>) -> Self {
        Self::ReturnStatement(expr.map(Box::new))
    }

    /// Create a function expression
    pub const fn func_expr(
        name: Option<Cow<'static, str>>,
        params: Vec<IRParam>,
        body: Vec<Self>,
    ) -> Self {
        Self::FunctionExpr {
            name,
            parameters: params,
            body,
            is_expression_body: false,
            body_source_range: None,
        }
    }

    /// Create a function declaration
    pub fn func_decl(
        name: impl Into<Cow<'static, str>>,
        params: Vec<IRParam>,
        body: Vec<Self>,
    ) -> Self {
        Self::FunctionDecl {
            name: name.into(),
            parameters: params,
            body,
            body_source_range: None,
            leading_comment: None,
        }
    }

    /// Create `this` reference
    pub const fn this() -> Self {
        Self::This { captured: false }
    }

    /// Create `_this` reference (captured)
    pub const fn this_captured() -> Self {
        Self::This { captured: true }
    }

    /// Create `void 0`
    pub const fn void_0() -> Self {
        Self::Undefined
    }

    /// Wrap in parentheses
    pub fn paren(self) -> Self {
        Self::Parenthesized(Box::new(self))
    }

    /// Create a block
    pub const fn block(stmts: Vec<Self>) -> Self {
        Self::Block(stmts)
    }

    /// Create an expression statement
    pub fn expr_stmt(expr: Self) -> Self {
        Self::ExpressionStatement(Box::new(expr))
    }

    /// Create an object literal
    pub const fn object(props: Vec<IRProperty>) -> Self {
        Self::ObjectLiteral {
            properties: props,
            source_range: None,
            extra_indent: 0,
        }
    }

    /// Create an empty object literal
    pub const fn empty_object() -> Self {
        Self::ObjectLiteral {
            properties: Vec::new(),
            source_range: None,
            extra_indent: 0,
        }
    }

    /// Create an array literal
    pub const fn array(elements: Vec<Self>) -> Self {
        Self::ArrayLiteral(elements)
    }

    /// Create an empty array literal
    pub const fn empty_array() -> Self {
        Self::ArrayLiteral(Vec::new())
    }

    /// Create a logical OR expression: `left || right`
    pub fn logical_or(left: Self, right: Self) -> Self {
        Self::LogicalOr {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a logical AND expression: `left && right`
    pub fn logical_and(left: Self, right: Self) -> Self {
        Self::LogicalAnd {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// Create a sequence of statements
    pub const fn sequence(nodes: Vec<Self>) -> Self {
        Self::Sequence(nodes)
    }
}

impl IRParam {
    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            rest: false,
            default_value: None,
            leading_comment: None,
        }
    }

    pub fn rest(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            name: name.into(),
            rest: true,
            default_value: None,
            leading_comment: None,
        }
    }

    pub fn with_default(mut self, default: IRNode) -> Self {
        self.default_value = Some(Box::new(default));
        self
    }
}

impl IRProperty {
    /// Create a simple property with identifier key: `{ key: value }`
    pub fn init(key: impl Into<Cow<'static, str>>, value: IRNode) -> Self {
        Self {
            key: IRPropertyKey::Identifier(key.into()),
            value,
            kind: IRPropertyKind::Init,
        }
    }
}
