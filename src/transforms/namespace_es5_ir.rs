//! ES5 Namespace Transform (IR-based)
//!
//! Transforms TypeScript namespaces to ES5 IIFE patterns, producing IR nodes.
//!
//! # Architecture
//!
//! This module provides two main types:
//! - `NamespaceES5Transformer`: The main transformer struct that produces IR nodes
//! - `NamespaceTransformContext`: A context helper for namespace transformations
//!
//! # Examples
//!
//! Simple namespace:
//! ```typescript
//! namespace foo {
//!     export class Provide { }
//! }
//! ```
//!
//! Becomes IR that prints as:
//! ```javascript
//! var foo;
//! (function (foo) {
//!     var Provide = /** @class */ (function () {
//!         function Provide() { }
//!         return Provide;
//!     }());
//!     foo.Provide = Provide;
//! })(foo || (foo = {}));
//! ```
//!
//! Qualified namespace name (A.B.C) produces nested IIFEs:
//! ```typescript
//! namespace A.B.C {
//!     export const x = 1;
//! }
//! ```
//!
//! Becomes:
//! ```javascript
//! var A;
//! (function (A) {
//!     var B;
//!     (function (B) {
//!         var C;
//!         (function (C) {
//!             var x = 1;
//!             C.x = x;
//!         })(C = B.C || (B.C = {}));
//!     })(B = A.B || (A.B = {}));
//! })(A || (A = {}));
//! ```

use crate::parser::node::NodeArena;
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

// =============================================================================
// NamespaceES5Transformer - Main transformer struct
// =============================================================================

/// ES5 Namespace Transformer
///
/// Transforms TypeScript namespace declarations into ES5-compatible IIFE patterns.
/// This is the primary entry point for namespace IR transformations.
///
/// # Example
///
/// ```ignore
/// use crate::transforms::namespace_es5_ir::NamespaceES5Transformer;
/// use crate::transforms::ir_printer::IRPrinter;
///
/// let transformer = NamespaceES5Transformer::new(&arena);
/// if let Some(ir) = transformer.transform_namespace(ns_idx) {
///     let output = IRPrinter::emit_to_string(&ir);
/// }
/// ```
pub struct NamespaceES5Transformer<'a> {
    arena: &'a NodeArena,
    is_commonjs: bool,
}

impl<'a> NamespaceES5Transformer<'a> {
    /// Create a new namespace transformer
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            is_commonjs: false,
        }
    }

    /// Create a namespace transformer with CommonJS mode enabled
    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        Self { arena, is_commonjs }
    }

    /// Set CommonJS mode
    pub fn set_commonjs(&mut self, is_commonjs: bool) {
        self.is_commonjs = is_commonjs;
    }

    /// Transform a namespace declaration to IR
    ///
    /// Returns `Some(IRNode::NamespaceIIFE { ... })` for valid namespaces,
    /// or `None` for ambient namespaces (declare namespace) or invalid nodes.
    ///
    /// # Arguments
    ///
    /// * `ns_idx` - NodeIndex of the namespace declaration
    ///
    /// # Returns
    ///
    /// `Option<IRNode>` - The transformed namespace as an IR node, or None if skipped
    pub fn transform_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        self.transform_namespace_with_export_flag(ns_idx, false)
    }

    /// Transform a namespace declaration that is known to be exported
    ///
    /// Use this when the namespace is wrapped in an EXPORT_DECLARATION.
    pub fn transform_exported_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        self.transform_namespace_with_export_flag(ns_idx, true)
    }

    /// Transform a namespace declaration with explicit export flag
    fn transform_namespace_with_export_flag(
        &self,
        ns_idx: NodeIndex,
        force_exported: bool,
    ) -> Option<IRNode> {
        let ns_node = self.arena.get(ns_idx)?;
        let ns_data = self.arena.get_module(ns_node)?;

        // Skip ambient namespaces (declare namespace)
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts for qualified names (A.B.C)
        // The parser creates nested MODULE_DECLARATION nodes for qualified names:
        // MODULE_DECLARATION "A" -> body: MODULE_DECLARATION "B" -> body: MODULE_DECLARATION "C" -> body: MODULE_BLOCK
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        // Check if exported from modifiers OR if forced (when wrapped in EXPORT_DECLARATION)
        let is_exported = force_exported || has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform the innermost body - use the last name part for member exports
        let body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types)
        if body.is_empty() {
            return None;
        }

        // Root name is the first part
        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
        })
    }

    /// Flatten a module name into parts (handles both identifiers and qualified names)
    ///
    /// For qualified names like `A.B.C` (parsed as nested MODULE_DECLARATIONs), returns `["A", "B", "C"]`.
    /// For simple identifiers like `foo`, returns `["foo"]`.
    ///
    /// Note: The parser creates nested MODULE_DECLARATION nodes for qualified namespace names,
    /// where each level has a single identifier name and the body points to the next level.
    pub fn flatten_module_name(&self, name_idx: NodeIndex) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        self.collect_name_parts(name_idx, &mut parts);
        if parts.is_empty() { None } else { Some(parts) }
    }

    /// Recursively collect name parts from qualified names
    ///
    /// Handles both:
    /// 1. QUALIFIED_NAME nodes (left.right structure)
    /// 2. Simple identifier nodes
    fn collect_name_parts(&self, idx: NodeIndex, parts: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // QualifiedName has left and right - recurse into both
            if let Some(qn_data) = self.arena.qualified_names.get(node.data_index as usize) {
                self.collect_name_parts(qn_data.left, parts);
                self.collect_name_parts(qn_data.right, parts);
            }
        } else if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
        {
            parts.push(ident.escaped_text.clone());
        }
    }

    /// Collect all name parts by walking through nested MODULE_DECLARATION chain
    ///
    /// For `namespace A.B.C {}`, the parser creates:
    /// MODULE_DECLARATION "A" -> body: MODULE_DECLARATION "B" -> body: MODULE_DECLARATION "C" -> body: MODULE_BLOCK
    ///
    /// This method walks through all levels and returns (["A", "B", "C"], innermost_body_idx)
    fn collect_all_namespace_parts(&self, ns_idx: NodeIndex) -> Option<(Vec<String>, NodeIndex)> {
        let mut parts = Vec::new();
        let mut current_idx = ns_idx;

        loop {
            let node = self.arena.get(current_idx)?;
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                // We've reached a non-namespace node (likely MODULE_BLOCK)
                break;
            }

            let ns_data = self.arena.get_module(node)?;

            // Get the name of this level
            let name_node = self.arena.get(ns_data.name)?;
            if let Some(ident) = self.arena.get_identifier(name_node) {
                parts.push(ident.escaped_text.clone());
            }

            // Check if body is another MODULE_DECLARATION (nested namespace) or MODULE_BLOCK
            let body_node = self.arena.get(ns_data.body)?;
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Continue walking nested declarations
                current_idx = ns_data.body;
            } else {
                // We've reached the innermost body (MODULE_BLOCK)
                return Some((parts, ns_data.body));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some((parts, current_idx))
        }
    }

    /// Transform namespace body into IR nodes
    fn transform_namespace_body(&self, body_idx: NodeIndex, name_parts: &[String]) -> Vec<IRNode> {
        let mut result = Vec::new();

        // The innermost namespace name (last part) is used for member exports
        let ns_name = name_parts.last().map(|s| s.as_str()).unwrap_or("");

        let Some(body_node) = self.arena.get(body_idx) else {
            return result;
        };

        // Check if it's a module block
        if let Some(block_data) = self.arena.get_module_block(body_node)
            && let Some(ref stmts) = block_data.statements
        {
            for &stmt_idx in &stmts.nodes {
                if let Some(ir) = self.transform_namespace_member(ns_name, stmt_idx) {
                    result.push(ir);
                }
            }
        }

        result
    }

    /// Transform a namespace member to IR
    fn transform_namespace_member(&self, ns_name: &str, member_idx: NodeIndex) -> Option<IRNode> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Handle export declarations by extracting the inner declaration
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    self.transform_namespace_member_exported(ns_name, export_data.export_clause)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => None,
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => None,
            _ => Some(IRNode::ASTRef(member_idx)),
        }
    }

    /// Transform an exported namespace member
    fn transform_namespace_member_exported(
        &self,
        ns_name: &str,
        decl_idx: NodeIndex,
    ) -> Option<IRNode> {
        let decl_node = self.arena.get(decl_idx)?;

        match decl_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace_exported(ns_name, decl_idx)
            }
            _ => None,
        }
    }

    /// Transform a function in namespace
    fn transform_function_in_namespace(
        &self,
        ns_name: &str,
        func_idx: NodeIndex,
    ) -> Option<IRNode> {
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        // Skip declaration-only functions (no body)
        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let is_exported = has_export_modifier(self.arena, &func_data.modifiers);

        if is_exported {
            Some(IRNode::Sequence(vec![
                IRNode::ASTRef(func_idx),
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: func_name.clone(),
                    value: Box::new(IRNode::Identifier(func_name)),
                },
            ]))
        } else {
            Some(IRNode::ASTRef(func_idx))
        }
    }

    /// Transform an exported function
    fn transform_function_exported(&self, ns_name: &str, func_idx: NodeIndex) -> Option<IRNode> {
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        Some(IRNode::Sequence(vec![
            IRNode::ASTRef(func_idx),
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: func_name.clone(),
                value: Box::new(IRNode::Identifier(func_name)),
            },
        ]))
    }

    /// Transform a class in namespace
    fn transform_class_in_namespace(&self, ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        let is_exported = has_export_modifier(self.arena, &class_data.modifiers);

        if is_exported {
            Some(IRNode::Sequence(vec![
                IRNode::ASTRef(class_idx),
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: class_name.clone(),
                    value: Box::new(IRNode::Identifier(class_name)),
                },
            ]))
        } else {
            Some(IRNode::ASTRef(class_idx))
        }
    }

    /// Transform an exported class
    fn transform_class_exported(&self, ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        Some(IRNode::Sequence(vec![
            IRNode::ASTRef(class_idx),
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: class_name.clone(),
                value: Box::new(IRNode::Identifier(class_name)),
            },
        ]))
    }

    /// Transform a variable statement in namespace
    fn transform_variable_in_namespace(&self, ns_name: &str, var_idx: NodeIndex) -> Option<IRNode> {
        let var_node = self.arena.get(var_idx)?;
        let var_data = self.arena.get_variable(var_node)?;

        let is_exported = has_export_modifier(self.arena, &var_data.modifiers);

        let mut result = vec![IRNode::ASTRef(var_idx)];

        if is_exported {
            let var_names = collect_variable_names(self.arena, &var_data.declarations);
            for name in var_names {
                result.push(IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: name.clone(),
                    value: Box::new(IRNode::Identifier(name)),
                });
            }
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an exported variable
    fn transform_variable_exported(&self, ns_name: &str, var_idx: NodeIndex) -> Option<IRNode> {
        let var_node = self.arena.get(var_idx)?;
        let var_data = self.arena.get_variable(var_node)?;

        let mut result = vec![IRNode::ASTRef(var_idx)];

        // Always export since this is from an export declaration
        let var_names = collect_variable_names(self.arena, &var_data.declarations);
        for name in var_names {
            result.push(IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: name.clone(),
                value: Box::new(IRNode::Identifier(name)),
            });
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an enum in namespace
    fn transform_enum_in_namespace(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        let is_exported = has_export_modifier(self.arena, &enum_data.modifiers);

        let mut result = vec![IRNode::ASTRef(enum_idx)];

        if is_exported {
            result.push(IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            });
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an exported enum
    fn transform_enum_exported(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        Some(IRNode::Sequence(vec![
            IRNode::ASTRef(enum_idx),
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            },
        ]))
    }

    /// Transform a nested namespace
    fn transform_nested_namespace(&self, _parent_ns: &str, ns_idx: NodeIndex) -> Option<IRNode> {
        let ns_node = self.arena.get(ns_idx)?;
        let ns_data = self.arena.get_module(ns_node)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        let name_parts = self.flatten_module_name(ns_data.name)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform body
        let body = self.transform_namespace_body(ns_data.body, &name_parts);

        // Skip non-instantiated namespaces (only contain types)
        if body.is_empty() {
            return None;
        }

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
        })
    }

    /// Transform an exported nested namespace
    fn transform_nested_namespace_exported(
        &self,
        _parent_ns: &str,
        ns_idx: NodeIndex,
    ) -> Option<IRNode> {
        let ns_node = self.arena.get(ns_idx)?;
        let ns_data = self.arena.get_module(ns_node)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        let name_parts = self.flatten_module_name(ns_data.name)?;
        if name_parts.is_empty() {
            return None;
        }

        // Always exported since this is from an export declaration
        let is_exported = true;

        // Transform body
        let body = self.transform_namespace_body(ns_data.body, &name_parts);

        // Skip non-instantiated namespaces (only contain types)
        if body.is_empty() {
            return None;
        }

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
        })
    }
}

// =============================================================================
// NamespaceTransformContext - Legacy context (for backward compatibility)
// =============================================================================

/// Context for namespace transformation (legacy, use NamespaceES5Transformer instead)
pub struct NamespaceTransformContext<'a> {
    arena: &'a NodeArena,
    is_commonjs: bool,
}

impl<'a> NamespaceTransformContext<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            is_commonjs: false,
        }
    }

    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        Self { arena, is_commonjs }
    }

    /// Transform a namespace declaration to IR
    pub fn transform_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        let ns_node = self.arena.get(ns_idx)?;
        let ns_data = self.arena.get_module(ns_node)?;

        // Skip ambient namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts (handles nested MODULE_DECLARATIONs for A.B.C)
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform body
        let body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types)
        if body.is_empty() {
            return None;
        }

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: self.is_commonjs,
        })
    }

    /// Collect all name parts by walking through nested MODULE_DECLARATION chain
    fn collect_all_namespace_parts(&self, ns_idx: NodeIndex) -> Option<(Vec<String>, NodeIndex)> {
        let mut parts = Vec::new();
        let mut current_idx = ns_idx;

        loop {
            let node = self.arena.get(current_idx)?;
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                break;
            }

            let ns_data = self.arena.get_module(node)?;

            // Get the name of this level
            let name_node = self.arena.get(ns_data.name)?;
            if let Some(ident) = self.arena.get_identifier(name_node) {
                parts.push(ident.escaped_text.clone());
            }

            // Check if body is another MODULE_DECLARATION (nested namespace) or MODULE_BLOCK
            let body_node = self.arena.get(ns_data.body)?;
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                current_idx = ns_data.body;
            } else {
                return Some((parts, ns_data.body));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some((parts, current_idx))
        }
    }

    /// Flatten a module name into parts (for simple/qualified names)
    #[allow(dead_code)]
    fn flatten_module_name(&self, name_idx: NodeIndex) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        self.collect_name_parts(name_idx, &mut parts);
        if parts.is_empty() { None } else { Some(parts) }
    }

    /// Recursively collect name parts from qualified names
    #[allow(dead_code)]
    fn collect_name_parts(&self, idx: NodeIndex, parts: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // QualifiedName has left and right
            if let Some(qn_data) = self.arena.qualified_names.get(node.data_index as usize) {
                self.collect_name_parts(qn_data.left, parts);
                self.collect_name_parts(qn_data.right, parts);
            }
        } else if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
        {
            parts.push(ident.escaped_text.clone());
        }
    }

    /// Transform namespace body
    fn transform_namespace_body(&self, body_idx: NodeIndex, name_parts: &[String]) -> Vec<IRNode> {
        let mut result = Vec::new();
        let ns_name = name_parts.last().map(|s| s.as_str()).unwrap_or("");

        let Some(body_node) = self.arena.get(body_idx) else {
            return result;
        };

        // Check if it's a module block
        if let Some(block_data) = self.arena.get_module_block(body_node)
            && let Some(ref stmts) = block_data.statements
        {
            for &stmt_idx in &stmts.nodes {
                if let Some(ir) = self.transform_namespace_member(ns_name, stmt_idx) {
                    result.push(ir);
                }
            }
        }

        result
    }

    /// Transform a namespace member
    fn transform_namespace_member(&self, ns_name: &str, member_idx: NodeIndex) -> Option<IRNode> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Handle export declarations by extracting the inner declaration
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    self.transform_namespace_member_exported(ns_name, export_data.export_clause)
                } else {
                    None
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_in_namespace(ns_name, member_idx)
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => None,
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => None,
            _ => Some(IRNode::ASTRef(member_idx)),
        }
    }

    /// Transform an exported namespace member
    pub fn transform_namespace_member_exported(
        &self,
        ns_name: &str,
        decl_idx: NodeIndex,
    ) -> Option<IRNode> {
        let decl_node = self.arena.get(decl_idx)?;

        match decl_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_in_namespace_exported(ns_name, decl_idx)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace_exported(ns_name, decl_idx)
            }
            _ => None,
        }
    }

    /// Transform a function in namespace context
    pub fn transform_function_in_namespace(
        &self,
        ns_name: &str,
        func_idx: NodeIndex,
    ) -> Option<IRNode> {
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        // Skip declaration-only functions
        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let is_exported = has_export_modifier(self.arena, &func_data.modifiers);

        if is_exported {
            // Return AST ref + namespace export
            Some(IRNode::Sequence(vec![
                IRNode::ASTRef(func_idx),
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: func_name.clone(),
                    value: Box::new(IRNode::Identifier(func_name)),
                },
            ]))
        } else {
            Some(IRNode::ASTRef(func_idx))
        }
    }

    /// Transform an exported function in namespace
    fn transform_function_in_namespace_exported(
        &self,
        ns_name: &str,
        func_idx: NodeIndex,
    ) -> Option<IRNode> {
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        Some(IRNode::Sequence(vec![
            IRNode::ASTRef(func_idx),
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: func_name.clone(),
                value: Box::new(IRNode::Identifier(func_name)),
            },
        ]))
    }

    /// Transform a class in namespace context
    fn transform_class_in_namespace(&self, ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        let is_exported = has_export_modifier(self.arena, &class_data.modifiers);

        if is_exported {
            Some(IRNode::Sequence(vec![
                IRNode::ASTRef(class_idx),
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: class_name.clone(),
                    value: Box::new(IRNode::Identifier(class_name)),
                },
            ]))
        } else {
            Some(IRNode::ASTRef(class_idx))
        }
    }

    /// Transform an exported class in namespace
    fn transform_class_in_namespace_exported(
        &self,
        ns_name: &str,
        class_idx: NodeIndex,
    ) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        Some(IRNode::Sequence(vec![
            IRNode::ASTRef(class_idx),
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: class_name.clone(),
                value: Box::new(IRNode::Identifier(class_name)),
            },
        ]))
    }

    /// Transform a variable statement in namespace
    pub fn transform_variable_in_namespace(
        &self,
        ns_name: &str,
        var_idx: NodeIndex,
    ) -> Option<IRNode> {
        let var_node = self.arena.get(var_idx)?;
        let var_data = self.arena.get_variable(var_node)?;

        let is_exported = has_export_modifier(self.arena, &var_data.modifiers);

        let mut result = vec![IRNode::ASTRef(var_idx)];

        if is_exported {
            // Collect variable names for export
            let var_names = collect_variable_names(self.arena, &var_data.declarations);
            for name in var_names {
                result.push(IRNode::NamespaceExport {
                    namespace: ns_name.to_string(),
                    name: name.clone(),
                    value: Box::new(IRNode::Identifier(name)),
                });
            }
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an exported variable statement in namespace
    fn transform_variable_in_namespace_exported(
        &self,
        ns_name: &str,
        var_idx: NodeIndex,
    ) -> Option<IRNode> {
        let var_node = self.arena.get(var_idx)?;
        let var_data = self.arena.get_variable(var_node)?;

        let mut result = vec![IRNode::ASTRef(var_idx)];

        // Always export
        let var_names = collect_variable_names(self.arena, &var_data.declarations);
        for name in var_names {
            result.push(IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: name.clone(),
                value: Box::new(IRNode::Identifier(name)),
            });
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an enum in namespace
    fn transform_enum_in_namespace(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        let is_exported = has_export_modifier(self.arena, &enum_data.modifiers);

        let mut result = vec![IRNode::ASTRef(enum_idx)];

        if is_exported {
            result.push(IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            });
        }

        Some(IRNode::Sequence(result))
    }

    /// Transform an exported enum in namespace
    fn transform_enum_in_namespace_exported(
        &self,
        ns_name: &str,
        enum_idx: NodeIndex,
    ) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        Some(IRNode::Sequence(vec![
            IRNode::ASTRef(enum_idx),
            IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::Identifier(enum_name)),
            },
        ]))
    }

    /// Transform a nested namespace
    pub fn transform_nested_namespace(
        &self,
        _parent_ns: &str,
        ns_idx: NodeIndex,
    ) -> Option<IRNode> {
        let ns_node = self.arena.get(ns_idx)?;
        let ns_data = self.arena.get_module(ns_node)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts (handles nested MODULE_DECLARATIONs)
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform body
        let body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types)
        if body.is_empty() {
            return None;
        }

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: self.is_commonjs,
        })
    }

    /// Transform an exported nested namespace
    fn transform_nested_namespace_exported(
        &self,
        _parent_ns: &str,
        ns_idx: NodeIndex,
    ) -> Option<IRNode> {
        let ns_node = self.arena.get(ns_idx)?;
        let ns_data = self.arena.get_module(ns_node)?;

        // Skip ambient nested namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Collect all namespace parts (handles nested MODULE_DECLARATIONs)
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = true; // Always exported

        // Transform body
        let body = self.transform_namespace_body(innermost_body, &name_parts);

        // Skip non-instantiated namespaces (only contain types)
        if body.is_empty() {
            return None;
        }

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: self.is_commonjs,
        })
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

fn get_identifier_text(arena: &NodeArena, idx: NodeIndex) -> Option<String> {
    let node = arena.get(idx)?;
    if node.kind == SyntaxKind::Identifier as u16 {
        arena.get_identifier(node).map(|id| id.escaped_text.clone())
    } else {
        None
    }
}

fn has_modifier(arena: &NodeArena, modifiers: &Option<NodeList>, kind: u16) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx)
                && mod_node.kind == kind
            {
                return true;
            }
        }
    }
    false
}

fn has_declare_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::DeclareKeyword as u16)
}

fn has_export_modifier(arena: &NodeArena, modifiers: &Option<NodeList>) -> bool {
    has_modifier(arena, modifiers, SyntaxKind::ExportKeyword as u16)
}

/// Collect variable names from a declaration list
fn collect_variable_names(arena: &NodeArena, declarations: &NodeList) -> Vec<String> {
    let mut names = Vec::new();

    for &decl_list_idx in &declarations.nodes {
        if let Some(decl_list_node) = arena.get(decl_list_idx)
            && let Some(decl_list) = arena.get_variable(decl_list_node)
        {
            for &decl_idx in &decl_list.declarations.nodes {
                if let Some(decl_node) = arena.get(decl_idx)
                    && let Some(decl) = arena.get_variable_declaration(decl_node)
                    && let Some(name) = get_identifier_text(arena, decl.name)
                {
                    names.push(name);
                }
            }
        }
    }

    names
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;
    use crate::transforms::ir_printer::IRPrinter;

    /// Helper info for namespace extraction
    struct NamespaceInfo {
        ns_idx: NodeIndex,
        is_exported: bool,
    }

    /// Helper to find the namespace node (unwraps EXPORT_DECLARATION if needed)
    fn find_namespace_info(parser: &ParserState, stmt_idx: NodeIndex) -> Option<NamespaceInfo> {
        let stmt_node = parser.arena.get(stmt_idx)?;

        // If it's an export declaration, get the inner namespace
        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
            if let Some(export_data) = parser.arena.get_export_decl(stmt_node) {
                let inner_node = parser.arena.get(export_data.export_clause)?;
                if inner_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                    return Some(NamespaceInfo {
                        ns_idx: export_data.export_clause,
                        is_exported: true,
                    });
                }
            }
            return None;
        }

        // Otherwise, if it's a namespace directly
        if stmt_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            return Some(NamespaceInfo {
                ns_idx: stmt_idx,
                is_exported: false,
            });
        }

        None
    }

    /// Helper to parse and transform a namespace, returning the IR node
    fn transform_namespace(source: &str) -> Option<IRNode> {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&stmt_idx) = source_file.statements.nodes.first()
        {
            let info = find_namespace_info(&parser, stmt_idx)?;
            let transformer = NamespaceES5Transformer::new(&parser.arena);
            if info.is_exported {
                return transformer.transform_exported_namespace(info.ns_idx);
            } else {
                return transformer.transform_namespace(info.ns_idx);
            }
        }
        None
    }

    /// Helper to parse, transform and emit a namespace to string
    fn transform_and_emit(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&stmt_idx) = source_file.statements.nodes.first()
        {
            if let Some(info) = find_namespace_info(&parser, stmt_idx) {
                let transformer = NamespaceES5Transformer::new(&parser.arena);
                let ir = if info.is_exported {
                    transformer.transform_exported_namespace(info.ns_idx)
                } else {
                    transformer.transform_namespace(info.ns_idx)
                };
                if let Some(ir) = ir {
                    return IRPrinter::emit_to_string(&ir);
                }
            }
        }
        String::new()
    }

    /// Helper to parse, transform and emit with CommonJS mode
    fn transform_and_emit_commonjs(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&stmt_idx) = source_file.statements.nodes.first()
        {
            if let Some(info) = find_namespace_info(&parser, stmt_idx) {
                let transformer = NamespaceES5Transformer::with_commonjs(&parser.arena, true);
                let ir = if info.is_exported {
                    transformer.transform_exported_namespace(info.ns_idx)
                } else {
                    transformer.transform_namespace(info.ns_idx)
                };
                if let Some(ir) = ir {
                    return IRPrinter::emit_to_string(&ir);
                }
            }
        }
        String::new()
    }

    // =========================================================================
    // Basic namespace tests
    // =========================================================================

    #[test]
    fn test_namespace_es5_empty_namespace_skipped() {
        let ir = transform_namespace("namespace M { }");
        assert!(ir.is_none(), "Empty namespace should produce no IR");
    }

    #[test]
    fn test_namespace_es5_simple_namespace() {
        let ir = transform_namespace("namespace M { export var x = 1; }");
        assert!(ir.is_some(), "Should produce IR for namespace with content");

        if let Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            is_exported,
            attach_to_exports,
            ..
        }) = ir
        {
            assert_eq!(name, "M");
            assert_eq!(name_parts, vec!["M"]);
            assert!(!is_exported);
            assert!(!attach_to_exports);
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_simple_namespace_output() {
        let output = transform_and_emit("namespace M { export var x = 1; }");
        assert!(output.contains("var M;"), "Should declare var M");
        assert!(output.contains("(function (M)"), "Should have IIFE");
        assert!(
            output.contains("(M || (M = {}))"),
            "Should have M || (M = {{}})"
        );
    }

    #[test]
    fn test_namespace_es5_exported_empty_namespace_skipped() {
        let ir = transform_namespace("export namespace M { }");
        assert!(
            ir.is_none(),
            "Empty exported namespace should produce no IR"
        );
    }

    #[test]
    fn test_namespace_es5_exported_namespace() {
        let ir = transform_namespace("export namespace M { export var x = 1; }");
        assert!(
            ir.is_some(),
            "Should produce IR for exported namespace with content"
        );

        if let Some(IRNode::NamespaceIIFE {
            name,
            is_exported,
            attach_to_exports,
            ..
        }) = ir
        {
            assert_eq!(name, "M");
            assert!(is_exported);
            assert!(!attach_to_exports); // Not in CommonJS mode
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    // =========================================================================
    // Qualified namespace name tests (A.B.C)
    // =========================================================================

    #[test]
    fn test_namespace_es5_qualified_name_two_parts() {
        let ir = transform_namespace("namespace A.B { export var x = 1; }");
        assert!(ir.is_some(), "Should produce IR for qualified namespace");

        if let Some(IRNode::NamespaceIIFE {
            name, name_parts, ..
        }) = ir
        {
            assert_eq!(name, "A");
            assert_eq!(name_parts, vec!["A", "B"]);
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_qualified_name_three_parts() {
        let ir = transform_namespace("namespace A.B.C { export var x = 1; }");
        assert!(ir.is_some(), "Should produce IR for qualified namespace");

        if let Some(IRNode::NamespaceIIFE {
            name, name_parts, ..
        }) = ir
        {
            assert_eq!(name, "A");
            assert_eq!(name_parts, vec!["A", "B", "C"]);
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_qualified_name_output() {
        let output = transform_and_emit("namespace A.B.C { export var x = 1; }");
        // Should have var declarations for each level
        assert!(output.contains("var A;"), "Should declare var A");
        assert!(
            output.contains("var B;"),
            "Should declare var B inside A's IIFE"
        );
        assert!(
            output.contains("var C;"),
            "Should declare var C inside B's IIFE"
        );
        // Should have nested IIFEs
        assert!(
            output.contains("(function (A)"),
            "Should have outer IIFE for A"
        );
        assert!(
            output.contains("(function (B)"),
            "Should have middle IIFE for B"
        );
        assert!(
            output.contains("(function (C)"),
            "Should have inner IIFE for C"
        );
        // Should have proper argument patterns
        assert!(
            output.contains("A || (A = {})"),
            "Should have A || (A = {{}})"
        );
        assert!(
            output.contains("B = A.B || (A.B = {})"),
            "Should have B = A.B || (A.B = {{}})"
        );
        assert!(
            output.contains("C = B.C || (B.C = {})"),
            "Should have C = B.C || (B.C = {{}})"
        );
    }

    // =========================================================================
    // CommonJS mode tests
    // =========================================================================

    #[test]
    fn test_namespace_es5_commonjs_exported() {
        let mut parser = ParserState::new(
            "test.ts".to_string(),
            "export namespace M { export var x = 1; }".to_string(),
        );
        let root = parser.parse_source_file();

        let ir = if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&stmt_idx) = source_file.statements.nodes.first()
        {
            if let Some(info) = find_namespace_info(&parser, stmt_idx) {
                let transformer = NamespaceES5Transformer::with_commonjs(&parser.arena, true);
                if info.is_exported {
                    transformer.transform_exported_namespace(info.ns_idx)
                } else {
                    transformer.transform_namespace(info.ns_idx)
                }
            } else {
                None
            }
        } else {
            None
        };

        assert!(ir.is_some(), "Should produce IR for exported namespace");
        if let Some(IRNode::NamespaceIIFE {
            is_exported,
            attach_to_exports,
            ..
        }) = ir
        {
            assert!(is_exported, "Namespace should be marked as exported");
            assert!(
                attach_to_exports,
                "Should attach to exports in CommonJS mode"
            );
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_commonjs_exported_output() {
        let output = transform_and_emit_commonjs("export namespace M { export var x = 1; }");
        // In CommonJS mode, exported namespaces attach to exports
        // The pattern is: M = exports.M || (exports.M = {})
        assert!(
            output.contains("exports.M"),
            "Should reference exports.M in CommonJS mode. Got: {}",
            output
        );
    }

    #[test]
    fn test_namespace_es5_commonjs_non_exported() {
        let output = transform_and_emit_commonjs("namespace M { export var x = 1; }");
        // Non-exported namespace in CommonJS mode should not attach to exports
        assert!(
            !output.contains("exports.M"),
            "Non-exported namespace should not reference exports. Got: {}",
            output
        );
    }

    // =========================================================================
    // Declare namespace tests (should be skipped)
    // =========================================================================

    #[test]
    fn test_namespace_es5_declare_namespace_skipped() {
        let ir = transform_namespace("declare namespace M { }");
        assert!(ir.is_none(), "Declare namespaces should be skipped");
    }

    // =========================================================================
    // Namespace with members tests
    // =========================================================================

    #[test]
    fn test_namespace_es5_with_function() {
        let ir = transform_namespace("namespace M { export function foo() { } }");
        assert!(ir.is_some());

        if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
            assert!(!body.is_empty(), "Body should have function");
            // Check for namespace export
            let has_export = body.iter().any(|node| {
                matches!(
                    node,
                    IRNode::Sequence(nodes) if nodes.iter().any(|n| matches!(n, IRNode::NamespaceExport { namespace, name, .. } if namespace == "M" && name == "foo"))
                )
            });
            assert!(has_export, "Should have namespace export for foo");
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_with_class() {
        let ir = transform_namespace("namespace M { export class Foo { } }");
        assert!(ir.is_some());

        if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
            assert!(!body.is_empty(), "Body should have class");
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_with_variable() {
        let ir = transform_namespace("namespace M { export const x = 1; }");
        assert!(ir.is_some());

        if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
            assert!(!body.is_empty(), "Body should have variable");
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_interface_only_skipped() {
        // Namespace with only interfaces is non-instantiated, should be skipped
        let ir = transform_namespace("namespace M { interface Foo { } }");
        assert!(ir.is_none(), "Interface-only namespace should be skipped");
    }

    #[test]
    fn test_namespace_es5_type_alias_only_skipped() {
        // Namespace with only type aliases is non-instantiated, should be skipped
        let ir = transform_namespace("namespace M { type Foo = string; }");
        assert!(ir.is_none(), "Type-alias-only namespace should be skipped");
    }

    // =========================================================================
    // Nested namespace tests
    // =========================================================================

    #[test]
    fn test_namespace_es5_nested_namespace() {
        let ir = transform_namespace("namespace A { namespace B { export var x = 1; } }");
        assert!(ir.is_some());

        if let Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            ..
        }) = ir
        {
            assert_eq!(name, "A");
            assert_eq!(name_parts, vec!["A"]);
            // Should have nested namespace in body
            let has_nested = body
                .iter()
                .any(|node| matches!(node, IRNode::NamespaceIIFE { name, .. } if name == "B"));
            assert!(has_nested, "Should have nested namespace B");
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_nested_empty_namespace_skipped() {
        // Namespace A contains only an empty nested namespace B, which gets skipped.
        // Since A then has no runtime content, A should also be skipped.
        let ir = transform_namespace("namespace A { namespace B { } }");
        assert!(
            ir.is_none(),
            "Namespace with only empty nested namespace should be skipped"
        );
    }

    #[test]
    fn test_namespace_es5_nested_exported_namespace() {
        let ir = transform_namespace("namespace A { export namespace B { export var x = 1; } }");
        assert!(ir.is_some());

        if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
            // Check nested namespace is exported
            let has_exported_nested = body.iter().any(|node| {
                matches!(
                    node,
                    IRNode::NamespaceIIFE {
                        name,
                        is_exported: true,
                        ..
                    } if name == "B"
                )
            });
            assert!(
                has_exported_nested,
                "Should have exported nested namespace B"
            );
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    // =========================================================================
    // Edge case tests
    // =========================================================================

    #[test]
    fn test_namespace_es5_empty_namespace_no_output() {
        let output = transform_and_emit("namespace A { }");
        assert!(
            output.is_empty() || output.trim().is_empty(),
            "Empty namespace should produce no output"
        );
    }

    #[test]
    fn test_namespace_es5_multiple_exports() {
        let ir = transform_namespace("namespace M { export const a = 1; export const b = 2; }");
        assert!(ir.is_some());

        if let Some(IRNode::NamespaceIIFE { body, .. }) = ir {
            assert_eq!(body.len(), 2, "Should have two exports");
        } else {
            panic!("Expected NamespaceIIFE IR node");
        }
    }

    #[test]
    fn test_namespace_es5_transformer_set_commonjs() {
        let mut parser = ParserState::new(
            "test.ts".to_string(),
            "export namespace M { export var x = 1; }".to_string(),
        );
        let root = parser.parse_source_file();

        if let Some(root_node) = parser.arena.get(root)
            && let Some(source_file) = parser.arena.get_source_file(root_node)
            && let Some(&ns_idx) = source_file.statements.nodes.first()
        {
            let mut transformer = NamespaceES5Transformer::new(&parser.arena);

            // Initially not CommonJS
            let ir1 = transformer.transform_namespace(ns_idx);
            if let Some(IRNode::NamespaceIIFE {
                attach_to_exports, ..
            }) = ir1
            {
                assert!(!attach_to_exports);
            }

            // Set CommonJS mode
            transformer.set_commonjs(true);
            let ir2 = transformer.transform_namespace(ns_idx);
            if let Some(IRNode::NamespaceIIFE {
                attach_to_exports, ..
            }) = ir2
            {
                assert!(attach_to_exports);
            }
        }
    }
}
