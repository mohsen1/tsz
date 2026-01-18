//! Declaration Type Checking
//!
//! Handles classes, interfaces, functions, and variable declarations.
//! This module separates declaration checking logic from the monolithic ThinCheckerState.

use super::context::CheckerContext;
use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages};
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use std::collections::HashSet;

/// Declaration type checker that operates on the shared context.
///
/// This is a stateless checker that borrows the context mutably.
/// All declaration type checking goes through this checker.
pub struct DeclarationChecker<'a, 'ctx> {
    pub ctx: &'a mut CheckerContext<'ctx>,
}

/// Property key for tracking property assignments in control flow analysis.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum PropertyKey {
    Ident(String),
    Private(String),
    Computed(ComputedKey),
}

/// Computed property key.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
#[allow(dead_code)]
enum ComputedKey {
    Ident(String),
    String(String),
    Number(String),
    Qualified(String),
    /// Symbol call like Symbol("key") or Symbol() - stores optional description
    Symbol(Option<String>),
}

/// Result of control flow analysis for property assignments.
#[derive(Clone, Debug)]
struct FlowResult {
    normal: Option<HashSet<PropertyKey>>,
    exits: Option<HashSet<PropertyKey>>,
}

impl<'a, 'ctx> DeclarationChecker<'a, 'ctx> {
    /// Create a new declaration checker with a mutable context reference.
    pub fn new(ctx: &'a mut CheckerContext<'ctx>) -> Self {
        Self { ctx }
    }

    /// Check a declaration node.
    ///
    /// This dispatches to specialized handlers based on declaration kind.
    /// Currently a skeleton - logic will be migrated incrementally from ThinCheckerState.
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
    pub fn check_variable_declaration(&mut self, _decl_idx: NodeIndex) {
        // Variable declaration checking is handled by ThinCheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Type annotation vs initializer type compatibility
        // - Adding variable to scope
    }

    /// Check a function declaration.
    pub fn check_function_declaration(&mut self, _func_idx: NodeIndex) {
        // Function declaration checking is handled by ThinCheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Parameter types
        // - Return type vs actual returns
        // - Body statements
    }

    /// Check a class declaration.
    pub fn check_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return;
        };

        // Get the class data
        let Some(class_decl) = self.ctx.arena.get_class(node) else {
            return;
        };

        // Skip ambient classes (declare keyword)
        if self
            .ctx
            .has_modifier(&class_decl.modifiers, SyntaxKind::DeclareKeyword as u16)
        {
            return;
        }

        // Check property initialization if strictPropertyInitialization is enabled
        if self.ctx.strict_property_initialization() {
            self.check_property_initialization(class_idx, class_decl);
        }

        // Additional class checks will be added here:
        // - Heritage clauses (extends/implements)
        // - Member types and modifiers
        // - Abstract implementation requirements
        // - Constructor parameter properties
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
        class_decl: &crate::parser::thin_node::ClassData,
    ) {
        // Collect properties that need to be checked and create a set of tracked properties
        let mut tracked: HashSet<PropertyKey> = HashSet::new();
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

            let message =
                diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER.replace("{0}", &name);

            // Get the span for the property name
            if let Some((pos, end)) = self.ctx.get_node_span(name_node) {
                self.ctx.error(
                    pos,
                    end - pos,
                    message,
                    diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER,
                );
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
                k if k == SyntaxKind::StringLiteral as u16 => {
                    Some(PropertyKey::Computed(ComputedKey::String(name_str.to_string())))
                }
                k if k == SyntaxKind::NumericLiteral as u16 => {
                    Some(PropertyKey::Computed(ComputedKey::Number(name_str.to_string())))
                }
                k if k == syntax_kind_ext::COMPUTED_PROPERTY_NAME => {
                    // For computed properties, try to get the identifier
                    if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                        Some(PropertyKey::Computed(ComputedKey::Ident(ident.escaped_text.clone())))
                    } else {
                        None
                    }
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
        tracked: &HashSet<PropertyKey>,
    ) -> HashSet<PropertyKey> {
        let Some(node) = self.ctx.arena.get(class_idx) else {
            return HashSet::new();
        };

        let Some(class_decl) = self.ctx.arena.get_class(node) else {
            return HashSet::new();
        };

        // Find the constructor body
        let constructor_body = self.find_constructor_body(&class_decl.members);

        if let Some(body_idx) = constructor_body {
            self.analyze_constructor_assignments(body_idx, tracked)
        } else {
            HashSet::new()
        }
    }

    /// Find the constructor body in class members.
    fn find_constructor_body(&self, members: &crate::parser::NodeList) -> Option<NodeIndex> {
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
        tracked: &HashSet<PropertyKey>,
    ) -> HashSet<PropertyKey> {
        let result = self.analyze_statement(body_idx, &HashSet::default(), tracked);
        self.flow_result_to_assigned(result)
    }

    /// Convert flow result to a set of definitely assigned properties.
    fn flow_result_to_assigned(&self, result: FlowResult) -> HashSet<PropertyKey> {
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
        set1: &HashSet<PropertyKey>,
        set2: &HashSet<PropertyKey>,
    ) -> HashSet<PropertyKey> {
        set1.intersection(set2).cloned().collect()
    }

    /// Analyze a statement for property assignments.
    fn analyze_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &HashSet<PropertyKey>,
        tracked: &HashSet<PropertyKey>,
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
            k if k == syntax_kind_ext::THROW_STATEMENT => {
                self.analyze_throw_statement(assigned_in)
            }
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
        assigned_in: &HashSet<PropertyKey>,
        tracked: &HashSet<PropertyKey>,
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
        let mut exits: Option<HashSet<PropertyKey>> = None;

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
        assigned_in: &HashSet<PropertyKey>,
        tracked: &HashSet<PropertyKey>,
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

        let assigned = self.analyze_expression_for_assignment(expr_stmt.expression, assigned_in, tracked);

        FlowResult {
            normal: Some(assigned),
            exits: None,
        }
    }

    /// Analyze an expression for property assignments.
    fn analyze_expression_for_assignment(
        &self,
        expr_idx: NodeIndex,
        assigned_in: &HashSet<PropertyKey>,
        tracked: &HashSet<PropertyKey>,
    ) -> HashSet<PropertyKey> {
        let Some(node) = self.ctx.arena.get(expr_idx) else {
            return assigned_in.clone();
        };

        // Check for binary expression assignment (this.prop = value)
        if node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            if let Some(bin_expr) = self.ctx.arena.get_binary_expr(node) {
                if bin_expr.operator_token == SyntaxKind::EqualsToken as u16 {
                    // Check if left side is a property access (this.prop)
                    if self.is_this_property_access(bin_expr.left) {
                        if let Some(prop_key) = self.extract_property_key(bin_expr.left) {
                            // Only track if this property is in our tracked set
                            if tracked.contains(&prop_key) {
                                let mut assigned = assigned_in.clone();
                                assigned.insert(prop_key);
                                return assigned;
                            }
                        }
                    }
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

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }

        let Some(prop_access) = self.ctx.arena.get_access_expr(node) else {
            return None;
        };

        let Some(name_node) = self.ctx.arena.get(prop_access.name_or_argument) else {
            return None;
        };

        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            Some(PropertyKey::Ident(ident.escaped_text.clone()))
        } else {
            None
        }
    }

    /// Analyze an if statement.
    fn analyze_if_statement(
        &self,
        stmt_idx: NodeIndex,
        assigned_in: &HashSet<PropertyKey>,
        tracked: &HashSet<PropertyKey>,
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
        let mut exits: Option<HashSet<PropertyKey>> = None;
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
        assigned_in: &HashSet<PropertyKey>,
        tracked: &HashSet<PropertyKey>,
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
        if let Some(finally_res) = finally_result {
            if let Some(finally_set) = finally_res.normal {
                current = finally_set;
            }
        }

        FlowResult {
            normal: Some(current),
            exits: None,
        }
    }

    /// Analyze a return statement.
    fn analyze_return_statement(&self, assigned_in: &HashSet<PropertyKey>) -> FlowResult {
        FlowResult {
            normal: None,
            exits: Some(assigned_in.clone()),
        }
    }

    /// Analyze a throw statement.
    fn analyze_throw_statement(&self, assigned_in: &HashSet<PropertyKey>) -> FlowResult {
        FlowResult {
            normal: None,
            exits: Some(assigned_in.clone()),
        }
    }

    /// Check an interface declaration.
    pub fn check_interface_declaration(&mut self, _iface_idx: NodeIndex) {
        // Interface declaration checking is handled by ThinCheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Heritage clauses
        // - Member signatures
    }

    /// Check a type alias declaration.
    pub fn check_type_alias_declaration(&mut self, _alias_idx: NodeIndex) {
        // Type alias checking is handled by ThinCheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Type parameters
        // - Circular reference detection
    }

    /// Check an enum declaration.
    pub fn check_enum_declaration(&mut self, _enum_idx: NodeIndex) {
        // Enum declaration checking is handled by ThinCheckerState for now
        // Will be migrated incrementally
        // Key checks:
        // - Member types (numeric vs string)
        // - Computed members
    }

    /// Check a module/namespace declaration.
    pub fn check_module_declaration(&mut self, module_idx: NodeIndex) {
        use crate::checker::types::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use crate::scanner::SyntaxKind;

        let Some(node) = self.ctx.arena.get(module_idx) else {
            return;
        };

        if let Some(module) = self.ctx.arena.get_module(node) {
            // TS5061: Check for relative module names in ambient declarations
            // declare module "./foo" { } -> Error
            if self
                .ctx
                .has_modifier(&module.modifiers, SyntaxKind::DeclareKeyword as u16)
            {
                if let Some(name_node) = self.ctx.arena.get(module.name) {
                    if name_node.kind == SyntaxKind::StringLiteral as u16 {
                        if let Some(lit) = self.ctx.arena.get_literal(name_node) {
                            // Check TS5061 first
                            if self.is_relative_module_name(&lit.text) {
                                self.ctx.error(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    diagnostic_messages::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME.to_string(),
                                    diagnostic_codes::AMBIENT_MODULE_DECLARATION_CANNOT_SPECIFY_RELATIVE_MODULE_NAME,
                                );
                            }
                            // TS2664: Check if the module being augmented exists
                            // declare module "nonexistent" { } -> Error if module doesn't exist
                            // Only emit TS2664 in .ts files, not .d.ts files
                            // In .d.ts files, module augmentations are allowed even if the module doesn't exist
                            else if !self.module_exists(&lit.text) && !self.is_declaration_file() {
                                let message = format_message(
                                    diagnostic_messages::INVALID_MODULE_NAME_IN_AUGMENTATION,
                                    &[&lit.text]
                                );
                                self.ctx.error(
                                    name_node.pos,
                                    name_node.end - name_node.pos,
                                    message,
                                    diagnostic_codes::INVALID_MODULE_NAME_IN_AUGMENTATION,
                                );
                            }
                        }
                    }
                }
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

    /// Check if a module exists (for TS2664 check).
    /// Returns true if the module is in resolved_modules or module_exports.
    fn module_exists(&self, module_name: &str) -> bool {
        // Check if the module was resolved by the CLI driver (multi-file mode)
        if let Some(ref resolved) = self.ctx.resolved_modules {
            if resolved.contains(module_name) {
                return true;
            }
        }

        // Check if the module exists in the module_exports map (cross-file module resolution)
        if self.ctx.binder.module_exports.contains_key(module_name) {
            return true;
        }

        false
    }

    /// Check if a module name is relative (starts with ./ or ../)
    fn is_relative_module_name(&self, name: &str) -> bool {
        name.starts_with("./") || name.starts_with("../") || name == "." || name == ".."
    }

    /// Check a module body (block or nested module).
    fn check_module_body(&mut self, body_idx: NodeIndex) {
        let Some(node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::MODULE_BLOCK {
            if let Some(block) = self.ctx.arena.get_module_block(node) {
                if let Some(ref stmts) = block.statements {
                    for &stmt_idx in &stmts.nodes {
                        // Dispatch to statement/declaration checking
                        // Currently a no-op - will call StatementChecker
                        let _ = stmt_idx;
                    }
                }
            }
        } else if node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested module
            self.check_module_declaration(body_idx);
        }
    }

    /// Check parameter properties (only valid in constructors).
    pub fn check_parameter_properties(&mut self, parameters: &[NodeIndex]) {
        use crate::checker::types::diagnostics::diagnostic_codes;

        for &param_idx in parameters {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };

            if let Some(param) = self.ctx.arena.get_parameter(node) {
                // If parameter has accessibility modifiers (public/private/protected/readonly)
                // and we're not in a constructor, report error
                if param.modifiers.is_some() {
                    if let Some((pos, end)) = self.ctx.get_node_span(param_idx) {
                        self.ctx.error(
                            pos,
                            end - pos,
                            "A parameter property is only allowed in a constructor implementation."
                                .to_string(),
                            diagnostic_codes::PARAMETER_PROPERTY_NOT_ALLOWED,
                        );
                    }
                }
            }
        }
    }

    /// Check function implementations for overload sequences.
    pub fn check_function_implementations(&mut self, _nodes: &[NodeIndex]) {
        // Implementation of overload checking
        // Will be migrated from ThinCheckerState
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::types::diagnostics::diagnostic_codes;
    use crate::solver::TypeInterner;
    use crate::thin_binder::ThinBinderState;
    use crate::thin_parser::ThinParserState;

    #[test]
    fn test_declaration_checker_variable() {
        let source = "let x = 1;";
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx =
            CheckerContext::new(parser.get_arena(), &binder, &types, "test.ts".to_string(), crate::checker::context::CheckerOptions::default());

        // Get the variable statement
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);
                    // Test passes if no panic
                }
            }
        }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have one TS2564 error for property 'x'
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have NO TS2564 errors
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

                    assert_eq!(
                        ts2564_errors.len(),
                        0,
                        "Expected 0 TS2564 errors, got {}",
                        ts2564_errors.len()
                    );
                }
            }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have NO TS2564 errors
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

                    assert_eq!(
                        ts2564_errors.len(),
                        0,
                        "Expected 0 TS2564 errors, got {}",
                        ts2564_errors.len()
                    );
                }
            }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions::default(),
        );

        // Get the class declaration
        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have NO TS2564 errors (strict mode disabled)
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

                    assert_eq!(
                        ts2564_errors.len(),
                        0,
                        "Expected 0 TS2564 errors when strict mode disabled, got {}",
                        ts2564_errors.len()
                    );
                }
            }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have NO TS2564 errors (property initialized in constructor)
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

                    assert_eq!(
                        ts2564_errors.len(),
                        0,
                        "Expected 0 TS2564 errors for constructor-initialized property, got {}",
                        ts2564_errors.len()
                    );
                }
            }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have NO TS2564 errors (property initialized on all paths)
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

                    assert_eq!(
                        ts2564_errors.len(),
                        0,
                        "Expected 0 TS2564 errors for property initialized on all paths, got {}",
                        ts2564_errors.len()
                    );
                }
            }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have 1 TS2564 error (property not initialized on all paths)
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

                    assert_eq!(
                        ts2564_errors.len(),
                        1,
                        "Expected 1 TS2564 error for property not initialized on all paths, got {}",
                        ts2564_errors.len()
                    );
                }
            }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have 1 TS2564 error (property not initialized on all exit paths)
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

                    assert_eq!(
                        ts2564_errors.len(),
                        1,
                        "Expected 1 TS2564 error for property not initialized before early return, got {}",
                        ts2564_errors.len()
                    );
                }
            }
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
        let mut parser = ThinParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = ThinBinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let types = TypeInterner::new();
        let mut ctx = CheckerContext::new(
            parser.get_arena(),
            &binder,
            &types,
            "test.ts".to_string(),
            crate::checker::context::CheckerOptions { strict: true, strict_property_initialization: true, ..Default::default() },
        );

        if let Some(root_node) = parser.get_arena().get(root) {
            if let Some(sf_data) = parser.get_arena().get_source_file(root_node) {
                if let Some(&stmt_idx) = sf_data.statements.nodes.first() {
                    let mut checker = DeclarationChecker::new(&mut ctx);
                    checker.check(stmt_idx);

                    // Should have 1 TS2564 error for 'y'
                    let ts2564_errors: Vec<_> = ctx
                        .diagnostics
                        .iter()
                        .filter(|d| d.code == diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER)
                        .collect();

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
    }
}
