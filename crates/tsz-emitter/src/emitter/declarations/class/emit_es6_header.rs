use super::super::super::Printer;
use tsz_parser::parser::node::{ClassData, Node};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(super) fn emit_class_es6_header_and_open_body(
        &mut self,
        node: &Node,
        class: &ClassData,
        assignment_prefix_is_some: bool,
        static_super_base_alias: Option<&str>,
        class_expr_temp_is_some: bool,
    ) {
        if self.should_preserve_native_decorator_comments(&class.modifiers)
            && let Some(name_node) = self.arena.get(class.name)
        {
            self.emit_comments_before_pos(name_node.pos);
        }
        self.write("class");

        // Determine the class expression name.
        // When assignment_prefix is provided (e.g., `let C = class C {}`), a named class
        // keeps its name on the expression, but an anonymous class stays anonymous
        // (`let default_1 = class {}`), even if anonymous_default_export_name is set.
        if class.name.is_some() {
            self.write_space();
            self.emit_decl_name(class.name);
        } else if !assignment_prefix_is_some {
            // No assignment prefix — use anonymous_default_export_name if available
            // (e.g., `export default class {}` → `class default_1 {}`)
            let override_name = self.anonymous_default_export_name.clone();
            if let Some(name) = override_name
                && !name.is_empty()
            {
                self.write_space();
                self.write(&name);
            }
        }

        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.arena.get_heritage(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                if !heritage.types.nodes.is_empty() {
                    self.write(" extends ");
                    for (i, &extends_type) in heritage.types.nodes.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        if let Some(base_alias) = static_super_base_alias {
                            self.write("(");
                            self.write(base_alias);
                            self.write(" = ");
                            self.emit_heritage_expression(extends_type);
                            self.write(")");
                        } else {
                            self.emit_heritage_expression(extends_type);
                        }
                    }
                } else {
                    // Error recovery: source has `extends` with no base type.
                    // Preserve the keyword to match tsc output.
                    self.write(" extends ");
                }
            }
        }

        self.write(" {");
        // Suppress trailing comments on class body opening brace.
        // tsc drops same-line comments on `{` for class bodies, just like function
        // bodies (e.g. `class C { // error` → `class C {`).
        if !self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let bytes = text.as_bytes();
            let start = node.pos as usize;
            let end = (node.end as usize).min(bytes.len());
            if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                let brace_end = (start + offset + 1) as u32;
                // Only suppress if there's a newline between `{` and the first
                // member (or the closing `}` if empty).  Single-line class bodies
                // like `class C { x: T; } // error` have the comment after `}`,
                // so we must NOT suppress it.
                // For empty classes like `class C {} // comment`, scan_end must
                // be the closing `}` position, not node.end — otherwise a newline
                // after `}` (before the next statement) causes us to incorrectly
                // suppress the trailing comment that belongs to `}`.
                let scan_end = class
                    .members
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or_else(
                        || {
                            // Empty class: find the closing `}` to use as scan_end
                            let be = brace_end as usize;
                            if be <= end {
                                bytes[be..end]
                                    .iter()
                                    .position(|&b| b == b'}')
                                    .map_or(end, |p| be + p)
                            } else {
                                end
                            }
                        },
                        |m| m.pos as usize,
                    );
                let brace_end_usize = brace_end as usize;
                let scan_end_clamped = scan_end.min(end);
                let has_newline = if brace_end_usize <= scan_end_clamped {
                    bytes[brace_end_usize..scan_end_clamped]
                        .iter()
                        .any(|&b| b == b'\n' || b == b'\r')
                } else {
                    // Malformed source: first member pos precedes the opening
                    // brace we found — skip the suppression heuristic.
                    false
                };
                if has_newline {
                    self.skip_trailing_same_line_comments(brace_end, node.end);
                }
            }
        }
        self.write_line();
        self.increase_indent();
        // When inside a comma expression wrapper (class expression with private fields
        // or static fields), add one extra indent level for the class body to match tsc.
        if class_expr_temp_is_some {
            self.increase_indent();
        }
    }
}
