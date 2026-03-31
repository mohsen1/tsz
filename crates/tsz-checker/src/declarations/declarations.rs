//! Declaration Type Checking
//!
//! Handles classes, interfaces, functions, and variable declarations.
//! This module separates declaration checking logic from the monolithic `CheckerState`.

use crate::context::CheckerContext;
use crate::diagnostics::format_message;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::{NodeIndex, node_flags, syntax_kind_ext};
use tsz_scanner::SyntaxKind;

/// Declaration type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All declaration type checking goes through this checker.
pub struct DeclarationChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IsolatedEnumInitializerKind {
    LiteralNumeric,
    NonLiteralNumeric,
    LiteralString,
    NonLiteralString,
    Other,
}

impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    /// Create a new declaration checker with a mutable context reference.
    pub const fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check if a declaration is ambient (has declare keyword, AMBIENT node flag,
    /// or is inside an ambient context like `declare module` or a `.d.ts` file).
    pub(crate) fn is_ambient_declaration(&self, var_idx: NodeIndex) -> bool {
        self.ctx.is_ambient_declaration(var_idx)
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
        // Skip when file has real syntax errors — the parse error is sufficient.
        if is_const && decl_data.initializer.is_none() && !self.ctx.has_real_syntax_errors {
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
        if self.ctx.is_js_file()
            && !self.ctx.has_syntax_parse_errors
            && self.is_strict_mode_for_node(decl_data.name)
            && let Some(name_node) = self.ctx.arena.get(decl_data.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && crate::state_checking::is_eval_or_arguments(&ident.escaped_text)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
            let in_class = self
                .ctx
                .enclosing_class
                .as_ref()
                .is_some_and(|c| !c.is_declared);
            let (message, code) = if in_class {
                (
                    format_message(
                        diagnostic_messages::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                        &[&ident.escaped_text],
                    ),
                    diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                )
            } else if self.is_external_module() {
                (
                    format_message(
                        diagnostic_messages::INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE,
                        &[&ident.escaped_text],
                    ),
                    diagnostic_codes::INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE,
                )
            } else {
                (
                    format_message(
                        diagnostic_messages::INVALID_USE_OF_IN_STRICT_MODE,
                        &[&ident.escaped_text],
                    ),
                    diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
                )
            };
            if let Some((pos, end)) = self.ctx.get_node_span(decl_data.name) {
                self.ctx.error(pos, end - pos, message, code);
            }
        }
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
        let has_declare = self
            .ctx
            .arena
            .has_modifier(&func.modifiers, tsz_scanner::SyntaxKind::DeclareKeyword);

        // TS1184: `declare` on a function declaration in a block scope
        if has_declare {
            let parent_kind = self
                .ctx
                .arena
                .get_extended(func_idx)
                .and_then(|ext| self.ctx.arena.get(ext.parent))
                .map(|p| p.kind);
            let in_block = !matches!(
                parent_kind,
                Some(k) if k == tsz_parser::parser::syntax_kind_ext::SOURCE_FILE
                    || k == tsz_parser::parser::syntax_kind_ext::MODULE_BLOCK
                    || k == tsz_parser::parser::syntax_kind_ext::MODULE_DECLARATION
                    || k == tsz_parser::parser::syntax_kind_ext::EXPORT_DECLARATION
            ) && parent_kind.is_some();
            if in_block {
                // Find the `declare` keyword modifier node for the error span
                let declare_node = func.modifiers.as_ref().and_then(|mods| {
                    mods.nodes
                        .iter()
                        .find(|&&mod_idx| {
                            self.ctx.arena.get(mod_idx).is_some_and(|n| {
                                n.kind == tsz_scanner::SyntaxKind::DeclareKeyword as u16
                            })
                        })
                        .copied()
                });
                if let Some(decl_mod) = declare_node
                    && let Some(mod_node) = self.ctx.arena.get(decl_mod)
                {
                    self.ctx.error(
                        mod_node.pos,
                        mod_node.end - mod_node.pos,
                        "Modifiers cannot appear here.".to_string(),
                        crate::diagnostics::diagnostic_codes::MODIFIERS_CANNOT_APPEAR_HERE,
                    );
                }
            }
        }

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

        // TS1100: `eval` or `arguments` used as a function name in strict mode.
        // In class bodies, `arguments` is reported as TS1210 instead.
        // Skip for ambient declarations (functions inside `declare global`, `.d.ts` files, etc.)
        if !has_declare
            && !self.ctx.has_syntax_parse_errors
            && !self.is_ambient_declaration(func_idx)
            && func.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(func.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            let name = &ident.escaped_text;
            if self.is_strict_mode_for_node(func.name)
                && crate::state_checking::is_eval_or_arguments(name)
                && !(self.ctx.enclosing_class.is_some() && name.as_str() == "arguments")
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
                let in_class = self
                    .ctx
                    .enclosing_class
                    .as_ref()
                    .is_some_and(|c| !c.is_declared);
                let (message, code) = if in_class {
                    (
                        format_message(
                            diagnostic_messages::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                            &[name],
                        ),
                        diagnostic_codes::CODE_CONTAINED_IN_A_CLASS_IS_EVALUATED_IN_JAVASCRIPTS_STRICT_MODE_WHICH_DOES_NOT,
                    )
                } else if self.is_external_module() {
                    (
                        format_message(
                            diagnostic_messages::INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE,
                            &[name],
                        ),
                        diagnostic_codes::INVALID_USE_OF_MODULES_ARE_AUTOMATICALLY_IN_STRICT_MODE,
                    )
                } else {
                    (
                        format_message(diagnostic_messages::INVALID_USE_OF_IN_STRICT_MODE, &[name]),
                        diagnostic_codes::INVALID_USE_OF_IN_STRICT_MODE,
                    )
                };
                if let Some((pos, end)) = self.ctx.get_node_span(func.name) {
                    self.ctx.error(pos, end - pos, message, code);
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

        // TSC anchors the error at the function name, not the whole declaration.
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        let error_node = self
            .ctx
            .arena
            .get(func_idx)
            .and_then(|n| self.ctx.arena.get_function(n))
            .map(|f| f.name)
            .filter(|n| n.is_some())
            .unwrap_or(func_idx);
        let (pos, len) = self
            .ctx
            .arena
            .get(error_node)
            .map_or((0, 0), |n| (n.pos, n.end - n.pos));
        if in_class {
            self.ctx.error(
                pos,
                len,
                diagnostic_messages::FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_2
                    .to_string(),
                diagnostic_codes::FUNCTION_DECLARATIONS_ARE_NOT_ALLOWED_INSIDE_BLOCKS_IN_STRICT_MODE_WHEN_TARGETIN_2,
            );
        } else {
            self.ctx.error(
                pos,
                len,
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
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_parser::parser::node::NodeAccess;
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(enum_idx) else {
            return;
        };

        let Some(enum_data) = self.ctx.arena.get_enum(node) else {
            return;
        };

        // TS1294: erasableSyntaxOnly — non-ambient enums are not erasable.
        // tsc's error(node) uses getErrorSpanForNode which reports at the name.
        if self.ctx.compiler_options.erasable_syntax_only
            && !self.ctx.is_ambient_declaration(enum_idx)
        {
            let error_node = self.ctx.arena.get(enum_data.name).unwrap_or(node);
            self.ctx.error(
                error_node.pos,
                error_node.end - error_node.pos,
                diagnostic_messages::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED
                    .to_string(),
                diagnostic_codes::THIS_SYNTAX_IS_NOT_ALLOWED_WHEN_ERASABLESYNTAXONLY_IS_ENABLED,
            );
        }

        // TS2431: Enum name cannot be '{0}'.
        if let Some(name_text) = self.ctx.arena.get_identifier_text(enum_data.name) {
            match name_text {
                "any" | "unknown" | "never" | "number" | "bigint" | "boolean" | "string"
                | "symbol" | "void" | "object" | "undefined" => {
                    let name_node = self
                        .ctx
                        .arena
                        .get(enum_data.name)
                        .expect("enum name node must exist when identifier text was found");
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
        // tsc emits this for bare numeric literals (`1`), bigint literals,
        // string literals that parse as numbers (`"3"`), and computed property
        // names with numeric or numeric-string literal expressions (`[2]`, `["4"]`).
        for &member_idx in &enum_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(member_data) = self.ctx.arena.get_enum_member(member_node)
                && let Some(name_node) = self.ctx.arena.get(member_data.name)
            {
                let is_numeric_name = if name_node.kind == SyntaxKind::NumericLiteral as u16
                    || name_node.kind == SyntaxKind::BigIntLiteral as u16
                {
                    true
                } else if name_node.kind == SyntaxKind::StringLiteral as u16 {
                    // tsc only treats string literal names as numeric when they are already in
                    // canonical finite numeric-property form; `"13e-1"` and `"-Infinity"`
                    // should not trigger TS2452.
                    self.ctx.arena.get_literal(name_node).is_some_and(|lit| {
                        tsz_solver::utils::canonicalize_numeric_name(&lit.text)
                            .is_some_and(|canonical| canonical == "NaN" || canonical == lit.text)
                    })
                } else if name_node.kind
                    == tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME
                {
                    // Computed property names like `[2]` or `["4"]`
                    self.ctx
                        .arena
                        .get_computed_property(name_node)
                        .and_then(|cp| self.ctx.arena.get(cp.expression))
                        .is_some_and(|expr| {
                            if expr.kind == SyntaxKind::NumericLiteral as u16 {
                                true
                            } else if expr.kind == SyntaxKind::StringLiteral as u16 {
                                self.ctx
                                    .arena
                                    .get_literal(expr)
                                    .and_then(|lit| {
                                        tsz_solver::utils::canonicalize_numeric_name(&lit.text).map(
                                            |canonical| canonical == "NaN" || canonical == lit.text,
                                        )
                                    })
                                    .unwrap_or(false)
                            } else {
                                false
                            }
                        })
                } else {
                    false
                };
                if is_numeric_name {
                    self.ctx.error(
                        name_node.pos,
                        name_node.end - name_node.pos,
                        "An enum member cannot have a numeric name.".to_string(),
                        diagnostic_codes::AN_ENUM_MEMBER_CANNOT_HAVE_A_NUMERIC_NAME,
                    );
                }
            }
        }

        // TS1066: In ambient enum declarations, member initializer must be constant expression
        let is_ambient = self
            .ctx
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::DeclareKeyword);

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

        // TS18055/TS18056 isolatedModules enum restrictions, plus TS1061 fallback.
        let mut auto_incrementable = true;
        let mut previous_initializer_kind = IsolatedEnumInitializerKind::Other;
        for &member_idx in &enum_data.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx)
                && let Some(member_data) = self.ctx.arena.get_enum_member(member_node)
            {
                let name_node = self.ctx.arena.get(member_data.name).unwrap_or(member_node);
                if member_data.initializer.is_none() {
                    if !auto_incrementable {
                        if self.ctx.isolated_modules()
                            && previous_initializer_kind
                                == IsolatedEnumInitializerKind::NonLiteralNumeric
                        {
                            self.ctx.error(
                                name_node.pos,
                                name_node.end - name_node.pos,
                                diagnostic_messages::ENUM_MEMBER_FOLLOWING_A_NON_LITERAL_NUMERIC_MEMBER_MUST_HAVE_AN_INITIALIZER_WHEN.to_string(),
                                diagnostic_codes::ENUM_MEMBER_FOLLOWING_A_NON_LITERAL_NUMERIC_MEMBER_MUST_HAVE_AN_INITIALIZER_WHEN,
                            );
                        } else {
                            self.ctx.error(
                                name_node.pos,
                                name_node.end - name_node.pos,
                                "Enum member must have initializer.".to_string(),
                                diagnostic_codes::ENUM_MEMBER_MUST_HAVE_INITIALIZER,
                            );
                        }
                    }
                    auto_incrementable = true;
                    previous_initializer_kind = IsolatedEnumInitializerKind::Other;
                } else {
                    previous_initializer_kind = self.classify_isolated_enum_initializer(
                        member_data.initializer,
                        enum_data,
                        0,
                    );
                    if self.ctx.isolated_modules()
                        && previous_initializer_kind
                            == IsolatedEnumInitializerKind::NonLiteralString
                        && let Some(member_name) =
                            self.ctx.arena.get_identifier_text(member_data.name)
                    {
                        let enum_name = self
                            .ctx
                            .arena
                            .get_identifier_text(enum_data.name)
                            .unwrap_or("");
                        let display_name = format!("{enum_name}.{member_name}");
                        let error_node = self
                            .ctx
                            .arena
                            .get(member_data.initializer)
                            .unwrap_or(name_node);
                        self.ctx.error(
                            error_node.pos,
                            error_node.end - error_node.pos,
                            format_message(
                                diagnostic_messages::HAS_A_STRING_TYPE_BUT_MUST_HAVE_SYNTACTICALLY_RECOGNIZABLE_STRING_SYNTAX_WHEN_IS,
                                &[display_name.as_str()],
                            ),
                            diagnostic_codes::HAS_A_STRING_TYPE_BUT_MUST_HAVE_SYNTACTICALLY_RECOGNIZABLE_STRING_SYNTAX_WHEN_IS,
                        );
                    }
                    auto_incrementable =
                        self.is_numeric_constant_enum_expr(member_data.initializer, enum_data, 0);
                }

                // TS2565: check for property used before being assigned
                if member_data.initializer.is_some()
                    && let Some(member_name) = self.ctx.arena.get_identifier_text(member_data.name)
                {
                    let enum_name_text = self.ctx.arena.get_identifier_text(enum_data.name);
                    self.check_enum_member_self_reference(
                        member_data.initializer,
                        member_name,
                        enum_name_text,
                    );
                }
            }
        }

        // TS2651: check for forward references to later members (applies to ALL enums)
        {
            let member_names: Vec<&str> = enum_data
                .members
                .nodes
                .iter()
                .filter_map(|&m_idx| {
                    let m_node = self.ctx.arena.get(m_idx)?;
                    let m_data = self.ctx.arena.get_enum_member(m_node)?;
                    self.ctx.arena.get_identifier_text(m_data.name)
                })
                .collect();

            let enum_name_text = self.ctx.arena.get_identifier_text(enum_data.name);

            // Collect member names from later merged enum declarations.
            // tsc treats references to members in later declarations of the same
            // enum as forward references (TS2651).
            let mut later_decl_member_names: Vec<String> = Vec::new();
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(enum_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let current_pos = self.ctx.arena.get(enum_idx).map(|n| n.pos).unwrap_or(0);
                for &decl_idx in &symbol.declarations {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    // Only consider later enum declarations (higher position)
                    if decl_node.kind != syntax_kind_ext::ENUM_DECLARATION
                        || decl_node.pos <= current_pos
                    {
                        continue;
                    }
                    let Some(other_enum) = self.ctx.arena.get_enum(decl_node) else {
                        continue;
                    };
                    for &m_idx in &other_enum.members.nodes {
                        if let Some(m_node) = self.ctx.arena.get(m_idx)
                            && let Some(m_data) = self.ctx.arena.get_enum_member(m_node)
                            && let Some(name) = self.ctx.arena.get_identifier_text(m_data.name)
                        {
                            later_decl_member_names.push(name.to_string());
                        }
                    }
                }
            }

            for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member_data) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };

                if member_data.initializer.is_none() {
                    continue;
                }

                // Use .get() to avoid panic when member_names is shorter than
                // members.nodes (e.g. string-literal enum member names that fail
                // get_identifier_text are excluded from member_names).
                let mut later_members: Vec<&str> =
                    member_names.get(i + 1..).unwrap_or(&[]).to_vec();
                // Include members from later merged enum declarations
                for name in &later_decl_member_names {
                    later_members.push(name.as_str());
                }
                let has_forward_ref = self.enum_has_forward_reference(
                    member_data.initializer,
                    &later_members,
                    enum_name_text,
                );

                if has_forward_ref
                    && let Some(init_node) = self.ctx.arena.get(member_data.initializer)
                {
                    self.ctx.error(
                            init_node.pos,
                            init_node.end - init_node.pos,
                            diagnostic_messages::A_MEMBER_INITIALIZER_IN_A_ENUM_DECLARATION_CANNOT_REFERENCE_MEMBERS_DECLARED_AFT.to_string(),
                            diagnostic_codes::A_MEMBER_INITIALIZER_IN_A_ENUM_DECLARATION_CANNOT_REFERENCE_MEMBERS_DECLARED_AFT,
                        );
                }
            }
        }

        // Const enum specific checks
        let is_const_enum = self
            .ctx
            .arena
            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword);

        if is_const_enum {
            // TS2567: const enum cannot merge with namespace
            if let Some(sym_id) = self.ctx.binder.get_node_symbol(enum_idx)
                && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            {
                let has_module_decl = symbol.declarations.iter().any(|&decl_idx| {
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .is_some_and(|n| n.kind == syntax_kind_ext::MODULE_DECLARATION)
                });
                if has_module_decl && let Some(name_node) = self.ctx.arena.get(enum_data.name) {
                    self.ctx.error(
                            name_node.pos,
                            name_node.end - name_node.pos,
                            diagnostic_messages::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS.to_string(),
                            diagnostic_codes::ENUM_DECLARATIONS_CAN_ONLY_MERGE_WITH_NAMESPACE_OR_OTHER_ENUM_DECLARATIONS,
                        );
                }
            }

            let enum_name_text = self.ctx.arena.get_identifier_text(enum_data.name);

            // Clear const enum evaluation memo cache before evaluating members.
            // This ensures fresh evaluation for each enum declaration while still
            // benefiting from memoization across members within the same enum.
            crate::types_domain::utilities::const_enum_eval::clear_const_eval_memo();

            for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let Some(member_data) = self.ctx.arena.get_enum_member(member_node) else {
                    continue;
                };

                if member_data.initializer.is_none() {
                    continue;
                }

                // String literal initializers are always valid const enum initializers.
                // Only numeric initializers need evaluation for TS2474/TS2477/TS2478.
                let is_string_initializer = self
                    .ctx
                    .arena
                    .get(member_data.initializer)
                    .is_some_and(|n| {
                        n.kind == SyntaxKind::StringLiteral as u16
                            || n.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                    });
                if is_string_initializer {
                    continue;
                }

                // Collect member names for forward reference detection (used to skip TS2474 if forward ref)
                let member_names: Vec<&str> = enum_data
                    .members
                    .nodes
                    .iter()
                    .filter_map(|&m_idx| {
                        let m_node = self.ctx.arena.get(m_idx)?;
                        let m_data = self.ctx.arena.get_enum_member(m_node)?;
                        self.ctx.arena.get_identifier_text(m_data.name)
                    })
                    .collect();
                let later_members: Vec<&str> = member_names.get(i + 1..).unwrap_or(&[]).to_vec();
                let has_forward_ref = self.enum_has_forward_reference(
                    member_data.initializer,
                    &later_members,
                    enum_name_text,
                );
                if has_forward_ref {
                    continue; // Forward ref already reported above; skip TS2474
                }

                // Try to evaluate the initializer as a numeric constant
                let value =
                    crate::types_domain::utilities::const_enum_eval::evaluate_const_enum_initializer(
                        self.ctx.arena,
                        member_data.initializer,
                        enum_data,
                        enum_name_text,
                        0,
                    );

                match value {
                    None => {
                        // TS2474: const enum member initializer is not a constant expression
                        if let Some(init_node) = self.ctx.arena.get(member_data.initializer) {
                            self.ctx.error(
                                init_node.pos,
                                init_node.end - init_node.pos,
                                diagnostic_messages::CONST_ENUM_MEMBER_INITIALIZERS_MUST_BE_CONSTANT_EXPRESSIONS.to_string(),
                                diagnostic_codes::CONST_ENUM_MEMBER_INITIALIZERS_MUST_BE_CONSTANT_EXPRESSIONS,
                            );
                        }
                    }
                    Some(v) if f64::is_nan(v) => {
                        // TS2478: const enum member evaluated to NaN
                        if let Some(init_node) = self.ctx.arena.get(member_data.initializer) {
                            self.ctx.error(
                                init_node.pos,
                                init_node.end - init_node.pos,
                                diagnostic_messages::CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_DISALLOWED_VALUE_NAN.to_string(),
                                diagnostic_codes::CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_DISALLOWED_VALUE_NAN,
                            );
                        }
                    }
                    Some(v) if f64::is_infinite(v) => {
                        // TS2477: const enum member evaluated to non-finite value
                        if let Some(init_node) = self.ctx.arena.get(member_data.initializer) {
                            self.ctx.error(
                                init_node.pos,
                                init_node.end - init_node.pos,
                                diagnostic_messages::CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_A_NON_FINITE_VALUE.to_string(),
                                diagnostic_codes::CONST_ENUM_MEMBER_INITIALIZER_WAS_EVALUATED_TO_A_NON_FINITE_VALUE,
                            );
                        }
                    }
                    _ => {} // Valid constant value
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
                            }
                            return self.is_numeric_constant_enum_expr(
                                member_data.initializer,
                                enum_data,
                                depth + 1,
                            );
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

    fn classify_isolated_enum_initializer(
        &self,
        expr_idx: NodeIndex,
        enum_data: &tsz_parser::parser::node::EnumData,
        depth: u32,
    ) -> IsolatedEnumInitializerKind {
        if depth > 100 || expr_idx.is_none() {
            return IsolatedEnumInitializerKind::Other;
        }

        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return IsolatedEnumInitializerKind::Other;
        };

        if self.is_numeric_constant_enum_expr(expr_idx, enum_data, 0) {
            return IsolatedEnumInitializerKind::LiteralNumeric;
        }

        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                IsolatedEnumInitializerKind::LiteralString
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => self
                .ctx
                .arena
                .get_parenthesized(node)
                .map_or(IsolatedEnumInitializerKind::Other, |paren| {
                    self.classify_isolated_enum_initializer(paren.expression, enum_data, depth + 1)
                }),
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                self.ctx.arena.get_type_assertion(node).map_or(
                    IsolatedEnumInitializerKind::Other,
                    |assertion| {
                        self.classify_isolated_enum_initializer(
                            assertion.expression,
                            enum_data,
                            depth + 1,
                        )
                    },
                )
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => self
                .ctx
                .arena
                .get_binary_expr(node)
                .map_or(IsolatedEnumInitializerKind::Other, |binary| {
                    if binary.operator_token == SyntaxKind::PlusToken as u16
                        && (self.is_syntactically_recognizable_string_initializer(binary.left)
                            || self.is_syntactically_recognizable_string_initializer(binary.right))
                    {
                        IsolatedEnumInitializerKind::LiteralString
                    } else {
                        IsolatedEnumInitializerKind::Other
                    }
                }),
            k if k == SyntaxKind::Identifier as u16 => {
                let resolved = self
                    .resolve_identifier_like_symbol(expr_idx)
                    .and_then(|sym_id| self.resolve_imported_const_target(sym_id))
                    .or_else(|| self.resolve_identifier_like_symbol(expr_idx));
                resolved.map_or(IsolatedEnumInitializerKind::Other, |sym_id| {
                    self.classify_symbol_backed_enum_initializer(sym_id, enum_data, depth + 1)
                })
            }
            // Unrecognized syntax — return Other (not NonLiteralString).
            // TS18055 should only fire when the initializer value IS a known string
            // but the syntax isn't recognizable. Runtime expressions like method calls
            // (e.g., `2..toFixed(0)`) have string TYPE but no compile-time string VALUE,
            // so tsc doesn't emit TS18055 for them.
            _ => IsolatedEnumInitializerKind::Other,
        }
    }

    fn is_syntactically_recognizable_string_initializer(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                self.ctx.arena.get_parenthesized(node).is_some_and(|paren| {
                    self.is_syntactically_recognizable_string_initializer(paren.expression)
                })
            }
            _ => false,
        }
    }

    fn resolve_identifier_like_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        self.ctx
            .binder
            .get_node_symbol(expr_idx)
            .or_else(|| self.ctx.binder.resolve_identifier(self.ctx.arena, expr_idx))
    }

    fn resolve_imported_const_target(&self, sym_id: SymbolId) -> Option<SymbolId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        if (symbol.flags & symbol_flags::ALIAS) == 0 {
            return Some(sym_id);
        }
        let module_specifier = symbol.import_module.as_ref()?;
        let target_name = symbol
            .import_name
            .as_deref()
            .unwrap_or(&symbol.escaped_name);
        let source_file_idx = if symbol.decl_file_idx == u32::MAX {
            self.ctx.current_file_idx
        } else {
            symbol.decl_file_idx as usize
        };
        if !self.ctx.has_symbol_file_index(sym_id) {
            self.ctx
                .register_symbol_file_target(sym_id, source_file_idx);
        }
        self.ctx
            .resolve_alias_import_member(sym_id, module_specifier, target_name)
    }

    fn classify_symbol_backed_enum_initializer(
        &self,
        sym_id: SymbolId,
        enum_data: &tsz_parser::parser::node::EnumData,
        depth: u32,
    ) -> IsolatedEnumInitializerKind {
        let cross_file_idx = self.ctx.resolve_symbol_file_index(sym_id);
        // A symbol is truly cross-file only if its file index differs from
        // the file currently being checked. In project mode, cross_file_symbol_targets
        // contains ALL symbols (including same-file ones).
        let is_cross_file = cross_file_idx.is_some_and(|idx| idx != self.ctx.current_file_idx);
        let (symbol, arena) = if let Some(file_idx) = cross_file_idx {
            let Some(binder) = self.ctx.get_binder_for_file(file_idx) else {
                return IsolatedEnumInitializerKind::Other;
            };
            let Some(symbol) = binder.get_symbol(sym_id) else {
                return IsolatedEnumInitializerKind::Other;
            };
            (symbol, self.ctx.get_arena_for_file(file_idx as u32))
        } else {
            let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                return IsolatedEnumInitializerKind::Other;
            };
            (symbol, self.ctx.arena)
        };

        let decl_idx = if symbol.value_declaration.is_none() {
            symbol
                .declarations
                .first()
                .copied()
                .unwrap_or(NodeIndex::NONE)
        } else {
            symbol.value_declaration
        };
        let Some(decl_node) = arena.get(decl_idx) else {
            return IsolatedEnumInitializerKind::Other;
        };
        let Some(var_decl) = arena.get_variable_declaration(decl_node) else {
            return IsolatedEnumInitializerKind::Other;
        };
        if var_decl.initializer.is_none() {
            return self
                .declared_type_annotation_kind_in_arena(arena, var_decl.type_annotation)
                .unwrap_or(IsolatedEnumInitializerKind::Other);
        }
        if is_cross_file {
            // Cross-file reference under isolatedModules: a single-file transpiler
            // can't trace the value, so downgrade to non-literal. This correctly
            // triggers TS18055/TS18056 for imported string/numeric consts.
            let inner = self.classify_initializer_kind_in_arena(arena, var_decl.initializer, depth);
            match inner {
                IsolatedEnumInitializerKind::LiteralNumeric
                | IsolatedEnumInitializerKind::NonLiteralNumeric => {
                    IsolatedEnumInitializerKind::NonLiteralNumeric
                }
                IsolatedEnumInitializerKind::LiteralString
                | IsolatedEnumInitializerKind::NonLiteralString => {
                    IsolatedEnumInitializerKind::NonLiteralString
                }
                IsolatedEnumInitializerKind::Other => IsolatedEnumInitializerKind::Other,
            }
        } else {
            // Same-file reference: tsc traces through const variable declarations
            // in the same file, so preserve the inner classification. A same-file
            // `const LOCAL = 'hello'` is syntactically recognizable through the const.
            self.classify_isolated_enum_initializer(var_decl.initializer, enum_data, depth)
        }
    }

    fn classify_initializer_kind_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
        depth: u32,
    ) -> IsolatedEnumInitializerKind {
        if depth > 100 || expr_idx.is_none() {
            return IsolatedEnumInitializerKind::Other;
        }

        let Some(node) = arena.get(expr_idx) else {
            return IsolatedEnumInitializerKind::Other;
        };

        match node.kind {
            k if k == SyntaxKind::NumericLiteral as u16 => {
                IsolatedEnumInitializerKind::LiteralNumeric
            }
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                IsolatedEnumInitializerKind::LiteralString
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => arena
                .get_parenthesized(node)
                .map_or(IsolatedEnumInitializerKind::Other, |paren| {
                    self.classify_initializer_kind_in_arena(arena, paren.expression, depth + 1)
                }),
            k if k == syntax_kind_ext::AS_EXPRESSION
                || k == syntax_kind_ext::SATISFIES_EXPRESSION
                || k == syntax_kind_ext::TYPE_ASSERTION =>
            {
                arena.get_type_assertion(node).map_or(
                    IsolatedEnumInitializerKind::Other,
                    |assertion| {
                        self.classify_initializer_kind_in_arena(
                            arena,
                            assertion.expression,
                            depth + 1,
                        )
                    },
                )
            }
            k if k == syntax_kind_ext::PREFIX_UNARY_EXPRESSION => arena
                .get_unary_expr(node)
                .map_or(IsolatedEnumInitializerKind::Other, |unary| {
                    match self.classify_initializer_kind_in_arena(arena, unary.operand, depth + 1) {
                        IsolatedEnumInitializerKind::LiteralNumeric
                        | IsolatedEnumInitializerKind::NonLiteralNumeric => {
                            IsolatedEnumInitializerKind::NonLiteralNumeric
                        }
                        _ => IsolatedEnumInitializerKind::Other,
                    }
                }),
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                arena
                    .get_binary_expr(node)
                    .map_or(IsolatedEnumInitializerKind::Other, |binary| {
                        if binary.operator_token == SyntaxKind::PlusToken as u16
                            && (self.is_syntactically_recognizable_string_initializer_in_arena(
                                arena,
                                binary.left,
                            ) || self
                                .is_syntactically_recognizable_string_initializer_in_arena(
                                    arena,
                                    binary.right,
                                ))
                        {
                            IsolatedEnumInitializerKind::LiteralString
                        } else {
                            match (
                                self.classify_initializer_kind_in_arena(
                                    arena,
                                    binary.left,
                                    depth + 1,
                                ),
                                self.classify_initializer_kind_in_arena(
                                    arena,
                                    binary.right,
                                    depth + 1,
                                ),
                            ) {
                                (
                                    IsolatedEnumInitializerKind::LiteralNumeric
                                    | IsolatedEnumInitializerKind::NonLiteralNumeric,
                                    IsolatedEnumInitializerKind::LiteralNumeric
                                    | IsolatedEnumInitializerKind::NonLiteralNumeric,
                                ) => IsolatedEnumInitializerKind::NonLiteralNumeric,
                                _ => IsolatedEnumInitializerKind::Other,
                            }
                        }
                    })
            }
            _ => IsolatedEnumInitializerKind::Other,
        }
    }

    fn declared_type_annotation_kind_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        type_annotation: NodeIndex,
    ) -> Option<IsolatedEnumInitializerKind> {
        let type_node = arena.get(type_annotation)?;
        match type_node.kind {
            k if k == SyntaxKind::StringKeyword as u16 => {
                Some(IsolatedEnumInitializerKind::NonLiteralString)
            }
            k if k == SyntaxKind::NumberKeyword as u16 => {
                Some(IsolatedEnumInitializerKind::NonLiteralNumeric)
            }
            _ => None,
        }
    }

    fn is_syntactically_recognizable_string_initializer_in_arena(
        &self,
        arena: &tsz_parser::parser::NodeArena,
        expr_idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(expr_idx) else {
            return false;
        };
        match node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == syntax_kind_ext::TEMPLATE_EXPRESSION =>
            {
                true
            }
            k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
                arena.get_parenthesized(node).is_some_and(|paren| {
                    self.is_syntactically_recognizable_string_initializer_in_arena(
                        arena,
                        paren.expression,
                    )
                })
            }
            _ => false,
        }
    }

    // Module/namespace declaration checking is in `declarations_module.rs`.
    // Module resolution helpers are in `declarations_module_helpers.rs`.

    /// Check a statement inside an ambient context (declare namespace/module).
    /// Emits TS1036 for non-declaration statements, plus specific errors for
    /// continue (TS1104), return (TS1108), and with (TS2410).
    pub(crate) fn check_statement_in_ambient_context(
        &mut self,
        stmt_idx: NodeIndex,
        reported_generic_ambient_statement_error: &mut bool,
    ) {
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

        if is_non_declaration
            && !*reported_generic_ambient_statement_error
            && !self.ctx.has_syntax_parse_errors
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS.to_string(),
                    diagnostic_codes::STATEMENTS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS,
                );
                *reported_generic_ambient_statement_error = true;
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
            self.check_statement_in_ambient_context(
                labeled.statement,
                reported_generic_ambient_statement_error,
            );
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
        self.ctx.is_strict_mode_for_node(idx)
    }

    fn check_with_statement(&mut self, stmt_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
            if !self.ctx.is_js_file() || self.ctx.js_strict_mode_diagnostics_enabled() {
                self.ctx.error(
                    pos,
                    end - pos,
                    diagnostic_messages::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A.to_string(),
                    diagnostic_codes::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
                );
            }

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
                // and we're not in a constructor, report error at the modifier keyword (matching tsc).
                // Decorators on parameters are NOT parameter properties.
                let modifier_idx = if let Some(ref mods) = param.modifiers {
                    mods.nodes.iter().copied().find(|&mod_idx| {
                        self.ctx.arena.get(mod_idx).is_some_and(|m| {
                            use tsz_scanner::SyntaxKind;
                            m.kind == SyntaxKind::PublicKeyword as u16
                                || m.kind == SyntaxKind::PrivateKeyword as u16
                                || m.kind == SyntaxKind::ProtectedKeyword as u16
                                || m.kind == SyntaxKind::ReadonlyKeyword as u16
                        })
                    })
                } else {
                    None
                };
                if let Some(mod_idx) = modifier_idx
                    && let Some((pos, end)) = self.ctx.get_node_span(mod_idx)
                {
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
#[path = "../../tests/declarations.rs"]
mod tests;
