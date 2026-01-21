//! Block Scoping ES5 Transform
//!
//! Transforms ES6 `let` and `const` to ES5 `var` with proper scoping semantics.
//!
//! ## Basic Transform
//! ```typescript
//! let x = 1;
//! const y = 2;
//! ```
//! Becomes:
//! ```javascript
//! var x = 1;
//! var y = 2;
//! ```
//!
//! ## Loop Capture Transform (complex)
//! When loop variables are captured by closures, TypeScript emits an IIFE pattern:
//! ```typescript
//! for (let i = 0; i < 3; i++) {
//!     setTimeout(() => console.log(i), 100);
//! }
//! ```
//! Becomes:
//! ```javascript
//! var _loop_1 = function (i) {
//!     setTimeout(function () { return console.log(i); }, 100);
//! };
//! for (var i = 0; i < 3; i++) {
//!     _loop_1(i);
//! }
//! ```

use crate::parser::node::{Node, NodeArena};
use crate::parser::{NodeIndex, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use rustc_hash::FxHashSet;
use std::collections::HashMap;

/// State for block scoping transformation
#[derive(Debug, Default)]
pub struct BlockScopeState {
    /// Stack of scopes, each containing variable names declared in that scope
    /// Maps original name -> emitted name (e.g., "x" -> "x_1" if renamed)
    scope_stack: Vec<HashMap<String, String>>,

    /// Counter for generating unique loop function names (_loop_1, _loop_2, etc.)
    loop_counter: u32,

    /// Counter for generating unique renamed variable suffixes
    rename_counter: u32,
}

impl BlockScopeState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a new block scope
    pub fn enter_scope(&mut self) {
        self.scope_stack.push(HashMap::new());
    }

    /// Exit the current block scope
    pub fn exit_scope(&mut self) {
        self.scope_stack.pop();
    }

    /// Register a variable declaration in the current scope
    /// Returns the name to emit (may be renamed if shadowing)
    pub fn register_variable(&mut self, original_name: &str) -> String {
        // Check if this name exists in any parent scope (shadowing)
        let needs_rename = self
            .scope_stack
            .iter()
            .any(|scope| scope.contains_key(original_name));

        let emitted_name = if needs_rename {
            self.rename_counter += 1;
            format!("{}_{}", original_name, self.rename_counter)
        } else {
            original_name.to_string()
        };

        // Register in current scope
        if let Some(current_scope) = self.scope_stack.last_mut() {
            current_scope.insert(original_name.to_string(), emitted_name.clone());
        }

        emitted_name
    }

    /// Look up the emitted name for a variable reference
    pub fn get_emitted_name(&self, original_name: &str) -> Option<String> {
        // Search from innermost to outermost scope
        for scope in self.scope_stack.iter().rev() {
            if let Some(name) = scope.get(original_name) {
                return Some(name.clone());
            }
        }
        None
    }

    /// Get the next loop function name
    pub fn next_loop_function_name(&mut self) -> String {
        self.loop_counter += 1;
        format!("_loop_{}", self.loop_counter)
    }

    /// Reset state for a new file
    pub fn reset(&mut self) {
        self.scope_stack.clear();
        self.loop_counter = 0;
        self.rename_counter = 0;
    }
}

/// Result of analyzing a loop for variable capture
#[derive(Debug, Default)]
pub struct LoopCaptureInfo {
    /// Whether any loop variables are captured by closures
    pub needs_capture: bool,

    /// Names of variables that are captured
    pub captured_vars: Vec<String>,
}

/// Analyze whether loop variables are captured by closures in the loop body
///
/// This is needed to determine if we need the IIFE pattern for the loop
pub fn analyze_loop_capture(
    arena: &NodeArena,
    body_idx: NodeIndex,
    loop_vars: &[String],
) -> LoopCaptureInfo {
    let mut info = LoopCaptureInfo::default();

    if loop_vars.is_empty() {
        return info;
    }

    let var_set: FxHashSet<&str> = loop_vars.iter().map(|s| s.as_str()).collect();

    // Recursively check for closures that capture loop variables
    check_closure_capture(arena, body_idx, &var_set, &mut info, false);

    info
}

/// Recursively check for closures that capture variables from the loop
fn check_closure_capture(
    arena: &NodeArena,
    idx: NodeIndex,
    loop_vars: &FxHashSet<&str>,
    info: &mut LoopCaptureInfo,
    inside_closure: bool,
) {
    let Some(node) = arena.get(idx) else { return };

    match node.kind {
        // Function declarations/expressions/arrows create closures
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::ARROW_FUNCTION =>
        {
            // Inside this function, we're in a closure
            if let Some(func) = arena.get_function(node) {
                // Check parameters
                for &param_idx in &func.parameters.nodes {
                    check_closure_capture(arena, param_idx, loop_vars, info, true);
                }
                // Check body
                check_closure_capture(arena, func.body, loop_vars, info, true);
            }
        }

        // Identifier - check if it references a captured variable
        k if k == SyntaxKind::Identifier as u16 => {
            if inside_closure
                && let Some(ident) = arena.get_identifier(node)
                && loop_vars.contains(ident.escaped_text.as_str())
            {
                if !info.captured_vars.contains(&ident.escaped_text) {
                    info.captured_vars.push(ident.escaped_text.clone());
                }
                info.needs_capture = true;
            }
        }

        // For all other nodes, recurse into children
        _ => {
            // Visit all children
            visit_children(arena, node, |child_idx| {
                check_closure_capture(arena, child_idx, loop_vars, info, inside_closure);
            });
        }
    }
}

/// Helper to visit children of a node
fn visit_children<F: FnMut(NodeIndex)>(arena: &NodeArena, node: &Node, mut visitor: F) {
    match node.kind {
        k if k == syntax_kind_ext::BLOCK => {
            if let Some(block) = arena.get_block(node) {
                for &stmt_idx in &block.statements.nodes {
                    visitor(stmt_idx);
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
            if let Some(var_stmt) = arena.get_variable(node) {
                for &decl_idx in &var_stmt.declarations.nodes {
                    visitor(decl_idx);
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_DECLARATION_LIST => {
            if let Some(decl_list) = arena.get_variable(node) {
                for &decl_idx in &decl_list.declarations.nodes {
                    visitor(decl_idx);
                }
            }
        }
        k if k == syntax_kind_ext::VARIABLE_DECLARATION => {
            if let Some(decl) = arena.get_variable_declaration(node) {
                visitor(decl.name);
                visitor(decl.initializer);
            }
        }
        k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
            if let Some(expr_stmt) = arena.get_expression_statement(node) {
                visitor(expr_stmt.expression);
            }
        }
        k if k == syntax_kind_ext::CALL_EXPRESSION => {
            if let Some(call) = arena.get_call_expr(node) {
                visitor(call.expression);
                if let Some(ref args) = call.arguments {
                    for &arg_idx in &args.nodes {
                        visitor(arg_idx);
                    }
                }
            }
        }
        k if k == syntax_kind_ext::BINARY_EXPRESSION => {
            if let Some(bin) = arena.get_binary_expr(node) {
                visitor(bin.left);
                visitor(bin.right);
            }
        }
        k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION =>
        {
            if let Some(access) = arena.get_access_expr(node) {
                visitor(access.expression);
                visitor(access.name_or_argument);
            }
        }
        k if k == syntax_kind_ext::PARENTHESIZED_EXPRESSION => {
            if let Some(unary) = arena.get_unary_expr(node) {
                visitor(unary.operand);
            }
        }
        k if k == syntax_kind_ext::IF_STATEMENT => {
            if let Some(if_stmt) = arena.get_if_statement(node) {
                visitor(if_stmt.expression);
                visitor(if_stmt.then_statement);
                visitor(if_stmt.else_statement);
            }
        }
        // For, while, do-while all use get_loop
        k if k == syntax_kind_ext::FOR_STATEMENT
            || k == syntax_kind_ext::WHILE_STATEMENT
            || k == syntax_kind_ext::DO_STATEMENT =>
        {
            if let Some(loop_data) = arena.get_loop(node) {
                visitor(loop_data.initializer);
                visitor(loop_data.condition);
                visitor(loop_data.incrementor);
                visitor(loop_data.statement);
            }
        }
        k if k == syntax_kind_ext::RETURN_STATEMENT => {
            if let Some(ret) = arena.get_return_statement(node) {
                visitor(ret.expression);
            }
        }
        // Add more node types as needed
        _ => {}
    }
}

/// Collect variable names from a for loop initializer
pub fn collect_loop_vars(arena: &NodeArena, initializer_idx: NodeIndex) -> Vec<String> {
    let mut vars = Vec::new();

    let Some(node) = arena.get(initializer_idx) else {
        return vars;
    };

    // Initializer can be VARIABLE_DECLARATION_LIST or expression
    if node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
        && let Some(decl_list) = arena.get_variable(node)
    {
        for &decl_idx in &decl_list.declarations.nodes {
            if let Some(decl_node) = arena.get(decl_idx)
                && let Some(decl) = arena.get_variable_declaration(decl_node)
                && let Some(name_node) = arena.get(decl.name)
                && name_node.kind == SyntaxKind::Identifier as u16
                && let Some(ident) = arena.get_identifier(name_node)
            {
                vars.push(ident.escaped_text.clone());
            }
        }
    }

    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_scope_state() {
        let mut state = BlockScopeState::new();

        state.enter_scope();
        assert_eq!(state.register_variable("x"), "x");
        assert_eq!(state.get_emitted_name("x"), Some("x".to_string()));

        state.enter_scope();
        // Shadowing - should rename
        assert_eq!(state.register_variable("x"), "x_1");
        assert_eq!(state.get_emitted_name("x"), Some("x_1".to_string()));

        state.exit_scope();
        // Back to outer scope
        assert_eq!(state.get_emitted_name("x"), Some("x".to_string()));

        state.exit_scope();
    }

    #[test]
    fn test_loop_function_names() {
        let mut state = BlockScopeState::new();

        assert_eq!(state.next_loop_function_name(), "_loop_1");
        assert_eq!(state.next_loop_function_name(), "_loop_2");
        assert_eq!(state.next_loop_function_name(), "_loop_3");
    }
}

#[cfg(test)]
#[path = "block_scoping_es5_tests.rs"]
mod block_scoping_es5_tests;
