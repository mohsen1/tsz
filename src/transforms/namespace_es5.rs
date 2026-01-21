//! ES5 Namespace Transform
//!
//! Transforms TypeScript namespaces to ES5 IIFE patterns:
//!
//! ```typescript
//! namespace foo {
//!     export class Provide { }
//! }
//! ```
//!
//! Becomes:
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
//!
//! Also handles qualified names like `namespace A.B.C`:
//! ```javascript
//! var A;
//! (function (A) {
//!     var B;
//!     (function (B) {
//!         var C;
//!         (function (C) {
//!             // body
//!         })(C = B.C || (B.C = {}));
//!     })(B = A.B || (A.B = {}));
//! })(A || (A = {}));
//! ```

use crate::parser::syntax_kind_ext;
use crate::parser::node::NodeArena;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::transforms::class_es5::ClassES5Emitter;
use crate::transforms::enum_es5::EnumES5Emitter;
use crate::transforms::ir::IRNode;
use crate::transforms::ir_printer::IRPrinter;
use crate::transforms::namespace_es5_ir::NamespaceTransformContext;
use crate::transforms::emit_utils;

/// Namespace ES5 emitter
pub struct NamespaceES5Emitter<'a> {
    arena: &'a NodeArena,
    source_text: Option<&'a str>,
    output: String,
    indent_level: u32,
    is_commonjs: bool,
}

impl<'a> NamespaceES5Emitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            output: String::with_capacity(4096),
            indent_level: 0,
            is_commonjs: false,
        }
    }

    /// Create a namespace emitter with CommonJS mode
    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        NamespaceES5Emitter {
            arena,
            source_text: None,
            output: String::with_capacity(4096),
            indent_level: 0,
            is_commonjs,
        }
    }

    /// Set the source text for ASTRef emission
    pub fn set_source_text(&mut self, text: &'a str) {
        self.source_text = Some(text);
    }

    /// Emit a namespace declaration
    pub fn emit_namespace(&mut self, ns_idx: NodeIndex) -> String {
        self.output.clear();

        let Some(ns_node) = self.arena.get(ns_idx) else {
            return String::new();
        };

        let Some(ns_data) = self.arena.get_module(ns_node) else {
            return String::new();
        };

        // Skip ambient namespaces (declare namespace)
        if self.has_declare_modifier(&ns_data.modifiers) {
            return String::new();
        }

        // Flatten name parts for qualified names (A.B.C)
        let name_parts = self.flatten_module_name(ns_data.name);
        if name_parts.is_empty() {
            return String::new();
        }

        let is_exported = self.has_export_modifier(&ns_data.modifiers);
        let root_name = &name_parts[0];

        // var A;
        self.write("var ");
        self.write(root_name);
        self.write(";");
        self.write_line();

        // Recursive IIFE generation for qualified names
        self.emit_nested_iifes(&name_parts, 0, ns_data.body, is_exported);

        std::mem::take(&mut self.output)
    }

    /// Flatten a module name into parts (handles both identifiers and qualified names)
    /// e.g., `A.B.C` becomes `["A", "B", "C"]`
    fn flatten_module_name(&self, name_idx: NodeIndex) -> Vec<String> {
        let mut parts = Vec::new();
        self.collect_name_parts(name_idx, &mut parts);
        parts
    }

    /// Recursively collect name parts from qualified names
    fn collect_name_parts(&self, idx: NodeIndex, parts: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // QualifiedName has left and right - need to access via data pool
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

    /// Emit nested IIFEs for qualified namespace names
    fn emit_nested_iifes(
        &mut self,
        parts: &[String],
        index: usize,
        body_idx: NodeIndex,
        root_is_exported: bool,
    ) {
        let current_name = &parts[index];
        let is_last = index == parts.len() - 1;

        // Open IIFE
        self.write_indent();
        self.write("(function (");
        self.write(current_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        if is_last {
            // Inner-most body
            self.emit_namespace_body(current_name, body_idx);
        } else {
            // Nested namespace part: var B;
            let next_name = &parts[index + 1];
            self.write_indent();
            self.write("var ");
            self.write(next_name);
            self.write(";");
            self.write_line();

            // Recurse
            self.emit_nested_iifes(parts, index + 1, body_idx, root_is_exported);
        }

        // Close IIFE
        self.decrease_indent();
        self.write_indent();
        self.write("})(");

        // Argument logic
        if index == 0 {
            // Root argument
            if root_is_exported && self.is_commonjs {
                // A = exports.A || (exports.A = {})
                self.write(current_name);
                self.write(" = exports.");
                self.write(current_name);
                self.write(" || (exports.");
                self.write(current_name);
                self.write(" = {})");
            } else {
                // A || (A = {})
                self.write(current_name);
                self.write(" || (");
                self.write(current_name);
                self.write(" = {})");
            }
        } else {
            // Nested argument: B = A.B || (A.B = {})
            let parent_name = &parts[index - 1];
            self.write(current_name);
            self.write(" = ");
            self.write(parent_name);
            self.write(".");
            self.write(current_name);
            self.write(" || (");
            self.write(parent_name);
            self.write(".");
            self.write(current_name);
            self.write(" = {})");
        }

        self.write(");");
        self.write_line();
    }

    /// Emit namespace body contents
    fn emit_namespace_body(&mut self, ns_name: &str, body_idx: NodeIndex) {
        let Some(body_node) = self.arena.get(body_idx) else {
            return;
        };

        // Check if it's a module block
        if let Some(block_data) = self.arena.get_module_block(body_node) {
            if let Some(ref stmts) = block_data.statements {
                for &stmt_idx in &stmts.nodes {
                    self.emit_namespace_member(ns_name, stmt_idx);
                }
            }
        } else if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
            // Nested module declaration (for `namespace A.B` where B is the body)
            self.emit_nested_namespace(ns_name, body_idx);
        }
    }

    /// Check if modifiers contain the `declare` keyword
    fn has_declare_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        let Some(mods) = modifiers else {
            return false;
        };
        for &mod_idx in &mods.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::DeclareKeyword as u16 {
                return true;
            }
        }
        false
    }

    /// Emit a namespace member and its export assignment if needed
    fn emit_namespace_member(&mut self, ns_name: &str, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                // Handle export declarations by extracting the inner declaration
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    let inner_decl_idx = export_data.export_clause;
                    self.emit_namespace_member_exported(ns_name, inner_decl_idx);
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                eprintln!("DEBUG: Found FUNCTION_DECLARATION (non-exported)");
                self.emit_function_in_namespace(ns_name, member_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_in_namespace(ns_name, member_idx);
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_in_namespace(ns_name, member_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_nested_namespace(ns_name, member_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_in_namespace(ns_name, member_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {}
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                // TypeScript-only: skip in JS emit
            }
            _ => {
                // Other statements - emit directly
                self.emit_statement(member_idx);
            }
        }
    }

    /// Emit an exported namespace member (extracted from EXPORT_DECLARATION)
    fn emit_namespace_member_exported(&mut self, ns_name: &str, decl_idx: NodeIndex) {
        let Some(decl_node) = self.arena.get(decl_idx) else {
            return;
        };

        match decl_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                eprintln!("DEBUG: Found FUNCTION_DECLARATION (non-exported)");
                self.emit_function_in_namespace_exported(ns_name, decl_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_in_namespace_exported(ns_name, decl_idx);
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_in_namespace_exported(ns_name, decl_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_in_namespace_exported(ns_name, decl_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                // Nested namespace export
                self.emit_nested_namespace_exported(ns_name, decl_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {}
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {}
            _ => {}
        }
    }

    /// Emit a function declaration in namespace context
    fn emit_function_in_namespace(&mut self, ns_name: &str, func_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(ns_name, func_idx) {
            self.write_indented_ir(&output);
        }
    }

    /// Emit an exported function in namespace context
    fn emit_function_in_namespace_exported(&mut self, ns_name: &str, func_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(ns_name, func_idx) {
            self.write_indented_ir(&output);
        }
    }

    /// Emit a class declaration in namespace context
    fn emit_class_in_namespace(&mut self, ns_name: &str, class_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(ns_name, class_idx) {
            self.write_indented_ir(&output);
        }
    }

    /// Emit an exported class in namespace context
    fn emit_class_in_namespace_exported(&mut self, ns_name: &str, class_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(ns_name, class_idx) {
            self.write_indented_ir(&output);
        }
    }

    /// Emit a variable statement in namespace context
    fn emit_variable_in_namespace(&mut self, ns_name: &str, var_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(ns_name, var_idx) {
            self.write_indented_ir(&output);
        }
    }

    /// Emit an exported variable statement in namespace context
    fn emit_variable_in_namespace_exported(&mut self, ns_name: &str, var_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(ns_name, var_idx) {
            self.write_indented_ir(&output);
        }
    }

    /// Emit an exported enum in namespace context
    fn emit_enum_in_namespace_exported(&mut self, ns_name: &str, enum_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(ns_name, enum_idx) {
            self.write_indented_ir(&output);
        }
    }

    /// Emit an exported nested namespace
    fn emit_nested_namespace_exported(&mut self, parent_ns: &str, ns_idx: NodeIndex) {
        if let Some(output) = self.transform_namespace_member_ir(parent_ns, ns_idx) {
            self.write_indented_ir(&output);
        }
    }

    fn transform_namespace_member_ir(&self, ns_name: &str, member_idx: NodeIndex) -> Option<String> {
        let transform_context =
            NamespaceTransformContext::with_commonjs(self.arena, self.is_commonjs);
        let mut output = String::new();
        let mut printer = IRPrinter::with_arena(self.arena);
        printer.set_indent_level(self.indent_level);
        if let Some(source_text) = self.source_text {
            printer.set_source_text(source_text);
        }
        let mut push_ir = |ir: IRNode| {
            output.push_str(printer.emit(&ir));
            output.push('\n');
        };

        let member_node = self.arena.get(member_idx)?;
        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                let export_data = self.arena.get_export_decl(member_node)?;
                let export_node = self.arena.get(export_data.export_clause)?;
                match export_node.kind {
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        if let Some(class_output) =
                            self.transform_namespace_class_string(ns_name, export_data.export_clause)
                        {
                            output.push_str(&class_output);
                        }
                    }
                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                        if let Some(enum_output) =
                            self.transform_namespace_enum_ir(ns_name, export_data.export_clause)
                        {
                            output.push_str(&enum_output);
                        }
                    }
                    _ => {
                        let inner_ir = transform_context
                            .transform_namespace_member_exported(ns_name, export_data.export_clause)?;
                        push_ir(inner_ir);
                    }
                }
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                let func_data = self.arena.get_function(member_node)?;
                if func_data.body.is_none() {
                    return None;
                }
                let inner_ir = transform_context.transform_function_in_namespace(ns_name, member_idx)?;
                push_ir(inner_ir);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                if let Some(class_output) = self.transform_namespace_class_string(ns_name, member_idx)
                {
                    output.push_str(&class_output);
                }
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                let inner_ir = transform_context.transform_variable_in_namespace(ns_name, member_idx)?;
                push_ir(inner_ir);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let inner_ir = transform_context.transform_nested_namespace(ns_name, member_idx)?;
                push_ir(inner_ir);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                if let Some(enum_ir) = self.transform_namespace_enum_ir(ns_name, member_idx) {
                    output.push_str(&enum_ir);
                }
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                return None;
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                return None;
            }
            _ => {
                push_ir(IRNode::ASTRef(member_idx));
            }
        }

        if output.trim().is_empty() {
            None
        } else {
            Some(output)
        }
    }

    fn transform_namespace_class_string(&self, ns_name: &str, class_idx: NodeIndex) -> Option<String> {
        let class_node = self.arena.get(class_idx)?;
        let class_data = self.arena.get_class(class_node)?;
        let class_name = self.get_identifier_text(class_data.name);
        if class_name.is_empty() {
            return None;
        }
        let mut output = String::new();

        let mut class_emitter = ClassES5Emitter::new(self.arena);
        class_emitter.set_indent_level(self.indent_level);
        let class_output = class_emitter.emit_class(class_idx);
        if !class_output.is_empty() {
            output.push_str(&class_output);
            if !class_output.ends_with('\n') {
                output.push('\n');
            }
        }

        if self.has_export_modifier(&class_data.modifiers) {
            let mut printer = IRPrinter::new();
            printer.set_indent_level(self.indent_level);
            output.push_str(printer.emit(&IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: class_name.clone(),
                value: Box::new(IRNode::id(class_name)),
            }));
            output.push('\n');
        }

        if output.trim().is_empty() {
            None
        } else {
            Some(output)
        }
    }

    fn transform_namespace_enum_ir(&self, ns_name: &str, enum_idx: NodeIndex) -> Option<String> {
        let enum_node = self.arena.get(enum_idx)?;
        let enum_data = self.arena.get_enum(enum_node)?;
        let enum_name = self.get_identifier_text(enum_data.name);
        if enum_name.is_empty() {
            return None;
        }

        let mut enum_emitter = EnumES5Emitter::new(self.arena);
        enum_emitter.set_indent_level(self.indent_level);
        let enum_output = enum_emitter.emit_enum(enum_idx);
        let mut output = String::new();
        if !enum_output.is_empty() {
            output.push_str(&enum_output);
            if !enum_output.ends_with('\n') {
                output.push('\n');
            }
        }

        if self.has_export_modifier(&enum_data.modifiers) {
            let mut printer = IRPrinter::new();
            printer.set_indent_level(self.indent_level);
            output.push_str(printer.emit(&IRNode::NamespaceExport {
                namespace: ns_name.to_string(),
                name: enum_name.clone(),
                value: Box::new(IRNode::id(enum_name)),
            }));
            output.push('\n');
        }

        if output.trim().is_empty() {
            None
        } else {
            Some(output)
        }
    }

    fn write_indented_ir(&mut self, output: &str) {
        for line in output.lines() {
            self.write_indent();
            self.write(line);
            self.write_line();
        }
    }

    /// Emit a nested namespace
    fn emit_nested_namespace(&mut self, parent_ns: &str, ns_idx: NodeIndex) {
        let Some(ns_node) = self.arena.get(ns_idx) else {
            return;
        };
        let Some(ns_data) = self.arena.get_module(ns_node) else {
            return;
        };

        // Skip ambient nested namespaces
        if self.has_declare_modifier(&ns_data.modifiers) {
            return;
        }

        // Handle qualified names
        let name_parts = self.flatten_module_name(ns_data.name);
        if name_parts.is_empty() {
            return;
        }
        let nested_name = &name_parts[0];
        let is_exported = self.has_export_modifier(&ns_data.modifiers);

        // var bar;
        self.write_indent();
        self.write("var ");
        self.write(nested_name);
        self.write(";");
        self.write_line();

        // If exported, attach to parent; otherwise local
        if is_exported {
            self.emit_nested_namespace_iife(parent_ns, &name_parts, 0, ns_data.body);
        } else {
            // Non-exported namespace stays local
            self.emit_local_namespace_iife(&name_parts, 0, ns_data.body);
        }
    }

    /// Emit IIFE for nested namespace attached to parent
    fn emit_nested_namespace_iife(
        &mut self,
        parent_ns: &str,
        parts: &[String],
        index: usize,
        body_idx: NodeIndex,
    ) {
        let current_name = &parts[index];
        let is_last = index == parts.len() - 1;

        self.write_indent();
        self.write("(function (");
        self.write(current_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        if is_last {
            self.emit_namespace_body(current_name, body_idx);
        } else {
            // var NextPart;
            let next_name = &parts[index + 1];
            self.write_indent();
            self.write("var ");
            self.write(next_name);
            self.write(";");
            self.write_line();
            // Recurse with current as parent
            self.emit_nested_namespace_iife(current_name, parts, index + 1, body_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("})(");

        // Argument: Name = Parent.Name || (Parent.Name = {})
        let attach_parent = if index == 0 {
            parent_ns
        } else {
            &parts[index - 1]
        };
        self.write(current_name);
        self.write(" = ");
        self.write(attach_parent);
        self.write(".");
        self.write(current_name);
        self.write(" || (");
        self.write(attach_parent);
        self.write(".");
        self.write(current_name);
        self.write(" = {}));");
        self.write_line();
    }

    /// Emit IIFE for local (non-exported) nested namespace
    fn emit_local_namespace_iife(&mut self, parts: &[String], index: usize, body_idx: NodeIndex) {
        let current_name = &parts[index];
        let is_last = index == parts.len() - 1;

        self.write_indent();
        self.write("(function (");
        self.write(current_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        if is_last {
            self.emit_namespace_body(current_name, body_idx);
        } else {
            let next_name = &parts[index + 1];
            self.write_indent();
            self.write("var ");
            self.write(next_name);
            self.write(";");
            self.write_line();
            self.emit_local_namespace_iife(parts, index + 1, body_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("})(");

        if index == 0 {
            // Root: Name || (Name = {})
            self.write(current_name);
            self.write(" || (");
            self.write(current_name);
            self.write(" = {})");
        } else {
            // Nested: Name = Parent.Name || (Parent.Name = {})
            let parent_name = &parts[index - 1];
            self.write(current_name);
            self.write(" = ");
            self.write(parent_name);
            self.write(".");
            self.write(current_name);
            self.write(" || (");
            self.write(parent_name);
            self.write(".");
            self.write(current_name);
            self.write(" = {})");
        }

        self.write(");");
        self.write_line();
    }

    /// Emit enum in namespace
    fn emit_enum_in_namespace(&mut self, ns_name: &str, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        let enum_name = self.get_identifier_text(enum_data.name);
        let is_exported = self.has_export_modifier(&enum_data.modifiers);

        // var EnumName;
        self.write_indent();
        self.write("var ");
        self.write(&enum_name);
        self.write(";");
        self.write_line();

        // (function (EnumName) { ... })(EnumName || (EnumName = {}));
        self.write_indent();
        self.write("(function (");
        self.write(&enum_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        // Emit enum members
        let mut value = 0i64;
        for &member_idx in &enum_data.members.nodes {
            if let Some(member_node) = self.arena.get(member_idx) {
                if let Some(member_data) = self.arena.get_enum_member(member_node) {
                    let member_name = self.get_identifier_text(member_data.name);

                    // Check for initializer
                    if !member_data.initializer.is_none() {
                        // EnumName[EnumName["Name"] = value] = "Name";
                        self.write_indent();
                        self.write(&enum_name);
                        self.write("[");
                        self.write(&enum_name);
                        self.write("[\"");
                        self.write(&member_name);
                        self.write("\"] = ");
                        self.emit_expression(member_data.initializer);
                        self.write("] = \"");
                        self.write(&member_name);
                        self.write("\";");
                        self.write_line();
                    } else {
                        self.write_indent();
                        self.write(&enum_name);
                        self.write("[");
                        self.write(&enum_name);
                        self.write("[\"");
                        self.write(&member_name);
                        self.write("\"] = ");
                        self.write_i64(value);
                        self.write("] = \"");
                        self.write(&member_name);
                        self.write("\";");
                        self.write_line();
                        value += 1;
                    }
                }
            }
        }

        self.decrease_indent();
        self.write_indent();
        self.write("})(");
        self.write(&enum_name);
        self.write(" || (");
        self.write(&enum_name);
        self.write(" = {}));");
        self.write_line();

        // Export assignment
        if is_exported {
            self.write_indent();
            self.write(ns_name);
            self.write(".");
            self.write(&enum_name);
            self.write(" = ");
            self.write(&enum_name);
            self.write(";");
            self.write_line();
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    fn emit_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };

        match stmt_node.kind {
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) {
                    self.write_indent();
                    self.emit_expression(expr_stmt.expression);
                    self.write(";");
                    self.write_line();
                }
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret) = self.arena.get_return_statement(stmt_node) {
                    self.write_indent();
                    self.write("return");
                    if !ret.expression.is_none() {
                        self.write(" ");
                        self.emit_expression(ret.expression);
                    }
                    self.write(";");
                    self.write_line();
                }
            }
            _ => {}
        }
    }

    fn emit_block(&mut self, block_idx: NodeIndex) {
        let Some(block_node) = self.arena.get(block_idx) else {
            return;
        };
        let Some(block) = self.arena.get_block(block_node) else {
            return;
        };

        if block.statements.nodes.is_empty() {
            self.write("{ }");
            return;
        }

        self.write("{");
        self.write_line();
        self.increase_indent();

        for &stmt_idx in &block.statements.nodes {
            self.emit_statement(stmt_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    fn emit_parameters(&mut self, params: &NodeList) {
        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx) {
                if let Some(param) = self.arena.get_parameter(param_node) {
                    let name = self.get_identifier_text(param.name);
                    self.write(&name);
                }
            }
        }
    }

    fn emit_expression(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };

        match expr_node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(expr_node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write(&lit.text);
                }
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(expr_node) {
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == syntax_kind_ext::BINARY_EXPRESSION => {
                if let Some(bin) = self.arena.get_binary_expr(expr_node) {
                    self.emit_expression(bin.left);
                    self.write(" ");
                    self.emit_operator_token(bin.operator_token);
                    self.write(" ");
                    self.emit_expression(bin.right);
                }
            }
            k if k == syntax_kind_ext::CALL_EXPRESSION => {
                if let Some(call) = self.arena.get_call_expr(expr_node) {
                    self.emit_expression(call.expression);
                    self.write("(");
                    if let Some(ref args) = call.arguments {
                        let mut first = true;
                        for &arg_idx in &args.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit_expression(arg_idx);
                        }
                    }
                    self.write(")");
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(expr_node) {
                    self.emit_expression(access.expression);
                    self.write(".");
                    self.emit_expression(access.name_or_argument);
                }
            }
            _ => {}
        }
    }

    fn emit_operator_token(&mut self, op: u16) {
        let op_str = match op {
            k if k == SyntaxKind::PlusToken as u16 => "+",
            k if k == SyntaxKind::MinusToken as u16 => "-",
            k if k == SyntaxKind::AsteriskToken as u16 => "*",
            k if k == SyntaxKind::SlashToken as u16 => "/",
            k if k == SyntaxKind::EqualsToken as u16 => "=",
            k if k == SyntaxKind::EqualsEqualsToken as u16 => "==",
            k if k == SyntaxKind::EqualsEqualsEqualsToken as u16 => "===",
            k if k == SyntaxKind::ExclamationEqualsToken as u16 => "!=",
            k if k == SyntaxKind::ExclamationEqualsEqualsToken as u16 => "!==",
            k if k == SyntaxKind::LessThanToken as u16 => "<",
            k if k == SyntaxKind::GreaterThanToken as u16 => ">",
            k if k == SyntaxKind::LessThanEqualsToken as u16 => "<=",
            k if k == SyntaxKind::GreaterThanEqualsToken as u16 => ">=",
            k if k == SyntaxKind::PlusEqualsToken as u16 => "+=",
            k if k == SyntaxKind::MinusEqualsToken as u16 => "-=",
            k if k == SyntaxKind::AsteriskEqualsToken as u16 => "*=",
            k if k == SyntaxKind::SlashEqualsToken as u16 => "/=",
            k if k == SyntaxKind::AmpersandAmpersandToken as u16 => "&&",
            k if k == SyntaxKind::BarBarToken as u16 => "||",
            _ => "?",
        };
        self.write(op_str);
    }

    fn get_identifier_text(&self, idx: NodeIndex) -> String {
        if let Some(node) = self.arena.get(idx) {
            if let Some(ident) = self.arena.get_identifier(node) {
                return ident.escaped_text.clone();
            }
        }
        String::new()
    }

    fn has_export_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn write_i64(&mut self, value: i64) {
        emit_utils::push_i64(&mut self.output, value);
    }

    fn write_line(&mut self) {
        self.output.push('\n');
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
    }

    fn increase_indent(&mut self) {
        self.indent_level += 1;
    }

    fn decrease_indent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParserState;

    fn emit_namespace(source: &str) -> String {
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        // Find the namespace declaration
        if let Some(root_node) = parser.arena.get(root) {
            if let Some(source_file) = parser.arena.get_source_file(root_node) {
                if let Some(&ns_idx) = source_file.statements.nodes.first() {
                    let mut emitter = NamespaceES5Emitter::new(&parser.arena);
                    emitter.set_source_text(source);
                    return emitter.emit_namespace(ns_idx);
                }
            }
        }
        String::new()
    }

    #[test]
    fn test_empty_namespace() {
        let output = emit_namespace("namespace M { }");
        assert!(output.contains("var M;"), "Should declare var M");
        assert!(output.contains("(function (M)"), "Should have IIFE");
        assert!(
            output.contains("(M || (M = {}))"),
            "Should have M || (M = {{}})"
        );
    }

    #[test]
    fn test_namespace_with_function() {
        let output = emit_namespace("namespace M { export function foo() { return 1; } }");
        eprintln!("=== Namespace output ===");
        eprintln!("{}", output);
        eprintln!("=== End output ===");
        assert!(output.contains("var M;"), "Should declare var M");
        assert!(
            output.contains("function foo()"),
            "Should have function foo"
        );
        assert!(output.contains("M.foo = foo;"), "Should export foo");
    }

    // Note: test_declare_namespace_skipped is skipped because the parser
    // currently doesn't attach the `declare` modifier to namespace nodes.
    // This is a known parser limitation that should be fixed separately.
    // The has_declare_modifier() check is still in place for when the parser is fixed.
}
