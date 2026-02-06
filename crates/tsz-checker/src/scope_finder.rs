//! Scope Finding Module
//!
//! This module contains methods for finding enclosing scopes and contexts.
//! It handles:
//! - Finding enclosing functions (regular and non-arrow)
//! - Finding enclosing variable statements and declarations
//! - Finding enclosing source files
//! - Finding enclosing static blocks, computed properties, and heritage clauses
//! - Finding class contexts for various member types
//!
//! This module extends CheckerState with scope-finding methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

// TODO: Remove this once the methods are used by the checker
#![allow(dead_code)]

use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

// =============================================================================
// Scope Finding Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Function Enclosure
    // =========================================================================

    /// Find the enclosing function for a given node.
    ///
    /// Traverses up the AST to find the first function-like node
    /// (FunctionDeclaration, FunctionExpression, ArrowFunction, Method, etc.).
    ///
    /// Returns Some(NodeIndex) if inside a function, None if at module/global scope.
    pub(crate) fn find_enclosing_function(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current)
                && node.is_function_like()
            {
                return Some(current);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the enclosing NON-ARROW function for a given node.
    ///
    /// Returns Some(NodeIndex) if inside a non-arrow function (function declaration/expression),
    /// None if at module/global scope or only inside arrow functions.
    ///
    /// This is used for `this` type checking: arrow functions capture `this` from their
    /// enclosing scope, so we need to skip past them to find the actual function that
    /// defines the `this` context.
    pub(crate) fn find_enclosing_non_arrow_function(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::*;
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == FUNCTION_DECLARATION
                    || node.kind == FUNCTION_EXPRESSION
                    || node.kind == METHOD_DECLARATION
                    || node.kind == CONSTRUCTOR
                    || node.kind == GET_ACCESSOR
                    || node.kind == SET_ACCESSOR
                {
                    return Some(current);
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    // =========================================================================
    // Variable Enclosure
    // =========================================================================

    /// Find the enclosing variable statement for a given node.
    ///
    /// Traverses up the AST to find a VARIABLE_STATEMENT.
    ///
    /// Returns Some(NodeIndex) if a variable statement is found, None otherwise.
    pub(crate) fn find_enclosing_variable_statement(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current)
                && node.kind == syntax_kind_ext::VARIABLE_STATEMENT
            {
                return Some(current);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the enclosing variable declaration for a given node.
    ///
    /// Traverses up the AST to find a VARIABLE_DECLARATION.
    ///
    /// Returns Some(NodeIndex) if a variable declaration is found, None otherwise.
    pub(crate) fn find_enclosing_variable_declaration(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            let node = self.ctx.arena.get(current)?;
            if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                return Some(current);
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
    }

    // =========================================================================
    // Source File Enclosure
    // =========================================================================

    /// Find the enclosing source file for a given node.
    ///
    /// Traverses up the AST to find the SOURCE_FILE node.
    ///
    /// Returns Some(NodeIndex) if a source file is found, None otherwise.
    pub(crate) fn find_enclosing_source_file(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current)
                && node.kind == syntax_kind_ext::SOURCE_FILE
            {
                return Some(current);
            }
            if let Some(ext) = self.ctx.arena.get_extended(current) {
                current = ext.parent;
            } else {
                break;
            }
        }
        None
    }

    // =========================================================================
    // Static Block Enclosure
    // =========================================================================

    /// Find the enclosing static block for a given node.
    ///
    /// Traverses up the AST to find a CLASS_STATIC_BLOCK_DECLARATION.
    /// Stops at function boundaries to avoid considering outer static blocks.
    ///
    /// Returns Some(NodeIndex) if inside a static block, None otherwise.
    pub(crate) fn find_enclosing_static_block(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        let mut iterations = 0;
        while !current.is_none() {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                    return Some(current);
                }
                // Stop at function boundaries (don't consider outer static blocks)
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the class declaration containing a static block.
    ///
    /// Given a static block node, returns the parent CLASS_DECLARATION or CLASS_EXPRESSION.
    ///
    /// Returns Some(NodeIndex) if the parent is a class, None otherwise.
    pub(crate) fn find_class_for_static_block(
        &self,
        static_block_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let ext = self.ctx.arena.get_extended(static_block_idx)?;
        let parent = ext.parent;
        if parent.is_none() {
            return None;
        }
        let parent_node = self.ctx.arena.get(parent)?;
        if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
        {
            Some(parent)
        } else {
            None
        }
    }

    // =========================================================================
    // Computed Property Enclosure
    // =========================================================================

    /// Find the enclosing computed property name for a given node.
    ///
    /// Traverses up the AST to find a COMPUTED_PROPERTY_NAME.
    /// Stops at function boundaries (computed properties inside functions are evaluated at call time).
    ///
    /// Returns Some(NodeIndex) if inside a computed property name, None otherwise.
    pub(crate) fn find_enclosing_computed_property(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    return Some(current);
                }
                // Stop at function boundaries
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the class declaration containing a computed property name.
    ///
    /// Walks up from a computed property to find the containing class member,
    /// then finds the class declaration.
    ///
    /// Returns Some(NodeIndex) if the parent is a class, None otherwise.
    pub(crate) fn find_class_for_computed_property(
        &self,
        computed_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let mut current = computed_idx;
        while !current.is_none() {
            let ext = self.ctx.arena.get_extended(current)?;
            let parent = ext.parent;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    // =========================================================================
    // Heritage Clause Enclosure
    // =========================================================================

    /// Find the enclosing heritage clause (extends/implements) for a node.
    ///
    /// Returns the NodeIndex of the HERITAGE_CLAUSE if the node is inside one.
    /// Stops at function/class/interface boundaries.
    ///
    /// Returns Some(NodeIndex) if inside a heritage clause, None otherwise.
    pub(crate) fn find_enclosing_heritage_clause(&self, idx: NodeIndex) -> Option<NodeIndex> {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;

        let mut current = idx;
        while !current.is_none() {
            if let Some(node) = self.ctx.arena.get(current) {
                if node.kind == HERITAGE_CLAUSE {
                    return Some(current);
                }
                // Stop at function/class/interface boundaries
                if node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || node.kind == syntax_kind_ext::CONSTRUCTOR
                    || node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    || node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                {
                    return None;
                }
            }
            let ext = self.ctx.arena.get_extended(current)?;
            if ext.parent.is_none() {
                return None;
            }
            current = ext.parent;
        }
        None
    }

    /// Find the class or interface declaration containing a heritage clause.
    ///
    /// Given a heritage clause node, returns the parent CLASS_DECLARATION,
    /// CLASS_EXPRESSION, or INTERFACE_DECLARATION.
    ///
    /// Returns Some(NodeIndex) if the parent is a class/interface, None otherwise.
    pub(crate) fn find_class_for_heritage_clause(
        &self,
        heritage_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let ext = self.ctx.arena.get_extended(heritage_idx)?;
        let parent = ext.parent;
        if parent.is_none() {
            return None;
        }
        let parent_node = self.ctx.arena.get(parent)?;
        if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
            || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION
            || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
        {
            Some(parent)
        } else {
            None
        }
    }
}
