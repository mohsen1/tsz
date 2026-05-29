//! Class private-field helper discovery for the lowering pass.

use super::*;

impl<'a> LoweringPass<'a> {
    pub(super) fn mark_class_helpers(
        &mut self,
        class_node: NodeIndex,
        heritage: Option<NodeIndex>,
    ) {
        if heritage.is_some() && self.ctx.target_es5 {
            self.transforms.helpers_mut().extends = true;
        }

        let Some(class_node) = self.arena.get(class_node) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(class_node) else {
            return;
        };

        // Private field helpers (__classPrivateFieldGet/__classPrivateFieldSet) are only
        // needed when the target doesn't support native private fields (< ES2022).
        // For ES2022+, private fields use native #field syntax and no helpers are emitted.
        // Auto-accessors always need helpers because the generated getter/setter pair
        // accesses the backing private storage field via WeakMap.
        let has_auto_accessors = self.class_has_auto_accessor_members(class_data);
        let needs_private_lowering = self.ctx.needs_es2022_lowering;

        if needs_private_lowering
            && (has_auto_accessors || self.class_has_private_members(class_data))
        {
            // Compute which helpers are actually needed before taking the mutable borrow.
            let needs_get = has_auto_accessors || self.class_has_private_field_reads(class_data);
            let needs_set = has_auto_accessors || self.class_has_private_field_writes(class_data);
            let needs_in = self.class_has_private_in_expression(class_data);
            // tsc emits helpers in first-use order. If the first private field
            // operation is a write-only assignment, Set comes before Get.
            let set_first = needs_set
                && !has_auto_accessors
                && self.first_private_field_op_is_write_only(class_data);
            let helpers = self.transforms.helpers_mut();
            // Check ordering before setting flags: if Set was never registered
            // and this class has Set-first ordering, mark it
            if set_first && !helpers.class_private_field_get && !helpers.class_private_field_set {
                helpers.class_private_field_set_before_get = true;
            }
            if set_first {
                if needs_set {
                    helpers.mark_class_private_field_set();
                }
                if needs_get {
                    helpers.mark_class_private_field_get();
                }
            } else {
                if needs_get {
                    helpers.mark_class_private_field_get();
                }
                if needs_set {
                    helpers.mark_class_private_field_set();
                }
            }
            if needs_in {
                helpers.mark_class_private_field_in();
            }
        }
    }

    pub(super) fn class_has_private_members(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        if self.class_has_auto_accessor_members(class_data) {
            return true;
        }

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.arena.get_property_decl(member_node)
                        && is_private_identifier(self.arena, prop.name)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.arena.get_method_decl(member_node)
                        && is_private_identifier(self.arena, method.name)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.arena.get_accessor(member_node)
                        && is_private_identifier(self.arena, accessor.name)
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Get the body/initializer node index of a class member.
    /// For methods/constructors/accessors, returns the body.
    /// For property declarations, returns the initializer expression.
    fn get_member_body(&self, member_node: &tsz_parser::parser::node::Node) -> Option<NodeIndex> {
        match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(member_node).and_then(|m| {
                    let body = m.body;
                    if body.0 != 0 { Some(body) } else { None }
                })
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.arena.get_constructor(member_node).and_then(|c| {
                    let body = c.body;
                    if body.0 != 0 { Some(body) } else { None }
                })
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(member_node).and_then(|a| {
                    let body = a.body;
                    if body.0 != 0 { Some(body) } else { None }
                })
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.arena.get_property_decl(member_node).and_then(|p| {
                    let init = p.initializer;
                    if init.is_some() { Some(init) } else { None }
                })
            }
            _ => None,
        }
    }

    /// Get the property-name node of a class member, when the member has one.
    fn get_member_name(&self, member_node: &tsz_parser::parser::node::Node) -> Option<NodeIndex> {
        match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.arena.get_method_decl(member_node).map(|m| m.name)
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.arena.get_property_decl(member_node).map(|p| p.name)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                self.arena.get_accessor(member_node).map(|a| a.name)
            }
            _ => None,
        }
    }

    /// Check if a property access expression accesses a private identifier.
    fn is_private_field_access(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.arena.get_access_expr(node) else {
            return false;
        };
        self.arena
            .get(access.name_or_argument)
            .is_some_and(|name_n| name_n.kind == SyntaxKind::PrivateIdentifier as u16)
    }

    fn private_field_access_name(&self, node_idx: NodeIndex) -> Option<&str> {
        let node = self.arena.get(node_idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let name_node = self.arena.get(access.name_or_argument)?;
        if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
            return None;
        }
        self.arena
            .get_identifier(name_node)
            .map(|ident| ident.escaped_text.as_str())
    }

    fn class_private_member_names(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let name = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|prop| self.private_identifier_name(prop.name)),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|method| self.private_identifier_name(method.name)),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena
                        .get_accessor(member_node)
                        .and_then(|accessor| self.private_identifier_name(accessor.name))
                }
                _ => None,
            };
            if let Some(name) = name {
                names.insert(name.to_owned());
            }
        }
        names
    }

    fn private_identifier_name(&self, name_idx: NodeIndex) -> Option<&str> {
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != SyntaxKind::PrivateIdentifier as u16 {
            return None;
        }
        self.arena
            .get_identifier(name_node)
            .map(|ident| ident.escaped_text.as_str())
    }

    fn is_declared_private_field_access(
        &self,
        node_idx: NodeIndex,
        private_names: &rustc_hash::FxHashSet<String>,
    ) -> bool {
        self.private_field_access_name(node_idx)
            .is_some_and(|name| private_names.contains(name))
    }

    fn assignment_pattern_contains_private_field_access(
        &self,
        pattern_idx: NodeIndex,
        needle: NodeIndex,
    ) -> bool {
        let pattern_idx = self.unwrap_parens_and_types(pattern_idx);
        if pattern_idx == needle && self.is_private_field_access(pattern_idx) {
            return true;
        }

        let Some(node) = self.arena.get(pattern_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.arena.get_binary_expr(node).is_some_and(|binary| {
                    binary.operator_token == SyntaxKind::EqualsToken as u16
                        && self
                            .assignment_pattern_contains_private_field_access(binary.left, needle)
                })
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                self.arena.get_literal_expr(node).is_some_and(|literal| {
                    literal.elements.nodes.iter().any(|&element| {
                        self.assignment_pattern_contains_private_field_access(element, needle)
                    })
                })
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                self.arena.get_binding_pattern(node).is_some_and(|pattern| {
                    pattern.elements.nodes.iter().any(|&element| {
                        self.assignment_pattern_contains_private_field_access(element, needle)
                    })
                })
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .arena
                .get_property_assignment(node)
                .is_some_and(|prop| {
                    self.assignment_pattern_contains_private_field_access(prop.initializer, needle)
                }),
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                self.arena.get_spread(node).is_some_and(|spread| {
                    self.assignment_pattern_contains_private_field_access(spread.expression, needle)
                })
            }
            _ => false,
        }
    }

    fn assignment_pattern_has_private_field_access(&self, pattern_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(pattern_idx) else {
            return false;
        };
        let start = node.pos;
        let end = node.end;
        for i in 0..self.arena.len() {
            let nidx = NodeIndex(i as u32);
            let Some(candidate) = self.arena.get(nidx) else {
                continue;
            };
            if candidate.pos < start || candidate.end > end {
                continue;
            }
            if self.is_private_field_access(nidx)
                && self.assignment_pattern_contains_private_field_access(pattern_idx, nidx)
            {
                return true;
            }
        }
        false
    }

    fn assignment_pattern_contains_declared_private_field_access(
        &self,
        pattern_idx: NodeIndex,
        needle: NodeIndex,
        private_names: &rustc_hash::FxHashSet<String>,
    ) -> bool {
        let pattern_idx = self.unwrap_parens_and_types(pattern_idx);
        if pattern_idx == needle
            && self.is_declared_private_field_access(pattern_idx, private_names)
        {
            return true;
        }

        let Some(node) = self.arena.get(pattern_idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                self.arena.get_binary_expr(node).is_some_and(|binary| {
                    binary.operator_token == SyntaxKind::EqualsToken as u16
                        && self.assignment_pattern_contains_declared_private_field_access(
                            binary.left,
                            needle,
                            private_names,
                        )
                })
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                self.arena.get_literal_expr(node).is_some_and(|literal| {
                    literal.elements.nodes.iter().any(|&element| {
                        self.assignment_pattern_contains_declared_private_field_access(
                            element,
                            needle,
                            private_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                self.arena.get_binding_pattern(node).is_some_and(|pattern| {
                    pattern.elements.nodes.iter().any(|&element| {
                        self.assignment_pattern_contains_declared_private_field_access(
                            element,
                            needle,
                            private_names,
                        )
                    })
                })
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                .arena
                .get_property_assignment(node)
                .is_some_and(|prop| {
                    self.assignment_pattern_contains_declared_private_field_access(
                        prop.initializer,
                        needle,
                        private_names,
                    )
                }),
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                self.arena.get_spread(node).is_some_and(|spread| {
                    self.assignment_pattern_contains_declared_private_field_access(
                        spread.expression,
                        needle,
                        private_names,
                    )
                })
            }
            _ => false,
        }
    }

    fn assignment_pattern_has_declared_private_field_access(
        &self,
        pattern_idx: NodeIndex,
        private_names: &rustc_hash::FxHashSet<String>,
    ) -> bool {
        let Some(node) = self.arena.get(pattern_idx) else {
            return false;
        };
        let start = node.pos;
        let end = node.end;
        for i in 0..self.arena.len() {
            let nidx = NodeIndex(i as u32);
            let Some(candidate) = self.arena.get(nidx) else {
                continue;
            };
            if candidate.pos < start || candidate.end > end {
                continue;
            }
            if self.is_declared_private_field_access(nidx, private_names)
                && self.assignment_pattern_contains_declared_private_field_access(
                    pattern_idx,
                    nidx,
                    private_names,
                )
            {
                return true;
            }
        }
        false
    }

    fn collect_declared_private_field_accesses_in_assignment_pattern(
        &self,
        pattern_idx: NodeIndex,
        consumed: &mut std::collections::HashSet<u32>,
        private_names: &rustc_hash::FxHashSet<String>,
    ) {
        let Some(node) = self.arena.get(pattern_idx) else {
            return;
        };
        let start = node.pos;
        let end = node.end;
        for i in 0..self.arena.len() {
            let nidx = NodeIndex(i as u32);
            let Some(candidate) = self.arena.get(nidx) else {
                continue;
            };
            if candidate.pos < start || candidate.end > end {
                continue;
            }
            if self.is_declared_private_field_access(nidx, private_names)
                && self.assignment_pattern_contains_declared_private_field_access(
                    pattern_idx,
                    nidx,
                    private_names,
                )
            {
                consumed.insert(nidx.0);
            }
        }
    }

    /// Unwrap parenthesized expressions to get the inner expression.
    fn unwrap_parens(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(n) = self.arena.get(idx) else {
                return idx;
            };
            if n.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION {
                return idx;
            }
            let Some(paren) = self.arena.get_parenthesized(n) else {
                return idx;
            };
            idx = paren.expression;
        }
    }

    /// Unwrap parenthesized expressions AND type assertions (as/satisfies/type assertion)
    /// to get the inner runtime expression.
    fn unwrap_parens_and_types(&self, mut idx: NodeIndex) -> NodeIndex {
        loop {
            let Some(n) = self.arena.get(idx) else {
                return idx;
            };
            if n.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                && let Some(paren) = self.arena.get_parenthesized(n)
            {
                idx = paren.expression;
                continue;
            }
            if (n.kind == syntax_kind_ext::TYPE_ASSERTION
                || n.kind == syntax_kind_ext::AS_EXPRESSION
                || n.kind == syntax_kind_ext::SATISFIES_EXPRESSION)
                && let Some(ta) = self.arena.get_type_assertion(n)
            {
                idx = ta.expression;
                continue;
            }
            return idx;
        }
    }

    /// Check if a class has any reads of private fields in its method bodies.
    pub(super) fn class_has_private_field_reads(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if self.ctx.options.legacy_decorators
                && self.member_decorator_expressions_have_private_field_read(member_node)
            {
                return true;
            }
            // Static blocks: scan the block itself (its pos..end covers all statements)
            if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                if self.subtree_has_private_field_read(member_idx) {
                    return true;
                }
                continue;
            }
            if let Some(name) = self.get_member_name(member_node)
                && self.subtree_has_private_field_read(name)
            {
                return true;
            }
            if let Some(body) = self.get_member_body(member_node)
                && self.subtree_has_private_field_read(body)
            {
                return true;
            }
        }
        false
    }

    /// Check if a class has any writes to private fields in its method bodies.
    pub(super) fn class_has_private_field_writes(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if self.ctx.options.legacy_decorators
                && self.member_decorator_expressions_have_private_field_write(member_node)
            {
                return true;
            }
            // Static blocks: scan the block itself
            if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                if self.subtree_has_private_field_write(member_idx) {
                    return true;
                }
                continue;
            }
            if let Some(name) = self.get_member_name(member_node)
                && self.subtree_has_private_field_write(name)
            {
                return true;
            }
            if let Some(body) = self.get_member_body(member_node)
                && self.subtree_has_private_field_write(body)
            {
                return true;
            }
        }
        false
    }

    /// Check if a class has any `#field in obj` expressions.
    pub(super) fn class_has_private_in_expression(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if self.ctx.options.legacy_decorators
                && self.member_decorator_expressions_have_private_in_expression(member_node)
            {
                return true;
            }
            if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                if self.subtree_has_private_in_expression(member_idx) {
                    return true;
                }
                continue;
            }
            if let Some(name) = self.get_member_name(member_node)
                && self.subtree_has_private_in_expression(name)
            {
                return true;
            }
            if let Some(body) = self.get_member_body(member_node)
                && self.subtree_has_private_in_expression(body)
            {
                return true;
            }
        }
        false
    }

    fn member_decorator_expressions_have_private_field_read(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        self.member_decorator_expressions_match(member_node, |this, expr| {
            this.subtree_has_private_field_read(expr)
        })
    }

    fn member_decorator_expressions_have_private_field_write(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        self.member_decorator_expressions_match(member_node, |this, expr| {
            this.subtree_has_private_field_write(expr)
        })
    }

    fn member_decorator_expressions_have_private_in_expression(
        &self,
        member_node: &tsz_parser::parser::node::Node,
    ) -> bool {
        self.member_decorator_expressions_match(member_node, |this, expr| {
            this.subtree_has_private_in_expression(expr)
        })
    }

    fn member_decorator_expressions_match(
        &self,
        member_node: &tsz_parser::parser::node::Node,
        predicate: impl Fn(&Self, NodeIndex) -> bool,
    ) -> bool {
        let (modifiers, parameters): (
            Option<&tsz_parser::parser::NodeList>,
            Option<&tsz_parser::parser::NodeList>,
        ) = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                let Some(method) = self.arena.get_method_decl(member_node) else {
                    return false;
                };
                (method.modifiers.as_ref(), Some(&method.parameters))
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                let Some(prop) = self.arena.get_property_decl(member_node) else {
                    return false;
                };
                (prop.modifiers.as_ref(), None)
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                let Some(accessor) = self.arena.get_accessor(member_node) else {
                    return false;
                };
                (accessor.modifiers.as_ref(), None)
            }
            _ => return false,
        };

        if let Some(modifiers) = modifiers
            && self.decorator_expressions_match(&modifiers.nodes, &predicate)
        {
            return true;
        }

        if let Some(parameters) = parameters {
            for &param_idx in &parameters.nodes {
                let Some(param_node) = self.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.arena.get_parameter(param_node) else {
                    continue;
                };
                if let Some(modifiers) = param.modifiers.as_ref()
                    && self.decorator_expressions_match(&modifiers.nodes, &predicate)
                {
                    return true;
                }
            }
        }

        false
    }

    fn decorator_expressions_match(
        &self,
        modifiers: &[NodeIndex],
        predicate: &impl Fn(&Self, NodeIndex) -> bool,
    ) -> bool {
        modifiers.iter().copied().any(|mod_idx| {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                return false;
            };
            if mod_node.kind != syntax_kind_ext::DECORATOR {
                return false;
            }
            let Some(decorator) = self.arena.get_decorator(mod_node) else {
                return false;
            };
            predicate(self, decorator.expression)
        })
    }

    /// Scan a subtree for `#field in obj` expressions.
    fn subtree_has_private_in_expression(&self, idx: NodeIndex) -> bool {
        let Some(root) = self.arena.get(idx) else {
            return false;
        };
        let start = root.pos;
        let end = root.end;

        for i in 0..self.arena.len() {
            let nidx = NodeIndex(i as u32);
            let Some(n) = self.arena.get(nidx) else {
                continue;
            };
            if n.pos < start || n.end > end {
                continue;
            }
            if n.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(bin) = self.arena.get_binary_expr(n)
                && bin.operator_token == SyntaxKind::InKeyword as u16
                && self
                    .arena
                    .get(bin.left)
                    .is_some_and(|l| l.kind == SyntaxKind::PrivateIdentifier as u16)
            {
                return true;
            }
        }
        false
    }

    /// Scan a subtree for expressions that write to private fields.
    fn subtree_has_private_field_write(&self, idx: NodeIndex) -> bool {
        let Some(root) = self.arena.get(idx) else {
            return false;
        };
        let start = root.pos;
        let end = root.end;

        for i in 0..self.arena.len() {
            let nidx = NodeIndex(i as u32);
            let Some(n) = self.arena.get(nidx) else {
                continue;
            };
            if n.pos < start || n.end > end {
                continue;
            }
            match n.kind {
                k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                    let Some(bin) = self.arena.get_binary_expr(n) else {
                        continue;
                    };
                    if !tsz_solver::operations::compound_assignment::is_assignment_operator(
                        bin.operator_token,
                    ) {
                        continue;
                    }
                    let left = self.unwrap_parens_and_types(bin.left);
                    if self.is_private_field_access(left)
                        || self.assignment_pattern_has_private_field_access(bin.left)
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
                {
                    let Some(unary) = self.arena.get_unary_expr(n) else {
                        continue;
                    };
                    if unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16
                    {
                        let operand = self.unwrap_parens_and_types(unary.operand);
                        if self.is_private_field_access(operand) {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Scan a subtree for expressions that read from private fields.
    fn subtree_has_private_field_read(&self, idx: NodeIndex) -> bool {
        let Some(root) = self.arena.get(idx) else {
            return false;
        };
        let start = root.pos;
        let end = root.end;

        for i in 0..self.arena.len() {
            let nidx = NodeIndex(i as u32);
            let Some(n) = self.arena.get(nidx) else {
                continue;
            };
            if n.pos < start || n.end > end {
                continue;
            }
            if !self.is_private_field_access(nidx) {
                continue;
            }
            // Check if this access is exclusively a write target (LHS of plain `=`).
            let mut is_write_only = false;
            for j in 0..self.arena.len() {
                let parent_idx = NodeIndex(j as u32);
                let Some(parent) = self.arena.get(parent_idx) else {
                    continue;
                };
                if parent.pos > n.pos || parent.end < n.end {
                    continue;
                }
                if parent.kind == syntax_kind_ext::BINARY_EXPRESSION {
                    let Some(bin) = self.arena.get_binary_expr(parent) else {
                        continue;
                    };
                    if bin.operator_token == SyntaxKind::EqualsToken as u16 {
                        // Unwrap parens AND type assertions to find the actual LHS
                        let left = self.unwrap_parens_and_types(bin.left);
                        if left == nidx
                            || self.assignment_pattern_contains_private_field_access(bin.left, nidx)
                        {
                            is_write_only = true;
                            break;
                        }
                    }
                }
            }
            if !is_write_only {
                return true;
            }
        }
        false
    }

    /// Check if the first private field operation in a class is a write-only
    /// assignment (e.g., `this.#field = 1`). Used to determine helper emit order:
    /// tsc emits helpers in first-use order, so if the first op is a write-only
    /// assignment, `__classPrivateFieldSet` should be emitted before `Get`.
    pub(super) fn first_private_field_op_is_write_only(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        // Find the first private field operation by source position
        let mut earliest_pos: Option<u32> = None;
        let mut earliest_is_write_only = false;
        let private_names = self.class_private_member_names(class_data);

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let body = if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                Some(member_idx)
            } else {
                self.get_member_body(member_node)
            };
            let Some(body_idx) = body else {
                continue;
            };
            let Some(body_node) = self.arena.get(body_idx) else {
                continue;
            };
            let start = body_node.pos;
            let end = body_node.end;

            // First pass: collect indices of private-field property accesses that
            // appear as assignment LHS or unary mutation operands. These are
            // already classified by the binary/unary handling and must not be
            // double-counted as standalone reads.
            let mut consumed_pa: std::collections::HashSet<u32> = std::collections::HashSet::new();
            for i in 0..self.arena.len() {
                let nidx = NodeIndex(i as u32);
                let Some(n) = self.arena.get(nidx) else {
                    continue;
                };
                if n.pos < start || n.end > end {
                    continue;
                }
                if n.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && let Some(bin) = self.arena.get_binary_expr(n)
                    && tsz_solver::operations::compound_assignment::is_assignment_operator(
                        bin.operator_token,
                    )
                {
                    let left = self.unwrap_parens(bin.left);
                    if self.is_declared_private_field_access(left, &private_names) {
                        consumed_pa.insert(left.0);
                    }
                    self.collect_declared_private_field_accesses_in_assignment_pattern(
                        bin.left,
                        &mut consumed_pa,
                        &private_names,
                    );
                }
                if (n.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || n.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
                    && let Some(unary) = self.arena.get_unary_expr(n)
                    && (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                {
                    let operand = self.unwrap_parens(unary.operand);
                    if self.is_declared_private_field_access(operand, &private_names) {
                        consumed_pa.insert(operand.0);
                    }
                }
            }

            for i in 0..self.arena.len() {
                let nidx = NodeIndex(i as u32);
                let Some(n) = self.arena.get(nidx) else {
                    continue;
                };
                if n.pos < start || n.end > end {
                    continue;
                }
                // Check for binary expressions with private field on LHS
                if n.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && let Some(bin) = self.arena.get_binary_expr(n)
                    && tsz_solver::operations::compound_assignment::is_assignment_operator(
                        bin.operator_token,
                    )
                {
                    let left = self.unwrap_parens(bin.left);
                    let lhs_has_private_pattern = self
                        .assignment_pattern_has_declared_private_field_access(
                            bin.left,
                            &private_names,
                        );
                    if self.is_declared_private_field_access(left, &private_names)
                        || lhs_has_private_pattern
                    {
                        let is_plain_assign = bin.operator_token == SyntaxKind::EqualsToken as u16;
                        if earliest_pos.is_none() || n.pos < earliest_pos.unwrap() {
                            earliest_pos = Some(n.pos);
                            earliest_is_write_only = is_plain_assign;
                        }
                    }
                }
                // Check for unary mutation (++/--)
                if (n.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || n.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
                    && let Some(unary) = self.arena.get_unary_expr(n)
                    && (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                {
                    let operand = self.unwrap_parens(unary.operand);
                    if self.is_declared_private_field_access(operand, &private_names)
                        && (earliest_pos.is_none() || n.pos < earliest_pos.unwrap())
                    {
                        earliest_pos = Some(n.pos);
                        earliest_is_write_only = false; // ++/-- needs both get and set
                    }
                }
                // Check for non-assignment binary expressions that read private
                // fields (e.g., `this.#field + 1`). For assignment operators, the
                // LHS private field access has the same pos as the binary node
                // and would incorrectly shadow a write-only detection above, so
                // skip those — they're already handled.
                if n.kind == syntax_kind_ext::BINARY_EXPRESSION
                    && let Some(bin) = self.arena.get_binary_expr(n)
                    && !tsz_solver::operations::compound_assignment::is_assignment_operator(
                        bin.operator_token,
                    )
                {
                    // Check if either side has a private field access
                    let left = self.unwrap_parens(bin.left);
                    let right = self.unwrap_parens(bin.right);
                    if (self.is_declared_private_field_access(left, &private_names)
                        || self.is_declared_private_field_access(right, &private_names))
                        && (earliest_pos.is_none() || n.pos < earliest_pos.unwrap())
                    {
                        earliest_pos = Some(n.pos);
                        earliest_is_write_only = false;
                    }
                }
                // Standalone private-field read (e.g., `return this.#foo;`,
                // `f(this.#foo)`, `x.#m()`). Skip ones we already classified as
                // assignment targets or ++/-- operands.
                if n.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && self.is_declared_private_field_access(nidx, &private_names)
                    && !consumed_pa.contains(&nidx.0)
                    && (earliest_pos.is_none() || n.pos < earliest_pos.unwrap())
                {
                    earliest_pos = Some(n.pos);
                    earliest_is_write_only = false;
                }
            }
        }

        earliest_is_write_only
    }

    pub(super) fn class_has_auto_accessor_members(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };

            let Some(prop) = self.arena.get_property_decl(member_node) else {
                continue;
            };

            let has_accessor =
                self.has_class_member_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword as u16);
            if !has_accessor {
                continue;
            }

            if self.has_class_member_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword as u16)
                || self
                    .has_class_member_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword as u16)
            {
                continue;
            }

            // Skip static accessor fields. Static accessor syntax is currently emitted
            // as instance-compatible descriptor assignments and does not require private-field helpers.
            if self.has_class_member_modifier(&prop.modifiers, SyntaxKind::StaticKeyword as u16) {
                continue;
            }

            return true;
        }

        false
    }
}
