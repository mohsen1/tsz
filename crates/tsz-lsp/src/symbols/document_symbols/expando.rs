use super::*;

impl<'a> DocumentSymbolProvider<'a> {
    pub(super) fn apply_expando_assignments(
        &self,
        statements: &[NodeIndex],
        symbols: &mut [DocumentSymbolEntry],
    ) {
        // Group expando members by owner name. `(owner -> Vec<(member_name,
        // prototype?, method?, fn_body?)>)`. We also track whether any
        // assignment for that owner came through `.prototype` so we can
        // inject the synthetic constructor tsc shows for JS promoted classes.
        #[derive(Clone, Copy, Debug)]
        enum MemberKindHint {
            None,
            Method,
        }
        struct ExpandoMember {
            name: String,
            is_fn: bool,
            body: NodeIndex,
            stmt_idx: NodeIndex,
            /// Property descriptor node from `Object.defineProperty` calls; `NodeIndex::NONE` otherwise.
            descriptor: NodeIndex,
            via_prototype: bool,
            kind_hint: MemberKindHint,
        }
        #[derive(Default)]
        struct Expando {
            members: Vec<ExpandoMember>,
            via_prototype: bool,
        }
        let mut groups: std::collections::BTreeMap<String, Expando> =
            std::collections::BTreeMap::new();

        for &stmt_idx in statements {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
                continue;
            }
            let Some(exp_stmt) = self.arena.get_expression_statement(stmt_node) else {
                continue;
            };
            let expr_idx = exp_stmt.expression;
            let Some(expr_node) = self.arena.get(expr_idx) else {
                continue;
            };
            if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
                let Some(bin) = self.arena.get_binary_expr(expr_node) else {
                    continue;
                };
                if bin.operator_token != SyntaxKind::EqualsToken as u16 {
                    continue;
                }
                // Special case: `X.prototype = { a, b() {}, ... }` — treat
                // each property of the RHS object literal as a prototype
                // member (same as `X.prototype.a = ...` for each).
                if let Some(owner) = self.parse_prototype_assignment(bin.left) {
                    let rhs = self.arena.get(bin.right);
                    if let Some(rhs_node) = rhs
                        && rhs_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                        && let Some(obj) = self.arena.get_literal_expr(rhs_node)
                    {
                        let entry = groups.entry(owner).or_default();
                        entry.via_prototype = true;
                        for &prop_idx in &obj.elements.nodes {
                            let Some(prop_node) = self.arena.get(prop_idx) else {
                                continue;
                            };
                            let (name_idx, init_idx, is_shorthand_method) = match prop_node.kind {
                                k if k == syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                                    let Some(prop) = self.arena.get_property_assignment(prop_node)
                                    else {
                                        continue;
                                    };
                                    (prop.name, prop.initializer, false)
                                }
                                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                    let Some(m) = self.arena.get_method_decl(prop_node) else {
                                        continue;
                                    };
                                    (m.name, NodeIndex::NONE, true)
                                }
                                k if k == syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => {
                                    let Some(s) = self.arena.get_shorthand_property(prop_node)
                                    else {
                                        continue;
                                    };
                                    (s.name, NodeIndex::NONE, false)
                                }
                                _ => continue,
                            };
                            let Some(member_name) = self.get_name(name_idx) else {
                                continue;
                            };
                            let is_fn =
                                is_shorthand_method || self.is_function_like_expression(init_idx);
                            let body = if is_fn {
                                if is_shorthand_method {
                                    self.arena
                                        .get_method_decl_at(prop_idx)
                                        .map_or(NodeIndex::NONE, |m| m.body)
                                } else {
                                    self.arena
                                        .get(init_idx)
                                        .and_then(|n| self.arena.get_function(n))
                                        .map_or(NodeIndex::NONE, |f| f.body)
                                }
                            } else {
                                NodeIndex::NONE
                            };
                            let hint = if is_shorthand_method {
                                MemberKindHint::Method
                            } else {
                                MemberKindHint::None
                            };
                            entry.members.push(ExpandoMember {
                                name: member_name,
                                is_fn,
                                body,
                                stmt_idx,
                                descriptor: NodeIndex::NONE,
                                via_prototype: true,
                                kind_hint: hint,
                            });
                        }
                        continue;
                    }
                }
                if let Some((owner, name, via_prototype)) = self.parse_expando_lhs(bin.left) {
                    let is_fn = self.is_function_like_expression(bin.right);
                    let body = if is_fn {
                        self.arena
                            .get(bin.right)
                            .and_then(|n| self.arena.get_function(n))
                            .map_or(NodeIndex::NONE, |f| f.body)
                    } else {
                        NodeIndex::NONE
                    };
                    let entry = groups.entry(owner).or_default();
                    entry.members.push(ExpandoMember {
                        name,
                        is_fn,
                        body,
                        stmt_idx,
                        descriptor: NodeIndex::NONE,
                        via_prototype,
                        kind_hint: MemberKindHint::None,
                    });
                    entry.via_prototype |= via_prototype;
                }
            } else if expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                && let Some((owner, name, via_prototype, descriptor)) =
                    self.parse_define_property(expr_idx)
            {
                // `Object.defineProperty(X, 'y', descriptor)` /
                // `Object.defineProperty(X.prototype, 'y', descriptor)` —
                // descriptor's own property members (e.g. `get`/`set`) surface
                // as the navbar entry's children. is_fn=false gives it
                // `Unknown` kind so tsc's omit-empty-kind behavior kicks in.
                let entry = groups.entry(owner).or_default();
                entry.members.push(ExpandoMember {
                    name,
                    is_fn: false,
                    body: NodeIndex::NONE,
                    stmt_idx,
                    descriptor,
                    via_prototype,
                    kind_hint: MemberKindHint::None,
                });
                entry.via_prototype |= via_prototype;
            }
        }

        if groups.is_empty() {
            return;
        }

        for sym in symbols.iter_mut() {
            let Some(expando) = groups.get(&sym.name) else {
                continue;
            };
            let promote = matches!(
                sym.kind,
                SymbolKind::Function | SymbolKind::Variable | SymbolKind::Constant
            );
            if !promote {
                continue;
            }
            let was_function = matches!(sym.kind, SymbolKind::Function);
            sym.kind = SymbolKind::Class;
            let has_ctor = sym.children.iter().any(|c| c.name == "constructor");
            if (was_function || expando.via_prototype) && !has_ctor {
                sym.children.insert(
                    0,
                    DocumentSymbolEntry {
                        name: "constructor".to_string(),
                        detail: None,
                        kind: SymbolKind::SynthesizedConstructor,
                        kind_modifiers: String::new(),
                        range: sym.range,
                        selection_range: sym.selection_range,
                        container_name: sym.container_name.clone(),
                        children: vec![],
                    },
                );
            }
            for member in &expando.members {
                let children = if member.body.is_some() {
                    self.collect_children_from_block(member.body, Some(&sym.name))
                } else if member.descriptor.is_some() {
                    self.collect_object_literal_members(member.descriptor, Some(&sym.name))
                } else {
                    Vec::new()
                };
                let kind = match member.kind_hint {
                    MemberKindHint::Method => SymbolKind::Method,
                    MemberKindHint::None => {
                        if member.is_fn {
                            SymbolKind::Function
                        } else if member.descriptor.is_some() {
                            SymbolKind::Unknown
                        } else if member.via_prototype {
                            SymbolKind::Property
                        } else {
                            SymbolKind::Unknown
                        }
                    }
                };
                let range =
                    node_range(self.arena, self.line_map, self.source_text, member.stmt_idx);
                sym.children.push(DocumentSymbolEntry {
                    name: member.name.clone(),
                    detail: None,
                    kind,
                    kind_modifiers: String::new(),
                    range,
                    selection_range: range,
                    container_name: Some(sym.name.clone()),
                    children,
                });
            }
        }
    }

    /// Match the LHS of an assignment as `X.prototype` (or
    /// `X["prototype"]`). Returns `X`'s name on success. This is the
    /// whole-object prototype form (`X.prototype = {...}`), not the
    /// per-member form handled by `parse_expando_lhs`.
    fn parse_prototype_assignment(&self, lhs: NodeIndex) -> Option<String> {
        let node = self.arena.get(lhs)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let member = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.get_name(access.name_or_argument)?
        } else {
            let arg = self.arena.get(access.name_or_argument)?;
            if arg.kind != SyntaxKind::StringLiteral as u16 {
                return None;
            }
            self.arena.get_literal(arg)?.text.clone()
        };
        if member != "prototype" {
            return None;
        }
        let root = self.arena.get(access.expression)?;
        if root.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        self.get_name(access.expression)
    }

    fn parse_expando_lhs(&self, lhs: NodeIndex) -> Option<(String, String, bool)> {
        let node = self.arena.get(lhs)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let member_name = if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            self.get_name(access.name_or_argument)?
        } else {
            let arg = self.arena.get(access.name_or_argument)?;
            if arg.kind == SyntaxKind::StringLiteral as u16 {
                let start = arg.pos as usize;
                let end = arg.end as usize;
                if start > end || end > self.source_text.len() {
                    return None;
                }
                self.source_text[start..end].trim().to_string()
            } else {
                let start = arg.pos as usize;
                let end = arg.end as usize;
                if start > end || end > self.source_text.len() {
                    return None;
                }
                let mut inner = self.source_text[start..end].trim();
                if inner.ends_with(']') {
                    inner = &inner[..inner.len() - 1];
                }
                format!("[{}]", inner.trim_end())
            }
        };

        let inner = access.expression;
        let inner_node = self.arena.get(inner)?;
        if inner_node.kind == SyntaxKind::Identifier as u16 {
            let owner = self.get_name(inner)?;
            return Some((owner, member_name, false));
        }
        if inner_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || inner_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            let inner_access = self.arena.get_access_expr(inner_node)?;
            let proto = if inner_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                self.get_name(inner_access.name_or_argument)?
            } else {
                let arg = self.arena.get(inner_access.name_or_argument)?;
                if arg.kind != SyntaxKind::StringLiteral as u16 {
                    return None;
                }
                self.arena.get_literal(arg)?.text.clone()
            };
            if proto != "prototype" {
                return None;
            }
            let root = self.arena.get(inner_access.expression)?;
            if root.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let owner = self.get_name(inner_access.expression)?;
            return Some((owner, member_name, true));
        }
        None
    }

    /// Detect `Object.defineProperty(X, 'y', descriptor)`.
    fn parse_define_property(
        &self,
        call_idx: NodeIndex,
    ) -> Option<(String, String, bool, NodeIndex)> {
        let call_node = self.arena.get(call_idx)?;
        if call_node.kind != syntax_kind_ext::CALL_EXPRESSION {
            return None;
        }
        let call = self.arena.get_call_expr(call_node)?;
        let callee = self.arena.get(call.expression)?;
        if callee.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let callee_access = self.arena.get_access_expr(callee)?;
        let callee_name = self.get_name(callee_access.name_or_argument)?;
        if callee_name != "defineProperty" {
            return None;
        }
        let root = self.arena.get(callee_access.expression)?;
        if root.kind != SyntaxKind::Identifier as u16 {
            return None;
        }
        let root_name = self.get_name(callee_access.expression)?;
        if root_name != "Object" {
            return None;
        }
        let args = call.arguments.as_ref()?;
        if args.nodes.len() < 2 {
            return None;
        }
        let target_idx = args.nodes[0];
        let name_idx = args.nodes[1];
        let name_node = self.arena.get(name_idx)?;
        if name_node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }
        let member = self.arena.get_literal(name_node)?.text.clone();
        let descriptor = args.nodes.get(2).copied().unwrap_or(NodeIndex::NONE);

        let target = self.arena.get(target_idx)?;
        if target.kind == SyntaxKind::Identifier as u16 {
            let owner = self.get_name(target_idx)?;
            return Some((owner, member, false, descriptor));
        }
        if target.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.arena.get_access_expr(target)?;
            let proto_name = self.get_name(access.name_or_argument)?;
            if proto_name != "prototype" {
                return None;
            }
            let root = self.arena.get(access.expression)?;
            if root.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let owner = self.get_name(access.expression)?;
            return Some((owner, member, true, descriptor));
        }
        None
    }

    /// Check whether an expression is a function-like value
    /// (`function () {}`, `function name() {}`, or `(a) => {}`).
    fn is_function_like_expression(&self, expr: NodeIndex) -> bool {
        let Some(node) = self.arena.get(expr) else {
            return false;
        };
        matches!(
            node.kind,
            k if k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
        )
    }
}
