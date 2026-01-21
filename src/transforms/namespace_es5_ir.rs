//! ES5 Namespace Transform (IR-based)
//!
//! Transforms TypeScript namespaces to ES5 IIFE patterns, producing IR nodes.
//!
//! ```typescript
//! namespace foo {
//!     export class Provide { }
//! }
//! ```
//!
//! Becomes IR that prints as:
//!
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

use crate::parser::syntax_kind_ext;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::ir::*;

/// Context for namespace transformation
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
        Self {
            arena,
            is_commonjs,
        }
    }

    /// Transform a namespace declaration to IR
    pub fn transform_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        let ns_node = self.arena.get(ns_idx)?;
        let ns_data = self.arena.get_module(ns_node)?;

        // Skip ambient namespaces
        if has_declare_modifier(self.arena, &ns_data.modifiers) {
            return None;
        }

        // Flatten name parts for qualified names (A.B.C)
        let name_parts = self.flatten_module_name(ns_data.name)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = has_export_modifier(self.arena, &ns_data.modifiers);

        // Transform body
        let body = self.transform_namespace_body(ns_data.body, &name_parts);

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name,
            name_parts,
            body,
            is_exported,
            attach_to_exports: self.is_commonjs,
        })
    }

    /// Flatten a module name into parts
    fn flatten_module_name(&self, name_idx: NodeIndex) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        self.collect_name_parts(name_idx, &mut parts);
        if parts.is_empty() {
            None
        } else {
            Some(parts)
        }
    }

    /// Recursively collect name parts from qualified names
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
        } else if node.kind == SyntaxKind::Identifier as u16 {
            if let Some(ident) = self.arena.get_identifier(node) {
                parts.push(ident.escaped_text.clone());
            }
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
        if let Some(block_data) = self.arena.get_module_block(body_node) {
            if let Some(ref stmts) = block_data.statements {
                for &stmt_idx in &stmts.nodes {
                    if let Some(ir) = self.transform_namespace_member(ns_name, stmt_idx) {
                        result.push(ir);
                    }
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
    pub fn transform_namespace_member_exported(&self, ns_name: &str, decl_idx: NodeIndex) -> Option<IRNode> {
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
    pub fn transform_function_in_namespace(&self, _ns_name: &str, func_idx: NodeIndex) -> Option<IRNode> {
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        // Skip declaration-only functions
        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let is_exported = has_export_modifier(self.arena, &func_data.modifiers);

        if is_exported {
            // Return AST ref + export assignment
            Some(IRNode::Block(vec![
                IRNode::ASTRef(func_idx),
                IRNode::ExportAssignment { name: func_name },
            ]))
        } else {
            Some(IRNode::ASTRef(func_idx))
        }
    }

    /// Transform an exported function in namespace
    fn transform_function_in_namespace_exported(&self, _ns_name: &str, func_idx: NodeIndex) -> Option<IRNode> {
        let func_node = self.arena.get(func_idx)?;
        let func_data = self.arena.get_function(func_node)?;

        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        Some(IRNode::Block(vec![
            IRNode::ASTRef(func_idx),
            IRNode::ExportAssignment { name: func_name },
        ]))
    }

    /// Transform a class in namespace context
    fn transform_class_in_namespace(&self, _ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        let is_exported = has_export_modifier(self.arena, &class_data.modifiers);

        if is_exported {
            Some(IRNode::Block(vec![
                IRNode::ASTRef(class_idx),
                IRNode::ExportAssignment { name: class_name },
            ]))
        } else {
            Some(IRNode::ASTRef(class_idx))
        }
    }

    /// Transform an exported class in namespace
    fn transform_class_in_namespace_exported(&self, _ns_name: &str, class_idx: NodeIndex) -> Option<IRNode> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        Some(IRNode::Block(vec![
            IRNode::ASTRef(class_idx),
            IRNode::ExportAssignment { name: class_name },
        ]))
    }

    /// Transform a variable statement in namespace
    pub fn transform_variable_in_namespace(&self, _ns_name: &str, var_idx: NodeIndex) -> Option<IRNode> {
        let var_node = self.arena.get(var_idx)?;
        let var_data = self.arena.get_variable(var_node)?;

        let is_exported = has_export_modifier(self.arena, &var_data.modifiers);

        let mut result = vec![IRNode::ASTRef(var_idx)];

        if is_exported {
            // Collect variable names for export
            let var_names = collect_variable_names(self.arena, &var_data.declarations);
            for name in var_names {
                result.push(IRNode::ExportAssignment { name });
            }
        }

        Some(IRNode::Block(result))
    }

    /// Transform an exported variable statement in namespace
    fn transform_variable_in_namespace_exported(&self, _ns_name: &str, var_idx: NodeIndex) -> Option<IRNode> {
        let var_node = self.arena.get(var_idx)?;
        let var_data = self.arena.get_variable(var_node)?;

        let mut result = vec![IRNode::ASTRef(var_idx)];

        // Always export
        let var_names = collect_variable_names(self.arena, &var_data.declarations);
        for name in var_names {
            result.push(IRNode::ExportAssignment { name });
        }

        Some(IRNode::Block(result))
    }

    /// Transform an enum in namespace
    fn transform_enum_in_namespace(&self, _ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        let is_exported = has_export_modifier(self.arena, &enum_data.modifiers);

        let mut result = vec![IRNode::ASTRef(enum_idx)];

        if is_exported {
            result.push(IRNode::ExportAssignment { name: enum_name });
        }

        Some(IRNode::Block(result))
    }

    /// Transform an exported enum in namespace
    fn transform_enum_in_namespace_exported(&self, _ns_name: &str, enum_idx: NodeIndex) -> Option<IRNode> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;

        let enum_name = get_identifier_text(self.arena, enum_data.name)?;
        Some(IRNode::Block(vec![
            IRNode::ASTRef(enum_idx),
            IRNode::ExportAssignment { name: enum_name },
        ]))
    }

    /// Transform a nested namespace
    pub fn transform_nested_namespace(&self, _parent_ns: &str, ns_idx: NodeIndex) -> Option<IRNode> {
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
    fn transform_nested_namespace_exported(&self, _parent_ns: &str, ns_idx: NodeIndex) -> Option<IRNode> {
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

        let is_exported = true; // Always exported

        // Transform body
        let body = self.transform_namespace_body(ns_data.body, &name_parts);

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

fn has_modifier(
    arena: &NodeArena,
    modifiers: &Option<NodeList>,
    kind: u16,
) -> bool {
    if let Some(mods) = modifiers {
        for &mod_idx in &mods.nodes {
            if let Some(mod_node) = arena.get(mod_idx) {
                if mod_node.kind == kind {
                    return true;
                }
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
        if let Some(decl_list_node) = arena.get(decl_list_idx) {
            if let Some(decl_list) = arena.get_variable(decl_list_node) {
                for &decl_idx in &decl_list.declarations.nodes {
                    if let Some(decl_node) = arena.get(decl_idx) {
                        if let Some(decl) = arena.get_variable_declaration(decl_node) {
                            if let Some(name) = get_identifier_text(arena, decl.name) {
                                names.push(name);
                            }
                        }
                    }
                }
            }
        }
    }

    names
}
