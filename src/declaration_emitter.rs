//! Declaration File (.d.ts) Emitter
//!
//! Generates TypeScript declaration files from source code.
//!
//! ```typescript
//! // input.ts
//! export function add(a: number, b: number): number {
//!     return a + b;
//! }
//! export class Calculator {
//!     private value: number;
//!     add(n: number): this { ... }
//! }
//! ```
//!
//! Generates:
//!
//! ```typescript
//! // input.d.ts
//! export declare function add(a: number, b: number): number;
//! export declare class Calculator {
//!     private value;
//!     add(n: number): this;
//! }
//! ```

use crate::parser::node::{Node, NodeArena};
use crate::parser::syntax_kind_ext;
use crate::parser::{NodeIndex, NodeList};
use crate::scanner::SyntaxKind;
use crate::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};

/// Declaration emitter for .d.ts files
pub struct DeclarationEmitter<'a> {
    arena: &'a NodeArena,
    writer: SourceWriter,
    indent_level: u32,
    source_map_text: Option<&'a str>,
    source_map_state: Option<SourceMapState>,
    pending_source_pos: Option<SourcePosition>,
}

struct SourceMapState {
    output_name: String,
    source_name: String,
}

impl<'a> DeclarationEmitter<'a> {
    pub fn new(arena: &'a NodeArena) -> Self {
        DeclarationEmitter {
            arena,
            writer: SourceWriter::with_capacity(4096),
            indent_level: 0,
            source_map_text: None,
            source_map_state: None,
            pending_source_pos: None,
        }
    }

    pub fn set_source_map_text(&mut self, text: &'a str) {
        self.source_map_text = Some(text);
    }

    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.source_map_state = Some(SourceMapState {
            output_name: output_name.to_string(),
            source_name: source_name.to_string(),
        });
    }

    pub fn generate_source_map_json(&mut self) -> Option<String> {
        self.writer.generate_source_map_json()
    }

    /// Emit declaration for a source file
    pub fn emit(&mut self, root_idx: NodeIndex) -> String {
        self.reset_writer();
        self.indent_level = 0;

        let Some(root_node) = self.arena.get(root_idx) else {
            return String::new();
        };

        let Some(source_file) = self.arena.get_source_file(root_node) else {
            return String::new();
        };

        for &stmt_idx in &source_file.statements.nodes {
            self.emit_statement(stmt_idx);
        }

        self.writer.get_output().to_string()
    }

    fn emit_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(stmt_node);

        match stmt_node.kind {
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.emit_function_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.emit_class_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                self.emit_interface_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                self.emit_type_alias_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.emit_enum_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.emit_variable_declaration_statement(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                self.emit_export_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::EXPORT_ASSIGNMENT => {
                self.emit_export_assignment(stmt_idx);
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => {
                self.emit_import_declaration(stmt_idx);
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.emit_module_declaration(stmt_idx);
            }
            _ => {}
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    fn emit_function_declaration(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        // Check for export modifier
        let is_exported = self.has_export_modifier(&func.modifiers);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("declare function ");

        // Function name
        self.emit_node(func.name);

        // Type parameters
        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Parameters
        self.write("(");
        self.emit_parameters(&func.parameters);
        self.write(")");

        // Return type
        if !func.type_annotation.is_none() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_class_declaration(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_exported = self.has_export_modifier(&class.modifiers);
        let is_abstract = self.has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword as u16);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("declare ");
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");

        // Class name
        self.emit_node(class.name);

        // Type parameters
        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Heritage clauses (extends, implements)
        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Members
        for &member_idx in &class.members.nodes {
            self.emit_class_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_class_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                self.emit_property_declaration(member_idx);
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                self.emit_method_declaration(member_idx);
            }
            k if k == syntax_kind_ext::CONSTRUCTOR => {
                self.emit_constructor_declaration(member_idx);
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, true);
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                self.emit_accessor_declaration(member_idx, false);
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                self.emit_index_signature(member_idx);
            }
            _ => {}
        }
    }

    fn emit_property_declaration(&mut self, prop_idx: NodeIndex) {
        let Some(prop_node) = self.arena.get(prop_idx) else {
            return;
        };
        let Some(prop) = self.arena.get_property_decl(prop_node) else {
            return;
        };

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&prop.modifiers);

        // Name
        self.emit_node(prop.name);

        // Optional marker
        if prop.question_token {
            self.write("?");
        }

        // Type
        if !prop.type_annotation.is_none() {
            self.write(": ");
            self.emit_type(prop.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_method_declaration(&mut self, method_idx: NodeIndex) {
        let Some(method_node) = self.arena.get(method_idx) else {
            return;
        };
        let Some(method) = self.arena.get_method_decl(method_node) else {
            return;
        };

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&method.modifiers);

        // Name
        self.emit_node(method.name);

        // Type parameters
        if let Some(ref type_params) = method.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Parameters
        self.write("(");
        self.emit_parameters(&method.parameters);
        self.write(")");

        // Return type
        if !method.type_annotation.is_none() {
            self.write(": ");
            self.emit_type(method.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_constructor_declaration(&mut self, ctor_idx: NodeIndex) {
        let Some(ctor_node) = self.arena.get(ctor_idx) else {
            return;
        };
        let Some(ctor) = self.arena.get_constructor(ctor_node) else {
            return;
        };

        self.write_indent();
        self.write("constructor(");
        self.emit_parameters(&ctor.parameters);
        self.write(");");
        self.write_line();
    }

    fn emit_accessor_declaration(&mut self, accessor_idx: NodeIndex, is_getter: bool) {
        let Some(accessor_node) = self.arena.get(accessor_idx) else {
            return;
        };
        let Some(accessor) = self.arena.get_accessor(accessor_node) else {
            return;
        };

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&accessor.modifiers);

        if is_getter {
            self.write("get ");
        } else {
            self.write("set ");
        }

        // Name
        self.emit_node(accessor.name);

        // Parameters
        self.write("(");
        self.emit_parameters(&accessor.parameters);
        self.write(")");

        // Return type (for getters)
        if is_getter && !accessor.type_annotation.is_none() {
            self.write(": ");
            self.emit_type(accessor.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_index_signature(&mut self, sig_idx: NodeIndex) {
        let Some(sig_node) = self.arena.get(sig_idx) else {
            return;
        };
        let Some(sig) = self.arena.get_index_signature(sig_node) else {
            return;
        };

        self.write_indent();

        // Modifiers
        self.emit_member_modifiers(&sig.modifiers);

        self.write("[");
        self.emit_parameters(&sig.parameters);
        self.write("]");

        if !sig.type_annotation.is_none() {
            self.write(": ");
            self.emit_type(sig.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_interface_declaration(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };

        let is_exported = self.has_export_modifier(&iface.modifiers);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("interface ");

        // Name
        self.emit_node(iface.name);

        // Type parameters
        if let Some(ref type_params) = iface.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        // Heritage (extends)
        if let Some(ref heritage) = iface.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Members
        for &member_idx in &iface.members.nodes {
            self.emit_interface_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_interface_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        self.write_indent();

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    // Modifiers
                    self.emit_member_modifiers(&sig.modifiers);
                    self.emit_node(sig.name);
                    if sig.question_token {
                        self.write("?");
                    }
                    if !sig.type_annotation.is_none() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    self.emit_node(sig.name);
                    if let Some(ref type_params) = sig.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    if let Some(ref params) = sig.parameters {
                        self.emit_parameters(params);
                    }
                    self.write(")");
                    if !sig.type_annotation.is_none() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::CALL_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    if let Some(ref type_params) = sig.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    if let Some(ref params) = sig.parameters {
                        self.emit_parameters(params);
                    }
                    self.write(")");
                    if !sig.type_annotation.is_none() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::CONSTRUCT_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    self.write("new ");
                    if let Some(ref type_params) = sig.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    if let Some(ref params) = sig.parameters {
                        self.emit_parameters(params);
                    }
                    self.write(")");
                    if !sig.type_annotation.is_none() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                if let Some(sig) = self.arena.get_index_signature(member_node) {
                    self.write("[");
                    self.emit_parameters(&sig.parameters);
                    self.write("]");
                    if !sig.type_annotation.is_none() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    }
                }
            }
            _ => {}
        }

        self.write(";");
        self.write_line();
    }

    fn emit_type_alias_declaration(&mut self, alias_idx: NodeIndex) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        let is_exported = self.has_export_modifier(&alias.modifiers);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("type ");

        // Name
        self.emit_node(alias.name);

        // Type parameters
        if let Some(ref type_params) = alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write(" = ");
        self.emit_type(alias.type_node);
        self.write(";");
        self.write_line();
    }

    fn emit_enum_declaration(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        let is_exported = self.has_export_modifier(&enum_data.modifiers);
        let is_const = self.has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword as u16);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("declare ");
        if is_const {
            self.write("const ");
        }
        self.write("enum ");

        self.emit_node(enum_data.name);

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                if !member.initializer.is_none() {
                    self.write(" = ");
                    self.emit_expression(member.initializer);
                }
            }
            if i < enum_data.members.nodes.len() - 1 {
                self.write(",");
            }
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_variable_declaration_statement(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        let is_exported = self.has_export_modifier(&var_stmt.modifiers);

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                // Determine let/const/var
                let flags = decl_list_node.flags as u32;
                let keyword = if flags & crate::parser::node_flags::CONST != 0 {
                    "const"
                } else if flags & crate::parser::node_flags::LET != 0 {
                    "let"
                } else {
                    "var"
                };

                for &decl_idx in &decl_list.declarations.nodes {
                    self.write_indent();
                    if is_exported {
                        self.write("export ");
                    }
                    self.write("declare ");
                    self.write(keyword);
                    self.write(" ");

                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        self.emit_node(decl.name);
                        if !decl.type_annotation.is_none() {
                            self.write(": ");
                            self.emit_type(decl.type_annotation);
                        }
                    }

                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    fn emit_export_declaration(&mut self, export_idx: NodeIndex) {
        let Some(export_node) = self.arena.get(export_idx) else {
            return;
        };
        let Some(export) = self.arena.get_export_decl(export_node) else {
            return;
        };

        if export.is_default_export {
            if !export.export_clause.is_none()
                && let Some(clause_node) = self.arena.get(export.export_clause)
            {
                match clause_node.kind {
                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                        self.emit_export_default_function(export.export_clause);
                        return;
                    }
                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                        self.emit_export_default_class(export.export_clause);
                        return;
                    }
                    _ => {}
                }
            }

            self.emit_export_default_expression(export.export_clause);
            return;
        }

        // Check if export_clause is a declaration (interface, class, function, type, enum)
        if !export.export_clause.is_none()
            && let Some(clause_node) = self.arena.get(export.export_clause)
        {
            match clause_node.kind {
                k if k == syntax_kind_ext::INTERFACE_DECLARATION => {
                    // Emit: export interface Foo {...}
                    self.emit_exported_interface(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::CLASS_DECLARATION => {
                    self.emit_exported_class(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                    self.emit_exported_function(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::TYPE_ALIAS_DECLARATION => {
                    self.emit_exported_type_alias(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::ENUM_DECLARATION => {
                    self.emit_exported_enum(export.export_clause);
                    return;
                }
                k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                    self.emit_exported_variable(export.export_clause);
                    return;
                }
                _ => {}
            }
        }

        // Handle named exports: export { a, b } from "mod"
        // or star exports: export * from "mod"
        self.write_indent();
        self.write("export ");

        if export.is_type_only {
            self.write("type ");
        }

        if !export.export_clause.is_none() {
            if let Some(clause_node) = self.arena.get(export.export_clause) {
                if clause_node.kind == syntax_kind_ext::NAMED_EXPORTS {
                    self.emit_named_exports(export.export_clause, !export.is_type_only);
                } else if clause_node.kind == SyntaxKind::Identifier as u16 {
                    self.emit_namespace_export_clause(export.export_clause);
                } else {
                    self.emit_node(export.export_clause);
                }
            }
        } else {
            self.write("*");
        }

        if !export.module_specifier.is_none() {
            self.write(" from ");
            self.emit_node(export.module_specifier);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_export_assignment(&mut self, assign_idx: NodeIndex) {
        let Some(assign_node) = self.arena.get(assign_idx) else {
            return;
        };
        let Some(assign) = self.arena.get_export_assignment(assign_node) else {
            return;
        };

        self.write_indent();
        if assign.is_export_equals {
            self.write("export = ");
        } else {
            self.write("export default ");
        }
        self.emit_expression(assign.expression);
        self.write(";");
        self.write_line();
    }

    fn emit_export_default_function(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        self.write_indent();
        self.write("export default function ");
        self.emit_node(func.name);

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write("(");
        self.emit_parameters(&func.parameters);
        self.write(")");

        if !func.type_annotation.is_none() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_export_default_class(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        let is_abstract = self.has_modifier(&class.modifiers, SyntaxKind::AbstractKeyword as u16);

        self.write_indent();
        self.write("export default ");
        if is_abstract {
            self.write("abstract ");
        }
        self.write("class ");
        self.emit_node(class.name);

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &class.members.nodes {
            self.emit_class_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_export_default_expression(&mut self, expr_idx: NodeIndex) {
        self.write_indent();
        self.write("export default ");
        if !expr_idx.is_none() {
            self.emit_expression(expr_idx);
        }
        self.write(";");
        self.write_line();
    }

    fn emit_namespace_export_clause(&mut self, clause_idx: NodeIndex) {
        self.write("* as ");
        self.emit_node(clause_idx);
    }

    fn emit_named_exports(&mut self, exports_idx: NodeIndex, allow_type_prefix: bool) {
        let Some(exports_node) = self.arena.get(exports_idx) else {
            return;
        };
        let Some(exports) = self.arena.get_named_imports(exports_node) else {
            return;
        };

        if !exports.name.is_none() && exports.elements.nodes.is_empty() {
            self.write("* as ");
            self.emit_node(exports.name);
            return;
        }

        self.write("{ ");
        let mut first = true;
        for &spec_idx in &exports.elements.nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_export_specifier(spec_idx, allow_type_prefix);
        }
        self.write(" }");
    }

    fn emit_export_specifier(&mut self, spec_idx: NodeIndex, allow_type_prefix: bool) {
        let Some(spec_node) = self.arena.get(spec_idx) else {
            return;
        };
        let Some(spec) = self.arena.get_specifier(spec_node) else {
            return;
        };

        if allow_type_prefix && spec.is_type_only {
            self.write("type ");
        }

        if !spec.property_name.is_none() {
            self.emit_node(spec.property_name);
            self.write(" as ");
        }
        self.emit_node(spec.name);
    }

    // Helper to emit exported interface with "export" prefix
    fn emit_exported_interface(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };

        self.write_indent();
        self.write("export interface ");
        self.emit_node(iface.name);

        if let Some(ref type_params) = iface.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        if let Some(ref heritage) = iface.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &iface.members.nodes {
            self.emit_interface_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_exported_class(&mut self, class_idx: NodeIndex) {
        let Some(class_node) = self.arena.get(class_idx) else {
            return;
        };
        let Some(class) = self.arena.get_class(class_node) else {
            return;
        };

        self.write_indent();
        self.write("export declare class ");
        self.emit_node(class.name);

        if let Some(ref type_params) = class.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        if let Some(ref heritage) = class.heritage_clauses {
            self.emit_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for &member_idx in &class.members.nodes {
            self.emit_class_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_exported_function(&mut self, func_idx: NodeIndex) {
        let Some(func_node) = self.arena.get(func_idx) else {
            return;
        };
        let Some(func) = self.arena.get_function(func_node) else {
            return;
        };

        self.write_indent();
        self.write("export declare function ");
        self.emit_node(func.name);

        if let Some(ref type_params) = func.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write("(");
        self.emit_parameters(&func.parameters);
        self.write(")");

        if !func.type_annotation.is_none() {
            self.write(": ");
            self.emit_type(func.type_annotation);
        }

        self.write(";");
        self.write_line();
    }

    fn emit_exported_type_alias(&mut self, alias_idx: NodeIndex) {
        let Some(alias_node) = self.arena.get(alias_idx) else {
            return;
        };
        let Some(alias) = self.arena.get_type_alias(alias_node) else {
            return;
        };

        self.write_indent();
        self.write("export type ");
        self.emit_node(alias.name);

        if let Some(ref type_params) = alias.type_parameters
            && !type_params.nodes.is_empty()
        {
            self.emit_type_parameters(type_params);
        }

        self.write(" = ");
        self.emit_type(alias.type_node);
        self.write(";");
        self.write_line();
    }

    fn emit_exported_enum(&mut self, enum_idx: NodeIndex) {
        let Some(enum_node) = self.arena.get(enum_idx) else {
            return;
        };
        let Some(enum_data) = self.arena.get_enum(enum_node) else {
            return;
        };

        self.write_indent();
        self.write("export declare enum ");
        self.emit_node(enum_data.name);

        self.write(" {");
        self.write_line();
        self.increase_indent();

        for (i, &member_idx) in enum_data.members.nodes.iter().enumerate() {
            self.write_indent();
            if let Some(member_node) = self.arena.get(member_idx)
                && let Some(member) = self.arena.get_enum_member(member_node)
            {
                self.emit_node(member.name);
                if !member.initializer.is_none() {
                    self.write(" = ");
                    self.emit_expression(member.initializer);
                }
            }
            if i < enum_data.members.nodes.len() - 1 {
                self.write(",");
            }
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    fn emit_exported_variable(&mut self, stmt_idx: NodeIndex) {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return;
        };
        let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
            return;
        };

        for &decl_list_idx in &var_stmt.declarations.nodes {
            let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                continue;
            };

            if decl_list_node.kind == syntax_kind_ext::VARIABLE_DECLARATION_LIST
                && let Some(decl_list) = self.arena.get_variable(decl_list_node)
            {
                let flags = decl_list_node.flags as u32;
                let keyword = if flags & crate::parser::node_flags::CONST != 0 {
                    "const"
                } else if flags & crate::parser::node_flags::LET != 0 {
                    "let"
                } else {
                    "var"
                };

                for &decl_idx in &decl_list.declarations.nodes {
                    self.write_indent();
                    self.write("export declare ");
                    self.write(keyword);
                    self.write(" ");

                    if let Some(decl_node) = self.arena.get(decl_idx)
                        && let Some(decl) = self.arena.get_variable_declaration(decl_node)
                    {
                        self.emit_node(decl.name);
                        if !decl.type_annotation.is_none() {
                            self.write(": ");
                            self.emit_type(decl.type_annotation);
                        }
                    }

                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    fn emit_import_declaration(&mut self, import_idx: NodeIndex) {
        let Some(import_node) = self.arena.get(import_idx) else {
            return;
        };
        let Some(import) = self.arena.get_import_decl(import_node) else {
            return;
        };

        self.write_indent();
        self.write("import ");

        if !import.import_clause.is_none()
            && let Some(clause_node) = self.arena.get(import.import_clause)
            && let Some(clause) = self.arena.get_import_clause(clause_node)
        {
            if clause.is_type_only {
                self.write("type ");
            }

            let mut has_default = false;

            // Default import
            if !clause.name.is_none() {
                self.emit_node(clause.name);
                has_default = true;
            }

            // Named imports
            if !clause.named_bindings.is_none() {
                if has_default {
                    self.write(", ");
                }
                self.emit_named_imports(clause.named_bindings, !clause.is_type_only);
            }

            self.write(" from ");
        }

        self.emit_node(import.module_specifier);
        self.write(";");
        self.write_line();
    }

    fn emit_named_imports(&mut self, imports_idx: NodeIndex, allow_type_prefix: bool) {
        let Some(imports_node) = self.arena.get(imports_idx) else {
            return;
        };
        let Some(imports) = self.arena.get_named_imports(imports_node) else {
            return;
        };

        if !imports.name.is_none() && imports.elements.nodes.is_empty() {
            self.write("* as ");
            self.emit_node(imports.name);
            return;
        }

        self.write("{ ");
        let mut first = true;
        for &spec_idx in &imports.elements.nodes {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_import_specifier(spec_idx, allow_type_prefix);
        }
        self.write(" }");
    }

    fn emit_import_specifier(&mut self, spec_idx: NodeIndex, allow_type_prefix: bool) {
        let Some(spec_node) = self.arena.get(spec_idx) else {
            return;
        };
        let Some(spec) = self.arena.get_specifier(spec_node) else {
            return;
        };

        if allow_type_prefix && spec.is_type_only {
            self.write("type ");
        }

        if !spec.property_name.is_none() {
            self.emit_node(spec.property_name);
            self.write(" as ");
        }
        self.emit_node(spec.name);
    }

    fn emit_module_declaration(&mut self, module_idx: NodeIndex) {
        let Some(module_node) = self.arena.get(module_idx) else {
            return;
        };
        let Some(module) = self.arena.get_module(module_node) else {
            return;
        };

        let is_exported = self.has_export_modifier(&module.modifiers);

        self.write_indent();
        if is_exported {
            self.write("export ");
        }
        self.write("declare ");

        // namespace or module
        self.write("namespace ");
        self.emit_node(module.name);

        if !module.body.is_none() {
            self.write(" {");
            self.write_line();
            self.increase_indent();

            if let Some(body_node) = self.arena.get(module.body)
                && let Some(block) = self.arena.get_block(body_node)
            {
                for &stmt_idx in &block.statements.nodes {
                    self.emit_statement(stmt_idx);
                }
            }

            self.decrease_indent();
            self.write_indent();
            self.write("}");
        }

        self.write_line();
    }

    // Helper methods

    fn emit_parameters(&mut self, params: &NodeList) {
        let mut first = true;
        for &param_idx in &params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_parameter(param_node)
            {
                // Modifiers (public, private, etc for constructor parameters)
                self.emit_member_modifiers(&param.modifiers);

                // Rest parameter
                if param.dot_dot_dot_token {
                    self.write("...");
                }

                // Name
                self.emit_node(param.name);

                // Optional
                if param.question_token {
                    self.write("?");
                }

                // Type
                if !param.type_annotation.is_none() {
                    self.write(": ");
                    self.emit_type(param.type_annotation);
                }
            }
        }
    }

    fn emit_type_parameters(&mut self, type_params: &NodeList) {
        self.write("<");
        let mut first = true;
        for &param_idx in &type_params.nodes {
            if !first {
                self.write(", ");
            }
            first = false;

            if let Some(param_node) = self.arena.get(param_idx)
                && let Some(param) = self.arena.get_type_parameter(param_node)
            {
                self.emit_node(param.name);

                if !param.constraint.is_none() {
                    self.write(" extends ");
                    self.emit_type(param.constraint);
                }

                if !param.default.is_none() {
                    self.write(" = ");
                    self.emit_type(param.default);
                }
            }
        }
        self.write(">");
    }

    fn emit_heritage_clauses(&mut self, clauses: &NodeList) {
        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            let keyword = match heritage.token {
                k if k == SyntaxKind::ExtendsKeyword as u16 => "extends",
                k if k == SyntaxKind::ImplementsKeyword as u16 => "implements",
                _ => continue,
            };

            self.write(" ");
            self.write(keyword);
            self.write(" ");

            let mut first = true;
            for &type_idx in &heritage.types.nodes {
                if !first {
                    self.write(", ");
                }
                first = false;
                self.emit_type(type_idx);
            }
        }
    }

    fn emit_member_modifiers(&mut self, modifiers: &Option<NodeList>) {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    match mod_node.kind {
                        k if k == SyntaxKind::PublicKeyword as u16 => self.write("public "),
                        k if k == SyntaxKind::PrivateKeyword as u16 => self.write("private "),
                        k if k == SyntaxKind::ProtectedKeyword as u16 => self.write("protected "),
                        k if k == SyntaxKind::StaticKeyword as u16 => self.write("static "),
                        k if k == SyntaxKind::ReadonlyKeyword as u16 => self.write("readonly "),
                        k if k == SyntaxKind::AbstractKeyword as u16 => self.write("abstract "),
                        k if k == SyntaxKind::AsyncKeyword as u16 => self.write("async "),
                        _ => {}
                    }
                }
            }
        }
    }

    fn emit_type(&mut self, type_idx: NodeIndex) {
        let Some(type_node) = self.arena.get(type_idx) else {
            return;
        };

        match type_node.kind {
            // Keyword types
            k if k == SyntaxKind::NumberKeyword as u16 => self.write("number"),
            k if k == SyntaxKind::StringKeyword as u16 => self.write("string"),
            k if k == SyntaxKind::BooleanKeyword as u16 => self.write("boolean"),
            k if k == SyntaxKind::VoidKeyword as u16 => self.write("void"),
            k if k == SyntaxKind::AnyKeyword as u16 => self.write("any"),
            k if k == SyntaxKind::UnknownKeyword as u16 => self.write("unknown"),
            k if k == SyntaxKind::NeverKeyword as u16 => self.write("never"),
            k if k == SyntaxKind::NullKeyword as u16 => self.write("null"),
            k if k == SyntaxKind::UndefinedKeyword as u16 => self.write("undefined"),
            k if k == SyntaxKind::ObjectKeyword as u16 => self.write("object"),
            k if k == SyntaxKind::SymbolKeyword as u16 => self.write("symbol"),
            k if k == SyntaxKind::BigIntKeyword as u16 => self.write("bigint"),
            k if k == SyntaxKind::ThisKeyword as u16 => self.write("this"),

            // Type reference
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.arena.get_type_ref(type_node) {
                    self.emit_node(type_ref.type_name);
                    if let Some(ref type_args) = type_ref.type_arguments {
                        self.write("<");
                        let mut first = true;
                        for &arg_idx in &type_args.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit_type(arg_idx);
                        }
                        self.write(">");
                    }
                }
            }

            // Expression with type arguments (heritage clauses)
            k if k == syntax_kind_ext::EXPRESSION_WITH_TYPE_ARGUMENTS => {
                if let Some(expr) = self.arena.get_expr_type_args(type_node) {
                    self.emit_entity_name(expr.expression);
                    if let Some(ref type_args) = expr.type_arguments
                        && !type_args.nodes.is_empty()
                    {
                        self.write("<");
                        let mut first = true;
                        for &arg_idx in &type_args.nodes {
                            if !first {
                                self.write(", ");
                            }
                            first = false;
                            self.emit_type(arg_idx);
                        }
                        self.write(">");
                    }
                }
            }

            // Array type
            k if k == syntax_kind_ext::ARRAY_TYPE => {
                if let Some(arr) = self.arena.get_array_type(type_node) {
                    self.emit_type(arr.element_type);
                    self.write("[]");
                }
            }

            // Union type
            k if k == syntax_kind_ext::UNION_TYPE => {
                if let Some(union) = self.arena.get_composite_type(type_node) {
                    let mut first = true;
                    for &type_idx in &union.types.nodes {
                        if !first {
                            self.write(" | ");
                        }
                        first = false;
                        self.emit_type(type_idx);
                    }
                }
            }

            // Intersection type
            k if k == syntax_kind_ext::INTERSECTION_TYPE => {
                if let Some(inter) = self.arena.get_composite_type(type_node) {
                    let mut first = true;
                    for &type_idx in &inter.types.nodes {
                        if !first {
                            self.write(" & ");
                        }
                        first = false;
                        self.emit_type(type_idx);
                    }
                }
            }

            // Tuple type
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                if let Some(tuple) = self.arena.get_tuple_type(type_node) {
                    self.write("[");
                    let mut first = true;
                    for &elem_idx in &tuple.elements.nodes {
                        if !first {
                            self.write(", ");
                        }
                        first = false;
                        self.emit_type(elem_idx);
                    }
                    self.write("]");
                }
            }

            // Function type
            k if k == syntax_kind_ext::FUNCTION_TYPE => {
                if let Some(func) = self.arena.get_function_type(type_node) {
                    if let Some(ref type_params) = func.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    self.emit_parameters(&func.parameters);
                    self.write(") => ");
                    self.emit_type(func.type_annotation);
                }
            }

            // Type literal
            k if k == syntax_kind_ext::TYPE_LITERAL => {
                if let Some(lit) = self.arena.get_type_literal(type_node) {
                    self.write("{ ");
                    for &member_idx in &lit.members.nodes {
                        self.emit_interface_member(member_idx);
                    }
                    self.write(" }");
                }
            }

            // Parenthesized type
            k if k == syntax_kind_ext::PARENTHESIZED_TYPE => {
                if let Some(paren) = self.arena.get_wrapped_type(type_node) {
                    self.write("(");
                    self.emit_type(paren.type_node);
                    self.write(")");
                }
            }

            // Literal types
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(type_node) {
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(type_node) {
                    self.write(&lit.text);
                }
            }
            k if k == SyntaxKind::TrueKeyword as u16 => self.write("true"),
            k if k == SyntaxKind::FalseKeyword as u16 => self.write("false"),

            _ => {
                // Fallback: emit as node
                self.emit_node(type_idx);
            }
        }
    }

    fn emit_entity_name(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == SyntaxKind::ThisKeyword as u16 => self.write("this"),
            k if k == SyntaxKind::SuperKeyword as u16 => self.write("super"),
            k if k == syntax_kind_ext::QUALIFIED_NAME => {
                if let Some(name) = self.arena.get_qualified_name(node) {
                    self.emit_entity_name(name.left);
                    self.write(".");
                    self.emit_entity_name(name.right);
                }
            }
            k if k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION => {
                if let Some(access) = self.arena.get_access_expr(node) {
                    self.emit_entity_name(access.expression);
                    self.write(".");
                    self.emit_entity_name(access.name_or_argument);
                }
            }
            _ => {}
        }
    }

    fn emit_expression(&mut self, expr_idx: NodeIndex) {
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(expr_node);

        match expr_node.kind {
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
            _ => self.emit_node(expr_idx),
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    fn emit_node(&mut self, node_idx: NodeIndex) {
        let Some(node) = self.arena.get(node_idx) else {
            return;
        };
        let before_len = self.writer.len();
        self.queue_source_mapping(node);

        match node.kind {
            k if k == SyntaxKind::Identifier as u16 => {
                if let Some(ident) = self.arena.get_identifier(node) {
                    self.write(&ident.escaped_text);
                }
            }
            k if k == syntax_kind_ext::QUALIFIED_NAME
                || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || k == SyntaxKind::ThisKeyword as u16
                || k == SyntaxKind::SuperKeyword as u16 =>
            {
                self.emit_entity_name(node_idx);
            }
            k if k == SyntaxKind::StringLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.write("\"");
                    self.write(&lit.text);
                    self.write("\"");
                }
            }
            k if k == SyntaxKind::NumericLiteral as u16 => {
                if let Some(lit) = self.arena.get_literal(node) {
                    self.write(&lit.text);
                }
            }
            _ => {}
        }

        if self.writer.len() == before_len {
            self.pending_source_pos = None;
        }
    }

    fn has_export_modifier(&self, modifiers: &Option<NodeList>) -> bool {
        self.has_modifier(modifiers, SyntaxKind::ExportKeyword as u16)
    }

    fn has_modifier(&self, modifiers: &Option<NodeList>, kind: u16) -> bool {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx)
                    && mod_node.kind == kind
                {
                    return true;
                }
            }
        }
        false
    }

    fn reset_writer(&mut self) {
        self.writer = SourceWriter::with_capacity(4096);
        self.pending_source_pos = None;
        if let Some(state) = &self.source_map_state {
            self.writer.enable_source_map(state.output_name.clone());
            let content = self.source_map_text.map(|text| text.to_string());
            self.writer.add_source(state.source_name.clone(), content);
        }
    }

    fn queue_source_mapping(&mut self, node: &Node) {
        if !self.writer.has_source_map() {
            self.pending_source_pos = None;
            return;
        }

        let Some(text) = self.source_map_text else {
            self.pending_source_pos = None;
            return;
        };

        self.pending_source_pos = Some(source_position_from_offset(text, node.pos));
    }

    fn take_pending_source_pos(&mut self) -> Option<SourcePosition> {
        self.pending_source_pos.take()
    }

    fn write_raw(&mut self, s: &str) {
        self.writer.write(s);
    }

    fn write(&mut self, s: &str) {
        if let Some(source_pos) = self.take_pending_source_pos() {
            self.writer.write_node(s, source_pos);
        } else {
            self.writer.write(s);
        }
    }

    fn write_line(&mut self) {
        self.writer.write_line();
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.write_raw("    ");
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

    #[test]
    fn test_function_declaration() {
        let source = "export function add(a: number, b: number): number { return a + b; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut emitter = DeclarationEmitter::new(&parser.arena);
        let output = emitter.emit(root);

        assert!(
            output.contains("export declare function add"),
            "Expected export declare: {}",
            output
        );
        assert!(
            output.contains("a: number"),
            "Expected parameter type: {}",
            output
        );
        assert!(
            output.contains("): number;"),
            "Expected return type: {}",
            output
        );
    }

    #[test]
    fn test_class_declaration() {
        let source = r#"
        export class Calculator {
            private value: number;
            add(n: number): this {
                this.value += n;
                return this;
            }
        }
        "#;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut emitter = DeclarationEmitter::new(&parser.arena);
        let output = emitter.emit(root);

        assert!(
            output.contains("class Calculator"),
            "Expected class declaration: {}",
            output
        );
        assert!(output.contains("value"), "Expected property: {}", output);
        assert!(
            output.contains("add") && output.contains("number"),
            "Expected method signature with add and number: {}",
            output
        );
    }

    #[test]
    fn test_interface_declaration() {
        let source = "export interface Point { x: number; y: number; }";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut emitter = DeclarationEmitter::new(&parser.arena);
        let output = emitter.emit(root);

        assert!(
            output.contains("interface Point"),
            "Expected interface: {}",
            output
        );
        assert!(
            output.contains("number"),
            "Expected number type: {}",
            output
        );
    }

    #[test]
    fn test_type_alias() {
        let source = "export type ID = string | number;";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut emitter = DeclarationEmitter::new(&parser.arena);
        let output = emitter.emit(root);

        assert!(
            output.contains("export type ID = string | number"),
            "Expected type alias: {}",
            output
        );
    }
}
