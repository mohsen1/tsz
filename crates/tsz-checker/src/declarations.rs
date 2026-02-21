//! Declaration Type Checking
//!
//! Handles classes, interfaces, functions, and variable declarations.
//! This module separates declaration checking logic from the monolithic `CheckerState`.

use super::context::CheckerContext;
use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// Declaration type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All declaration type checking goes through this checker.
pub struct DeclarationChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
}

impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    /// Create a new declaration checker with a mutable context reference.
    pub const fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check if a declaration is ambient (has declare keyword, AMBIENT node flag,
    /// or is inside an ambient context like `declare module`).
    pub(crate) fn is_ambient_declaration(&self, var_idx: NodeIndex) -> bool {
        // .d.ts files are always ambient
        if self.ctx.file_name.ends_with(".d.ts") {
            return true;
        }

        // Check if the node itself has a `declare` modifier
        if let Some(node) = self.ctx.arena.get(var_idx)
            && self.node_has_declare_modifier(var_idx, node)
        {
            return true;
        }

        // Check if the node or any ancestor has the AMBIENT flag or `declare` modifier
        let mut current = var_idx;
        while current.is_some() {
            if let Some(node) = self.ctx.arena.get(current) {
                if (node.flags as u32) & node_flags::AMBIENT != 0 {
                    return true;
                }
                // Check if this ancestor has the `declare` keyword modifier
                // (covers `declare module`, `declare namespace`, `declare class`, etc.)
                if self.node_has_declare_modifier_any(current, node) {
                    return true;
                }

                // Move to parent
                if let Some(ext) = self.ctx.arena.get_extended(current) {
                    current = ext.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        false
    }

    /// Check if a node (class or function declaration) has the `declare` keyword modifier.
    fn node_has_declare_modifier(
        &self,
        _node_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        let modifiers = if let Some(class) = self.ctx.arena.get_class(node) {
            &class.modifiers
        } else if let Some(func) = self.ctx.arena.get_function(node) {
            &func.modifiers
        } else {
            return false;
        };
        self.modifiers_contain_declare(modifiers)
    }

    /// Check if any node type (class, function, module, variable, enum, etc.) has `declare`.
    fn node_has_declare_modifier_any(
        &self,
        _node_idx: NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        // Try each node type that can carry modifiers
        let modifiers = if let Some(class) = self.ctx.arena.get_class(node) {
            &class.modifiers
        } else if let Some(func) = self.ctx.arena.get_function(node) {
            &func.modifiers
        } else if node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Module declarations store modifiers differently
            if let Some(module) = self.ctx.arena.get_module(node) {
                &module.modifiers
            } else {
                return false;
            }
        } else if let Some(var_data) = self.ctx.arena.get_variable(node) {
            &var_data.modifiers
        } else if let Some(enum_decl) = self.ctx.arena.get_enum(node) {
            &enum_decl.modifiers
        } else {
            return false;
        };
        self.modifiers_contain_declare(modifiers)
    }

    /// Check if a modifier list contains the `declare` keyword.
    fn modifiers_contain_declare(&self, modifiers: &Option<tsz_parser::parser::NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::DeclareKeyword as u16
                {
                    return true;
                }
            }
        }
        false
    }

    /// Check a declaration node.
    ///
    /// This dispatches to specialized handlers based on declaration kind.
    /// Currently a skeleton - logic will be migrated incrementally from `CheckerState`.
    pub fn check(&mut self, decl_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        match node.kind {
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.check_variable_statement(decl_idx);
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.check_function_declaration(decl_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.check_class_declaration(decl_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.check_interface_declaration(decl_idx);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.check_type_alias_declaration(decl_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.check_enum_declaration(decl_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.check_module_declaration(decl_idx);
            }
            _ => {
                // Unhandled declaration types - will be expanded incrementally
            }
        }
    }

    /// Check a variable statement.
    pub fn check_variable_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
            for &decl_idx in &var_stmt.declarations.nodes {
                self.check_variable_declaration(decl_idx);
            }
        }
    }

    /// Check a variable declaration list.
    pub fn check_variable_declaration_list(&mut self, list_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(list_idx) else {
            return;
        };

        if let Some(var_list) = self.ctx.arena.get_variable(node) {
            for &decl_idx in &var_list.declarations.nodes {
                self.check_variable_declaration(decl_idx);
            }
        }
    }

    /// Check a variable declaration.
    pub fn check_variable_declaration(&mut self, decl_idx: NodeIndex) {
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let Some(decl_data) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return;
        };

        // TS1155: Check if const declarations must be initialized
        // Get the parent node (VARIABLE_DECLARATION_LIST) via extended info
        let parent_idx = if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
            ext.parent
        } else {
            return;
        };

        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return;
        };

        // Check if this is a const declaration by checking the parent's flags
        let is_const = (parent_node.flags & node_flags::CONST as u16) != 0;

        // TS1155: 'const' declarations must be initialized
        if is_const && decl_data.initializer.is_none() {
            // Skip for destructuring patterns - they get TS1182 from the parser
            if let Some(name_node) = self.ctx.arena.get(decl_data.name) {
                if name_node.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                    || name_node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                {
                    // TS1182 is emitted by parser for destructuring without initializer
                    // Don't also emit TS1155
                } else {
                    // Check if this is in a for-in or for-of loop (allowed)
                    if let Some(parent_ext) = self.ctx.arena.get_extended(parent_idx)
                        && let Some(gp_node) = self.ctx.arena.get(parent_ext.parent)
                        && (gp_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                            || gp_node.kind == syntax_kind_ext::FOR_OF_STATEMENT)
                    {
                        // const in for-in/for-of is allowed without initializer
                        return;
                    }

                    // Check if this is an ambient declaration (allowed)
                    let is_ambient = self.is_ambient_declaration(decl_idx);
                    if is_ambient {
                        return;
                    }

                    self.ctx.error(
                        decl_node.pos,
                        decl_node.end - decl_node.pos,
                        "'const' declarations must be initialized.".to_string(),
                        1155,
                    );
                }
            }
        }

        // Variable declaration checking is handled by CheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Type annotation vs initializer type compatibility
        // - Adding variable to scope
    }

    /// Check a function declaration.
    pub fn check_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.ctx.arena.get_function(node) else {
            return;
        };

        // TS2371: Check for parameter initializers in ambient functions
        // Ambient functions (with 'declare' modifier) cannot have default parameter values
        let has_declare = self.ctx.has_modifier(
            &func.modifiers,
            tsz_scanner::SyntaxKind::DeclareKeyword as u16,
        );

        if has_declare && !func.parameters.nodes.is_empty() {
            for &param_idx in &func.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                // If parameter has an initializer in an ambient function, emit TS2371
                // TSC anchors the error at the parameter name, not the whole parameter.
                if param.initializer.is_some() {
                    let name_node = self.ctx.arena.get(param.name).unwrap_or(param_node);
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        "A parameter initializer is only allowed in a function or constructor implementation.".to_string(),
                        2371, // TS2371
                    );
                }
            }
        }

        // TS1250/TS1251: Function declarations not allowed inside blocks in strict mode
        // when targeting ES3 or ES5
        self.check_strict_mode_function_in_block(func_idx);
    }

    /// TS1250: "Function declarations are not allowed inside blocks in strict mode when targeting 'ES3' or 'ES5'."
    /// TS1251: Same, with "Class definitions are automatically in strict mode."
    fn check_strict_mode_function_in_block(&mut self, func_idx: NodeIndex) {
        // Only applies when targeting ES5 or lower
        if !self.ctx.compiler_options.target.is_es5() {
            return;
        }

        // Check if the function declaration is inside a block that is NOT a function
        // body, source file, or module block. Walk up to find the block scope container.
        let Some(ext) = self.ctx.arena.get_extended(func_idx) else {
            return;
        };
        let parent_idx = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return;
        };

        // The parent must be a Block (curly braces)
        if parent_node.kind != syntax_kind_ext::BLOCK {
            return;
        }

        // Now check the Block's parent — if it's a function-like, source file, or module,
        // then this is a valid position for a function declaration
        let Some(block_ext) = self.ctx.arena.get_extended(parent_idx) else {
            return;
        };
        let block_parent_idx = block_ext.parent;
        let Some(block_parent) = self.ctx.arena.get(block_parent_idx) else {
            return;
        };

        match block_parent.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::CONSTRUCTOR
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR
                || k == syntax_kind_ext::SOURCE_FILE
                || k == syntax_kind_ext::MODULE_DECLARATION
                || k == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION =>
            {
                // Function declaration at a valid scope level, no error
                return;
            }
            _ => {}
        }

        // The function is inside a block (if/while/for/etc.) — check strict mode
        let in_class = self.is_inside_class(func_idx);
        let in_strict = in_class
            || self.ctx.compiler_options.always_strict
            || self.has_use_strict_directive(func_idx);

        if !in_strict {
            return;
        }

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        if in_class {
            self.ctx.error(
                self.ctx.arena.get(func_idx).map_or(0, |n| n.pos),
                self.ctx
                    .arena
                    .get(func_idx)
                    .map_or(0, |n| n.end - n.pos),
                diagnostic_messages::FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_2
                    .to_string(),
                diagnostic_codes::FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_2,
            );
        } else {
            self.ctx.error(
                self.ctx.arena.get(func_idx).map_or(0, |n| n.pos),
                self.ctx
                    .arena
                    .get(func_idx)
                    .map_or(0, |n| n.end - n.pos),
                diagnostic_messages::FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN
                    .to_string(),
                diagnostic_codes::FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN,
            );
        }
    }

    /// Check if a node is inside a class definition (which is always strict mode).
    fn is_inside_class(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            if parent.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return true;
            }
            current = parent_idx;
        }
        false
    }

    /// Check if a "use strict" directive is in effect for a node by walking up to
    /// the nearest function or source file and checking for a "use strict" prologue.
    fn has_use_strict_directive(&self, node_idx: NodeIndex) -> bool {
        let mut current = node_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                return false;
            }
            let Some(parent) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            // Check function bodies and source files for "use strict"
            match parent.kind {
                k if k == syntax_kind_ext::SOURCE_FILE => {
                    return self.source_file_has_use_strict(parent_idx);
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    if let Some(func) = self.ctx.arena.get_function(parent)
                        && self.block_has_use_strict(func.body)
                    {
                        return true;
                    }
                    // Continue walking up — the outer scope might have "use strict"
                }
                _ => {}
            }
            current = parent_idx;
        }
        false
    }

    /// Check if a source file starts with "use strict"
    fn source_file_has_use_strict(&self, sf_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(sf_idx) else {
            return false;
        };
        let Some(sf) = self.ctx.arena.get_source_file(node) else {
            return false;
        };
        self.statements_have_use_strict(&sf.statements.nodes)
    }

    /// Check if a block node starts with "use strict"
    fn block_has_use_strict(&self, block_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(block_idx) else {
            return false;
        };
        let Some(block) = self.ctx.arena.get_block(node) else {
            return false;
        };
        self.statements_have_use_strict(&block.statements.nodes)
    }

    /// Check if a list of statements starts with a "use strict" expression statement
    fn statements_have_use_strict(&self, stmts: &[NodeIndex]) -> bool {
        for &stmt_idx in stmts {
            let Some(stmt) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                break; // Prologues must be at the top
            }
            let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt) else {
                break;
            };
            let Some(expr) = self.ctx.arena.get(expr_stmt.expression) else {
                break;
            };
            if expr.kind == SyntaxKind::StringLiteral as u16 {
                if let Some(lit) = self.ctx.arena.get_literal(expr)
                    && lit.text == "use strict"
                {
                    return true;
                }
            } else {
                break; // Non-string expression, stop looking for prologues
            }
        }
        false
    }

    /// Check a class declaration.
    ///
    /// Handles declaration-specific class checks including TS2564 (strict property initialization).
    pub fn check_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return;
        };
        let Some(class_decl) = self.ctx.arena.get_class(node) else {
            return;
        };

        // Check strict property initialization (TS2564)
        self.check_property_initialization(class_idx, class_decl);
    }

    /// Check property initialization for TS2564.
    ///
    /// Reports errors for class properties that:
    /// - Don't have initializers
    /// - Don't have definite assignment assertions (!)
    /// - Are not assigned in all constructor code paths
    const fn check_property_initialization(
        &mut self,
        _class_idx: NodeIndex,
        _class_decl: &tsz_parser::parser::node::ClassData,
    ) {
        // Canonical TS2564/TS2454 definite-assignment engine lives in
        // `CheckerState` + `query_boundaries::definite_assignment`.
        // DeclarationChecker class checks intentionally delegate through that path
        // in the normal checker pipeline to avoid duplicate algorithms.
    }

    /// Check an interface declaration.
    pub const fn check_interface_declaration(&mut self, _iface_idx: NodeIndex) {
        // Interface declaration checking is handled by CheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Heritage clauses
        // - Member signatures
    }

    /// Check a type alias declaration.
    pub const fn check_type_alias_declaration(&mut self, _alias_idx: NodeIndex) {
        // Type alias checking is handled by CheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Type parameters
        // - Circular reference detection
    }

    /// Check an enum declaration.
    pub fn check_enum_declaration(&mut self, enum_idx: NodeIndex) {
        use crate::diagnostics::diagnostic_codes;
        use tsz_parser::parser::node::NodeAccess;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(enum_idx) else {
            return;
        };

        let Some(enum_data) = self.ctx.arena.get_enum(node) else {
            return;
        };

        // TS2431: Enum name cannot be '{0}'.
        if let Some(name_text) = self.ctx.arena.get_identifier_text(enum_data.name) {
            match name_text {
                "any" | "unknown" | "never" | "number" | "bigint" | "boolean" | "string"
                | "symbol" | "void" | "object" | "undefined" => {
                    let name_node = self.ctx.arena.get(enum_data.name).unwrap();
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        format!("Enum name cannot be '{name_text}'."),
                        diagnostic_codes::ENUM_NAME_CANNOT_BE,
                    );
                }
                _ => {}
            }
        }

        // TS2452: An enum member cannot have a numeric name
        for &member_idx in &enum_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(member_data) = self.ctx.arena.get_enum_member(member_node)
                && let Some(name_node) = self.ctx.arena.get(member_data.name)
                && (name_node.kind == SyntaxKind::NumericLiteral as u16
                    || name_node.kind == SyntaxKind::BigIntLiteral as u16)
            {
                self.ctx.error(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "An enum member cannot have a numeric name.".to_string(),
                    diagnostic_codes::AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME,
                );
            }
        }

        // TS1066: In ambient enum declarations, member initializer must be constant expression
        let is_ambient = self
            .ctx
            .has_modifier(&enum_data.modifiers, SyntaxKind::DeclareKeyword as u16);

        if is_ambient {
            // Check each member's initializer
            for &member_idx in &enum_data.members.nodes {
                if let Some(member_node) = self.ctx.arena.get(member_idx)
                    && let Some(member_data) = self.ctx.arena.get_enum_member(member_node)
                    && member_data.initializer.is_some()
                {
                    // Check if the initializer is a constant expression
                    if !self.is_constant_expression(member_data.initializer)
                        && let Some(init_node) = self.ctx.arena.get(member_data.initializer)
                    {
                        self.ctx.error(
                                init_node.pos,
                                init_node.end - init_node.pos,
                                "In ambient enum declarations member initializer must be constant expression.".to_string(),
                                diagnostic_codes::IN_AMBIENT_ENUM_DECLARATIONS_MEMBER_INITIALIZER_MUST_BE_CONSTANT_EXPRESSION,
                            );
                    }
                }
            }
        }

        // TS1061: Enum member must have initializer
        let mut auto_incrementable = true;
        for &member_idx in &enum_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(member_data) = self.ctx.arena.get_enum_member(member_node)
            {
                if member_data.initializer.is_none() {
                    if !auto_incrementable {
                        let name_node = self.ctx.arena.get(member_data.name).unwrap_or(member_node);
                        self.ctx.error(
                            name_node.pos,
                            name_node.end - name_node.pos,
                            "Enum member must have initializer.".to_string(),
                            diagnostic_codes::ENUM_MEMBER_MUST_HAVE_INITIALIZER,
                        );
                    }
                    auto_incrementable = true;
                } else {
                    auto_incrementable =
                        self.is_numeric_constant_enum_expr(member_data.initializer, enum_data, 0);
                }
            }
        }
    }

    /// Check if an expression is a constant expression for ambient enum members.
    ///
    /// Constant expressions include:
    /// - Literals (numeric, string, boolean, null)
    /// - Identifier references (to other enum members or constants)
    /// - Unary expressions (+, -, ~) on constant expressions
    /// - Binary expressions on constant expressions
    /// - Parenthesized constant expressions
    ///
    /// Property access expressions like 'foo'.length are NOT constant.
    fn is_constant_expression(&self, expr_idx: NodeIndex) -> bool {
        if expr_idx.is_none() {
            return true;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        use tsz_scanner::SyntaxKind;

        match node.kind {
            // Literals are always constant
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            k if k == SyntaxKind::StringLiteral as u16 => true,
            k if k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 => true,
            k if k == SyntaxKind::TrueKeyword as u16 => true,
            k if k == SyntaxKind::FalseKeyword as u16 => true,
            k if k == SyntaxKind::NullKeyword as u16 => true,

            // Identifiers (enum member references) are constant
            k if k == SyntaxKind::Identifier as u16 => true,

            // Template expressions
            k if k == tsz_parser::parser::syntax_kind_ext::TEMPLATE_EXPRESSION => {
                if let Some(template) = self.ctx.arena.get_template_expr(node) {
                    for &span_idx in &template.template_spans.nodes {
                        if let Some(span_node) = self.ctx.arena.get(span_idx)
                            && let Some(span) = self.ctx.arena.get_template_span(span_node)
                            && !self.is_constant_expression(span.expression)
                        {
                            return false;
                        }
                    }
                    true
                } else {
                    false
                }
            }

            // Default case: check using accessor methods
            _ => {
                // Unary expressions: +x, -x, ~x
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    return self.is_constant_expression(unary.operand);
                }

                // Binary expressions: x + y, x * y, etc.
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    return self.is_constant_expression(binary.left)
                        && self.is_constant_expression(binary.right);
                }

                // Parenthesized expressions
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    return self.is_constant_expression(paren.expression);
                }

                // Everything else (including property access) is not constant
                false
            }
        }
    }

    fn is_numeric_constant_enum_expr(
        &self,
        expr_idx: NodeIndex,
        enum_data: &tsz_parser::parser::node::EnumData,
        depth: u32,
    ) -> bool {
        if depth > 100 {
            return false;
        }
        if expr_idx.is_none() {
            return true;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        use tsz_parser::parser::node::NodeAccess;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => true,
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(name_text) = self.ctx.arena.get_identifier_text(expr_idx) {
                    for &member_idx in &enum_data.members.nodes {
                        if let Some(member_node) = self.ctx.arena.get(member_idx)
                            && let Some(member_data) = self.ctx.arena.get_enum_member(member_node)
                            && let Some(member_name_text) =
                                self.ctx.arena.get_identifier_text(member_data.name)
                            && member_name_text == name_text
                        {
                            if member_data.initializer.is_none() {
                                return true;
                            } else {
                                return self.is_numeric_constant_enum_expr(
                                    member_data.initializer,
                                    enum_data,
                                    depth + 1,
                                );
                            }
                        }
                    }
                }
                false
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                if let Some(unary) = self.ctx.arena.get_unary_expr(node) {
                    self.is_numeric_constant_enum_expr(unary.operand, enum_data, depth + 1)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(binary) = self.ctx.arena.get_binary_expr(node) {
                    self.is_numeric_constant_enum_expr(binary.left, enum_data, depth + 1)
                        && self.is_numeric_constant_enum_expr(binary.right, enum_data, depth + 1)
                } else {
                    false
                }
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                if let Some(paren) = self.ctx.arena.get_parenthesized(node) {
                    self.is_numeric_constant_enum_expr(paren.expression, enum_data, depth + 1)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    // Module/namespace declaration checking is in `declarations_module.rs`.
    // Module resolution helpers are in `declarations_module_helpers.rs`.

    /// Check a statement inside an ambient context (declare namespace/module).
    /// Emits TS1036 for non-declaration statements, plus specific errors for
    /// continue (TS1104), return (TS1108), and with (TS2410).
    pub(crate) fn check_statement_in_ambient_context(&mut self, stmt_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };

        // Non-declaration statements are not allowed in ambient contexts
        let is_non_declaration = matches!(
            node.kind,
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT
                || k == syntax_kind_ext::IF_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT
                || k == syntax_kind_ext::BREAK_STATEMENT
                || k == syntax_kind_ext::CONTINUE_STATEMENT
                || k == syntax_kind_ext::RETURN_STATEMENT
                || k == syntax_kind_ext::WITH_STATEMENT
                || k == syntax_kind_ext::SWITCH_STATEMENT
                || k == syntax_kind_ext::THROW_STATEMENT
                || k == syntax_kind_ext::TRY_STATEMENT
                || k == syntax_kind_ext::DEBUGGER_STATEMENT
                || k == syntax_kind_ext::EMPTY_STATEMENT
        );

        if is_non_declaration {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS.to_string(),
                    diagnostic_codes::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                );
            }
        }

        // Additional specific checks for certain statements
        if node.kind == syntax_kind_ext::CONTINUE_STATEMENT {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT.to_string(),
                    diagnostic_codes::A_CONTINUE_STATEMENT_CAN_ONLY_BE_USED_WITHIN_AN_ENCLOSING_ITERATION_STATEMENT,
                );
            }
        }

        if node.kind == syntax_kind_ext::RETURN_STATEMENT {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY
                        .to_string(),
                    diagnostic_codes::A_RETURN_STATEMENT_CAN_ONLY_BE_USED_WITHIN_A_FUNCTION_BODY,
                );
            }
        }

        if node.kind == syntax_kind_ext::WITH_STATEMENT {
            self.check_with_statement(stmt_idx);
        }

        // Ambient declarations still need index-signature parameter validation (TS1268).
        if node.kind == syntax_kind_ext::VARIABLE_STATEMENT {
            self.check_ambient_variable_type_annotations_for_index_signatures(stmt_idx);
        }

        // Check labeled statements — the inner statement should also be checked
        if node.kind == syntax_kind_ext::LABELED_STATEMENT
            && let Some(labeled) = self.ctx.arena.get_labeled_statement(node)
        {
            self.check_label_on_declaration(labeled.label, labeled.statement);
            self.check_statement_in_ambient_context(labeled.statement);
        }
    }

    fn check_ambient_variable_type_annotations_for_index_signatures(
        &mut self,
        stmt_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node) else {
            return;
        };

        for &list_idx in &var_stmt.declarations.nodes {
            let Some(list_node) = self.ctx.arena.get(list_idx) else {
                continue;
            };
            let Some(decl_list) = self.ctx.arena.get_variable(list_node) else {
                continue;
            };
            for &decl_idx in &decl_list.declarations.nodes {
                let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                    continue;
                };
                let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                    continue;
                };
                if var_decl.type_annotation.is_none() {
                    continue;
                }
                let Some(type_node) = self.ctx.arena.get(var_decl.type_annotation) else {
                    continue;
                };
                if type_node.kind != syntax_kind_ext::TYPE_LITERAL {
                    continue;
                }
                let Some(type_lit) = self.ctx.arena.get_type_literal(type_node) else {
                    continue;
                };
                for &member_idx in &type_lit.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                        continue;
                    };
                    let Some(&param_idx) = index_sig.parameters.nodes.first() else {
                        continue;
                    };
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                        continue;
                    };
                    if param.type_annotation.is_none() {
                        continue;
                    }
                    let Some(type_node) = self.ctx.arena.get(param.type_annotation) else {
                        continue;
                    };
                    let is_valid = type_node.kind == tsz_scanner::SyntaxKind::StringKeyword as u16
                        || type_node.kind == tsz_scanner::SyntaxKind::NumberKeyword as u16
                        || type_node.kind == tsz_scanner::SyntaxKind::SymbolKeyword as u16
                        || type_node.kind == syntax_kind_ext::TEMPLATE_LITERAL_TYPE;
                    if !is_valid && let Some((pos, end)) = self.ctx.get_node_span(param_idx) {
                        self.ctx.error(
                            pos,
                            end - pos,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT.to_string(),
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                        );
                    }
                }
            }
        }
    }

    fn is_strict_mode_for_node(&self, idx: NodeIndex) -> bool {
        if self.ctx.compiler_options.always_strict {
            return true;
        }

        // is_external_module check
        if self.is_external_module() {
            return true;
        }

        let statement_is_use_strict = |stmt_idx: NodeIndex| -> bool {
            self.ctx
                .arena
                .get(stmt_idx)
                .filter(|stmt| stmt.kind == syntax_kind_ext::EXPRESSION_STATEMENT)
                .and_then(|stmt| self.ctx.arena.get_expression_statement(stmt))
                .and_then(|expr_stmt| self.ctx.arena.get(expr_stmt.expression))
                .filter(|expr_node| expr_node.kind == SyntaxKind::StringLiteral as u16)
                .and_then(|expr_node| self.ctx.arena.get_literal(expr_node))
                .is_some_and(|lit| lit.text == "use strict")
        };

        let block_has_use_strict = |block_idx: NodeIndex| -> bool {
            let Some(block_node) = self.ctx.arena.get(block_idx) else {
                return false;
            };
            let Some(block) = self.ctx.arena.get_block(block_node) else {
                return false;
            };
            for &stmt_idx in &block.statements.nodes {
                if statement_is_use_strict(stmt_idx) {
                    return true;
                }
                let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                    return false;
                };
                if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                    break;
                }
            }
            false
        };

        let mut current = idx;
        loop {
            let Some(node) = self.ctx.arena.get(current) else {
                return false;
            };

            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return true;
            }

            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };

            match parent_node.kind {
                k if k == syntax_kind_ext::SOURCE_FILE => {
                    if let Some(sf) = self.ctx.arena.get_source_file(parent_node)
                        && sf
                            .statements
                            .nodes
                            .iter()
                            .any(|&stmt_idx| statement_is_use_strict(stmt_idx))
                    {
                        return true;
                    }
                    return false;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::FUNCTION_EXPRESSION
                    || k == syntax_kind_ext::ARROW_FUNCTION =>
                {
                    if let Some(func) = self.ctx.arena.get_function(parent_node)
                        && func.body.is_some()
                        && block_has_use_strict(func.body)
                    {
                        return true;
                    }
                }
                _ => {}
            }

            current = parent;
        }
    }

    fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
            self.ctx.error(
                pos,
                end - pos,
                diagnostic_messages::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A.to_string(),
                diagnostic_codes::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
            );

            if self.is_strict_mode_for_node(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE.to_string(),
                    diagnostic_codes::WITH_STATEMENTS_ARE_NOT_ALLOWED_IN_STRICT_MODE,
                );
            }
        }
    }

    fn check_label_on_declaration(&mut self, label_idx: NodeIndex, statement_idx: NodeIndex) {
        if !self.ctx.compiler_options.target.supports_es2015() {
            return;
        }
        if !self.is_strict_mode_for_node(label_idx) {
            return;
        }

        let Some(stmt_node) = self.ctx.arena.get(statement_idx) else {
            return;
        };

        let is_declaration_or_variable = matches!(
            stmt_node.kind,
            syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::INTERFACE_DECLARATION
                | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::IMPORT_DECLARATION
                | syntax_kind_ext::EXPORT_DECLARATION
                | syntax_kind_ext::VARIABLE_STATEMENT
        );

        if is_declaration_or_variable && let Some((pos, end)) = self.ctx.get_node_span(label_idx) {
            self.ctx.error(
                pos,
                end - pos,
                "'A label is not allowed here.".to_string(),
                1344, // TS1344
            );
        }
    }

    /// Check parameter properties (only valid in constructors).
    pub fn check_parameter_properties(&mut self, parameters: &[NodeIndex]) {
        use crate::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };

            if let Some(param) = self.ctx.arena.get_parameter(node) {
                // If parameter has parameter property modifiers (public/private/protected/readonly)
                // and we're not in a constructor, report error.
                // Decorators on parameters are NOT parameter properties.
                let has_prop_modifier = if let Some(ref mods) = param.modifiers {
                    mods.nodes.iter().any(|&mod_idx| {
                        self.ctx.arena.get(mod_idx).is_some_and(|m| {
                            use tsz_scanner::SyntaxKind;
                            m.kind == SyntaxKind::PublicKeyword as u16
                                || m.kind == SyntaxKind::PrivateKeyword as u16
                                || m.kind == SyntaxKind::ProtectedKeyword as u16
                                || m.kind == SyntaxKind::ReadonlyKeyword as u16
                        })
                    })
                } else {
                    false
                };
                if has_prop_modifier && let Some((pos, end)) = self.ctx.get_node_span(param_idx) {
                    self.ctx.error(
                        pos,
                        end - pos,
                        "A parameter property is only allowed in a constructor implementation."
                            .to_string(),
                        diagnostic_codes::A_PARAMETER_PROPERTY_IS_ONLY_ALLOWED_IN_A_CONSTRUCTOR_IMPLEMENTATION,
                    );
                }
            }
        }
    }

    /// Check function implementations for overload sequences.
    pub const fn check_function_implementations(&mut self, _nodes: &[NodeIndex]) {
        // Implementation of overload checking
        // Will be migrated from CheckerState
    }
}

#[cfg(test)]
#[path = "../tests/declarations.rs"]
mod tests;
