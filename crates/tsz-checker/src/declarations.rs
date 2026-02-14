//! Declaration Type Checking
//!
//! Handles classes, interfaces, functions, and variable declarations.
//! This module separates declaration checking logic from the monolithic CheckerState.

use super::context::CheckerContext;
use crate::types::diagnostics::diagnostic_messages;
use rustc_hash::FxHashSet;
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

/// Property key for tracking property assignments in control flow analysis.
/// Note: Currently unused as property initialization is handled by CheckerState.
/// These types will be used when property initialization is migrated to DeclarationChecker.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum PropertyKey {
    Ident(String),
    Private(String),
    Computed(ComputedKey),
}

/// Computed property key.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum ComputedKey {
    Ident(String),
    String(String),
    Number(String),
}

/// Result of control flow analysis for property assignments.
/// Note: Currently unused as property initialization is handled by CheckerState.
#[derive(Clone, Debug)]
struct FlowResult {
    normal: Option<FxHashSet<PropertyKey>>,
    exits: Option<FxHashSet<PropertyKey>>,
}

impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    /// Create a new declaration checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check if a declaration is ambient (has declare keyword or AMBIENT flag).
    fn is_ambient_declaration(&self, var_idx: NodeIndex) -> bool {
        // .d.ts files are always ambient
        if self.ctx.file_name.ends_with(".d.ts") {
            return true;
        }

        // Check if the node or any ancestor has the AMBIENT flag
        let mut current = var_idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                // Check if this node has the AMBIENT flag set
                if (node.flags as u32) & node_flags::AMBIENT != 0 {
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

    /// Check a declaration node.
    ///
    /// This dispatches to specialized handlers based on declaration kind.
    /// Currently a skeleton - logic will be migrated incrementally from CheckerState.
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
                    if let Some(parent_ext) = self.ctx.arena.get_extended(parent_idx) {
                        if let Some(gp_node) = self.ctx.arena.get(parent_ext.parent) {
                            if gp_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                                || gp_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                            {
                                // const in for-in/for-of is allowed without initializer
                                return;
                            }
                        }
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

        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
                    if let Some(func) = self.ctx.arena.get_function(parent) {
                        if self.block_has_use_strict(func.body) {
                            return true;
                        }
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
                if let Some(lit) = self.ctx.arena.get_literal(expr) {
                    if lit.text == "use strict" {
                        return true;
                    }
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
    fn check_property_initialization(
        &mut self,
        class_idx: NodeIndex,
        class_decl: &tsz_parser::parser::node::ClassData,
    ) {
        // Skip if strict property initialization is not enabled
        if !self.ctx.compiler_options.strict_property_initialization {
            return;
        }

        // Collect properties that need to be checked and create a set of tracked properties
        let mut tracked: FxHashSet<PropertyKey> = FxHashSet::default();
        let mut properties: Vec<(PropertyKey, String, NodeIndex)> = Vec::new();

        for &member_idx in &class_decl.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Only check PropertyDeclaration nodes
            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }

            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };

            // Skip if property has definite assignment assertion (!)
            if prop.exclamation_token {
                continue;
            }

            // Skip if property has an initializer
            if !prop.initializer.is_none() {
                continue;
            }

            // Skip static properties
            if self
                .ctx
                .has_modifier(&prop.modifiers, SyntaxKind::StaticKeyword as u16)
            {
                continue;
            }

            // Skip abstract properties
            if self
                .ctx
                .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword as u16)
            {
                continue;
            }

            // Skip ambient properties (declare keyword)
            if self
                .ctx
                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword as u16)
            {
                continue;
            }

            // Get property name for error message and tracking
            let prop_name = self.get_property_name(prop.name);
            let key = self.property_to_key(prop.name, &prop_name);

            if let Some(k) = key {
                tracked.insert(k.clone());
                properties.push((k, prop_name, prop.name));
            }
        }

        if properties.is_empty() {
            return;
        }

        // Analyze constructor to find assigned properties
        let assigned = self.is_property_initialized_in_constructor(class_idx, &tracked);

        // Report errors for properties that are not initialized
        for (key, name, name_node) in properties {
            if assigned.contains(&key) {
                continue;
            }

            let message = diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR.replace("{0}", &name);

            // Get the span for the property name
            if let Some((pos, end)) = self.ctx.get_node_span(name_node) {
                self.ctx.error(pos, end - pos, message, 2564);
            }
        }
    }

    /// Get the name of a property as a string.
    fn get_property_name(&self, name_idx: NodeIndex) -> String {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                return ident.escaped_text.clone();
            }
            // Handle computed properties and private identifiers
            "[computed]".to_string()
        } else {
            "[unknown]".to_string()
        }
    }

    /// Convert a property name node to a PropertyKey for tracking.
    fn property_to_key(&self, name_idx: NodeIndex, name_str: &str) -> Option<PropertyKey> {
        if let Some(name_node) = self.ctx.arena.get(name_idx) {
            match name_node.kind {
                k if k == SyntaxKind::Identifier as u16 => {
                    Some(PropertyKey::Ident(name_str.to_string()))
                }
                k if k == SyntaxKind::PrivateIdentifier as u16 => {
                    Some(PropertyKey::Private(name_str.to_string()))
                }
                k if k == SyntaxKind::StringLiteral as u16 => Some(PropertyKey::Computed(
                    ComputedKey::String(name_str.to_string()),
                )),
                k if k == SyntaxKind::NumericLiteral as u16 => Some(PropertyKey::Computed(
                    ComputedKey::Number(name_str.to_string()),
                )),
                k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                    // For computed properties, try to get the identifier
                    self.ctx.arena.get_identifier(name_node).map(|ident| {
                        PropertyKey::Computed(ComputedKey::Ident(ident.escaped_text.clone()))
                    })
                }
                _ => None,
            }
        } else {
            None
        }
    }

    /// Check if properties are initialized in constructor using control flow analysis.
    ///
    /// Returns the set of properties that are definitely assigned in all code paths.
    fn is_property_initialized_in_constructor(
        &self,
        class_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FxHashSet<PropertyKey> {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return FxHashSet::default();
        };

        let Some(class_decl) = self.ctx.arena.get_class(node) else {
            return FxHashSet::default();
        };

        // Find the constructor body
        let constructor_body = self.find_constructor_body(&class_decl.members);

        if let Some(body_idx) = constructor_body {
            self.analyze_constructor_assignments(body_idx, tracked)
        } else {
            FxHashSet::default()
        }
    }

    /// Find the constructor body in class members.
    fn find_constructor_body(&self, members: &tsz_parser::parser::NodeList) -> Option<NodeIndex> {
        for &member_idx in &members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };
            if !ctor.body.is_none() {
                return Some(ctor.body);
            }
        }
        None
    }

    /// Analyze constructor to find which properties are assigned.
    fn analyze_constructor_assignments(
        &self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FxHashSet<PropertyKey> {
        let result = self.analyze_statement(body_idx, &FxHashSet::default(), tracked);
        self.flow_result_to_assigned(result)
    }

    /// Convert flow result to a set of definitely assigned properties.
    fn flow_result_to_assigned(&self, result: FlowResult) -> FxHashSet<PropertyKey> {
        let mut assigned = None;
        if let Some(normal) = result.normal {
            assigned = Some(normal);
        }
        if let Some(exits) = result.exits {
            assigned = Some(match assigned {
                Some(current) => self.intersect_sets(&current, &exits),
                None => exits,
            });
        }

        assigned.unwrap_or_default()
    }

    /// Intersect two sets of property keys.
    fn intersect_sets(
        &self,
        set1: &FxHashSet<PropertyKey>,
        set2: &FxHashSet<PropertyKey>,
    ) -> FxHashSet<PropertyKey> {
        set1.intersection(set2).cloned().collect()
    }

    /// Analyze a statement for property assignments.
    fn analyze_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        if stmt_idx.is_none() {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => self.analyze_block(stmt_idx, assigned_in, tracked),
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                self.analyze_expression_statement(stmt_idx, assigned_in, tracked)
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                self.analyze_if_statement(stmt_idx, assigned_in, tracked)
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                self.analyze_return_statement(assigned_in)
            }
            k if k == syntax_kind_ext::THROW_STATEMENT => self.analyze_throw_statement(assigned_in),
            k if k == syntax_kind_ext::WHILE_STATEMENT || k == syntax_kind_ext::FOR_STATEMENT => {
                // For loops, we conservatively assume the property might not be assigned
                FlowResult {
                    normal: Some(assigned_in.clone()),
                    exits: None,
                }
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                // For try statements, analyze both try and catch blocks
                self.analyze_try_statement(stmt_idx, assigned_in, tracked)
            }
            _ => FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            },
        }
    }

    /// Analyze a block of statements.
    fn analyze_block(
        &self,
        block_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let Some(node) = self.ctx.arena.get(block_idx) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        if node.kind != syntax_kind_ext::BLOCK {
            return self.analyze_statement(block_idx, assigned_in, tracked);
        }

        let Some(block) = self.ctx.arena.get_block(node) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        let statements = &block.statements;

        let mut current = assigned_in.clone();
        let mut exits: Option<FxHashSet<PropertyKey>> = None;

        for &stmt_idx in &statements.nodes {
            let result = self.analyze_statement(stmt_idx, &current, tracked);

            // If we hit a return or throw, track it as an exit
            if result.normal.is_none() {
                if let Some(exit_set) = result.exits {
                    exits = Some(match exits {
                        Some(current_exits) => self.intersect_sets(&current_exits, &exit_set),
                        None => exit_set,
                    });
                }
                // Don't update current after a return/throw
            } else if let Some(normal_set) = result.normal {
                current = normal_set;
                // Handle exits from statements that have both normal and exit flows (like if statements)
                if let Some(exit_set) = result.exits {
                    exits = Some(match exits {
                        Some(current_exits) => self.intersect_sets(&current_exits, &exit_set),
                        None => exit_set,
                    });
                }
            }
        }

        FlowResult {
            normal: Some(current),
            exits,
        }
    }

    /// Analyze an expression statement for property assignments.
    fn analyze_expression_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        let assigned =
            self.analyze_expression_for_assignment(expr_stmt.expression, assigned_in, tracked);

        FlowResult {
            normal: Some(assigned),
            exits: None,
        }
    }

    /// Analyze an expression for property assignments.
    fn analyze_expression_for_assignment(
        &self,
        expr_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FxHashSet<PropertyKey> {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return assigned_in.clone();
        };

        // Check for binary expression assignment (this.prop = value)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(bin_expr) = self.ctx.arena.get_binary_expr(node)
            && bin_expr.operator_token == SyntaxKind::EqualsToken as u16
        {
            // Check if left side is a property access (this.prop)
            if self.is_this_property_access(bin_expr.left)
                && let Some(prop_key) = self.extract_property_key(bin_expr.left)
            {
                // Only track if this property is in our tracked set
                if tracked.contains(&prop_key) {
                    let mut assigned = assigned_in.clone();
                    assigned.insert(prop_key);
                    return assigned;
                }
            }
        }

        assigned_in.clone()
    }

    /// Check if an expression is a `this.property` access.
    fn is_this_property_access(&self, expr_idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(prop_access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };

        // Check if the expression is `this`
        let Some(expr_node) = self.ctx.arena.get(prop_access.expression) else {
            return false;
        };

        expr_node.kind == SyntaxKind::ThisKeyword as u16
    }

    /// Extract the property key from a property access expression.
    fn extract_property_key(&self, expr_idx: NodeIndex) -> Option<PropertyKey> {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return None;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                let Some(prop_access) = self.ctx.arena.get_access_expr(node) else {
                    return None;
                };

                let Some(name_node) = self.ctx.arena.get(prop_access.name_or_argument) else {
                    return None;
                };

                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map(|ident| PropertyKey::Ident(ident.escaped_text.clone()))
            }
            k if k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION => {
                // Handle this["x"] = 1 pattern
                let Some(elem_access) = self.ctx.arena.get_access_expr(node) else {
                    return None;
                };

                // Extract the property name if it's a string literal
                let Some(arg_node) = self.ctx.arena.get(elem_access.name_or_argument) else {
                    return None;
                };

                if arg_node.kind == (SyntaxKind::StringLiteral as u16) {
                    self.ctx
                        .arena
                        .get_literal(arg_node)
                        .map(|lit| PropertyKey::Ident(lit.text.clone()))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Analyze an if statement.
    fn analyze_if_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        let Some(if_stmt) = self.ctx.arena.get_if_statement(node) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        // Analyze then branch
        let then_result = self.analyze_statement(if_stmt.then_statement, assigned_in, tracked);

        // Analyze else branch if present
        let else_result = if !if_stmt.else_statement.is_none() {
            Some(self.analyze_statement(if_stmt.else_statement, assigned_in, tracked))
        } else {
            None
        };

        // For if-else, we need the intersection of both branches
        // For if-only, we need to intersect with the input (property might not be assigned if condition is false)
        let normal = if let Some(ref else_res) = else_result {
            // Both branches exist - intersect them
            let then_set = then_result.normal.unwrap_or_default();
            let else_set = else_res.normal.clone().unwrap_or_default();
            Some(self.intersect_sets(&then_set, &else_set))
        } else {
            // Only then branch - intersect with input
            let then_set = then_result.normal.unwrap_or_else(|| assigned_in.clone());
            Some(self.intersect_sets(&then_set, assigned_in))
        };

        // Handle exits from both branches
        let mut exits: Option<FxHashSet<PropertyKey>> = None;
        if let Some(then_exits) = then_result.exits {
            exits = Some(match &else_result {
                Some(else_res) => {
                    if let Some(else_exits) = &else_res.exits {
                        self.intersect_sets(&then_exits, else_exits)
                    } else {
                        then_exits
                    }
                }
                None => then_exits,
            });
        }

        FlowResult { normal, exits }
    }

    /// Analyze a try statement.
    fn analyze_try_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> FlowResult {
        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        let Some(try_stmt) = self.ctx.arena.get_try(node) else {
            return FlowResult {
                normal: Some(assigned_in.clone()),
                exits: None,
            };
        };

        // Analyze try block
        let try_result = self.analyze_statement(try_stmt.try_block, assigned_in, tracked);

        // Analyze catch block if present
        let catch_result = if !try_stmt.catch_clause.is_none() {
            Some(self.analyze_statement(try_stmt.catch_clause, assigned_in, tracked))
        } else {
            None
        };

        // Analyze finally block if present
        let finally_result = if !try_stmt.finally_block.is_none() {
            Some(self.analyze_statement(try_stmt.finally_block, assigned_in, tracked))
        } else {
            None
        };

        // Conservative approach: only count assignments that are in all paths
        // For try-catch, we need the intersection
        let mut current = try_result.normal.unwrap_or_else(|| assigned_in.clone());

        if let Some(catch_res) = catch_result {
            let catch_set = catch_res.normal.unwrap_or_else(|| assigned_in.clone());
            current = self.intersect_sets(&current, &catch_set);
        }

        // Finally block always runs, so we can just update current
        if let Some(finally_res) = finally_result
            && let Some(finally_set) = finally_res.normal
        {
            current = finally_set;
        }

        FlowResult {
            normal: Some(current),
            exits: None,
        }
    }

    /// Analyze a return statement.
    fn analyze_return_statement(&self, assigned_in: &FxHashSet<PropertyKey>) -> FlowResult {
        FlowResult {
            normal: None,
            exits: Some(assigned_in.clone()),
        }
    }

    /// Analyze a throw statement.
    fn analyze_throw_statement(&self, assigned_in: &FxHashSet<PropertyKey>) -> FlowResult {
        FlowResult {
            normal: None,
            exits: Some(assigned_in.clone()),
        }
    }

    /// Check an interface declaration.
    pub fn check_interface_declaration(&mut self, _iface_idx: NodeIndex) {
        // Interface declaration checking is handled by CheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Heritage clauses
        // - Member signatures
    }

    /// Check a type alias declaration.
    pub fn check_type_alias_declaration(&mut self, _alias_idx: NodeIndex) {
        // Type alias checking is handled by CheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Type parameters
        // - Circular reference detection
    }

    /// Check an enum declaration.
    pub fn check_enum_declaration(&mut self, enum_idx: NodeIndex) {
        use crate::types::diagnostics::diagnostic_codes;
        use tsz_scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(enum_idx) else {
            return;
        };

        let Some(enum_data) = self.ctx.arena.get_enum(node) else {
            return;
        };

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
                    if !self.is_constant_expression(member_data.initializer) {
                        if let Some(init_node) = self.ctx.arena.get(member_data.initializer) {
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
        use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
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
            if !is_namespace {
                if let Some(name_node) = self.ctx.arena.get(module.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text.is_empty()
                {
                    self.ctx.error(
                        node.pos,
                        6, // length of "module"
                        format_message(
                            diagnostic_messages::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
                            &["module"],
                        ),
                        diagnostic_codes::CANNOT_FIND_NAME_DO_YOU_NEED_TO_INSTALL_TYPE_DEFINITIONS_FOR_NODE_TRY_NPM_I_SAVE,
                    );
                }
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
                        if let Some(mod_node) = self.ctx.arena.get(mod_idx) {
                            if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
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
                    if !parent.is_none() {
                        if let Some(parent_node) = self.ctx.arena.get(parent) {
                            if parent_node.kind == syntax_kind_ext::SOURCE_FILE {
                                allowed_context = self.is_external_module();
                            } else if parent_node.kind == syntax_kind_ext::MODULE_BLOCK {
                                if let Some(parent_ext) = self.ctx.arena.get_extended(parent) {
                                    let gp = parent_ext.parent;
                                    if let Some(gp_node) = self.ctx.arena.get(gp) {
                                        if gp_node.kind == syntax_kind_ext::MODULE_DECLARATION
                                            && let Some(gp_module) =
                                                self.ctx.arena.get_module(gp_node)
                                            && self.ctx.has_modifier(
                                                &gp_module.modifiers,
                                                SyntaxKind::DeclareKeyword as u16,
                                            )
                                        {
                                            let gp_name_node = self.ctx.arena.get(gp_module.name);
                                            let gp_is_string_named = gp_name_node
                                                .is_some_and(|name_node| {
                                                    name_node.kind
                                                        == SyntaxKind::StringLiteral as u16
                                                        || name_node.kind
                                                            == SyntaxKind::NoSubstitutionTemplateLiteral
                                                                as u16
                                                });
                                            if gp_is_string_named {
                                                allowed_context = true;
                                            }
                                        }
                                    }
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
                let module_key = module_specifier
                    .as_deref()
                    .map(|spec| self.normalize_module_augmentation_key(spec))
                    .unwrap_or_else(|| "<unknown>".to_string());

                let mut value_decl_map = self
                    .ctx
                    .module_augmentation_value_decls
                    .remove(&module_key)
                    .unwrap_or_default();
                let mut reported_import = false;
                let mut reported_export = false;
                if !module.body.is_none() {
                    if let Some(body_node) = self.ctx.arena.get(module.body) {
                        if body_node.kind == syntax_kind_ext::MODULE_BLOCK {
                            if let Some(block) = self.ctx.arena.get_module_block(body_node)
                                && let Some(ref stmts) = block.statements
                            {
                                let mut register_value_name =
                                    |name: &str, name_node: NodeIndex| -> bool {
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
                                        let is_forbidden_export = if kind
                                            == syntax_kind_ext::EXPORT_ASSIGNMENT
                                            || kind == syntax_kind_ext::NAMESPACE_EXPORT_DECLARATION
                                        {
                                            true
                                        } else if kind == syntax_kind_ext::EXPORT_DECLARATION {
                                            match self.ctx.arena.get_export_decl(stmt_node) {
                                                Some(export_decl) => {
                                                    if export_decl.is_default_export {
                                                        true
                                                    } else if !export_decl
                                                        .module_specifier
                                                        .is_none()
                                                    {
                                                        // Re-exports are not permitted in augmentations
                                                        true
                                                    } else if export_decl.export_clause.is_none() {
                                                        true
                                                    } else if let Some(clause_node) = self
                                                        .ctx
                                                        .arena
                                                        .get(export_decl.export_clause)
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
                                        let Some(export_decl) =
                                            self.ctx.arena.get_export_decl(stmt_node)
                                        else {
                                            continue;
                                        };
                                        if export_decl.is_default_export
                                            || !export_decl.module_specifier.is_none()
                                            || export_decl.export_clause.is_none()
                                        {
                                            continue;
                                        }
                                        let Some(clause_node) =
                                            self.ctx.arena.get(export_decl.export_clause)
                                        else {
                                            continue;
                                        };
                                        match clause_node.kind {
                                            syntax_kind_ext::VARIABLE_STATEMENT => {
                                                if let Some(var_stmt) =
                                                    self.ctx.arena.get_variable(clause_node)
                                                {
                                                    for &decl_list_idx in
                                                        &var_stmt.declarations.nodes
                                                    {
                                                        let Some(decl_list_node) =
                                                            self.ctx.arena.get(decl_list_idx)
                                                        else {
                                                            continue;
                                                        };
                                                        if decl_list_node.kind
                                                            == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                                                        {
                                                            if let Some(decl_list) = self
                                                                .ctx
                                                                .arena
                                                                .get_variable(decl_list_node)
                                                            {
                                                                for &decl_idx in
                                                                    &decl_list.declarations.nodes
                                                                {
                                                                    if let Some(decl_node) =
                                                                        self.ctx.arena.get(
                                                                            decl_idx,
                                                                        )
                                                                        && let Some(decl) = self
                                                                            .ctx
                                                                            .arena
                                                                            .get_variable_declaration(decl_node)
                                                                        && let Some(name_node) =
                                                                            self.ctx.arena.get(
                                                                                decl.name,
                                                                            )
                                                                        && let Some(ident) =
                                                                            self.ctx.arena.get_identifier(
                                                                                name_node,
                                                                            )
                                                                    {
                                                                        if register_value_name(
                                                                            &ident.escaped_text,
                                                                            decl.name,
                                                                        ) {
                                                                            if let Some(node) = self
                                                                                .ctx
                                                                                .arena
                                                                                .get(decl.name)
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
                                                            }
                                                        } else if let Some(decl) = self
                                                            .ctx
                                                            .arena
                                                            .get_variable_declaration(decl_list_node)
                                                            && let Some(name_node) =
                                                                self.ctx.arena.get(decl.name)
                                                            && let Some(ident) = self
                                                                .ctx
                                                                .arena
                                                                .get_identifier(name_node)
                                                        {
                                                            if register_value_name(
                                                                &ident.escaped_text,
                                                                decl.name,
                                                            ) {
                                                                if let Some(node) =
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
                                                    }
                                                }
                                            }
                                            syntax_kind_ext::FUNCTION_DECLARATION => {
                                                if let Some(func) =
                                                    self.ctx.arena.get_function(clause_node)
                                                    && let Some(name_node) =
                                                        self.ctx.arena.get(func.name)
                                                    && let Some(ident) =
                                                        self.ctx.arena.get_identifier(name_node)
                                                {
                                                    if register_value_name(
                                                        &ident.escaped_text,
                                                        func.name,
                                                    ) {
                                                        if let Some(node) =
                                                            self.ctx.arena.get(func.name)
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
                                            syntax_kind_ext::CLASS_DECLARATION => {
                                                if let Some(class) =
                                                    self.ctx.arena.get_class(clause_node)
                                                    && let Some(name_node) =
                                                        self.ctx.arena.get(class.name)
                                                    && let Some(ident) =
                                                        self.ctx.arena.get_identifier(name_node)
                                                {
                                                    if register_value_name(
                                                        &ident.escaped_text,
                                                        class.name,
                                                    ) {
                                                        if let Some(node) =
                                                            self.ctx.arena.get(class.name)
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
                                            syntax_kind_ext::ENUM_DECLARATION => {
                                                if let Some(enm) =
                                                    self.ctx.arena.get_enum(clause_node)
                                                    && let Some(name_node) =
                                                        self.ctx.arena.get(enm.name)
                                                    && let Some(ident) =
                                                        self.ctx.arena.get_identifier(name_node)
                                                {
                                                    if let Some(specifier) =
                                                        module_specifier.as_deref()
                                                        && let Some(target_idx) = self
                                                            .ctx
                                                            .resolve_import_target(specifier)
                                                        && let Some(target_binder) =
                                                            self.ctx.get_binder_for_file(target_idx)
                                                    {
                                                        let target_arena = self
                                                            .ctx
                                                            .get_arena_for_file(target_idx as u32);
                                                        if let Some(source_file) =
                                                            target_arena.source_files.first()
                                                            && let Some(existing_sym_id) =
                                                                target_binder
                                                                    .resolve_import_if_needed_public(
                                                                        &source_file.file_name,
                                                                        &ident.escaped_text,
                                                                    )
                                                        {
                                                            if let Some(symbol) = target_binder
                                                                .get_symbol(existing_sym_id)
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
                                                    }
                                                    if register_value_name(
                                                        &ident.escaped_text,
                                                        enm.name,
                                                    ) {
                                                        if let Some(node) =
                                                            self.ctx.arena.get(enm.name)
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
                                            _ => {}
                                        }
                                    } else if kind == syntax_kind_ext::VARIABLE_STATEMENT {
                                        if let Some(var_stmt) =
                                            self.ctx.arena.get_variable(stmt_node)
                                            && self.ctx.has_modifier(
                                                &var_stmt.modifiers,
                                                SyntaxKind::ExportKeyword as u16,
                                            )
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
                                                        for &decl_idx in
                                                            &decl_list.declarations.nodes
                                                        {
                                                            if let Some(decl_node) =
                                                                self.ctx.arena.get(decl_idx)
                                                                && let Some(decl) = self
                                                                    .ctx
                                                                    .arena
                                                                    .get_variable_declaration(
                                                                        decl_node,
                                                                    )
                                                                && let Some(name_node) =
                                                                    self.ctx.arena.get(decl.name)
                                                                && let Some(ident) = self
                                                                    .ctx
                                                                    .arena
                                                                    .get_identifier(name_node)
                                                            {
                                                                if register_value_name(
                                                                    &ident.escaped_text,
                                                                    decl.name,
                                                                ) {
                                                                    if let Some(node) = self
                                                                        .ctx
                                                                        .arena
                                                                        .get(decl.name)
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
                                                    }
                                                } else if let Some(decl) = self
                                                    .ctx
                                                    .arena
                                                    .get_variable_declaration(decl_list_node)
                                                    && let Some(name_node) =
                                                        self.ctx.arena.get(decl.name)
                                                    && let Some(ident) =
                                                        self.ctx.arena.get_identifier(name_node)
                                                {
                                                    if register_value_name(
                                                        &ident.escaped_text,
                                                        decl.name,
                                                    ) {
                                                        if let Some(node) =
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
                                            }
                                        }
                                    } else if kind == syntax_kind_ext::FUNCTION_DECLARATION {
                                        if let Some(func) = self.ctx.arena.get_function(stmt_node)
                                            && self.ctx.has_modifier(
                                                &func.modifiers,
                                                SyntaxKind::ExportKeyword as u16,
                                            )
                                            && let Some(name_node) = self.ctx.arena.get(func.name)
                                            && let Some(ident) =
                                                self.ctx.arena.get_identifier(name_node)
                                        {
                                            if register_value_name(&ident.escaped_text, func.name) {
                                                if let Some(node) = self.ctx.arena.get(func.name) {
                                                    self.ctx.error(
                                                        node.pos,
                                                        node.end - node.pos,
                                                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                        diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                    );
                                                }
                                            }
                                        }
                                    } else if kind == syntax_kind_ext::CLASS_DECLARATION {
                                        if let Some(class) = self.ctx.arena.get_class(stmt_node)
                                            && self.ctx.has_modifier(
                                                &class.modifiers,
                                                SyntaxKind::ExportKeyword as u16,
                                            )
                                            && let Some(name_node) = self.ctx.arena.get(class.name)
                                            && let Some(ident) =
                                                self.ctx.arena.get_identifier(name_node)
                                        {
                                            if register_value_name(&ident.escaped_text, class.name)
                                            {
                                                if let Some(node) = self.ctx.arena.get(class.name) {
                                                    self.ctx.error(
                                                        node.pos,
                                                        node.end - node.pos,
                                                        diagnostic_messages::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE.to_string(),
                                                        diagnostic_codes::CANNOT_REDECLARE_BLOCK_SCOPED_VARIABLE,
                                                    );
                                                }
                                            }
                                        }
                                    } else if kind == syntax_kind_ext::ENUM_DECLARATION {
                                        if let Some(enm) = self.ctx.arena.get_enum(stmt_node)
                                            && self.ctx.has_modifier(
                                                &enm.modifiers,
                                                SyntaxKind::ExportKeyword as u16,
                                            )
                                            && let Some(name_node) = self.ctx.arena.get(enm.name)
                                            && let Some(ident) =
                                                self.ctx.arena.get_identifier(name_node)
                                        {
                                            if register_value_name(&ident.escaped_text, enm.name) {
                                                if let Some(node) = self.ctx.arena.get(enm.name) {
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
                                }
                            }
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
        if let Some(ref map) = self.ctx.is_external_module_by_file {
            if let Some(&is_ext) = map.get(&self.ctx.file_name) {
                return is_ext;
            }
        }
        // Fallback to binder (for single-file mode or tests)
        self.ctx.binder.is_external_module()
    }

    /// Check if a module exists (for TS2664 check).
    /// Returns true if the module is in resolved_modules, module_exports,
    /// declared_modules, or shorthand_ambient_modules.
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
                if pattern.contains('*') {
                    if let Ok(glob) = globset::GlobBuilder::new(pattern)
                        .literal_separator(false)
                        .build()
                    {
                        if glob.compile_matcher().is_match(module_name) {
                            return true;
                        }
                    }
                }
            }
        }

        // Also check module_exports keys for wildcard patterns
        for pattern in self.ctx.binder.module_exports.keys() {
            let pattern = pattern.trim().trim_matches('"').trim_matches('\'');
            if pattern.contains('*') {
                if let Ok(glob) = globset::GlobBuilder::new(pattern)
                    .literal_separator(false)
                    .build()
                {
                    if glob.compile_matcher().is_match(module_name) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check if a module name is relative (starts with ./ or ../)
    fn is_relative_module_name(&self, name: &str) -> bool {
        name.starts_with("./") || name.starts_with("../") || name == "." || name == ".."
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
        if self.is_relative_module_name(name) {
            if let Some(parent) = Path::new(&self.ctx.file_name).parent() {
                let joined = parent.join(name);
                let normalized = Self::normalize_path(&joined);
                return normalized.to_string_lossy().to_string();
            }
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
            if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                if let Some(module) = self.ctx.arena.get_module(parent_node) {
                    if self
                        .ctx
                        .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword as u16)
                    {
                        return true;
                    }
                }
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
                }
            }
        } else if node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested module
            self.check_module_declaration(body_idx);
        }
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
        );

        if is_non_declaration {
            use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
            use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
            use crate::types::diagnostics::{diagnostic_codes, diagnostic_messages};
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
            use crate::types::diagnostics::diagnostic_codes;
            if let Some((pos, end)) = self.ctx.get_node_span(stmt_idx) {
                self.ctx.error(
                    pos,
                    end - pos,
                    "The 'with' statement is not supported. All symbols in a 'with' block will have type 'any'.".to_string(),
                    diagnostic_codes::THE_WITH_STATEMENT_IS_NOT_SUPPORTED_ALL_SYMBOLS_IN_A_WITH_BLOCK_WILL_HAVE_TYPE_A,
                );
            }
        }

        // Check labeled statements — the inner statement should also be checked
        if node.kind == syntax_kind_ext::LABELED_STATEMENT {
            if let Some(labeled) = self.ctx.arena.get_labeled_statement(node) {
                self.check_statement_in_ambient_context(labeled.statement);
            }
        }
    }

    /// Check parameter properties (only valid in constructors).
    pub fn check_parameter_properties(&mut self, parameters: &[NodeIndex]) {
        use crate::types::diagnostics::diagnostic_codes;

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
    pub fn check_function_implementations(&mut self, _nodes: &[NodeIndex]) {
        // Implementation of overload checking
        // Will be migrated from CheckerState
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_binder::BinderState;
    use tsz_parser::parser::ParserState;
    use tsz_solver::TypeInterner;

    #[test]
    fn test_declaration_checker_variable() {
        let source = "let x = 1;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );

        // Get the variable statement
        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);
            // Test passes if no panic
        }
    }

    #[test]
    fn test_module_augmentation_duplicate_value_exports() {
        let source = r#"
export {};

declare module "./a" {
    export const x = 0;
}

declare module "../dir/a" {
    export const x = 0;
}
"#;
        let file_name = "/dir/b.ts".to_string();
        let mut parser = ParserState::new(file_name.clone(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            file_name,
            crate::context::CheckerOptions::default(),
        );
        ctx.set_current_file_idx(0);

        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            for &stmt_idx in &sf_data.statements.nodes {
                if let Some(stmt_node) = parser.get_arena().get(stmt_idx)
                    && stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION
                {
                    checker.check_module_declaration(stmt_idx);
                }
            }
        }

        let ts2451_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2451).collect();
        assert_eq!(
            ts2451_errors.len(),
            1,
            "Expected 1 TS2451 error, got {}",
            ts2451_errors.len()
        );
    }

    #[test]
    fn test_ts2564_property_without_initializer() {
        // Test that TS2564 is reported for properties without initializers
        let source = r#"
class Foo {
    x: number;  // Should report TS2564
    y: string = "hello";  // Should NOT report (has initializer)
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have one TS2564 error for property 'x'
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                1,
                "Expected 1 TS2564 error, got {}",
                ts2564_errors.len()
            );

            // Verify the error message contains 'x'
            if let Some(err) = ts2564_errors.first() {
                assert!(
                    err.message_text.contains("x"),
                    "Error message should contain 'x', got: {}",
                    err.message_text
                );
            }
        }
    }

    #[test]
    fn test_ts2564_with_definite_assignment_assertion() {
        // Test that TS2564 is NOT reported for properties with definite assignment assertion (!)
        let source = r#"
class Foo {
    x!: number;  // Should NOT report (has definite assignment assertion)
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have NO TS2564 errors
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                0,
                "Expected 0 TS2564 errors, got {}",
                ts2564_errors.len()
            );
        }
    }

    #[test]
    fn test_ts2564_skips_static_properties() {
        // Test that TS2564 is NOT reported for static properties
        let source = r#"
class Foo {
    static x: number;  // Should NOT report (static property)
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have NO TS2564 errors
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                0,
                "Expected 0 TS2564 errors, got {}",
                ts2564_errors.len()
            );
        }
    }

    #[test]
    fn test_ts2564_disabled_when_strict_false() {
        // Test that TS2564 is NOT reported when strict mode is disabled
        let source = r#"
class Foo {
    x: number;  // Should NOT report (strict mode disabled)
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions::default(),
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have NO TS2564 errors (strict mode disabled)
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                0,
                "Expected 0 TS2564 errors when strict mode disabled, got {}",
                ts2564_errors.len()
            );
        }
    }

    // ========== Phase 2 Tests: Control Flow Analysis ==========

    #[test]
    fn test_ts2564_phase2_simple_constructor_initialization() {
        // Test that TS2564 is NOT reported for properties initialized in simple constructor
        let source = r#"
class Foo {
    x: number;  // Should NOT report (initialized in constructor)
    constructor() {
        this.x = 1;
    }
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have NO TS2564 errors (property initialized in constructor)
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                0,
                "Expected 0 TS2564 errors for constructor-initialized property, got {}",
                ts2564_errors.len()
            );
        }
    }

    #[test]
    fn test_ts2564_phase2_conditional_all_paths_assigned() {
        // Test that TS2564 is NOT reported when property is initialized on all code paths
        let source = r#"
class Foo {
    x: number;  // Should NOT report (initialized on all paths)
    constructor(flag: boolean) {
        if (flag) {
            this.x = 1;
        } else {
            this.x = 2;
        }
    }
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have NO TS2564 errors (property initialized on all paths)
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                0,
                "Expected 0 TS2564 errors for property initialized on all paths, got {}",
                ts2564_errors.len()
            );
        }
    }

    #[test]
    fn test_ts2564_phase2_conditional_not_all_paths_assigned() {
        // Test that TS2564 IS reported when property is not initialized on all code paths
        let source = r#"
class Foo {
    x: number;  // Should report TS2564 (not initialized on all paths)
    constructor(flag: boolean) {
        if (flag) {
            this.x = 1;
        }
        // else branch doesn't assign this.x
    }
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have 1 TS2564 error (property not initialized on all paths)
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                1,
                "Expected 1 TS2564 error for property not initialized on all paths, got {}",
                ts2564_errors.len()
            );
        }
    }

    #[test]
    fn test_ts2564_phase2_return_statement_exits() {
        // Test that TS2564 IS reported when property is not initialized before early return
        let source = r#"
class Foo {
    x: number;  // Should report TS2564 (not initialized before early return)
    constructor(flag: boolean) {
        if (flag) {
            return;
        }
        this.x = 1;
    }
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have 1 TS2564 error (property not initialized on all exit paths)
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                1,
                "Expected 1 TS2564 error for property not initialized before early return, got {}",
                ts2564_errors.len()
            );
        }
    }

    #[test]
    fn test_ts2564_phase2_multiple_properties() {
        // Test mixed scenario: some properties initialized, some not
        let source = r#"
class Foo {
    x: number;  // Should NOT report (initialized in constructor)
    y: string;  // Should report TS2564 (not initialized)
    z: boolean = true;  // Should NOT report (has initializer)
    constructor() {
        this.x = 1;
    }
}
"#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::context::CheckerOptions {
                strict: true,
                strict_property_initialization: true,
                ..Default::default()
            },
        );

        if let Some(root_node) = parser.get_arena().get(root)
            && let Some(sf_data) = parser.get_arena().get_source_file(root_node)
            && let Some(&stmt_idx) = sf_data.statements.nodes.first()
        {
            let mut checker = DeclarationChecker::new(&mut ctx);
            checker.check(stmt_idx);

            // Should have 1 TS2564 error for 'y'
            let ts2564_errors: Vec<_> = ctx.diagnostics.iter().filter(|d| d.code == 2564).collect();

            assert_eq!(
                ts2564_errors.len(),
                1,
                "Expected 1 TS2564 error for property 'y', got {}",
                ts2564_errors.len()
            );

            if let Some(err) = ts2564_errors.first() {
                assert!(
                    err.message_text.contains("y"),
                    "Error message should contain 'y', got: {}",
                    err.message_text
                );
            }
        }
    }
}
