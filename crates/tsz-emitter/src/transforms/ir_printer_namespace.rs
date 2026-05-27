//! Namespace IIFE, enum, and block emission helpers for [`IRPrinter`].
//!
//! Extracted from `ir_printer.rs` to keep file sizes manageable.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct NamespaceIifeContext<'a> {
    pub(super) is_exported: bool,
    pub(super) attach_to_exports: bool,
    pub(super) commonjs_export_names: &'a [Cow<'static, str>],
    pub(super) system_export_names: &'a [Cow<'static, str>],
    pub(super) should_declare_var: bool,
    pub(super) default_export_merge: bool,
    pub(super) parent_name: Option<&'a str>,
    pub(super) param_name: Option<&'a str>,
    pub(super) invalid_static_declaration: bool,
}

impl<'a> IRPrinter<'a> {
    pub(super) fn enum_with_matching_namespace_export<'b>(
        first: &'b IRNode,
        second: &'b IRNode,
    ) -> Option<(&'b str, &'b Vec<EnumMember>, &'b str)> {
        let IRNode::EnumIIFE { name, members, .. } = first else {
            return None;
        };
        let IRNode::NamespaceExport {
            namespace,
            name: export_name,
            value,
        } = second
        else {
            return None;
        };
        let IRNode::Identifier(identifier_name) = &**value else {
            return None;
        };
        (export_name == name && identifier_name == name).then_some((&**name, members, &**namespace))
    }

    pub(super) fn emit_namespace_bound_enum_iife(
        &mut self,
        enum_name: &str,
        members: &[EnumMember],
        namespace: &str,
    ) {
        // Inside namespace body, use `let` (ES2015+ block scoping).
        // ES5 doesn't support `let`, so must always use `var`.
        let keyword = if self.in_namespace_iife_body && !self.target_es5 {
            "let"
        } else {
            "var"
        };
        self.write(keyword);
        self.write(" ");
        self.write(enum_name);
        self.write(";");
        self.write_line();
        self.write_indent();
        self.write("(function (");
        self.write(enum_name);
        self.write(") {");
        self.write_line();
        self.increase_indent();
        for member in members {
            self.write_indent();
            self.emit_enum_member(enum_name, member);
            self.write_line();
        }
        self.decrease_indent();
        self.write_indent();
        self.write("})(");
        self.write(enum_name);
        self.write(" = ");
        self.write(namespace);
        self.write(".");
        self.write(enum_name);
        self.write(" || (");
        self.write(namespace);
        self.write(".");
        self.write(enum_name);
        self.write(" = {}));");
    }

    pub(super) fn emit_enum_member(&mut self, enum_name: &str, member: &EnumMember) {
        self.write(enum_name);

        match &member.value {
            EnumMemberValue::Auto(value) | EnumMemberValue::Numeric(value) => {
                // Numeric enum with reverse mapping: E[E["A"] = 0] = "A";
                self.write("[");
                self.write(enum_name);
                self.write("[\"");
                self.write(&member.name);
                self.write("\"] = ");
                self.write(&value.to_string());
                self.write("] = \"");
                self.write(&member.name);
                self.write("\";");
            }
            EnumMemberValue::String(s) => {
                // String enum, no reverse mapping: E["A"] = "val";
                self.write("[\"");
                self.write(&member.name);
                self.write("\"] = \"");
                self.write_escaped(s);
                self.write("\";");
            }
            EnumMemberValue::Computed(expr) => {
                // Computed enum with reverse mapping
                self.write("[");
                self.write(enum_name);
                self.write("[\"");
                self.write(&member.name);
                self.write("\"] = ");
                self.emit_node(expr);
                self.write("] = \"");
                self.write(&member.name);
                self.write("\";");
            }
        }
    }

    fn emit_system_export_folded_namespace_assignment(
        &mut self,
        export_names: &[Cow<'static, str>],
        current_name: &str,
    ) {
        let Some((export_name, inner_names)) = export_names.split_last() else {
            self.write(current_name);
            self.write(" = {}");
            return;
        };

        self.write("exports_1(\"");
        self.write_escaped(export_name.as_ref());
        self.write("\", ");
        self.emit_system_export_folded_namespace_assignment(inner_names, current_name);
        self.write(")");
    }

    fn emit_commonjs_export_folded_namespace_assignment(
        &mut self,
        export_names: &[Cow<'static, str>],
        current_name: &str,
    ) {
        let Some((export_name, inner_names)) = export_names.split_last() else {
            self.write(current_name);
            self.write(" = {}");
            return;
        };

        self.write("exports.");
        self.write(export_name);
        self.write(" = ");
        self.emit_commonjs_export_folded_namespace_assignment(inner_names, current_name);
    }

    pub(super) fn emit_namespace_iife(
        &mut self,
        name_parts: &[Cow<'static, str>],
        index: usize,
        body: &[IRNode],
        context: NamespaceIifeContext<'_>,
    ) {
        let current_name = &name_parts[index];
        let is_last = index == name_parts.len() - 1;
        // Use renamed parameter name only at the innermost (last) level for collision avoidance.
        // Outer levels of qualified names (A.B.C) always use their original name.
        let iife_param = if is_last {
            context.param_name.unwrap_or(current_name)
        } else {
            current_name
        };

        // Emit var/let declaration only for the outermost namespace and if flag is true.
        // Inside a namespace IIFE body, use `let` (ES2015+ semantics for nested namespaces).
        // At the outermost level, use `var` (needed for declaration merging across files).
        if index == 0 && context.should_declare_var {
            let decl_keyword = if self.in_namespace_iife_body && !self.target_es5 {
                "let"
            } else {
                "var"
            };
            if context.invalid_static_declaration && self.in_namespace_iife_body && !self.target_es5
            {
                self.write("static ");
            }
            self.write(decl_keyword);
            self.write(" ");
            self.write(current_name);
            self.write(";");
            self.write_line();
            // Need indent after the newline from var declaration
            self.write_indent();
        }
        // When should_declare_var is false, the caller already wrote the indent
        // (the parent namespace body loop calls write_indent before emit_node).

        // Open IIFE: (function (name) {
        self.write("(function (");
        self.write(iife_param);
        self.write(") {");
        self.write_line();
        self.increase_indent();

        if is_last {
            // Emit body with trailing comment peek-ahead.
            // Set in_namespace_iife_body so nested namespace declarations use `let`.
            let prev_in_ns_body = self.in_namespace_iife_body;
            self.in_namespace_iife_body = true;
            let mut i = 0;
            while i < body.len() {
                // Skip standalone TrailingComment nodes (consumed by peek-ahead)
                if matches!(&body[i], IRNode::TrailingComment(_)) {
                    i += 1;
                    continue;
                }
                // Skip comment nodes when removeComments is enabled
                if self.remove_comments
                    && (matches!(&body[i], IRNode::Comment { .. })
                        || matches!(&body[i], IRNode::Raw(s) if s.trim_start().starts_with("//") || s.trim_start().starts_with("/*")))
                {
                    i += 1;
                    continue;
                }

                if Self::is_noop_statement(&body[i]) {
                    i += 1;
                    continue;
                }

                self.write_indent();
                let suppress_for_this_node = i + 1 < body.len()
                    && matches!(&body[i], IRNode::FunctionDecl { .. })
                    && matches!(&body[i + 1], IRNode::TrailingComment(_));
                let prev_suppress = self.suppress_function_trailing_extraction;
                self.suppress_function_trailing_extraction = suppress_for_this_node;
                self.emit_node(&body[i]);
                self.suppress_function_trailing_extraction = prev_suppress;
                // Peek ahead for trailing comment
                if i + 1 < body.len()
                    && let IRNode::TrailingComment(text) = &body[i + 1]
                {
                    if !self.remove_comments {
                        self.write(" ");
                        self.write(text);
                    }
                    i += 1; // consume the trailing comment
                }
                self.write_line();
                i += 1;
            }
            // Restore in_namespace_iife_body after emitting this namespace's body
            self.in_namespace_iife_body = prev_in_ns_body;
        } else {
            // Emit var/let declaration for nested namespace (dotted namespaces: A.B.C)
            // Use `let` inside namespace bodies (ES2015+ semantics).
            let next_name = &name_parts[index + 1];
            self.write_indent();
            let nested_decl_keyword = if self.in_namespace_iife_body && !self.target_es5 {
                "let"
            } else {
                "var"
            };
            self.write(nested_decl_keyword);
            self.write(" ");
            self.write(next_name);
            self.write(";");
            self.write_line();
            // Recurse for nested namespace (inner levels use name_parts[index-1] as parent).
            // Write indent since we're on a new line after "var Y;\n".
            self.write_indent();
            self.emit_namespace_iife(
                name_parts,
                index + 1,
                body,
                NamespaceIifeContext {
                    is_exported: context.is_exported,
                    attach_to_exports: context.attach_to_exports,
                    commonjs_export_names: context.commonjs_export_names,
                    system_export_names: &[],
                    should_declare_var: true,
                    default_export_merge: false,
                    parent_name: None,
                    param_name: context.param_name,
                    invalid_static_declaration: false,
                },
            );
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("})(");

        // Argument: emit the IIFE argument binding
        if index == 0 {
            if let Some(parent) = context.parent_name {
                // Nested namespace with parent: Name = Parent.Name || (Parent.Name = {})
                self.write(current_name);
                self.write(" = ");
                self.write(parent);
                self.write(".");
                self.write(current_name);
                self.write(" || (");
                self.write(parent);
                self.write(".");
                self.write(current_name);
                self.write(" = {})");
            } else if context.is_exported && context.attach_to_exports {
                self.write(current_name);
                self.write(" || (");
                if context.commonjs_export_names.is_empty() {
                    // No explicit export alias: fold the namespace under its own name.
                    // `export namespace Foo {}` in CJS → `Foo || (exports.Foo = Foo = {})`
                    self.write("exports.");
                    self.write(current_name);
                    self.write(" = ");
                    self.write(current_name);
                    self.write(" = {}");
                } else {
                    self.emit_commonjs_export_folded_namespace_assignment(
                        context.commonjs_export_names,
                        current_name,
                    );
                }
                self.write(")");
            } else if !context.system_export_names.is_empty() {
                self.write(current_name);
                self.write(" || (");
                self.emit_system_export_folded_namespace_assignment(
                    context.system_export_names,
                    current_name,
                );
                self.write(")");
            } else if context.default_export_merge {
                self.write("exports.");
                self.write(current_name);
                self.write(" || (exports.");
                self.write(current_name);
                self.write(" = {})");
            } else {
                self.write(current_name);
                self.write(" || (");
                self.write(current_name);
                self.write(" = {})");
            }
        } else {
            // Qualified name parts (A.B.C): Name = Parent.Name || (Parent.Name = {})
            let parent = &name_parts[index - 1];
            self.write(current_name);
            self.write(" = ");
            self.write(parent);
            self.write(".");
            self.write(current_name);
            self.write(" || (");
            self.write(parent);
            self.write(".");
            self.write(current_name);
            self.write(" = {})");
        }

        self.write(");");
    }

    pub(super) fn emit_block(&mut self, stmts: &[IRNode]) {
        if stmts.is_empty() {
            self.write("{ }");
            return;
        }

        self.write("{");
        self.write_line();
        self.increase_indent();

        for stmt in stmts {
            self.write_indent();
            self.emit_node(stmt);
            self.write_line();
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
    }

    pub(super) fn emit_empty_block_multiline(&mut self) {
        self.write("{");
        self.write_line();
        self.write_indent();
        self.write("}");
    }
}
