//! Declaration Type Checking
//!
//! Handles classes, interfaces, functions, and variable declarations.
//! This module separates declaration checking logic from the monolithic `CheckerState`.

use super::context::CheckerContext;
use std::path::{Component, Path, PathBuf};
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
    fn is_ambient_declaration(&self, var_idx: NodeIndex) -> bool {
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
        while !current.is_none() {
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
                if !param.initializer.is_none() {
                    self.ctx.error(
                        param_node.pos,
                        param_node.end - param_node.pos,
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
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(enum_idx) else {
            return;
        };

        let Some(enum_data) = self.ctx.arena.get_enum(node) else {
            return;
        };

        // TS2452: An enum member cannot have a numeric name
        for &member_idx in &enum_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(member_data) = self.ctx.arena.get_enum_member(member_node)
                && let Some(name_node) = self.ctx.arena.get(member_data.name)
                && name_node.kind == SyntaxKind::NumericLiteral as u16
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
                    && !member_data.initializer.is_none()
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
            k if k == SyntaxKind::TrueKeyword as u16 => true,
            k if k == SyntaxKind::FalseKeyword as u16 => true,
            k if k == SyntaxKind::NullKeyword as u16 => true,

            // Identifiers (enum member references) are constant
            k if k == SyntaxKind::Identifier as u16 => true,

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

    /// Check a module/namespace declaration.
    pub fn check_module_declaration(&mut self, module_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_binder::symbol_flags;
        use tsz_parser::parser::node_flags;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(module_idx) else {
            return;
        };

        if let Some(module) = self.ctx.arena.get_module(node) {
            // TS2580: Anonymous module declaration with `module` keyword (not `namespace`)
            // When `module {` is parsed as a module declaration with a missing name,
            // TSC also emits TS2580 because `module` could be a Node.js identifier reference.
            let is_namespace = (node.flags as u32) & node_flags::NAMESPACE != 0;

            if !is_namespace
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                && ident.escaped_text.is_empty()
            {
                // Detailed node types error (TS2591) is preferred in recent TS versions.
                let code =
                    diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2;
                let message = format_message(
                    diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE_2,
                    &["module"],
                );

                self.ctx.error(node.pos, 6, message, code);
            }

            // TS2668: 'export' modifier cannot be applied to ambient modules
            // This only applies to string-literal-named ambient modules (declare module "foo"),
            // not to namespace-form modules (declare namespace Foo)
            // Check this FIRST before early returns so we can emit multiple errors
            let has_declare = self
                .ctx
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword as u16);
            let has_export = self
                .ctx
                .has_modifier(&module.modifiers, SyntaxKind::ExportKeyword as u16);

            // Only check for TS2668 if this is a string-literal-named module
            let is_string_named = if let Some(name_node) = self.ctx.arena.get(module.name) {
                name_node.kind == SyntaxKind::StringLiteral as u16
                    || name_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            } else {
                false
            };

            if has_declare && has_export && is_string_named {
                // Find the export modifier position to report error there
                if let Some(ref mods) = module.modifiers {
                    for &mod_idx in &mods.nodes {
                        if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                            && mod_node.kind == SyntaxKind::ExportKeyword as u16
                        {
                            self.ctx.error(
                                    mod_node.pos,
                                    mod_node.end - mod_node.pos,
                                    "'export' modifier cannot be applied to ambient modules and module augmentations since they are always visible.".to_string(),
                                    2668, // TS2668
                                );
                            break;
                        }
                    }
                }
            }

            // TS2669/TS2670: Global scope augmentations must be directly nested in
            // external modules or ambient module declarations, and should have `declare`
            let is_global_augmentation = (node.flags as u32) & node_flags::GLOBAL_AUGMENTATION != 0
                || self
                    .ctx
                    .arena
                    .get(module.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|ident| ident.escaped_text == "global");
            if is_global_augmentation {
                let mut allowed_context = false;
                if let Some(ext) = self.ctx.arena.get_extended(module_idx) {
                    let parent = ext.parent;
                    if !parent.is_none()
                        && let Some(parent_node) = self.ctx.arena.get(parent)
                    {
                        if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                            allowed_context = self.is_external_module();
                        } else if parent_node.kind == syntax_kind_ext::MODULE_BLOCK
                            && let Some(parent_ext) = self.ctx.arena.get_extended(parent)
                        {
                            let gp = parent_ext.parent;
                            if let Some(gp_node) = self.ctx.arena.get(gp)
                                && gp_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                && let Some(gp_module) = self.ctx.arena.get_module(gp_node)
                                && self.ctx.has_modifier(
                                    &gp_module.modifiers,
                                    SyntaxKind::DeclareKeyword as u16,
                                )
                            {
                                let gp_name_node = self.ctx.arena.get(gp_module.name);
                                let gp_is_string_named = gp_name_node.is_some_and(|name_node| {
                                    name_node.kind == SyntaxKind::StringLiteral as u16
                                        || name_node.kind
                                            == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                                });
                                if gp_is_string_named {
                                    allowed_context = true;
                                }
                            }
                        }
                    }
                }

                let error_node = self.ctx.arena.get(module.name).unwrap_or(node);
                if !allowed_context {
                    self.ctx.error(
                        error_node.pos,
                        error_node.end - error_node.pos,
                        diagnostic_messages::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL.to_string(),
                        diagnostic_codes::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_CAN_ONLY_BE_DIRECTLY_NESTED_IN_EXTERNAL_MODUL,
                    );
                }
                if !has_declare && !self.is_in_ambient_context(module_idx) {
                    self.ctx.error(
                        error_node.pos,
                        error_node.end - error_node.pos,
                        diagnostic_messages::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_SHOULD_HAVE_DECLARE_MODIFIER_UNLESS_THEY_APPE.to_string(),
                        diagnostic_codes::AUGMENTATIONS_FOR_THE_GLOBAL_SCOPE_SHOULD_HAVE_DECLARE_MODIFIER_UNLESS_THEY_APPE,
                    );
                }
            }

            // TS2433/TS2434: Check namespace merging with class/function
            // A namespace declaration cannot be in a different file from a class/function
            // with which it is merged (TS2433), or located prior to the class/function (TS2434).
            // Only check for non-ambient, non-string-named, INSTANTIATED modules.
            // Uninstantiated namespaces (containing only interfaces/type aliases) are allowed
            // to precede a class/function they merge with.
            if !has_declare
                && !is_string_named
                && !module.body.is_none()
                && !self.is_in_ambient_context(module_idx)
                && self.is_namespace_declaration_instantiated(module_idx)
            {
                self.check_namespace_merges_with_class_or_function(module_idx, module);
            }

            // TS1035: Only ambient modules can use quoted names.
            // `module "Foo" {}` without `declare` is invalid.
            if !has_declare
                && is_string_named
                && let Some(name_node) = self.ctx.arena.get(module.name)
            {
                self.ctx.error(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    diagnostic_messages::ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES.to_string(),
                    diagnostic_codes::ONLY_AMBIENT_MODULES_CAN_USE_QUOTED_NAMES,
                );
            }

            // TS2435: Ambient modules cannot be nested in other modules or namespaces
            // Check if this is an ambient external module (declare module "string")
            // inside another namespace/module
            if let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::StringLiteral as u16
            {
                // This is an ambient external module with a string name
                // Check if it's nested inside a namespace
                if self.is_inside_namespace(module_idx) {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        "Ambient modules cannot be nested in other modules or namespaces."
                            .to_string(),
                        diagnostic_codes::AMBIENT_MODULES_CANNOT_BE_NESTED_IN_OTHER_MODULES_OR_NAMESPACES,
                    );
                    return; // Don't emit other errors for nested ambient modules
                }
            }

            // TS1235: A namespace declaration is only allowed at the top level of a namespace or module.
            // This applies to non-string-named module/namespace declarations that are inside labeled statements
            // or other non-module constructs.
            if !is_string_named {
                // Check if the parent is a valid context
                // Valid parents:
                // - SourceFile (top-level namespace)
                // - ModuleBlock (namespace inside namespace body)
                // - ModuleDeclaration (dotted namespace like namespace A.B { })
                // - ExportDeclaration (export namespace X { })
                let is_valid_context = if let Some(ext) = self.ctx.arena.get_extended(module_idx) {
                    let parent = ext.parent;
                    if parent.is_none() {
                        true // Top level is valid
                    } else if let Some(parent_node) = self.ctx.arena.get(parent) {
                        // Valid parents: SourceFile, ModuleBlock, ModuleDeclaration
                        if parent_node.kind == syntax_kind_ext::SOURCE_FILE
                            || parent_node.kind == syntax_kind_ext::MODULE_BLOCK
                            || parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        {
                            true
                        } else if parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            // Check if the export declaration is inside a valid context
                            if let Some(parent_ext) = self.ctx.arena.get_extended(parent) {
                                let grandparent = parent_ext.parent;
                                if let Some(gp_node) = self.ctx.arena.get(grandparent) {
                                    gp_node.kind == syntax_kind_ext::SOURCE_FILE
                                        || gp_node.kind == syntax_kind_ext::MODULE_BLOCK
                                        || gp_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                } else {
                                    true
                                }
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    } else {
                        true
                    }
                } else {
                    true
                };

                if !is_valid_context && let Some(name_node) = self.ctx.arena.get(module.name) {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        diagnostic_messages::A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODUL.to_string(),
                        diagnostic_codes::A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODUL,
                    );
                }
            }

            // TS5061: Check for relative module names in ambient declarations
            // declare module "./foo" { } -> Error (only in script/non-module files)
            // In module files, `declare module "./foo"` is a module augmentation, not
            // an ambient module declaration, and relative paths are valid.
            if self
                .ctx
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword as u16)
                && let Some(name_node) = self.ctx.arena.get(module.name)
                && name_node.kind == SyntaxKind::StringLiteral as u16
                && let Some(lit) = self.ctx.arena.get_literal(name_node)
            {
                // Check TS5061 first - only for true ambient declarations (non-module files)
                if self.is_relative_module_name(&lit.text) && !self.is_external_module() {
                    self.ctx.error(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    diagnostic_messages::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME.to_string(),
                                    diagnostic_codes::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME,
                                );
                }
                // TS2664: Check if the module being augmented exists
                // declare module "nonexistent" { } -> Error if module doesn't exist
                // Only emit TS2664 if:
                // 1. The file is a module file (has import/export statements)
                // 2. The file is not a .d.ts file
                // 3. The module name is not a relative path (relative augmentations
                //    refer to local files which may not be resolved in all contexts)
                // In script files (no imports/exports), declare module "xxx" declares
                // an ambient external module, which is always valid.
                else if !self.module_exists(&lit.text)
                    && !self.is_declaration_file()
                    && self.is_external_module()
                    && !self.is_relative_module_name(&lit.text)
                {
                    let message = format_message(
                        diagnostic_messages::INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND,
                        &[&lit.text],
                    );
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        message,
                        diagnostic_codes::INVALID_MODULE_NAME_IN_AUGMENTATION_MODULE_CANNOT_BE_FOUND,
                    );
                } else if self.is_external_module()
                    && self.module_exists(&lit.text)
                    && self.ctx.module_resolves_to_non_module_entity(&lit.text)
                {
                    let has_value_exports = self.module_augmentation_has_value_exports(module.body);
                    let (code, message) = if has_value_exports {
                        (
                            diagnostic_codes::CANNOT_AUGMENT_MODULE_WITH_VALUE_EXPORTS_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENT,
                            format_message(
                                diagnostic_messages::CANNOT_AUGMENT_MODULE_WITH_VALUE_EXPORTS_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENT,
                                &[&lit.text],
                            ),
                        )
                    } else {
                        (
                            diagnostic_codes::CANNOT_AUGMENT_MODULE_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENTITY,
                            format_message(
                                diagnostic_messages::CANNOT_AUGMENT_MODULE_BECAUSE_IT_RESOLVES_TO_A_NON_MODULE_ENTITY,
                                &[&lit.text],
                            ),
                        )
                    };
                    self.ctx
                        .error(name_node.pos, name_node.end - name_node.pos, message, code);
                }
            }

            // TS2666/TS2667: Imports/exports are not permitted in module augmentations
            if has_declare && is_string_named && self.is_external_module() {
                let module_specifier = self
                    .ctx
                    .arena
                    .get(module.name)
                    .and_then(|name_node| self.ctx.arena.get_literal(name_node))
                    .map(|lit| lit.text.clone());
                let module_key = module_specifier.as_deref().map_or_else(
                    || "<unknown>".to_string(),
                    |spec| self.normalize_module_augmentation_key(spec),
                );

                let mut value_decl_map = self
                    .ctx
                    .module_augmentation_value_decls
                    .remove(&module_key)
                    .unwrap_or_default();
                let mut reported_import = false;
                let mut reported_export = false;
                if !module.body.is_none()
                    && let Some(body_node) = self.ctx.arena.get(module.body)
                    && body_node.kind == syntax_kind_ext::MODULE_BLOCK
                    && let Some(block) = self.ctx.arena.get_module_block(body_node)
                    && let Some(ref stmts) = block.statements
                {
                    let mut register_value_name = |name: &str, name_node: NodeIndex| -> bool {
                        if value_decl_map.contains_key(name) {
                            true
                        } else {
                            value_decl_map.insert(name.to_string(), name_node);
                            false
                        }
                    };
                    for &stmt_idx in &stmts.nodes {
                        if reported_import && reported_export {
                            break;
                        }
                        let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                            continue;
                        };
                        let kind = stmt_node.kind;
                        if !reported_import
                            && (kind == syntax_kind_ext::IMPORT_DECLARATION
                                || kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
                        {
                            self.ctx.error(
                                            stmt_node.pos,
                                            stmt_node.end - stmt_node.pos,
                                            diagnostic_messages::IMPORTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS_CONSIDER_MOVING_THEM_TO_THE_EN.to_string(),
                                            diagnostic_codes::IMPORTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS_CONSIDER_MOVING_THEM_TO_THE_EN,
                                        );
                            reported_import = true;
                        } else if !reported_export {
                            let is_forbidden_export = if kind == syntax_kind_ext::EXPORT_ASSIGNMENT
                                || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                            {
                                true
                            } else if kind == syntax_kind_ext::EXPORT_DECLARATION {
                                match self.ctx.arena.get_export_decl(stmt_node) {
                                    Some(export_decl) => {
                                        if export_decl.is_default_export {
                                            true
                                        } else if !export_decl.module_specifier.is_none() {
                                            // Re-exports are not permitted in augmentations
                                            true
                                        } else if export_decl.export_clause.is_none() {
                                            true
                                        } else if let Some(clause_node) =
                                            self.ctx.arena.get(export_decl.export_clause)
                                        {
                                            !matches!(
                                                clause_node.kind,
                                                syntax_kind_ext::FUNCTION_DECLARATION
                                                    | syntax_kind_ext::CLASS_DECLARATION
                                                    | syntax_kind_ext::INTERFACE_DECLARATION
                                                    | syntax_kind_ext::TYPE_ALIAS_DECLARATION
                                                    | syntax_kind_ext::ENUM_DECLARATION
                                                    | syntax_kind_ext::MODULE_DECLARATION
                                                    | syntax_kind_ext::VARIABLE_STATEMENT
                                            )
                                        } else {
                                            true
                                        }
                                    }
                                    None => true,
                                }
                            } else {
                                false
                            };
                            if is_forbidden_export {
                                self.ctx.error(
                                                stmt_node.pos,
                                                stmt_node.end - stmt_node.pos,
                                                diagnostic_messages::EXPORTS_AND_EXPORT_ASSIGNMENTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS.to_string(),
                                                diagnostic_codes::EXPORTS_AND_EXPORT_ASSIGNMENTS_ARE_NOT_PERMITTED_IN_MODULE_AUGMENTATIONS,
                                            );
                                reported_export = true;
                            }
                        }

                        if kind == syntax_kind_ext::EXPORT_DECLARATION {
                            let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node)
                            else {
                                continue;
                            };
                            if export_decl.is_default_export
                                || !export_decl.module_specifier.is_none()
                                || export_decl.export_clause.is_none()
                            {
                                continue;
                            }
                            let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause)
                            else {
                                continue;
                            };
                            match clause_node.kind {
                                syntax_kind_ext::VARIABLE_STATEMENT => {
                                    if let Some(var_stmt) = self.ctx.arena.get_variable(clause_node)
                                    {
                                        for &decl_list_idx in &var_stmt.declarations.nodes {
                                            let Some(decl_list_node) =
                                                self.ctx.arena.get(decl_list_idx)
                                            else {
                                                continue;
                                            };
                                            if decl_list_node.kind
                                                == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                                            {
                                                if let Some(decl_list) =
                                                    self.ctx.arena.get_variable(decl_list_node)
                                                {
                                                    for &decl_idx in &decl_list.declarations.nodes {
                                                        if let Some(decl_node) =
                                                            self.ctx.arena.get(decl_idx)
                                                            && let Some(decl) = self
                                                                .ctx
                                                                .arena
                                                                .get_variable_declaration(decl_node)
                                                            && let Some(name_node) =
                                                                self.ctx.arena.get(decl.name)
                                                            && let Some(ident) = self
                                                                .ctx
                                                                .arena
                                                                .get_identifier(name_node)
                                                            && register_value_name(
                                                                &ident.escaped_text,
                                                                decl.name,
                                                            )
                                                            && let Some(node) =
                                                                self.ctx.arena.get(decl.name)
                                                        {
                                                            self.ctx.error(
                                                                                    node.pos,
                                                                                    node.end - node.pos,
                                                                                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                                                );
                                                        }
                                                    }
                                                }
                                            } else if let Some(decl) = self
                                                .ctx
                                                .arena
                                                .get_variable_declaration(decl_list_node)
                                                && let Some(name_node) =
                                                    self.ctx.arena.get(decl.name)
                                                && let Some(ident) =
                                                    self.ctx.arena.get_identifier(name_node)
                                                && register_value_name(
                                                    &ident.escaped_text,
                                                    decl.name,
                                                )
                                                && let Some(node) = self.ctx.arena.get(decl.name)
                                            {
                                                self.ctx.error(
                                                                        node.pos,
                                                                        node.end - node.pos,
                                                                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                        diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                                    );
                                            }
                                        }
                                    }
                                }
                                syntax_kind_ext::FUNCTION_DECLARATION => {
                                    if let Some(func) = self.ctx.arena.get_function(clause_node)
                                        && let Some(name_node) = self.ctx.arena.get(func.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                        && register_value_name(&ident.escaped_text, func.name)
                                        && let Some(node) = self.ctx.arena.get(func.name)
                                    {
                                        self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                    }
                                }
                                syntax_kind_ext::CLASS_DECLARATION => {
                                    if let Some(class) = self.ctx.arena.get_class(clause_node)
                                        && let Some(name_node) = self.ctx.arena.get(class.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                        && register_value_name(&ident.escaped_text, class.name)
                                        && let Some(node) = self.ctx.arena.get(class.name)
                                    {
                                        self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                    }
                                }
                                syntax_kind_ext::ENUM_DECLARATION => {
                                    if let Some(enm) = self.ctx.arena.get_enum(clause_node)
                                        && let Some(name_node) = self.ctx.arena.get(enm.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                    {
                                        if let Some(specifier) = module_specifier.as_deref()
                                            && let Some(target_idx) =
                                                self.ctx.resolve_import_target(specifier)
                                            && let Some(target_binder) =
                                                self.ctx.get_binder_for_file(target_idx)
                                        {
                                            let target_arena =
                                                self.ctx.get_arena_for_file(target_idx as u32);
                                            if let Some(source_file) =
                                                target_arena.source_files.first()
                                                && let Some(existing_sym_id) = target_binder
                                                    .resolve_import_if_needed_public(
                                                        &source_file.file_name,
                                                        &ident.escaped_text,
                                                    )
                                                && let Some(symbol) =
                                                    target_binder.get_symbol(existing_sym_id)
                                            {
                                                let allowed = (symbol.flags
                                                    & (symbol_flags::REGULAR_ENUM
                                                        | symbol_flags::CONST_ENUM
                                                        | symbol_flags::MODULE))
                                                    != 0;
                                                if !allowed {
                                                    self.ctx.error(
                                                                        name_node.pos,
                                                                        name_node.end
                                                                            - name_node.pos,
                                                                        diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS.to_string(),
                                                                        diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                                                                    );
                                                }
                                            }
                                        }
                                        if register_value_name(&ident.escaped_text, enm.name)
                                            && let Some(node) = self.ctx.arena.get(enm.name)
                                        {
                                            self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                        }
                                    }
                                }
                                _ => {}
                            }
                        } else if kind == syntax_kind_ext::VARIABLE_STATEMENT {
                            if let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node)
                                && self.ctx.has_modifier(
                                    &var_stmt.modifiers,
                                    SyntaxKind::ExportKeyword as u16,
                                )
                            {
                                for &decl_list_idx in &var_stmt.declarations.nodes {
                                    let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx)
                                    else {
                                        continue;
                                    };
                                    if decl_list_node.kind
                                        == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                                    {
                                        if let Some(decl_list) =
                                            self.ctx.arena.get_variable(decl_list_node)
                                        {
                                            for &decl_idx in &decl_list.declarations.nodes {
                                                if let Some(decl_node) =
                                                    self.ctx.arena.get(decl_idx)
                                                    && let Some(decl) = self
                                                        .ctx
                                                        .arena
                                                        .get_variable_declaration(decl_node)
                                                    && let Some(name_node) =
                                                        self.ctx.arena.get(decl.name)
                                                    && let Some(ident) =
                                                        self.ctx.arena.get_identifier(name_node)
                                                    && register_value_name(
                                                        &ident.escaped_text,
                                                        decl.name,
                                                    )
                                                    && let Some(node) =
                                                        self.ctx.arena.get(decl.name)
                                                {
                                                    self.ctx.error(
                                                                            node.pos,
                                                                            node.end - node.pos,
                                                                            diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                            diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                                        );
                                                }
                                            }
                                        }
                                    } else if let Some(decl) =
                                        self.ctx.arena.get_variable_declaration(decl_list_node)
                                        && let Some(name_node) = self.ctx.arena.get(decl.name)
                                        && let Some(ident) =
                                            self.ctx.arena.get_identifier(name_node)
                                        && register_value_name(&ident.escaped_text, decl.name)
                                        && let Some(node) = self.ctx.arena.get(decl.name)
                                    {
                                        self.ctx.error(
                                                                node.pos,
                                                                node.end - node.pos,
                                                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                            );
                                    }
                                }
                            }
                        } else if kind == syntax_kind_ext::FUNCTION_DECLARATION {
                            if let Some(func) = self.ctx.arena.get_function(stmt_node)
                                && self
                                    .ctx
                                    .has_modifier(&func.modifiers, SyntaxKind::ExportKeyword as u16)
                                && let Some(name_node) = self.ctx.arena.get(func.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && register_value_name(&ident.escaped_text, func.name)
                                && let Some(node) = self.ctx.arena.get(func.name)
                            {
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
                                        .to_string(),
                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                );
                            }
                        } else if kind == syntax_kind_ext::CLASS_DECLARATION {
                            if let Some(class) = self.ctx.arena.get_class(stmt_node)
                                && self.ctx.has_modifier(
                                    &class.modifiers,
                                    SyntaxKind::ExportKeyword as u16,
                                )
                                && let Some(name_node) = self.ctx.arena.get(class.name)
                                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                                && register_value_name(&ident.escaped_text, class.name)
                                && let Some(node) = self.ctx.arena.get(class.name)
                            {
                                self.ctx.error(
                                    node.pos,
                                    node.end - node.pos,
                                    diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
                                        .to_string(),
                                    diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                );
                            }
                        } else if kind == syntax_kind_ext::ENUM_DECLARATION
                            && let Some(enm) = self.ctx.arena.get_enum(stmt_node)
                            && self
                                .ctx
                                .has_modifier(&enm.modifiers, SyntaxKind::ExportKeyword as u16)
                            && let Some(name_node) = self.ctx.arena.get(enm.name)
                            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                            && register_value_name(&ident.escaped_text, enm.name)
                            && let Some(node) = self.ctx.arena.get(enm.name)
                        {
                            self.ctx.error(
                                node.pos,
                                node.end - node.pos,
                                diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE
                                    .to_string(),
                                diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                            );
                        }
                    }
                }
                self.ctx
                    .module_augmentation_value_decls
                    .insert(module_key, value_decl_map);
            }

            if !module.body.is_none() {
                // Check module body (which can be a block or nested module)
                self.check_module_body(module.body);
            }
        }
    }

    /// Check if the current file is a declaration file (.d.ts).
    fn is_declaration_file(&self) -> bool {
        self.ctx.file_name.ends_with(".d.ts")
    }

    /// Check if the current file is an external module (has import/export statements).
    /// Script files (global scope) don't have imports/exports.
    fn is_external_module(&self) -> bool {
        // Check the per-file cache first (set by CLI driver for multi-file mode)
        // This preserves the correct is_external_module value across sequential file bindings
        if let Some(ref map) = self.ctx.is_external_module_by_file
            && let Some(&is_ext) = map.get(&self.ctx.file_name)
        {
            return is_ext;
        }
        // Fallback to binder (for single-file mode or tests)
        self.ctx.binder.is_external_module()
    }

    /// Check if a module exists (for TS2664 check).
    /// Returns true if the module is in `resolved_modules`, `module_exports`,
    /// `declared_modules`, or `shorthand_ambient_modules`.
    fn module_exists(&self, module_name: &str) -> bool {
        if self.ctx.resolve_import_target(module_name).is_some() {
            return true;
        }

        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules
            && resolved.contains(module_name)
        {
            return true;
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return true;
        }

        if let Some(target_idx) = self.ctx.resolve_import_target(module_name)
            && let Some(target_binder) = self.ctx.get_binder_for_file(target_idx)
        {
            if let Some(target_file_name) = self
                .ctx
                .get_arena_for_file(target_idx as u32)
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                && target_binder.module_exports.contains_key(target_file_name)
            {
                return true;
            }
            if target_binder.module_exports.contains_key(module_name) {
                return true;
            }
        }

        // Check ambient module declarations (`declare module "X" { ... }`)
        if self.ctx.binder.declared_modules.contains(module_name) {
            return true;
        }

        // Check shorthand ambient modules (`declare module "X";`)
        if self
            .ctx
            .binder
            .shorthand_ambient_modules
            .contains(module_name)
        {
            return true;
        }

        // Check wildcard patterns in declared/shorthand ambient modules and module_exports
        if self.matches_ambient_module_pattern(module_name) {
            return true;
        }

        false
    }

    /// Check if a module name matches any wildcard ambient module pattern.
    fn matches_ambient_module_pattern(&self, module_name: &str) -> bool {
        let module_name = module_name.trim().trim_matches('"').trim_matches('\'');

        for patterns in [
            &self.ctx.binder.declared_modules,
            &self.ctx.binder.shorthand_ambient_modules,
        ] {
            for pattern in patterns {
                let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
                if pattern.contains('*')
                    && let Ok(glob) = globset::GlobBuilder::new(pattern)
                        .literal_separator(false)
                        .build()
                    && glob.compile_matcher().is_match(module_name)
                {
                    return true;
                }
            }
        }

        // Also check module_exports keys for wildcard patterns
        for pattern in self.ctx.binder.module_exports.keys() {
            let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
            if pattern.contains('*')
                && let Ok(glob) = globset::GlobBuilder::new(pattern)
                    .literal_separator(false)
                    .build()
                && glob.compile_matcher().is_match(module_name)
            {
                return true;
            }
        }

        false
    }

    /// Check if a module name is relative (starts with ./ or ../)
    fn is_relative_module_name(&self, name: &str) -> bool {
        if name.starts_with("./")
            || name.starts_with("../")
            || name == "."
            || name == ".."
            || name.starts_with('/')
        {
            return true;
        }

        // Treat rooted drive-specifier paths (e.g. "c:/x", "c:\\x") as invalid
        // for ambient module declarations as tsc does.
        let bytes = name.as_bytes();
        bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\')
    }

    fn module_augmentation_has_value_exports(&self, module_body: NodeIndex) -> bool {
        if module_body.is_none() {
            return false;
        }

        let Some(body_node) = self.ctx.arena.get(module_body) else {
            return false;
        };
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return false;
        }
        let Some(block) = self.ctx.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(stmts) = block.statements.as_ref() else {
            return false;
        };

        for &stmt_idx in &stmts.nodes {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            match stmt_node.kind {
                syntax_kind_ext::VARIABLE_STATEMENT
                | syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::ENUM_DECLARATION
                | syntax_kind_ext::EXPORT_ASSIGNMENT => return true,
                syntax_kind_ext::EXPORT_DECLARATION => {
                    if let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node) {
                        if export_decl.is_default_export
                            || !export_decl.module_specifier.is_none()
                            || export_decl.export_clause.is_none()
                        {
                            return true;
                        }
                        if let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause) {
                            match clause_node.kind {
                                syntax_kind_ext::VARIABLE_STATEMENT
                                | syntax_kind_ext::FUNCTION_DECLARATION
                                | syntax_kind_ext::CLASS_DECLARATION
                                | syntax_kind_ext::ENUM_DECLARATION => return true,
                                _ => {}
                            }
                        }
                    } else {
                        return true;
                    }
                }
                _ => {}
            }
        }

        false
    }

    /// Normalize module augmentation keys for relative specifiers.
    fn normalize_module_augmentation_key(&self, name: &str) -> String {
        if let Some(target_idx) = self.ctx.resolve_import_target(name) {
            return format!("file_idx:{target_idx}");
        }
        if self.is_relative_module_name(name)
            && let Some(parent) = Path::new(&self.ctx.file_name).parent()
        {
            let joined = parent.join(name);
            let normalized = Self::normalize_path(&joined);
            return normalized.to_string_lossy().to_string();
        }
        name.to_string()
    }

    fn normalize_path(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                Component::RootDir => normalized.push(component.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::Normal(part) => normalized.push(part),
            }
        }
        normalized
    }

    /// Check if a node is inside a namespace/module declaration.
    /// This is used for TS2435 (ambient modules cannot be nested).
    fn is_inside_namespace(&self, node_idx: NodeIndex) -> bool {
        // Walk up the parent chain to see if we're inside a namespace
        let mut current = node_idx;

        // Skip the first iteration (the node itself)
        if let Some(ext) = self.ctx.arena.get_extended(current) {
            current = ext.parent;
        } else {
            return false;
        }

        while !current.is_none() {
            let Some(node) = self.ctx.arena.get(current) else {
                break;
            };

            // If we find a namespace/module declaration in the parent chain,
            // the ambient module is nested
            if node.kind == syntax_kind_ext::MODULE_DECLARATION {
                return true;
            }

            // Move to the next parent
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }

        false
    }

    /// Check if a node is inside an ambient context (declare namespace/module or .d.ts file).
    fn is_in_ambient_context(&self, node_idx: NodeIndex) -> bool {
        if self.ctx.file_name.ends_with(".d.ts") {
            return true;
        }

        let mut current = node_idx;
        loop {
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
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.ctx.arena.get_module(parent_node)
                && self
                    .ctx
                    .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword as u16)
            {
                return true;
            }
            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                return false;
            }
            current = parent;
        }
    }

    /// Check a module body (block or nested module).
    fn check_module_body(&mut self, body_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(node)
                && let Some(ref stmts) = block.statements
            {
                let is_ambient = self.is_in_ambient_context(body_idx);
                for &stmt_idx in &stmts.nodes {
                    if is_ambient {
                        self.check_statement_in_ambient_context(stmt_idx);
                    }
                    // Also check for nested module declarations in non-ambient context
                    if let Some(stmt_node) = self.ctx.arena.get(stmt_idx) {
                        if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                            self.check_module_declaration(stmt_idx);
                        }
                        // Check for export declarations that contain nested modules
                        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION
                            && let Some(export_decl) = self.ctx.arena.get_export_decl(stmt_node)
                            && let Some(clause_node) = self.ctx.arena.get(export_decl.export_clause)
                            && clause_node.kind == syntax_kind_ext::MODULE_DECLARATION
                        {
                            self.check_module_declaration(export_decl.export_clause);
                        }
                    }
                }
            }
        } else if node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested module (for dotted namespace syntax like `namespace A.B { }`)
            self.check_module_declaration(body_idx);
        }
    }

    /// Check TS2433/TS2434: Namespace merging with class/function across files or out of order.
    ///
    /// TS2433: A namespace declaration cannot be in a different file from a class or function
    ///         with which it is merged.
    /// TS2434: A namespace declaration cannot be located prior to a class or function with
    ///         which it is merged.
    ///
    /// This check applies to non-ambient instantiated namespace declarations that have
    /// multiple declarations (merged with a class or function).
    fn check_namespace_merges_with_class_or_function(
        &mut self,
        module_idx: NodeIndex,
        module: &tsz_parser::parser::node::ModuleData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Get the symbol for this module declaration
        let Some(&sym_id) = self.ctx.binder.node_symbols.get(&module_idx.0) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };

        // Only check if the symbol has multiple declarations (merged)
        if symbol.declarations.len() <= 1 {
            return;
        }

        // Look for a non-ambient class or function declaration among the merged declarations
        for &decl_idx in &symbol.declarations {
            if decl_idx == module_idx {
                continue; // Skip the current namespace declaration
            }

            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let is_class = decl_node.kind == syntax_kind_ext::CLASS_DECLARATION;
            let is_function = decl_node.kind == syntax_kind_ext::FUNCTION_DECLARATION;

            if !is_class && !is_function {
                continue;
            }

            // Check if the declaration is ambient: `declare class`, or inside
            // an ambient context (e.g. `declare module 'M' { class C {} }`)
            if self.is_ambient_declaration(decl_idx) {
                continue;
            }

            // For functions, they must have a body to be considered a value declaration
            if is_function
                && let Some(func) = self.ctx.arena.get_function(decl_node)
                && func.body.is_none()
            {
                continue; // Function overload signature, not an implementation
            }

            // Found a non-ambient class or function declaration
            // Now check if they're in different files (TS2433) or namespace is prior (TS2434)

            // Get the source file of the current namespace declaration
            let current_file = self.get_source_file_of_node(module_idx);
            let other_file = self.get_source_file_of_node(decl_idx);

            if current_file != other_file {
                // TS2433: Different files
                if let Some(name_node) = self.ctx.arena.get(module.name) {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W.to_string(),
                        diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_IN_A_DIFFERENT_FILE_FROM_A_CLASS_OR_FUNCTION_W,
                    );
                }
            } else {
                // TS2434: Namespace comes before class/function in the same file
                // Compare positions - only emit error if namespace is before class/function
                let namespace_pos = self.ctx.arena.get(module_idx).map_or(0, |n| n.pos);
                let class_or_func_pos = self.ctx.arena.get(decl_idx).map_or(0, |n| n.pos);

                if namespace_pos < class_or_func_pos
                    && let Some(name_node) = self.ctx.arena.get(module.name)
                {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        diagnostic_messages::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC.to_string(),
                        diagnostic_codes::A_NAMESPACE_DECLARATION_CANNOT_BE_LOCATED_PRIOR_TO_A_CLASS_OR_FUNCTION_WITH_WHIC,
                    );
                }
            }

            // Only report error once (for the first matching class/function)
            break;
        }
    }

    /// Check if a namespace declaration is instantiated (contains runtime code).
    /// Uninstantiated namespaces only contain interfaces, type aliases, etc.
    fn is_namespace_declaration_instantiated(&self, namespace_idx: NodeIndex) -> bool {
        let Some(namespace_node) = self.ctx.arena.get(namespace_idx) else {
            return false;
        };
        if namespace_node.kind != syntax_kind_ext::MODULE_DECLARATION {
            return false;
        }
        let Some(module_decl) = self.ctx.arena.get_module(namespace_node) else {
            return false;
        };
        self.module_body_has_runtime_members(module_decl.body)
    }

    fn module_body_has_runtime_members(&self, body_idx: NodeIndex) -> bool {
        if body_idx.is_none() {
            return false;
        }
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return false;
        };
        if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return self.is_namespace_declaration_instantiated(body_idx);
        }
        if body_node.kind != syntax_kind_ext::MODULE_BLOCK {
            return false;
        }
        let Some(module_block) = self.ctx.arena.get_module_block(body_node) else {
            return false;
        };
        let Some(statements) = &module_block.statements else {
            return false;
        };
        for &statement_idx in &statements.nodes {
            let Some(statement_node) = self.ctx.arena.get(statement_idx) else {
                continue;
            };
            match statement_node.kind {
                k if k == syntax_kind_ext::VARIABLE_STATEMENT
                    || k == syntax_kind_ext::FUNCTION_DECLARATION
                    || k == syntax_kind_ext::CLASS_DECLARATION
                    || k == syntax_kind_ext::ENUM_DECLARATION
                    || k == syntax_kind_ext::EXPRESSION_STATEMENT
                    || k == syntax_kind_ext::EXPORT_ASSIGNMENT =>
                {
                    return true;
                }
                k if k == syntax_kind_ext::MODULE_DECLARATION => {
                    if self.is_namespace_declaration_instantiated(statement_idx) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Get the source file path of a node's declaration.
    /// Returns the file name if we can determine it, or empty string if unknown.
    fn get_source_file_of_node(&self, node_idx: NodeIndex) -> String {
        // Walk up to find the source file
        let mut current = node_idx;
        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                break;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent)
                && parent_node.kind == syntax_kind_ext::SOURCE_FILE
            {
                // Found the source file - return the file name from context
                return self.ctx.file_name.clone();
            }
            current = parent;
        }
        // Fallback to current file name
        self.ctx.file_name.clone()
    }

    /// Check a statement inside an ambient context (declare namespace/module).
    /// Emits TS1036 for non-declaration statements, plus specific errors for
    /// continue (TS1104), return (TS1108), and with (TS2410).
    fn check_statement_in_ambient_context(&mut self, stmt_idx: NodeIndex) {
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
                        && !func.body.is_none()
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

        if is_declaration_or_variable
            && let Some((pos, end)) = self.ctx.get_node_span(label_idx) {
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
