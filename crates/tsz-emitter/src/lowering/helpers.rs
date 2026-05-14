//! Helper methods for the lowering pass.
//!
//! Contains module initialization, modifier checking, helper detection,
//! binding pattern analysis, and this-capture computation.

use super::*;
use crate::emitter::JsxEmit;
use crate::transforms::emit_utils;

impl<'a> LoweringPass<'a> {
    // =========================================================================
    // Helper Methods
    // =========================================================================

    pub(super) fn init_module_state(&mut self, source_file: NodeIndex) {
        let Some(node) = self.arena.get(source_file) else {
            return;
        };
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        self.has_export_assignment = self.contains_export_assignment(&source.statements);
        // AMD/UMD wrapper bodies are processed as CJS (the wrapper provides
        // `exports` parameter), so the lowering pass must produce CommonJSExport
        // directives for them just like it does for CommonJS module kind.
        self.commonjs_mode = if self.ctx.is_commonjs()
            || matches!(self.ctx.options.module, ModuleKind::AMD | ModuleKind::UMD)
        {
            true
        } else if self.ctx.auto_detect_module && matches!(self.ctx.options.module, ModuleKind::None)
        {
            self.file_is_module(&source.statements)
        } else {
            false
        };

        // Pre-scan for `export { Name }` re-exports (without module specifier).
        // These names need the IIFE export fold even though their declarations
        // don't have the `export` keyword directly.
        if self.commonjs_mode {
            self.collect_re_exported_names(&source.statements);
        }
    }

    /// Collect names from `export { Name }` statements (without a module specifier).
    fn collect_re_exported_names(&mut self, statements: &tsz_parser::parser::NodeList) {
        for &stmt_idx in &statements.nodes {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::EXPORT_DECLARATION {
                continue;
            }
            let Some(export_decl) = self.arena.get_export_decl(node) else {
                continue;
            };
            // Only local re-exports (no module specifier)
            if export_decl.module_specifier.is_some() || export_decl.is_type_only {
                continue;
            }
            // The export_clause for `export { A }` is a NAMED_EXPORTS node
            let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
                continue;
            };
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
                // The local name (property_name if aliased, otherwise name)
                let local_name_idx = if spec.property_name.is_some() {
                    spec.property_name
                } else {
                    spec.name
                };
                if let Some(name) = self.get_identifier_text_ref(local_name_idx) {
                    self.re_exported_names.insert(name.to_string());
                }
            }
        }
    }

    pub(super) const fn is_commonjs(&self) -> bool {
        self.commonjs_mode
    }

    /// Check if a modifier list contains the 'const' keyword
    pub(super) fn has_const_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.arena.has_modifier(modifiers, SyntaxKind::ConstKeyword)
    }

    /// Check if a class member (method, property, accessor) is static
    pub(super) fn is_static_member(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };

        let modifiers = match member_node.kind {
            k if k == syntax_kind_ext::METHOD_DECLARATION => self
                .arena
                .get_method_decl(member_node)
                .and_then(|m| m.modifiers.as_ref()),
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT
                || k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT =>
            {
                self.arena
                    .get_property_assignment(member_node)
                    .and_then(|p| p.modifiers.as_ref())
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => self
                .arena
                .get_accessor(member_node)
                .and_then(|a| a.modifiers.as_ref()),
            _ => None,
        };

        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::StaticKeyword as u16)
        })
    }

    pub(super) fn get_extends_heritage(
        &self,
        heritage_clauses: &Option<NodeList>,
    ) -> Option<NodeIndex> {
        let clauses = heritage_clauses.as_ref()?;

        for &clause_idx in &clauses.nodes {
            let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
            if heritage.token == SyntaxKind::ExtendsKeyword as u16 {
                return Some(clause_idx);
            }
        }

        None
    }

    /// Check if a function has the 'async' modifier
    pub(super) fn has_async_modifier(&self, func_idx: NodeIndex) -> bool {
        let Some(func_node) = self.arena.get(func_idx) else {
            return false;
        };

        let Some(func) = self.arena.get_function(func_node) else {
            return false;
        };

        if func.is_async {
            return true;
        }

        let Some(mods) = &func.modifiers else {
            return false;
        };

        mods.nodes.iter().any(|&mod_idx| {
            self.arena
                .get(mod_idx)
                .is_some_and(|n| n.kind == SyntaxKind::AsyncKeyword as u16)
        })
    }

    pub(super) const fn mark_async_helpers(&mut self) {
        let helpers = self.transforms.helpers_mut();
        helpers.awaiter = true;
        // __generator is only needed for ES5 (ES2015+ has native generators)
        if self.ctx.target_es5 {
            helpers.generator = true;
        }
    }

    /// Mark helpers needed for async generator functions (async function*).
    pub(super) const fn mark_async_generator_helpers(&mut self) {
        let helpers = self.transforms.helpers_mut();
        helpers.await_helper = true;
        helpers.async_generator = true;
        if self.ctx.target_es5 {
            helpers.generator = true;
        }
    }

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
            if set_first
                && !helpers.class_private_field_set
                && !helpers.class_private_field_set_before_get
            {
                helpers.class_private_field_set_before_get = true;
            }
            if needs_get {
                helpers.class_private_field_get = true;
            }
            if needs_set {
                helpers.class_private_field_set = true;
            }
            if needs_in {
                helpers.class_private_field_in = true;
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
                    if !tsz_solver::is_assignment_operator(bin.operator_token) {
                        continue;
                    }
                    let left = self.unwrap_parens_and_types(bin.left);
                    if self.is_private_field_access(left) {
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
                        if left == nidx {
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
                    && tsz_solver::is_assignment_operator(bin.operator_token)
                {
                    let left = self.unwrap_parens(bin.left);
                    if self.is_private_field_access(left) {
                        consumed_pa.insert(left.0);
                    }
                }
                if (n.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || n.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
                    && let Some(unary) = self.arena.get_unary_expr(n)
                    && (unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16)
                {
                    let operand = self.unwrap_parens(unary.operand);
                    if self.is_private_field_access(operand) {
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
                    && tsz_solver::is_assignment_operator(bin.operator_token)
                {
                    let left = self.unwrap_parens(bin.left);
                    if self.is_private_field_access(left) {
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
                    if self.is_private_field_access(operand)
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
                    && !tsz_solver::is_assignment_operator(bin.operator_token)
                {
                    // Check if either side has a private field access
                    let left = self.unwrap_parens(bin.left);
                    let right = self.unwrap_parens(bin.right);
                    if (self.is_private_field_access(left) || self.is_private_field_access(right))
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
                    && self.is_private_field_access(nidx)
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

    pub(super) fn has_class_member_modifier(
        &self,
        modifiers: &Option<NodeList>,
        modifier: u16,
    ) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };

        mods.nodes
            .iter()
            .any(|&mod_idx| self.arena.get(mod_idx).is_some_and(|n| n.kind == modifier))
    }

    /// Check if a class has any decorators (class-level or member-level)
    pub(super) fn class_has_decorators(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        // Check class-level decorators
        if let Some(mods) = &class_data.modifiers
            && mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
            })
        {
            return true;
        }
        // Check member-level decorators
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let mods = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .arena
                    .get_method_decl(member_node)
                    .and_then(|m| m.modifiers.as_ref()),
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .arena
                    .get_property_decl(member_node)
                    .and_then(|p| p.modifiers.as_ref()),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.arena
                        .get_accessor(member_node)
                        .and_then(|a| a.modifiers.as_ref())
                }
                _ => None,
            };
            if let Some(mods) = mods
                && mods.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            {
                return true;
            }
        }
        false
    }

    /// Check if a class has any decorated members with computed property names
    pub(super) fn class_has_computed_decorated_member(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let (mods, name_idx) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(m) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (m.modifiers.as_ref(), m.name)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(p) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    (p.modifiers.as_ref(), p.name)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(a) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (a.modifiers.as_ref(), a.name)
                }
                _ => continue,
            };
            // Check if member has decorators
            let has_decorators = mods.is_some_and(|m| {
                m.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            });
            if !has_decorators {
                continue;
            }
            // Check if name is computed (but not a string literal)
            if let Some(name_node) = self.arena.get(name_idx)
                && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                && let Some(computed) = self.arena.get_computed_property(name_node)
                && let Some(expr_node) = self.arena.get(computed.expression)
                && expr_node.kind != SyntaxKind::StringLiteral as u16
            {
                return true;
            }
        }
        false
    }

    /// Check if a class has any decorated private members
    pub(super) fn class_has_private_decorated_member(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            let (mods, name_idx) = match member_node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(m) = self.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    (m.modifiers.as_ref(), m.name)
                }
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(p) = self.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    (p.modifiers.as_ref(), p.name)
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(a) = self.arena.get_accessor(member_node) else {
                        continue;
                    };
                    (a.modifiers.as_ref(), a.name)
                }
                _ => continue,
            };
            let has_decorators = mods.is_some_and(|m| {
                m.nodes.iter().any(|&mod_idx| {
                    self.arena
                        .get(mod_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                })
            });
            if !has_decorators {
                continue;
            }
            if let Some(name_node) = self.arena.get(name_idx)
                && name_node.kind == SyntaxKind::PrivateIdentifier as u16
            {
                return true;
            }
        }
        false
    }

    pub(super) fn needs_es5_object_literal_transform(&self, elements: &[NodeIndex]) -> bool {
        elements.iter().any(|&idx| {
            if emit_utils::is_computed_property_member(self.arena, idx)
                || emit_utils::is_spread_element(self.arena, idx)
            {
                return true;
            }

            let Some(node) = self.arena.get(idx) else {
                return false;
            };

            // Shorthand properties are ES2015+ syntax and don't need lowering for ES2015+ targets
            // Only method declarations need lowering (computed property names are checked above)
            node.kind == syntax_kind_ext::METHOD_DECLARATION
        })
    }

    /// Check if an array literal needs ES5 transformation (has spread elements)
    pub(super) fn needs_es5_array_literal_transform(&self, elements: &[NodeIndex]) -> bool {
        elements
            .iter()
            .any(|&idx| emit_utils::is_spread_element(self.arena, idx))
    }

    pub(super) fn function_parameters_need_es5_transform(&self, params: &NodeList) -> bool {
        params.nodes.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };

            param.dot_dot_dot_token
                || param.initializer.is_some()
                || self.is_binding_pattern_idx(param.name)
        })
    }

    /// Check if function parameters have rest that needs __rest helper.
    /// Only object rest patterns need __rest. Function rest params use arguments loop,
    /// and array rest elements use .`slice()`.
    pub(super) fn function_parameters_need_rest_helper(&self, params: &NodeList) -> bool {
        params.nodes.iter().any(|&param_idx| {
            let Some(param_node) = self.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.arena.get_parameter(param_node) else {
                return false;
            };

            // Function rest parameters (...args) do NOT need __rest helper.
            // They are lowered using an arguments loop, not __rest.

            // Check if binding patterns contain object rest
            if self.is_binding_pattern_idx(param.name) {
                self.binding_pattern_has_object_rest(param.name)
            } else {
                false
            }
        })
    }

    /// Check if a binding pattern (recursively) has an object rest element.
    /// Only object rest patterns need the __rest helper. Array rest uses .`slice()`.
    pub(super) fn binding_pattern_has_object_rest(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
            && node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            return false;
        };

        let Some(pattern) = self.arena.get_binding_pattern(node) else {
            return false;
        };

        let is_object = node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN;

        pattern.elements.nodes.iter().any(|&elem_idx| {
            let Some(elem_node) = self.arena.get(elem_idx) else {
                return false;
            };
            let Some(elem) = self.arena.get_binding_element(elem_node) else {
                return false;
            };
            // Rest in object pattern needs __rest
            if is_object && elem.dot_dot_dot_token {
                return true;
            }
            // Recursively check nested binding patterns
            self.binding_pattern_has_object_rest(elem.name)
        })
    }

    /// Check if an assignment destructuring pattern has object rest.
    ///
    /// Assignment destructuring uses object/array literal nodes rather than
    /// binding-pattern nodes, but it still lowers object rest through `__rest`.
    pub(super) fn assignment_pattern_has_object_rest(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                match elem_node.kind {
                    k if k == syntax_kind_ext::SPREAD_ASSIGNMENT => true,
                    k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                        .arena
                        .get_property_assignment(elem_node)
                        .is_some_and(|prop| {
                            self.assignment_pattern_has_object_rest(prop.initializer)
                        }),
                    _ => false,
                }
            });
        }

        if node.kind == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION {
            let Some(lit) = self.arena.get_literal_expr(node) else {
                return false;
            };
            return lit.elements.nodes.iter().any(|&elem_idx| {
                let Some(elem_node) = self.arena.get(elem_idx) else {
                    return false;
                };
                if elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
                    && let Some(spread) = self.arena.get_spread(elem_node)
                {
                    return self.assignment_pattern_has_object_rest(spread.expression);
                }
                self.assignment_pattern_has_object_rest(elem_idx)
            });
        }

        false
    }

    pub(super) fn is_binding_pattern_idx(&self, idx: NodeIndex) -> bool {
        self.arena.get(idx).is_some_and(|n| n.is_binding_pattern())
    }

    pub(super) fn call_spread_needs_spread_array(&self, args: &[NodeIndex]) -> bool {
        let mut spread_count = 0usize;
        let mut real_arg_count = 0usize;

        for &idx in args {
            if idx.is_none() {
                continue;
            }
            real_arg_count += 1;
            if emit_utils::is_spread_element(self.arena, idx) {
                spread_count += 1;
            }
        }

        // No spread means no spread helper.
        if spread_count == 0 {
            return false;
        }

        // Exactly one spread and no other args: foo(...arr) -> foo.apply(void 0, arr)
        // This does not require __spreadArray.
        if spread_count == 1 && real_arg_count == 1 {
            return false;
        }

        true
    }

    /// Check if a for-of initializer contains binding patterns (destructuring)
    /// Initializer can be `VARIABLE_DECLARATION_LIST` with declarations that have binding patterns
    pub(super) fn for_of_initializer_has_binding_pattern(&self, initializer: NodeIndex) -> bool {
        let Some(init_node) = self.arena.get(initializer) else {
            return false;
        };

        // Check if initializer is a variable declaration list
        if init_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
            && let Some(var_data) = self.arena.get_variable(init_node)
        {
            // Check each declaration in the list
            for &decl_idx in &var_data.declarations.nodes {
                if let Some(decl_node) = self.arena.get(decl_idx)
                    && let Some(decl_data) = self.arena.get_variable_declaration(decl_node)
                    && let Some(name_node) = self.arena.get(decl_data.name)
                {
                    // Check if name is an ARRAY binding pattern
                    // __read helper is only needed for array destructuring, not object destructuring
                    // Object destructuring accesses properties by name, not by iterator position
                    if name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub(super) fn get_identifier_id(&self, idx: NodeIndex) -> Option<IdentifierId> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        Some(node.data_index)
    }

    pub(super) fn get_identifier_text_ref(&self, idx: NodeIndex) -> Option<&str> {
        if idx.is_none() {
            return None;
        }

        let node = self.arena.get(idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let ident = self.arena.get_identifier(node)?;
        Some(&ident.escaped_text)
    }

    pub(super) fn resolve_class_expr_binding_name(&self, class_idx: NodeIndex) -> Option<&str> {
        let mut current = class_idx;
        let mut hops = 0;

        while hops < 8 {
            let parent_idx = self.arena.get_extended(current)?.parent;
            if parent_idx.is_none() {
                return None;
            }
            let parent_node = self.arena.get(parent_idx)?;

            match parent_node.kind {
                syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                    let paren = self.arena.get_parenthesized(parent_node)?;
                    if paren.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::TYPE_ASSERTION
                | syntax_kind_ext::AS_EXPRESSION
                | syntax_kind_ext::SATISFIES_EXPRESSION => {
                    let assertion = self.arena.get_type_assertion(parent_node)?;
                    if assertion.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::NON_NULL_EXPRESSION => {
                    let non_null = self.arena.get_unary_expr_ex(parent_node)?;
                    if non_null.expression != current {
                        return None;
                    }
                    current = parent_idx;
                    hops += 1;
                }
                syntax_kind_ext::VARIABLE_DECLARATION => {
                    let decl = self.arena.get_variable_declaration(parent_node)?;
                    if decl.initializer != current {
                        return None;
                    }
                    return self
                        .get_identifier_text_ref(decl.name)
                        .filter(|name| !name.is_empty());
                }
                syntax_kind_ext::PARAMETER => {
                    let param = self.arena.get_parameter(parent_node)?;
                    if param.initializer != current {
                        return None;
                    }
                    return self
                        .get_identifier_text_ref(param.name)
                        .filter(|name| !name.is_empty());
                }
                syntax_kind_ext::BINARY_EXPRESSION => {
                    let binary = self.arena.get_binary_expr(parent_node)?;
                    if binary.right != current
                        || binary.operator_token != SyntaxKind::EqualsToken as u16
                    {
                        return None;
                    }
                    return self
                        .get_identifier_text_ref(binary.left)
                        .filter(|name| !name.is_empty());
                }
                _ => return None,
            }
        }

        None
    }

    pub(super) fn get_module_root_name(&self, name_idx: NodeIndex) -> Option<IdentifierId> {
        self.get_module_root_name_inner(name_idx, 0)
    }

    pub(super) fn get_module_root_name_inner(
        &self,
        name_idx: NodeIndex,
        depth: u32,
    ) -> Option<IdentifierId> {
        // Stack overflow protection for qualified names
        if depth >= MAX_QUALIFIED_NAME_DEPTH {
            return None;
        }

        if name_idx.is_none() {
            return None;
        }

        let node = self.arena.get(name_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(node.data_index);
        }

        if node.kind == syntax_kind_ext::QUALIFIED_NAME
            && let Some(qn) = self.arena.qualified_names.get(node.data_index as usize)
        {
            return self.get_module_root_name_inner(qn.left, depth + 1);
        }

        None
    }

    /// Get the root name of a module as a String for merging detection
    pub(super) fn get_module_root_name_text(&self, name_idx: NodeIndex) -> Option<String> {
        let id = self.get_module_root_name(name_idx)?;
        let ident = self.arena.identifiers.get(id as usize)?;
        Some(ident.escaped_text.clone())
    }

    pub(super) fn get_block_like(
        &self,
        node: &Node,
    ) -> Option<&tsz_parser::parser::node::BlockData> {
        if node.kind == syntax_kind_ext::BLOCK
            || node.kind == syntax_kind_ext::CASE_BLOCK
            || node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
        {
            self.arena.blocks.get(node.data_index as usize)
        } else {
            None
        }
    }

    pub(super) fn collect_variable_names(&self, declarations: &NodeList) -> Vec<IdentifierId> {
        let mut names = Vec::new();
        for &decl_list_idx in &declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };
            let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                continue;
            };

            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.arena.get(decl_idx) else {
                    continue;
                };
                let Some(decl) = self.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                self.collect_binding_names(decl.name, &mut names);
            }
        }
        names
    }

    pub(super) fn collect_binding_names(&self, name_idx: NodeIndex, names: &mut Vec<IdentifierId>) {
        self.collect_binding_names_inner(name_idx, names, 0);
    }

    pub(super) fn collect_binding_names_inner(
        &self,
        name_idx: NodeIndex,
        names: &mut Vec<IdentifierId>,
        depth: u32,
    ) {
        // Stack overflow protection for deeply nested binding patterns
        if depth >= MAX_BINDING_PATTERN_DEPTH {
            return;
        }

        if name_idx.is_none() {
            return;
        }

        let Some(node) = self.arena.get(name_idx) else {
            return;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            names.push(node.data_index);
            return;
        }

        match node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem_idx in &pattern.elements.nodes {
                        self.collect_binding_names_from_element_inner(elem_idx, names, depth + 1);
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(elem) = self.arena.get_binding_element(node) {
                    self.collect_binding_names_inner(elem.name, names, depth + 1);
                }
            }
            _ => {}
        }
    }

    pub(super) fn collect_binding_names_from_element_inner(
        &self,
        elem_idx: NodeIndex,
        names: &mut Vec<IdentifierId>,
        depth: u32,
    ) {
        // Stack overflow protection
        if depth >= MAX_BINDING_PATTERN_DEPTH {
            return;
        }

        if elem_idx.is_none() {
            return;
        }

        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };

        if let Some(elem) = self.arena.get_binding_element(elem_node) {
            self.collect_binding_names_inner(elem.name, names, depth + 1);
        }
    }

    pub(super) fn maybe_wrap_module(&mut self, source_file: NodeIndex) {
        let format = match self.ctx.options.module {
            ModuleKind::AMD => ModuleFormat::AMD,
            ModuleKind::System => ModuleFormat::System,
            ModuleKind::UMD => ModuleFormat::UMD,
            _ => return,
        };

        let Some(node) = self.arena.get(source_file) else {
            return;
        };
        let Some(source) = self.arena.get_source_file(node) else {
            return;
        };

        if !self.file_is_module(&source.statements) {
            return;
        }

        let dependencies = Arc::from(self.collect_module_dependencies(&source.statements.nodes));
        self.transforms.insert(
            source_file,
            TransformDirective::ModuleWrapper {
                format,
                dependencies,
            },
        );
    }

    pub(super) fn file_is_module(&self, statements: &NodeList) -> bool {
        // moduleDetection=force: treat all non-declaration files as modules
        if self.ctx.options.module_detection_force {
            return true;
        }
        if self.jsx_automatic_runtime_makes_module() {
            return true;
        }
        // Node16/NodeNext resolved to ESM: file is definitively a module
        if self.ctx.options.resolved_node_module_to_esm {
            return true;
        }
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx) {
                match node.kind {
                    k if k == syntax_kind_ext::IMPORT_DECLARATION
                        || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION =>
                    {
                        if let Some(import_decl) = self.arena.get_import_decl(node)
                            && self.import_has_runtime_dependency(import_decl)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::EXPORT_DECLARATION
                        || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                    {
                        // Any export declaration (even ambient / type-only) makes the
                        // file a module.  tsc wraps AMD/UMD/System output even when
                        // all exports are `export declare`.  The runtime-value filter
                        // is for *emitting* exports, not for module detection.
                        return true;
                    }
                    k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                        if let Some(var_stmt) = self.arena.get_variable(node)
                            && self
                                .arena
                                .has_modifier(&var_stmt.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&var_stmt.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        if let Some(func) = self.arena.get_function(node)
                            && self
                                .arena
                                .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&func.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class) = self.arena.get_class(node)
                            && self
                                .arena
                                .has_modifier(&class.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&class.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_decl) = self.arena.get_enum(node)
                            && self
                                .arena
                                .has_modifier(&enum_decl.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&enum_decl.modifiers)
                            && !self.has_const_modifier(&enum_decl.modifiers)
                        {
                            return true;
                        }
                    }
                    k if k == syntax_kind_ext::MODULE_DECLARATION => {
                        if let Some(module) = self.arena.get_module(node)
                            && self
                                .arena
                                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword)
                            && !self.arena.is_declare(&module.modifiers)
                        {
                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    fn jsx_automatic_runtime_makes_module(&self) -> bool {
        if self.ctx.options.module_detection_legacy {
            return false;
        }
        if !matches!(
            self.ctx.options.jsx,
            JsxEmit::ReactJsx | JsxEmit::ReactJsxDev
        ) {
            return false;
        }
        (0..self.arena.len()).any(|idx| {
            self.arena.get(NodeIndex(idx as u32)).is_some_and(|node| {
                node.kind == syntax_kind_ext::JSX_ELEMENT
                    || node.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                    || node.kind == syntax_kind_ext::JSX_FRAGMENT
            })
        })
    }

    pub(super) fn contains_export_assignment(&self, statements: &NodeList) -> bool {
        for &stmt_idx in &statements.nodes {
            if let Some(node) = self.arena.get(stmt_idx)
                && node.kind == syntax_kind_ext::EXPORT_ASSIGNMENT
            {
                return true;
            }
        }
        false
    }

    pub(super) fn collect_module_dependencies(&self, statements: &[NodeIndex]) -> Vec<String> {
        let mut deps = Vec::new();
        for &stmt_idx in statements {
            let Some(node) = self.arena.get(stmt_idx) else {
                continue;
            };

            if node.kind == syntax_kind_ext::IMPORT_DECLARATION
                || node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            {
                if let Some(import_decl) = self.arena.get_import_decl(node) {
                    if !self.import_has_runtime_dependency(import_decl) {
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
                if !self.export_has_runtime_dependency(export_decl) {
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

    pub(super) fn import_has_runtime_dependency(
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
            return self.import_equals_has_external_module(import_decl.module_specifier);
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

    pub(super) fn import_equals_has_external_module(&self, module_specifier: NodeIndex) -> bool {
        if module_specifier.is_none() {
            // require(nonStringLiteral) — specifier failed to parse as string literal,
            // but the `import = require(...)` form still indicates an external module
            return true;
        }

        let Some(node) = self.arena.get(module_specifier) else {
            return true;
        };

        node.kind == SyntaxKind::StringLiteral as u16
    }

    #[allow(dead_code)]
    pub(super) fn export_decl_has_runtime_value(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        crate::transforms::emit_utils::export_decl_has_runtime_value(
            self.arena,
            export_decl,
            self.ctx.options.preserve_const_enums,
        )
    }

    pub(super) fn export_has_runtime_dependency(
        &self,
        export_decl: &tsz_parser::parser::node::ExportDeclData,
    ) -> bool {
        if export_decl.is_type_only {
            return false;
        }

        if export_decl.module_specifier.is_none() {
            return false;
        }

        if export_decl.export_clause.is_none() {
            return true;
        }

        let Some(clause_node) = self.arena.get(export_decl.export_clause) else {
            return true;
        };

        let Some(named) = self.arena.get_named_imports(clause_node) else {
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
}

#[cfg(test)]
#[path = "../../tests/lowering_helpers.rs"]
mod tests;
