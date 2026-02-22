//! AST child iteration dispatch for `ScopeWalker`.
//!
//! This module contains the `for_each_child` method, a large dispatch table
//! that enumerates the direct children of every AST node kind. It is separated
//! from the core scope-resolution logic in `mod.rs` purely for file-size
//! management — the two halves share no state beyond the `ScopeWalker` struct.

use tsz_parser::{NodeIndex, syntax_kind_ext};

use super::ScopeWalker;

impl<'a> ScopeWalker<'a> {
    /// Iterate over direct children of a node using proper typed accessors.
    ///
    /// The callback `f` receives the walker and the child node index.
    /// It should return `Some(T)` to stop iteration with a result, or `None` to continue.
    pub(crate) fn for_each_child<T, F>(&mut self, node_idx: NodeIndex, mut f: F) -> Option<T>
    where
        F: FnMut(&mut Self, NodeIndex) -> Option<T>,
    {
        let node = self.arena.get(node_idx)?;

        match node.kind {
            // --- Source File & Blocks ---
            k if k == syntax_kind_ext::SOURCE_FILE => {
                if let Some(sf) = self.arena.get_source_file(node) {
                    for &stmt in &sf.statements.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BLOCK
                || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                || k == syntax_kind_ext::CASE_BLOCK =>
            {
                if let Some(block) = self.arena.get_block(node) {
                    for &stmt in &block.statements.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_BLOCK => {
                if let Some(mod_block) = self.arena.get_module_block(node)
                    && let Some(ref stmts) = mod_block.statements
                {
                    for &stmt in &stmts.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }

            // --- Declarations ---
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION =>
            {
                if let Some(func) = self.arena.get_function(node) {
                    if let Some(ref modifiers) = func.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if func.name.is_some()
                        && let Some(res) = f(self, func.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = func.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &func.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if func.type_annotation.is_some()
                        && let Some(res) = f(self, func.type_annotation)
                    {
                        return Some(res);
                    }
                    if func.body.is_some()
                        && let Some(res) = f(self, func.body)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                if let Some(method) = self.arena.get_method_decl(node) {
                    if let Some(ref modifiers) = method.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if method.name.is_some()
                        && let Some(res) = f(self, method.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = method.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &method.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if method.type_annotation.is_some()
                        && let Some(res) = f(self, method.type_annotation)
                    {
                        return Some(res);
                    }
                    if method.body.is_some()
                        && let Some(res) = f(self, method.body)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                if let Some(ctor) = self.arena.get_constructor(node) {
                    if let Some(ref modifiers) = ctor.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref type_params) = ctor.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &ctor.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if ctor.body.is_some()
                        && let Some(res) = f(self, ctor.body)
                    {
                        return Some(res);
                    }
                }
            }

            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION =>
            {
                if let Some(class) = self.arena.get_class(node) {
                    if let Some(ref modifiers) = class.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if class.name.is_some()
                        && let Some(res) = f(self, class.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = class.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref heritage) = class.heritage_clauses {
                        for &clause in &heritage.nodes {
                            if let Some(res) = f(self, clause) {
                                return Some(res);
                            }
                        }
                    }
                    for &member in &class.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }

            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var) = self.arena.get_variable(node) {
                    for &decl_list in &var.declarations.nodes {
                        if let Some(res) = f(self, decl_list) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
                if let Some(list) = self.arena.get_variable(node) {
                    for &decl in &list.declarations.nodes {
                        if let Some(res) = f(self, decl) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
                if let Some(decl) = self.arena.get_variable_declaration(node) {
                    if let Some(res) = f(self, decl.name) {
                        return Some(res);
                    }
                    if decl.type_annotation.is_some()
                        && let Some(res) = f(self, decl.type_annotation)
                    {
                        return Some(res);
                    }
                    if decl.initializer.is_some()
                        && let Some(res) = f(self, decl.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PARAMETER => {
                if let Some(param) = self.arena.get_parameter(node) {
                    if let Some(ref modifiers) = param.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, param.name) {
                        return Some(res);
                    }
                    if param.type_annotation.is_some()
                        && let Some(res) = f(self, param.type_annotation)
                    {
                        return Some(res);
                    }
                    if param.initializer.is_some()
                        && let Some(res) = f(self, param.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                if let Some(prop) = self.arena.get_property_decl(node) {
                    if let Some(ref modifiers) = prop.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, prop.name) {
                        return Some(res);
                    }
                    if prop.type_annotation.is_some()
                        && let Some(res) = f(self, prop.type_annotation)
                    {
                        return Some(res);
                    }
                    if prop.initializer.is_some()
                        && let Some(res) = f(self, prop.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::DECORATOR => {
                if let Some(decorator) = self.arena.get_decorator(node)
                    && let Some(res) = f(self, decorator.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(node) {
                    if let Some(ref modifiers) = accessor.modifiers {
                        for &modifier in &modifiers.nodes {
                            if let Some(res) = f(self, modifier) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, accessor.name) {
                        return Some(res);
                    }
                    if let Some(ref type_params) = accessor.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &accessor.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if accessor.type_annotation.is_some()
                        && let Some(res) = f(self, accessor.type_annotation)
                    {
                        return Some(res);
                    }
                    if accessor.body.is_some()
                        && let Some(res) = f(self, accessor.body)
                    {
                        return Some(res);
                    }
                }
            }

            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                if let Some(iface) = self.arena.get_interface(node) {
                    if iface.name.is_some()
                        && let Some(res) = f(self, iface.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = iface.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref heritage) = iface.heritage_clauses {
                        for &clause in &heritage.nodes {
                            if let Some(res) = f(self, clause) {
                                return Some(res);
                            }
                        }
                    }
                    for &member in &iface.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                if let Some(alias) = self.arena.get_type_alias(node) {
                    if alias.name.is_some()
                        && let Some(res) = f(self, alias.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = alias.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if alias.type_node.is_some()
                        && let Some(res) = f(self, alias.type_node)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_decl) = self.arena.get_enum(node) {
                    if enum_decl.name.is_some()
                        && let Some(res) = f(self, enum_decl.name)
                    {
                        return Some(res);
                    }
                    for &member in &enum_decl.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                if let Some(module) = self.arena.get_module(node) {
                    if module.name.is_some()
                        && let Some(res) = f(self, module.name)
                    {
                        return Some(res);
                    }
                    if module.body.is_some()
                        && let Some(res) = f(self, module.body)
                    {
                        return Some(res);
                    }
                }
            }

            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                if let Some(import) = self.arena.get_import_decl(node)
                    && import.import_clause.is_some()
                    && let Some(res) = f(self, import.import_clause)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if let Some(import) = self.arena.get_import_decl(node) {
                    if import.import_clause.is_some()
                        && let Some(res) = f(self, import.import_clause)
                    {
                        return Some(res);
                    }
                    if import.module_specifier.is_some()
                        && let Some(res) = f(self, import.module_specifier)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export) = self.arena.get_export_decl(node)
                    && export.export_clause.is_some()
                    && let Some(res) = f(self, export.export_clause)
                {
                    return Some(res);
                }
            }

            // --- Statements ---
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(stmt) = self.arena.get_if_statement(node) {
                    if let Some(res) = f(self, stmt.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, stmt.then_statement) {
                        return Some(res);
                    }
                    if stmt.else_statement.is_some()
                        && let Some(res) = f(self, stmt.else_statement)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(node)
                    && ret.expression.is_some()
                    && let Some(res) = f(self, ret.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr) = self.arena.get_expression_statement(node)
                    && let Some(res) = f(self, expr.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::FOR_STATEMENT => {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    if loop_data.initializer.is_some()
                        && let Some(res) = f(self, loop_data.initializer)
                    {
                        return Some(res);
                    }
                    if loop_data.condition.is_some()
                        && let Some(res) = f(self, loop_data.condition)
                    {
                        return Some(res);
                    }
                    if loop_data.incrementor.is_some()
                        && let Some(res) = f(self, loop_data.incrementor)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, loop_data.statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                if let Some(for_in_of) = self.arena.get_for_in_of(node) {
                    if let Some(res) = f(self, for_in_of.initializer) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, for_in_of.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, for_in_of.statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT || k == syntax_kind_ext::DO_STATEMENT => {
                if let Some(loop_data) = self.arena.get_loop(node) {
                    if loop_data.condition.is_some()
                        && let Some(res) = f(self, loop_data.condition)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, loop_data.statement) {
                        return Some(res);
                    }
                }
            }

            // --- Expressions ---
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(node) {
                    if let Some(res) = f(self, bin.left) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, bin.right) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION || k == syntax_kind_ext::NEW_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(node) {
                    if let Some(res) = f(self, call.expression) {
                        return Some(res);
                    }
                    if let Some(ref args) = call.arguments {
                        for &arg in &args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION => {
                if node.has_data()
                    && let Some(tagged) = self.arena.tagged_templates.get(node.data_index as usize)
                {
                    if let Some(res) = f(self, tagged.tag) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = tagged.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, tagged.template) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
            {
                if let Some(access) = self.arena.get_access_expr(node) {
                    if let Some(res) = f(self, access.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, access.name_or_argument) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.arena.get_parenthesized(node)
                    && let Some(res) = f(self, paren.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TYPE_ASSERTION
                || k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION =>
            {
                if node.has_data()
                    && let Some(assertion) =
                        self.arena.type_assertions.get(node.data_index as usize)
                {
                    if let Some(res) = f(self, assertion.expression) {
                        return Some(res);
                    }
                    if assertion.type_node.is_some()
                        && let Some(res) = f(self, assertion.type_node)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                || k == syntax_kind_ext::ARRAY_LITERAL_EXPRESSION =>
            {
                if let Some(lit) = self.arena.get_literal_expr(node) {
                    for &elem in &lit.elements.nodes {
                        if let Some(res) = f(self, elem) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_property_assignment(node) {
                    if let Some(res) = f(self, prop.name) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, prop.initializer) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                if let Some(pattern) = self.arena.get_binding_pattern(node) {
                    for &elem in &pattern.elements.nodes {
                        if elem.is_none() {
                            continue;
                        }
                        if let Some(res) = f(self, elem) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::BINDING_ELEMENT => {
                if let Some(binding) = self.arena.get_binding_element(node) {
                    if binding.property_name.is_some()
                        && let Some(prop_node) = self.arena.get(binding.property_name)
                        && prop_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(res) = f(self, binding.property_name)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, binding.name) {
                        return Some(res);
                    }
                    if binding.initializer.is_some()
                        && let Some(res) = f(self, binding.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                if let Some(computed) = self.arena.get_computed_property(node)
                    && let Some(res) = f(self, computed.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_EXPRESSION => {
                if let Some(cond) = self.arena.get_conditional_expr(node) {
                    if let Some(res) = f(self, cond.condition) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.when_true) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.when_false) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = self.arena.get_template_expr(node) {
                    if let Some(res) = f(self, template.head) {
                        return Some(res);
                    }
                    for &span in &template.template_spans.nodes {
                        if let Some(res) = f(self, span) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_SPAN => {
                if let Some(span) = self.arena.get_template_span(node) {
                    if let Some(res) = f(self, span.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, span.literal) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ELEMENT => {
                if let Some(element) = self.arena.get_jsx_element(node) {
                    if let Some(res) = f(self, element.opening_element) {
                        return Some(res);
                    }
                    for &child in &element.children.nodes {
                        if let Some(res) = f(self, child) {
                            return Some(res);
                        }
                    }
                    if let Some(res) = f(self, element.closing_element) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || k == syntax_kind_ext::JSX_OPENING_ELEMENT =>
            {
                if let Some(opening) = self.arena.get_jsx_opening(node) {
                    if let Some(res) = f(self, opening.tag_name) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = opening.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(res) = f(self, opening.attributes) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_CLOSING_ELEMENT => {
                if let Some(closing) = self.arena.get_jsx_closing(node)
                    && let Some(res) = f(self, closing.tag_name)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::JSX_FRAGMENT => {
                if let Some(fragment) = self.arena.get_jsx_fragment(node) {
                    if let Some(res) = f(self, fragment.opening_fragment) {
                        return Some(res);
                    }
                    for &child in &fragment.children.nodes {
                        if let Some(res) = f(self, child) {
                            return Some(res);
                        }
                    }
                    if let Some(res) = f(self, fragment.closing_fragment) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTES => {
                if let Some(attrs) = self.arena.get_jsx_attributes(node) {
                    for &prop in &attrs.properties.nodes {
                        if let Some(res) = f(self, prop) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_ATTRIBUTE => {
                if let Some(attr) = self.arena.get_jsx_attribute(node) {
                    if let Some(res) = f(self, attr.name) {
                        return Some(res);
                    }
                    if attr.initializer.is_some()
                        && let Some(res) = f(self, attr.initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::JSX_SPREAD_ATTRIBUTE => {
                if let Some(spread) = self.arena.get_jsx_spread_attribute(node)
                    && let Some(res) = f(self, spread.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::JSX_EXPRESSION => {
                if let Some(expr) = self.arena.get_jsx_expression(node)
                    && expr.expression.is_some()
                    && let Some(res) = f(self, expr.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::JSX_NAMESPACED_NAME => {
                if let Some(ns) = self.arena.get_jsx_namespaced_name(node) {
                    if let Some(res) = f(self, ns.namespace) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, ns.name) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || k == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION =>
            {
                if let Some(unary) = self.arena.get_unary_expr(node)
                    && let Some(res) = f(self, unary.operand)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::AWAIT_EXPRESSION
                || k == syntax_kind_ext::YIELD_EXPRESSION
                || k == syntax_kind_ext::NON_NULL_EXPRESSION =>
            {
                if node.has_data()
                    && let Some(unary) = self.arena.unary_exprs_ex.get(node.data_index as usize)
                    && let Some(res) = f(self, unary.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                if let Some(prop) = self.arena.get_shorthand_property(node) {
                    if let Some(res) = f(self, prop.name) {
                        return Some(res);
                    }
                    if prop.object_assignment_initializer.is_some()
                        && let Some(res) = f(self, prop.object_assignment_initializer)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::SPREAD_ELEMENT
                || k == syntax_kind_ext::SPREAD_ASSIGNMENT =>
            {
                if let Some(spread) = self.arena.get_spread(node)
                    && let Some(res) = f(self, spread.expression)
                {
                    return Some(res);
                }
            }

            // --- Types ---
            k if k == syntax_kind_ext::HERITAGE_CLAUSE => {
                if let Some(heritage) = self.arena.get_heritage_clause(node) {
                    for &ty in &heritage.types.nodes {
                        if let Some(res) = f(self, ty) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = self.arena.get_expr_type_args(node) {
                    if let Some(res) = f(self, expr.expression) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = expr.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(node) {
                    if let Some(res) = f(self, type_ref.type_name) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = type_ref.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(qualified) = self.arena.get_qualified_name(node) {
                    if let Some(res) = f(self, qualified.left) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, qualified.right) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_QUERY => {
                if let Some(query) = self.arena.get_type_query(node) {
                    if let Some(res) = f(self, query.expr_name) {
                        return Some(res);
                    }
                    if let Some(ref type_args) = query.type_arguments {
                        for &arg in &type_args.nodes {
                            if let Some(res) = f(self, arg) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_OPERATOR => {
                if let Some(op) = self.arena.get_type_operator(node)
                    && let Some(res) = f(self, op.type_node)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TYPE_PREDICATE => {
                if let Some(pred) = self.arena.get_type_predicate(node) {
                    if let Some(res) = f(self, pred.parameter_name) {
                        return Some(res);
                    }
                    if pred.type_node.is_some()
                        && let Some(res) = f(self, pred.type_node)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_PARAMETER => {
                if let Some(param) = self.arena.get_type_parameter(node) {
                    if let Some(res) = f(self, param.name) {
                        return Some(res);
                    }
                    if param.constraint.is_some()
                        && let Some(res) = f(self, param.constraint)
                    {
                        return Some(res);
                    }
                    if param.default.is_some()
                        && let Some(res) = f(self, param.default)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                if let Some(func_type) = self.arena.get_function_type(node) {
                    if let Some(ref type_params) = func_type.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    for &param in &func_type.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if func_type.type_annotation.is_some()
                        && let Some(res) = f(self, func_type.type_annotation)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(literal) = self.arena.get_type_literal(node) {
                    for &member in &literal.members.nodes {
                        if let Some(res) = f(self, member) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE
                || k == syntax_kind_ext::METHOD_SIGNATURE
                || k == syntax_kind_ext::CALL_SIGNATURE
                || k == syntax_kind_ext::CONSTRUCT_SIGNATURE =>
            {
                if let Some(sig) = self.arena.get_signature(node) {
                    if sig.name.is_some()
                        && let Some(res) = f(self, sig.name)
                    {
                        return Some(res);
                    }
                    if let Some(ref type_params) = sig.type_parameters {
                        for &param in &type_params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if let Some(ref params) = sig.parameters {
                        for &param in &params.nodes {
                            if let Some(res) = f(self, param) {
                                return Some(res);
                            }
                        }
                    }
                    if sig.type_annotation.is_some()
                        && let Some(res) = f(self, sig.type_annotation)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                if let Some(sig) = self.arena.get_index_signature(node) {
                    for &param in &sig.parameters.nodes {
                        if let Some(res) = f(self, param) {
                            return Some(res);
                        }
                    }
                    if sig.type_annotation.is_some()
                        && let Some(res) = f(self, sig.type_annotation)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(array) = self.arena.get_array_type(node)
                    && let Some(res) = f(self, array.element_type)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.arena.get_tuple_type(node) {
                    for &elem in &tuple.elements.nodes {
                        if let Some(res) = f(self, elem) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::NAMED_TUPLE_MEMBER => {
                if let Some(member) = self.arena.get_named_tuple_member(node) {
                    if let Some(res) = f(self, member.name) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, member.type_node) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(comp) = self.arena.get_composite_type(node) {
                    for &ty in &comp.types.nodes {
                        if let Some(res) = f(self, ty) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::CONDITIONAL_TYPE => {
                if let Some(cond) = self.arena.get_conditional_type(node) {
                    if let Some(res) = f(self, cond.check_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.extends_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.true_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, cond.false_type) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE
                || k == syntax_kind_ext::OPTIONAL_TYPE
                || k == syntax_kind_ext::REST_TYPE =>
            {
                if let Some(wrapped) = self.arena.get_wrapped_type(node)
                    && let Some(res) = f(self, wrapped.type_node)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::INFER_TYPE => {
                if let Some(infer) = self.arena.get_infer_type(node)
                    && let Some(res) = f(self, infer.type_parameter)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::INDEXED_ACCESS_TYPE => {
                if let Some(indexed) = self.arena.get_indexed_access_type(node) {
                    if let Some(res) = f(self, indexed.object_type) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, indexed.index_type) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped) = self.arena.get_mapped_type(node) {
                    if let Some(res) = f(self, mapped.type_parameter) {
                        return Some(res);
                    }
                    if mapped.name_type.is_some()
                        && let Some(res) = f(self, mapped.name_type)
                    {
                        return Some(res);
                    }
                    if mapped.type_node.is_some()
                        && let Some(res) = f(self, mapped.type_node)
                    {
                        return Some(res);
                    }
                    if let Some(ref members) = mapped.members {
                        for &member in &members.nodes {
                            if let Some(res) = f(self, member) {
                                return Some(res);
                            }
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::LITERAL_TYPE => {
                if let Some(lit) = self.arena.get_literal_type(node)
                    && let Some(res) = f(self, lit.literal)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE => {
                if let Some(template) = self.arena.get_template_literal_type(node) {
                    if let Some(res) = f(self, template.head) {
                        return Some(res);
                    }
                    for &span in &template.template_spans.nodes {
                        if let Some(res) = f(self, span) {
                            return Some(res);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::TEMPLATE_LITERAL_TYPE_SPAN => {
                if let Some(span) = self.arena.get_template_span(node) {
                    if let Some(res) = f(self, span.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, span.literal) {
                        return Some(res);
                    }
                }
            }

            // --- Control Flow ---
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.arena.get_try(node) {
                    if let Some(res) = f(self, try_stmt.try_block) {
                        return Some(res);
                    }
                    if try_stmt.catch_clause.is_some()
                        && let Some(res) = f(self, try_stmt.catch_clause)
                    {
                        return Some(res);
                    }
                    if try_stmt.finally_block.is_some()
                        && let Some(res) = f(self, try_stmt.finally_block)
                    {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CATCH_CLAUSE => {
                if let Some(catch) = self.arena.get_catch_clause(node) {
                    if catch.variable_declaration.is_some()
                        && let Some(res) = f(self, catch.variable_declaration)
                    {
                        return Some(res);
                    }
                    if let Some(res) = f(self, catch.block) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::SWITCH_STATEMENT => {
                if let Some(switch) = self.arena.get_switch(node) {
                    if let Some(res) = f(self, switch.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, switch.case_block) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                if let Some(assign) = self.arena.get_export_assignment(node)
                    && let Some(res) = f(self, assign.expression)
                {
                    return Some(res);
                }
            }
            k if k == syntax_kind_ext::LABELED_STATEMENT => {
                if let Some(labeled) = self.arena.get_labeled_statement(node) {
                    if let Some(res) = f(self, labeled.label) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, labeled.statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::WITH_STATEMENT => {
                if let Some(with_stmt) = self.arena.get_with_statement(node) {
                    if let Some(res) = f(self, with_stmt.expression) {
                        return Some(res);
                    }
                    if let Some(res) = f(self, with_stmt.then_statement) {
                        return Some(res);
                    }
                }
            }
            k if k == syntax_kind_ext::CASE_CLAUSE || k == syntax_kind_ext::DEFAULT_CLAUSE => {
                if let Some(case) = self.arena.get_case_clause(node) {
                    if case.expression.is_some()
                        && let Some(res) = f(self, case.expression)
                    {
                        return Some(res);
                    }
                    for &stmt in &case.statements.nodes {
                        if let Some(res) = f(self, stmt) {
                            return Some(res);
                        }
                    }
                }
            }

            // --- Default: no children or not yet implemented ---
            _ => {}
        }

        None
    }
}
