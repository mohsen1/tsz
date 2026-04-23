//! Interface declaration emission for .d.ts files.
//!
//! Handles `interface` declarations, their members (property signatures, method
//! signatures, call/construct signatures, index signatures, mapped types,
//! get/set accessors), and inline type-literal member emission.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(crate) fn emit_interface_declaration(&mut self, iface_idx: NodeIndex) {
        let Some(iface_node) = self.arena.get(iface_idx) else {
            return;
        };
        let Some(iface) = self.arena.get_interface(iface_node) else {
            return;
        };

        let is_exported = self
            .arena
            .has_modifier(&iface.modifiers, SyntaxKind::ExportKeyword);
        if !self.should_emit_public_api_member(&iface.modifiers)
            && !self.should_emit_public_api_dependency(iface.name)
        {
            return;
        }
        if self.should_skip_ns_internal_member(&iface.modifiers, Some(iface_idx)) {
            return;
        }

        self.write_indent();
        if is_exported && self.should_emit_export_keyword() {
            self.write("export ");
        }
        // Preserve the `declare` modifier from the source when present
        let has_declare = self.arena.is_declare(&iface.modifiers);
        if has_declare {
            self.write("declare ");
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

        // Heritage (extends) — filter out non-entity-name expressions
        if let Some(ref heritage) = iface.heritage_clauses {
            self.emit_interface_heritage_clauses(heritage);
        }

        self.write(" {");
        self.write_line();
        self.increase_indent();

        // Members
        for &member_idx in &iface.members.nodes {
            if let Some(mn) = self.arena.get(member_idx) {
                self.emit_leading_jsdoc_comments(mn.pos);
            }
            self.emit_interface_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
    }

    pub(crate) fn emit_interface_member(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        // Skip members with computed property names that are not emittable in .d.ts
        if self.member_has_non_emittable_computed_name(member_idx) {
            return;
        }

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
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        self.write(": any");
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    self.emit_node(sig.name);
                    if sig.question_token {
                        self.write("?");
                    }
                    if let Some(ref type_params) = sig.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    if let Some(ref params) = sig.parameters {
                        self.emit_parameters(params);
                    }
                    self.write(")");
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        // In declaration emit from source, methods without explicit
                        // return types default to `any` (matching tsc behavior)
                        self.write(": any");
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
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        self.write(": any");
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
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        self.write(": any");
                    }
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                if let Some(sig) = self.arena.get_index_signature(member_node) {
                    self.emit_member_modifiers(&sig.modifiers);
                    self.write("[");
                    self.emit_parameters(&sig.parameters);
                    self.write("]");
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped_type) = self.arena.get_mapped_type(member_node) {
                    // Emit readonly modifier with +/- prefix support
                    if let Some(rt_node) = self.arena.get(mapped_type.readonly_token) {
                        if rt_node.kind == SyntaxKind::PlusToken as u16 {
                            self.write("+readonly ");
                        } else if rt_node.kind == SyntaxKind::MinusToken as u16 {
                            self.write("-readonly ");
                        } else {
                            self.write("readonly ");
                        }
                    }

                    self.write("[");

                    // Get the TypeParameter data
                    if let Some(type_param_node) = self.arena.get(mapped_type.type_parameter)
                        && let Some(type_param) = self.arena.get_type_parameter(type_param_node)
                    {
                        // Emit the parameter name (e.g., "P")
                        self.emit_node(type_param.name);

                        // Emit " in "
                        self.write(" in ");

                        // Emit the constraint (e.g., "keyof T")
                        if type_param.constraint.is_some() {
                            self.emit_type(type_param.constraint);
                        }
                    }

                    // Handle the optional 'as' clause (key remapping)
                    if mapped_type.name_type.is_some() {
                        self.write(" as ");
                        self.emit_type(mapped_type.name_type);
                    }

                    self.write("]");

                    // Emit question token with +/- prefix support
                    if let Some(qt_node) = self.arena.get(mapped_type.question_token) {
                        if qt_node.kind == SyntaxKind::PlusToken as u16 {
                            self.write("+?");
                        } else if qt_node.kind == SyntaxKind::MinusToken as u16 {
                            self.write("-?");
                        } else {
                            self.write("?");
                        }
                    }

                    self.write(": ");

                    // Emit type annotation
                    self.emit_type(mapped_type.type_node);

                    // Mapped types don't end with semicolon - return early
                    self.write_line();
                    return;
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(member_node) {
                    self.write("get ");
                    self.emit_node(accessor.name);
                    self.write("(");
                    self.emit_parameters(&accessor.parameters);
                    self.write(")");
                    if accessor.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(accessor.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(member_node) {
                    self.write("set ");
                    self.emit_node(accessor.name);
                    self.write("(");
                    self.emit_parameters(&accessor.parameters);
                    self.write(")");
                }
            }
            _ => {}
        }

        self.write(";");
        self.write_line();
    }

    /// Emit interface member without indentation or trailing newline.
    /// Used for inline type literals like `{ id: string }`
    pub(crate) fn emit_interface_member_inline(&mut self, member_idx: NodeIndex) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };

        // Skip members with computed property names that are not emittable in .d.ts
        if self.member_has_non_emittable_computed_name(member_idx) {
            return;
        }

        match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    // Modifiers
                    self.emit_member_modifiers(&sig.modifiers);
                    self.emit_node(sig.name);
                    if sig.question_token {
                        self.write("?");
                    }
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        self.write(": any");
                    }
                }
            }
            k if k == syntax_kind_ext::METHOD_SIGNATURE => {
                if let Some(sig) = self.arena.get_signature(member_node) {
                    self.emit_node(sig.name);
                    if sig.question_token {
                        self.write("?");
                    }
                    if let Some(ref type_params) = sig.type_parameters {
                        self.emit_type_parameters(type_params);
                    }
                    self.write("(");
                    if let Some(ref params) = sig.parameters {
                        self.emit_parameters(params);
                    }
                    self.write(")");
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        self.write(": any");
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
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        self.write(": any");
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
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    } else if !self.source_is_declaration_file {
                        self.write(": any");
                    }
                }
            }
            k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                if let Some(sig) = self.arena.get_index_signature(member_node) {
                    self.emit_member_modifiers(&sig.modifiers);
                    self.write("[");
                    self.emit_parameters(&sig.parameters);
                    self.write("]");
                    if sig.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(sig.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::MAPPED_TYPE => {
                if let Some(mapped_type) = self.arena.get_mapped_type(member_node) {
                    // Emit readonly modifier with +/- prefix support
                    if let Some(rt_node) = self.arena.get(mapped_type.readonly_token) {
                        if rt_node.kind == SyntaxKind::PlusToken as u16 {
                            self.write("+readonly ");
                        } else if rt_node.kind == SyntaxKind::MinusToken as u16 {
                            self.write("-readonly ");
                        } else {
                            self.write("readonly ");
                        }
                    }

                    self.write("[");

                    // Get the TypeParameter data
                    if let Some(type_param_node) = self.arena.get(mapped_type.type_parameter)
                        && let Some(type_param) = self.arena.get_type_parameter(type_param_node)
                    {
                        // Emit the parameter name (e.g., "P")
                        self.emit_node(type_param.name);

                        // Emit " in "
                        self.write(" in ");

                        // Emit constraint
                        if type_param.constraint.is_some() {
                            self.emit_type(type_param.constraint);
                        }
                    }

                    // Handle the optional 'as' clause (key remapping)
                    if mapped_type.name_type.is_some() {
                        self.write(" as ");
                        self.emit_type(mapped_type.name_type);
                    }

                    self.write("]");

                    // Emit question token with +/- prefix support
                    if let Some(qt_node) = self.arena.get(mapped_type.question_token) {
                        if qt_node.kind == SyntaxKind::PlusToken as u16 {
                            self.write("+?");
                        } else if qt_node.kind == SyntaxKind::MinusToken as u16 {
                            self.write("-?");
                        } else {
                            self.write("?");
                        }
                    }

                    self.write(": ");

                    // Emit type annotation
                    self.emit_type(mapped_type.type_node);

                    // Mapped types don't add semicolon in inline mode
                }
            }
            k if k == syntax_kind_ext::GET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(member_node) {
                    self.write("get ");
                    self.emit_node(accessor.name);
                    self.write("(");
                    self.emit_parameters(&accessor.parameters);
                    self.write(")");
                    if accessor.type_annotation.is_some() {
                        self.write(": ");
                        self.emit_type(accessor.type_annotation);
                    }
                }
            }
            k if k == syntax_kind_ext::SET_ACCESSOR => {
                if let Some(accessor) = self.arena.get_accessor(member_node) {
                    self.write("set ");
                    self.emit_node(accessor.name);
                    self.write("(");
                    self.emit_parameters(&accessor.parameters);
                    self.write(")");
                }
            }
            _ => {}
        }

        // Note: no semicolon or newline here - caller handles separation
    }
}
