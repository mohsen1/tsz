use super::super::super::Printer;
use tsz_parser::parser::node::Node;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    /// Structural predicate: is this initializer-less `PROPERTY_DECLARATION`
    /// still runtime-materialized as a *defined* field?
    ///
    /// Under `useDefineForClassFields` a field declared without an initializer is
    /// not erased: `tsc` materializes it with
    /// `Object.defineProperty(this, <name>, { ... value: void 0 })`. A computed
    /// name on such a field is therefore hoisted into a temp exactly like a
    /// computed-name field that *does* have an initializer.
    ///
    /// This is the single source of truth shared by the computed-name
    /// hoisting-classification pass and the runtime field-lowering site so the
    /// two cannot disagree about whether a no-initializer field survives to
    /// runtime. It keys purely on structural facts (no initializer + define
    /// semantics enabled) and intentionally does not re-check abstract/declare/
    /// private/accessor — those are filtered independently at each call site.
    pub(in crate::emitter) const fn no_init_property_is_runtime_materialized(
        &self,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        prop.initializer.is_none() && self.ctx.options.use_define_for_class_fields
    }

    pub(super) fn class_property_initializer_has_equals(
        &self,
        member_node: &Node,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        let Some(text) = self.source_text else {
            return true;
        };
        let Some(init_node) = self.arena.get(prop.initializer) else {
            return true;
        };
        if prop.type_annotation.is_none() {
            return true;
        }
        let start = member_node.pos as usize;
        let end = (init_node.pos as usize).min(text.len());
        if start >= end {
            return false;
        }
        let segment = &text.as_bytes()[start..end];
        let search_from = segment
            .iter()
            .rposition(|&byte| byte == b':')
            .map_or(0, |idx| idx + 1);
        segment[search_from..].contains(&b'=')
    }

    pub(super) fn node_text_contains_identifier(&self, idx: NodeIndex, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let (Some(text), Some(node)) = (self.source_text, self.arena.get(idx)) else {
            return false;
        };
        let start = (node.pos as usize).min(text.len());
        let end = (node.end as usize).min(text.len());
        if start >= end {
            return false;
        }
        let value_text = crate::import_usage::strip_type_only_content(&text[start..end]);
        super::text_contains_identifier(&value_text, name)
    }

    pub(super) fn recovered_class_body_statements(&self, node: &Node) -> Vec<String> {
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        let start = node.pos as usize;
        let end = (node.end as usize).min(text.len());
        let Some(source) = text.get(start..end) else {
            return Vec::new();
        };
        let member_spans: Vec<(u32, u32)> = self
            .arena
            .get_class(node)
            .map(|class| {
                class
                    .members
                    .nodes
                    .iter()
                    .filter_map(|&member_idx| {
                        let member_node = self.arena.get(member_idx)?;
                        Some((member_node.pos, member_node.end))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut depth = 0_i32;
        let mut recovered = Vec::new();
        let mut pending_empty_enum: Option<(String, bool)> = None;
        let mut pending_empty_class: Option<(String, bool)> = None;
        let mut in_block_comment = false;
        let mut line_offset = 0usize;
        for line in source.lines() {
            let trimmed = line.trim();
            let trimmed_start = line.len().saturating_sub(line.trim_start().len());
            let trimmed_pos = node
                .pos
                .saturating_add((line_offset + trimmed_start) as u32);
            let inside_member_span = member_spans.iter().any(|&(member_start, member_end)| {
                member_start <= trimmed_pos && trimmed_pos < member_end
            });
            if let Some((class_name, has_body)) = pending_empty_class.as_mut() {
                if depth == 2 && trimmed == "}" {
                    if !*has_body {
                        recovered.push(format!("class {class_name} {{"));
                        recovered.push("}".to_string());
                    }
                    pending_empty_class = None;
                } else if depth >= 2 && !trimmed.is_empty() {
                    *has_body = true;
                }
            }
            if let Some((enum_name, has_body)) = pending_empty_enum.as_mut() {
                if depth == 2 && trimmed == "}" {
                    if !*has_body {
                        let declaration = if self.in_namespace_iife && !self.ctx.target_es5 {
                            "let"
                        } else {
                            "var"
                        };
                        recovered.push(format!("{declaration} {enum_name};"));
                        recovered.push(format!("(function ({enum_name}) {{"));
                        recovered.push(format!("}})({enum_name} || ({enum_name} = {{}}));"));
                    }
                    pending_empty_enum = None;
                } else if depth >= 2 && !trimmed.is_empty() {
                    *has_body = true;
                }
            }
            if !inside_member_span
                && depth == 1
                && (trimmed.starts_with("function ")
                    || (trimmed.starts_with("var ")
                        && !trimmed.contains("//")
                        && !trimmed.contains("()")))
            {
                recovered.push(trimmed.replace("{}", "{ }"));
            } else if !inside_member_span
                && depth == 1
                && let Some(enum_name) = self.recovered_class_body_empty_enum_name(trimmed)
            {
                pending_empty_enum = Some((enum_name, false));
            } else if !inside_member_span
                && depth == 1
                && let Some(class_name) = self.recovered_class_body_empty_class_name(trimmed)
            {
                pending_empty_class = Some((class_name, false));
            } else if !inside_member_span
                && depth == 1
                && let Some(stmt) = self.recovered_public_class_block(trimmed)
            {
                recovered.push(stmt);
            }
            depth += class_recovery_brace_delta(line, &mut in_block_comment);
            line_offset += line.len();
            if source.as_bytes().get(line_offset) == Some(&b'\r') {
                line_offset += 1;
            }
            if source.as_bytes().get(line_offset) == Some(&b'\n') {
                line_offset += 1;
            }
        }
        recovered
    }

    fn recovered_class_body_empty_enum_name(&self, trimmed: &str) -> Option<String> {
        let rest = trimmed.strip_prefix("enum ")?;
        let name: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() {
            return None;
        }
        let after_name = rest.get(name.len()..)?.trim_start();
        (after_name == "{").then_some(name)
    }

    fn recovered_class_body_empty_class_name(&self, trimmed: &str) -> Option<String> {
        let rest = trimmed.strip_prefix("class ")?;
        let name: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() {
            return None;
        }
        let after_name = rest.get(name.len()..)?.trim_start();
        (after_name == "{").then_some(name)
    }

    pub(super) fn class_has_recovered_void_extends(
        &self,
        heritage_clauses: &Option<NodeList>,
    ) -> bool {
        let (Some(text), Some(clauses)) = (self.source_text, heritage_clauses.as_ref()) else {
            return false;
        };

        clauses.nodes.iter().any(|&clause_idx| {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                return false;
            };
            let Some(heritage) = self.arena.get_heritage(clause_node) else {
                return false;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                return false;
            }

            heritage.types.nodes.iter().any(|&type_idx| {
                let Some(type_node) = self.arena.get(type_idx) else {
                    return false;
                };
                if type_node.kind != SyntaxKind::Unknown as u16 {
                    return false;
                }
                let start = (type_node.pos as usize).min(text.len());
                let end = (type_node.end as usize).min(text.len());
                start <= end && text.get(start..end).is_some_and(|raw| raw.trim() == "void")
            })
        })
    }

    fn recovered_public_class_block(&self, trimmed: &str) -> Option<String> {
        let after_public = trimmed.strip_prefix("public")?.trim_start();
        if !after_public.starts_with('{') {
            return None;
        }
        let close = after_public.rfind('}')?;
        let inner = after_public[1..close].trim();
        if inner.is_empty() {
            return Some("{ }".to_string());
        }

        if let Some(after_open_bracket) = inner.strip_prefix('[')
            && let Some((index_params, value_type)) = after_open_bracket.split_once("]:")
        {
            let index_expr = index_params.replace(':', ", ");
            let value_type = value_type.trim();
            return Some(format!("{{\n    [{index_expr}];\n    {value_type};\n}}"));
        }

        Some(format!("{{\n    {inner};\n}}"))
    }
}

fn class_recovery_brace_delta(line: &str, in_block_comment: &mut bool) -> i32 {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut delta = 0;
    let mut quote: Option<u8> = None;

    while i < bytes.len() {
        let b = bytes[i];

        if *in_block_comment {
            if b == b'*' && bytes.get(i + 1) == Some(&b'/') {
                *in_block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

        if let Some(q) = quote {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }

        match b {
            b'\'' | b'"' | b'`' => {
                quote = Some(b);
                i += 1;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => break,
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                *in_block_comment = true;
                i += 2;
            }
            b'{' => {
                delta += 1;
                i += 1;
            }
            b'}' => {
                delta -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }

    delta
}
