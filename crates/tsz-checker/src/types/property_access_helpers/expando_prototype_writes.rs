//! JS prototype object-literal expando writes and their assignability display.
//!
//! Detects `Ctor.prototype = { ... }` object-literal expando writes and renders
//! the resulting instance shape for `CheckerState` assignability messages, plus
//! the cross-file JS expando property read resolution used by those checks.
//! Extracted verbatim from `expando.rs` to keep that shard under the size limit.

use crate::context::is_js_file_name;
use crate::state::CheckerState;
use tsz_parser::parser::NodeArena;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::types_domain) fn is_js_prototype_object_literal_expando_write(
        &mut self,
        this_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let owner_idx = match self.this_has_contextual_owner(this_expr_idx) {
            Some(owner_idx) => owner_idx,
            None => return false,
        };
        let owner_node = match self.ctx.arena.get(owner_idx) {
            Some(owner_node) => owner_node,
            None => return false,
        };
        if owner_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }

        let Some(owner_expr) = self.js_prototype_owner_expression_for_node(owner_idx) else {
            return false;
        };
        let Some(owner_target) = self.js_prototype_owner_function_target(owner_expr) else {
            return false;
        };
        let Some(instance_type) = self.js_constructor_body_instance_type_for_function(owner_target)
        else {
            return false;
        };

        !crate::query_boundaries::property_access::type_has_property(
            self.ctx.types,
            instance_type,
            self.ctx.types.intern_string(property_name),
        )
    }

    fn source_file_has_expando_assignment(
        arena: &NodeArena,
        idx: NodeIndex,
        expected_key: &str,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && Self::expando_assignment_access_key_in_arena(arena, binary.left)
                .is_some_and(|key| key == expected_key)
            && !Self::is_void_zero_or_undefined_rhs_in_arena(arena, binary.right)
        {
            return true;
        }

        for child_idx in arena.get_children(idx) {
            if Self::source_file_has_expando_assignment(arena, child_idx, expected_key) {
                return true;
            }
        }

        false
    }

    /// Accumulate a host verdict over the declaring assignments of one
    /// expando member key: `found` flips when any `<expected_key> = rhs`
    /// assignment (non-void-zero RHS) exists in this subtree, and `all_host`
    /// is AND-accumulated with each such RHS being an expando-host shape — an empty
    /// object literal, a function/arrow expression, or a class expression
    /// (tsc's `getExpandoInitializer` shapes). A single closed-shape write
    /// closes the member in either order (oracle-verified), so the verdict
    /// requires EVERY declaring write to be host-shaped.
    fn accumulate_expando_assignment_rhs_host_verdict(
        arena: &NodeArena,
        idx: NodeIndex,
        expected_key: &str,
        found: &mut bool,
        all_host: &mut bool,
    ) {
        let Some(node) = arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = arena.get_binary_expr(node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
            && Self::expando_assignment_access_key_in_arena(arena, binary.left)
                .is_some_and(|key| key == expected_key)
            && !Self::is_void_zero_or_undefined_rhs_in_arena(arena, binary.right)
        {
            *found = true;
            let rhs_is_host = arena.get(binary.right).is_some_and(|rhs| {
                rhs.is_function_expression_or_arrow()
                    || rhs.kind == syntax_kind_ext::CLASS_EXPRESSION
            }) || arena.is_empty_object_literal(binary.right);
            *all_host &= rhs_is_host;
        }

        for child_idx in arena.get_children(idx) {
            Self::accumulate_expando_assignment_rhs_host_verdict(
                arena,
                child_idx,
                expected_key,
                found,
                all_host,
            );
        }
    }

    /// Whether the base link of a nested expando chain (`a.b` in `a.b.c`) is
    /// itself an expando HOST: every syntactically visible declaring write
    /// `a.b = rhs` has a host-shaped RHS (empty literal, function, or class
    /// expression). A base with any closed-shape declaring write
    /// (`a.b = { k: 1 }`) is not a host — tsc types it as its literal shape
    /// and reports TS2339 on the nested member under `noImplicitAny`, and a
    /// single closed write closes the member even when a host-shaped write
    /// also exists (oracle-verified, either order). When NO declaring
    /// assignment is visible in the root's file or the current file (e.g. it
    /// lives in a third file, or the member came from an element-access
    /// write), stay permissive: the member-declared answer stands.
    pub(super) fn nested_expando_base_link_rhs_is_host(
        &self,
        base_expr_idx: NodeIndex,
        member_name: &str,
    ) -> bool {
        let mut file_indices: Vec<usize> = Vec::new();
        if let Some(file_idx) = self.expando_root_js_file_idx(base_expr_idx) {
            file_indices.push(file_idx);
        }
        let current_file_idx = self.ctx.current_file_idx;
        if !file_indices.contains(&current_file_idx) {
            file_indices.push(current_file_idx);
        }

        // Only the FULL chain key identifies the base link's declaring writes
        // precisely. The short last-segment key `expando_read_root_keys` also
        // returns (the import-namespace form) could alias an unrelated
        // same-named variable's writes into this verdict and close an open
        // member — a false positive; those chains simply stay on the
        // permissive no-visible-write path.
        let Some(base_key) = Self::property_access_chain_in_arena(self.ctx.arena, base_expr_idx)
        else {
            return true;
        };
        let root_keys = [base_key];
        let mut found = false;
        let mut all_host = true;
        for file_idx in file_indices {
            let arena = self.ctx.get_arena_for_file(file_idx as u32);
            let Some(source_file) = arena.source_files.first() else {
                continue;
            };
            for root_key in &root_keys {
                let expected_key = format!("{root_key}.{member_name}");
                for &stmt_idx in &source_file.statements.nodes {
                    Self::accumulate_expando_assignment_rhs_host_verdict(
                        arena,
                        stmt_idx,
                        &expected_key,
                        &mut found,
                        &mut all_host,
                    );
                    if found && !all_host {
                        return false;
                    }
                }
            }
        }
        !found || all_host
    }

    pub(super) fn js_file_has_expando_assignment_for_keys(
        &self,
        file_idx: usize,
        root_keys: &[String],
        property_name: &str,
    ) -> bool {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let Some(source_file) = arena.source_files.first() else {
            return false;
        };

        root_keys.iter().any(|root_key| {
            let expected_key = format!("{root_key}.{property_name}");
            source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    Self::source_file_has_expando_assignment(arena, stmt_idx, &expected_key)
                })
        })
    }

    fn cross_file_expando_property_read_type(
        &mut self,
        file_idx: usize,
        expected_key: &str,
    ) -> Option<TypeId> {
        let arena = self.ctx.get_arena_for_file(file_idx as u32);
        let binder = self.ctx.get_binder_for_file(file_idx)?;
        let file_name = arena
            .source_files
            .first()
            .map(|sf| sf.file_name.clone())
            .unwrap_or_else(|| self.ctx.file_name.clone());

        // No cache fast-path on this delegate; every entry is a miss.
        tsz_common::perf_counters::record_delegate_cross_arena_miss();
        let _delegate_depth_guard = tsz_common::perf_counters::enter_delegate();

        let mut checker = CheckerState::delegate_for_arena(
            arena,
            binder,
            file_name,
            self,
            tsz_common::perf_counters::CheckerCreationReason::ExpandoProperty,
        );
        checker.ctx.current_file_idx = file_idx;

        let source_file = arena.source_files.first()?;
        let mut collected: Vec<(u32, TypeId)> = Vec::new();
        for &stmt_idx in &source_file.statements.nodes {
            checker.collect_expando_property_assignment_type(
                stmt_idx,
                expected_key,
                u32::MAX,
                &mut collected,
            );
        }
        // Historical last-position-wins semantics for the JS cross-file reader.
        collected
            .into_iter()
            .max_by_key(|&(pos, _)| pos)
            .map(|(_, ty)| ty)
    }

    pub(super) fn js_expando_property_read_type_from_all_files(
        &mut self,
        root_keys: &[String],
        property_name: &str,
        preferred_file_idx: Option<usize>,
    ) -> Option<TypeId> {
        let mut file_indices = Vec::new();
        if let Some(file_idx) = preferred_file_idx {
            file_indices.push(file_idx);
        }
        if let Some(all_arenas) = self.ctx.all_arenas.as_ref() {
            for file_idx in 0..all_arenas.len() {
                if !file_indices.contains(&file_idx) {
                    file_indices.push(file_idx);
                }
            }
        } else if !file_indices.contains(&self.ctx.current_file_idx) {
            file_indices.push(self.ctx.current_file_idx);
        }

        for file_idx in file_indices {
            let arena = self.ctx.get_arena_for_file(file_idx as u32);
            let file_name = arena
                .source_files
                .first()
                .map(|sf| sf.file_name.as_str())
                .unwrap_or(self.ctx.file_name.as_str());
            if !is_js_file_name(file_name) {
                continue;
            }

            for root_key in root_keys {
                let expected_key = format!("{root_key}.{property_name}");
                if let Some(ty) =
                    self.cross_file_expando_property_read_type(file_idx, &expected_key)
                {
                    return Some(ty);
                }
            }
        }

        None
    }

    pub(in crate::types_domain) fn prior_js_prototype_object_literal_assignment_node(
        &self,
        prototype_root_expr: NodeIndex,
        read_pos: u32,
    ) -> Option<NodeIndex> {
        let root_key = Self::property_access_chain_in_arena(self.ctx.arena, prototype_root_expr)?;
        let expected_key = format!("{root_key}.prototype");
        let mut latest_match: Option<(u32, NodeIndex)> = None;

        for raw_idx in 0..self.ctx.arena.len() {
            let idx = NodeIndex(raw_idx as u32);
            let Some(node) = self.ctx.arena.get(idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::BINARY_EXPRESSION || node.pos >= read_pos {
                continue;
            }
            let Some(binary) = self.ctx.arena.get_binary_expr(node) else {
                continue;
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                continue;
            }
            if Self::expando_assignment_access_key_in_arena(self.ctx.arena, binary.left).as_deref()
                != Some(expected_key.as_str())
            {
                continue;
            }

            let rhs_idx = self.ctx.arena.skip_parenthesized(binary.right);
            let Some(rhs_node) = self.ctx.arena.get(rhs_idx) else {
                continue;
            };
            if rhs_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                continue;
            }
            if latest_match.is_none_or(|(best_pos, _)| node.pos >= best_pos) {
                latest_match = Some((node.pos, rhs_idx));
            }
        }

        latest_match.map(|(_, rhs_idx)| rhs_idx)
    }

    pub(in crate::types_domain) fn prior_js_prototype_object_literal_assignment_display(
        &mut self,
        prototype_root_expr: NodeIndex,
        read_pos: u32,
    ) -> Option<String> {
        let rhs_idx =
            self.prior_js_prototype_object_literal_assignment_node(prototype_root_expr, read_pos)?;
        self.prototype_object_literal_display(rhs_idx)
    }

    pub(crate) fn js_prototype_object_literal_receiver_display(
        &mut self,
        receiver_idx: NodeIndex,
    ) -> Option<String> {
        let mut current = receiver_idx;
        for _ in 0..16 {
            let parent = self.ctx.arena.parent_of(current)?;
            if parent.is_none() {
                return None;
            }
            let parent_node = self.ctx.arena.get(parent)?;
            if parent_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
                return self
                    .js_prototype_owner_expression_for_node(parent)
                    .and_then(|_| self.prototype_object_literal_display(parent));
            }
            current = parent;
        }
        None
    }

    fn prototype_object_literal_display(&mut self, object_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(object_idx)?;
        if node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return None;
        }
        let obj_lit = self.ctx.arena.get_literal_expr(node)?;
        let element_count = obj_lit.elements.nodes.len();
        let mut parts = Vec::with_capacity(element_count);

        for element_pos in 0..element_count {
            let elem_idx = self
                .ctx
                .arena
                .get(object_idx)
                .and_then(|node| self.ctx.arena.get_literal_expr(node))
                .and_then(|obj_lit| obj_lit.elements.nodes.get(element_pos).copied())?;
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                continue;
            };
            match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => {
                    let Some(prop) = self.ctx.arena.get_property_assignment(elem_node) else {
                        continue;
                    };
                    let Some(name) = self.prototype_object_literal_display_name(prop.name) else {
                        continue;
                    };
                    let Some(value_node) = self.ctx.arena.get(prop.initializer) else {
                        continue;
                    };
                    let value_display = if value_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION {
                        self.prototype_callable_display(prop.initializer)
                    } else {
                        let value_type = self.get_type_of_node(prop.initializer);
                        self.format_type_for_assignability_message(value_type)
                    };
                    parts.push(format!("{name}: {value_display}"));
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(elem_node) else {
                        continue;
                    };
                    let Some(name) = self.prototype_object_literal_display_name(method.name) else {
                        continue;
                    };
                    let method_display = self.prototype_callable_display(elem_idx);
                    parts.push(Self::prototype_method_display(&name, &method_display));
                }
                _ => {}
            }
        }

        Some(if parts.is_empty() {
            "{}".to_string()
        } else {
            format!("{{ {}; }}", parts.join("; "))
        })
    }

    fn prototype_callable_display(&mut self, callable_idx: NodeIndex) -> String {
        let callable_type = self.shallow_object_literal_callable_type(callable_idx);
        let display = self.format_type_for_assignability_message(callable_type);
        if self.prototype_callable_has_no_value_return(callable_idx)
            && let Some(prefix) = display.strip_suffix(" => any")
        {
            return format!("{prefix} => void");
        }

        display
    }

    fn prototype_callable_has_no_value_return(&self, callable_idx: NodeIndex) -> bool {
        let Some(callable_node) = self.ctx.arena.get(callable_idx) else {
            return false;
        };
        let body = self
            .ctx
            .arena
            .get_method_decl(callable_node)
            .map(|method| method.body)
            .or_else(|| {
                self.ctx
                    .arena
                    .get_function(callable_node)
                    .map(|func| func.body)
            });
        body.is_some_and(|body| !self.body_has_return_with_value(body))
    }

    fn prototype_object_literal_display_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        match name_node.kind {
            k if k == SyntaxKind::Identifier as u16 => self
                .ctx
                .arena
                .get_identifier(name_node)
                .map(|ident| ident.escaped_text.to_string()),
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                self.ctx
                    .arena
                    .get_literal(name_node)
                    .map(|lit| format!("\"{}\"", lit.text))
            }
            k if k == SyntaxKind::NumericLiteral as u16 => self
                .ctx
                .arena
                .get_literal(name_node)
                .map(|lit| lit.text.clone()),
            _ => self.get_property_name(name_idx),
        }
    }

    fn prototype_method_display(name: &str, function_display: &str) -> String {
        if let Some(signature) = function_display.strip_prefix('(')
            && let Some((params, return_type)) = signature.split_once(") => ")
        {
            return format!("{name}({params}): {return_type}");
        }

        format!("{name}: {function_display}")
    }

    pub(in crate::types_domain) fn prior_js_prototype_object_literal_declares_property(
        &self,
        prototype_root_expr: NodeIndex,
        property_name: &str,
        read_pos: u32,
    ) -> Option<bool> {
        let rhs_idx =
            self.prior_js_prototype_object_literal_assignment_node(prototype_root_expr, read_pos)?;
        let rhs_node = self.ctx.arena.get(rhs_idx)?;
        let obj_lit = self.ctx.arena.get_literal_expr(rhs_node)?;

        Some(obj_lit.elements.nodes.iter().copied().any(|elem_idx| {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };
            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(elem_node)
                    .and_then(|method| self.get_property_name(method.name)),
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                    .ctx
                    .arena
                    .get_accessor(elem_node)
                    .and_then(|accessor| self.get_property_name(accessor.name)),
                _ => None,
            };
            elem_prop_name.is_some_and(|name| name == property_name)
        }))
    }

    /// Whether the owner of a `X.prototype` expression is a JS *constructor*,
    /// in tsc's `isJSConstructor` sense: the function carries a `@constructor`
    /// (`@class`) JSDoc tag, or its symbol has members — which for a JS
    /// function means the body performs `this.x = ...` assignments.
    ///
    /// This is what separates a closed prototype from an open one. For a JS
    /// constructor, `X.prototype = { ... }` establishes the complete prototype
    /// and a later `X.prototype.y = ...` writing an undeclared property is
    /// TS2339. For a plain function it is an ordinary prototype-property
    /// declaration that merges with the literal, and reporting it is a false
    /// positive.
    pub(in crate::types_domain) fn js_prototype_owner_is_js_constructor(
        &mut self,
        prototype_root_expr: NodeIndex,
    ) -> bool {
        let Some(owner_target) = self.js_prototype_owner_function_target(prototype_root_expr)
        else {
            return false;
        };
        if self
            .get_jsdoc_for_function(owner_target)
            .is_some_and(|jsdoc| Self::jsdoc_contains_tag(&jsdoc, "constructor"))
        {
            return true;
        }
        self.resolve_identifier_symbol(prototype_root_expr)
            .or_else(|| self.resolve_qualified_symbol(prototype_root_expr))
            .is_some_and(|sym_id| self.symbol_has_js_constructor_evidence(sym_id))
    }
}
