//! Helper functions and secondary impl blocks for type node checking.
//!
//! Extracted from `type_node.rs` to keep module size manageable.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::type_node::{TypeLiteralSignatureScopeUpdates, TypeNodeChecker};

/// Extract the string literal text from a type-level index (e.g., `'y'` from `T['y']`).
/// In type position, the index is a `LiteralType` node wrapping a string literal.
pub(super) fn get_string_literal_from_type_index(
    arena: &tsz_parser::parser::NodeArena,
    idx: NodeIndex,
) -> Option<String> {
    let node = arena.get(idx)?;
    // Try direct literal first (for expression-like contexts)
    if let Some(lit) = arena.get_literal(node) {
        return Some(lit.text.to_string());
    }
    // In type position, the index is a LiteralType wrapping an inner literal
    if let Some(lit_type) = arena.get_literal_type(node) {
        let inner = arena.get(lit_type.literal)?;
        let lit = arena.get_literal(inner)?;
        return Some(lit.text.to_string());
    }
    None
}

/// Check if a type node is `typeof globalThis`, possibly wrapped in parentheses.
/// Used to detect `(typeof globalThis)['key']` patterns in indexed access types.
pub(super) fn is_typeof_global_this_type_node(
    arena: &tsz_parser::parser::NodeArena,
    mut node_idx: NodeIndex,
) -> bool {
    // Unwrap parenthesized types: (typeof globalThis) → typeof globalThis
    loop {
        let Some(node) = arena.get(node_idx) else {
            return false;
        };
        if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE {
            if let Some(wrapped) = arena.get_wrapped_type(node) {
                node_idx = wrapped.type_node;
                continue;
            }
            return false;
        }
        // Check if we reached a TYPE_QUERY with "globalThis" as expr_name
        if node.kind == syntax_kind_ext::TYPE_QUERY
            && let Some(tq) = arena.get_type_query(node)
            && let Some(ident_node) = arena.get(tq.expr_name)
            && let Some(ident) = arena.get_identifier(ident_node)
        {
            return ident.escaped_text == "globalThis";
        }
        return false;
    }
}

// Check duplicate parameters from a TypeNodeChecker context.
pub(crate) fn check_duplicate_parameters_in_type(
    ctx: &mut crate::CheckerContext,
    parameters: &tsz_parser::parser::NodeList,
) {
    let mut seen_names = rustc_hash::FxHashSet::default();
    for &param_idx in &parameters.nodes {
        if let Some(param_node) = ctx.arena.get(param_idx)
            && let Some(param) = ctx.arena.get_parameter(param_node)
        {
            collect_names_in_type(ctx, param.name, &mut seen_names);
        }
    }
}

fn collect_names_in_type(
    ctx: &mut crate::CheckerContext,
    name_idx: tsz_parser::parser::NodeIndex,
    seen: &mut rustc_hash::FxHashSet<String>,
) {
    use tsz_scanner::SyntaxKind;
    let Some(node) = ctx.arena.get(name_idx) else {
        return;
    };
    if node.kind == SyntaxKind::Identifier as u16 {
        if let Some(name) = ctx
            .arena
            .get_identifier(node)
            .map(|i| i.escaped_text.clone())
            && !seen.insert(name.clone())
        {
            let msg = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::DUPLICATE_IDENTIFIER,
                &[&name],
            );
            ctx.error(
                node.pos,
                node.end - node.pos,
                msg,
                crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER,
            );
        }
    } else if (node.kind == tsz_parser::parser::syntax_kind_ext::OBJECT_BINDING_PATTERN
        || node.kind == tsz_parser::parser::syntax_kind_ext::ARRAY_BINDING_PATTERN)
        && let Some(pattern) = ctx.arena.get_binding_pattern(node)
    {
        for &elem_idx in &pattern.elements.nodes {
            if let Some(elem_node) = ctx.arena.get(elem_idx) {
                if elem_node.kind == tsz_parser::parser::syntax_kind_ext::OMITTED_EXPRESSION {
                    continue;
                }
                if let Some(elem) = ctx.arena.get_binding_element(elem_node) {
                    if elem.property_name.is_some()
                        && let Some(prop_node) = ctx.arena.get(elem.property_name)
                        && prop_node.kind == SyntaxKind::Identifier as u16
                        && let Some(name_node) = ctx.arena.get(elem.name)
                        && name_node.kind == SyntaxKind::Identifier as u16
                    {
                        let prop_name = ctx
                            .arena
                            .get_identifier(prop_node)
                            .map(|i| i.escaped_text.trim_end_matches(":").trim().to_string())
                            .unwrap_or_default();
                        let name_str = ctx
                            .arena
                            .get_identifier(name_node)
                            .map(|i| i.escaped_text.clone())
                            .unwrap_or_default();
                        let msg = crate::diagnostics::format_message(crate::diagnostics::diagnostic_messages::IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION, &[&name_str, &prop_name]);
                        ctx.error(name_node.pos, name_node.end - name_node.pos, msg, crate::diagnostics::diagnostic_codes::IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION);
                    }
                    collect_names_in_type(ctx, elem.name, seen);
                }
            }
        }
    }
}

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Check if a type node is enclosed in parentheses by examining the source text.
    ///
    /// Our parser strips `ParenthesizedType` wrappers and returns the inner type
    /// directly, so we need to check the source text. We look backwards from the
    /// node's position for `(` as the first non-whitespace character. For abstract
    /// constructor types like `(abstract new () => T)`, the node starts at `new`
    /// with `abstract` before it, so we also check past `abstract`.
    fn is_type_node_parenthesized(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        let pos = node.pos as usize;
        if pos == 0 {
            return false;
        }
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return false;
        };
        let before = &sf.text[..pos];
        // Find last non-whitespace character
        let trimmed = before.trim_end();
        if trimmed.ends_with('(') {
            return true;
        }
        // Handle `abstract` modifier: `(abstract new () => T)` - the ConstructorType
        // node starts at `new`, so we need to look past `abstract` to find `(`.
        if let Some(stripped) = trimmed.strip_suffix("abstract") {
            let before_abstract = stripped.trim_end();
            if before_abstract.ends_with('(') {
                return true;
            }
        }
        false
    }

    /// TS1386/TS1388: Constructor type notation must be parenthesized when used
    /// in a union or intersection type.
    ///
    /// tsc emits these when a `new () => T` constructor type appears as a direct
    /// member of a union or intersection without enclosing parentheses.
    pub(super) fn check_grammar_constructor_type_in_union_or_intersection(
        &mut self,
        idx: NodeIndex,
    ) {
        if self.is_type_node_parenthesized(idx) {
            return;
        }
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return;
        };
        let parent = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return;
        };
        // tsc uses node.pos (full start, including leading trivia) for this error.
        let (full_start, end) = self
            .ctx
            .arena
            .get(idx)
            .map(|n| (self.full_start_of(n.pos), n.end))
            .unwrap_or((0, 0));
        let length = end.saturating_sub(full_start);

        if parent_node.kind == syntax_kind_ext::UNION_TYPE {
            self.ctx.error(
                full_start,
                length,
                "Constructor type notation must be parenthesized when used in a union type.".to_string(),
                crate::diagnostics::diagnostic_codes::CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE,
            );
        } else if parent_node.kind == syntax_kind_ext::INTERSECTION_TYPE {
            self.ctx.error(
                full_start,
                length,
                "Constructor type notation must be parenthesized when used in an intersection type.".to_string(),
                crate::diagnostics::diagnostic_codes::CONSTRUCTOR_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYP,
            );
        }
    }

    /// Compute the "full start" position of a node (including leading trivia/whitespace).
    /// tsc's `node.pos` includes leading trivia, but our parser uses token start (no trivia).
    /// This helper scans backward in the source text from the node's `pos` to find the
    /// start of leading whitespace, matching tsc's error position for TS1385/TS1387.
    fn full_start_of(&self, pos: u32) -> u32 {
        let Some(sf) = self.ctx.arena.source_files.first() else {
            return pos;
        };
        let text = &sf.text;
        let pos_usize = pos as usize;
        if pos_usize == 0 || pos_usize > text.len() {
            return pos;
        }
        let bytes = text.as_bytes();
        let mut start = pos_usize;
        // Scan backward past whitespace (spaces, tabs) but NOT past newlines
        // (newlines would cross line boundaries, which would be wrong).
        while start > 0 && matches!(bytes[start - 1], b' ' | b'\t') {
            start -= 1;
        }
        start as u32
    }

    /// TS1385/TS1387: Function type notation must be parenthesized when used
    /// in a union or intersection type.
    ///
    /// tsc also detects when a function type inside an intersection is transitively
    /// inside a union (e.g. `() => void | () => void & any`), emitting TS1385 for
    /// the union level in addition to TS1387 for the intersection level.
    pub(super) fn check_grammar_function_type_in_union_or_intersection(&mut self, idx: NodeIndex) {
        if self.is_type_node_parenthesized(idx) {
            return;
        }
        let Some(ext) = self.ctx.arena.get_extended(idx) else {
            return;
        };
        let parent = ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent) else {
            return;
        };
        // tsc uses node.pos (full start, including leading trivia) for this error.
        // Our nodes store token start (no trivia), so compute the full start.
        let (full_start, end) = self
            .ctx
            .arena
            .get(idx)
            .map(|n| (self.full_start_of(n.pos), n.end))
            .unwrap_or((0, 0));
        let length = end.saturating_sub(full_start);

        if parent_node.kind == syntax_kind_ext::UNION_TYPE {
            self.ctx.error(
                full_start,
                length,
                "Function type notation must be parenthesized when used in a union type.".to_string(),
                crate::diagnostics::diagnostic_codes::FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE,
            );
        } else if parent_node.kind == syntax_kind_ext::INTERSECTION_TYPE {
            self.ctx.error(
                full_start,
                length,
                "Function type notation must be parenthesized when used in an intersection type.".to_string(),
                crate::diagnostics::diagnostic_codes::FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_AN_INTERSECTION_TYPE,
            );
            // tsc also emits TS1385 when the intersection is itself inside a union
            // AND the function type is the first constituent of the intersection.
            // This matches tsc's parser behavior where `parseFunctionOrConstructorTypeToError`
            // catches function types at the union level before they get wrapped in an
            // intersection. E.g., `void | () => void & any` — the `(` is at the union
            // constituent boundary so tsc catches it at both levels.
            let is_first_in_intersection = self
                .ctx
                .arena
                .get_composite_type(parent_node)
                .is_some_and(|ct| ct.types.nodes.first().copied() == Some(idx));
            if is_first_in_intersection {
                if let Some(gp_ext) = self.ctx.arena.get_extended(parent) {
                    if let Some(gp_node) = self.ctx.arena.get(gp_ext.parent) {
                        if gp_node.kind == syntax_kind_ext::UNION_TYPE {
                            self.ctx.error(
                                full_start,
                                length,
                                "Function type notation must be parenthesized when used in a union type.".to_string(),
                                crate::diagnostics::diagnostic_codes::FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Recursively scan a type node subtree for unparenthesized function/constructor
    /// types in union/intersection contexts. This is needed because function type
    /// return types are processed through TypeLowering, which doesn't trigger the
    /// grammar checks that `compute_type` normally runs.
    pub(super) fn check_nested_function_types_in_type(&mut self, root: NodeIndex) {
        if root.is_none() {
            return;
        }
        let mut stack = vec![root];
        while let Some(node_idx) = stack.pop() {
            let Some(node) = self.ctx.arena.get(node_idx) else {
                continue;
            };
            match node.kind {
                k if k == syntax_kind_ext::FUNCTION_TYPE => {
                    self.check_grammar_function_type_in_union_or_intersection(node_idx);
                    // Also check nested return types recursively
                    if let Some(ft) = self.ctx.arena.get_function_type(node) {
                        stack.push(ft.type_annotation);
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                    self.check_grammar_constructor_type_in_union_or_intersection(node_idx);
                    if let Some(ft) = self.ctx.arena.get_function_type(node) {
                        stack.push(ft.type_annotation);
                    }
                }
                k if k == syntax_kind_ext::UNION_TYPE
                    || k == syntax_kind_ext::INTERSECTION_TYPE =>
                {
                    if let Some(ct) = self.ctx.arena.get_composite_type(node) {
                        for &child in &ct.types.nodes {
                            stack.push(child);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn push_type_parameters_for_type_literal_signature(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> (
        Vec<tsz_solver::TypeParamInfo>,
        TypeLiteralSignatureScopeUpdates,
    ) {
        let Some(list) = type_parameters else {
            return (Vec::new(), Vec::new());
        };

        let factory = self.ctx.types.factory();
        let mut params = Vec::with_capacity(list.nodes.len());
        let mut updates = Vec::with_capacity(list.nodes.len());

        // First pass: seed names so later constraints can reference earlier
        // or self-recursive type parameters.
        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };
            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
            let atom = self.ctx.types.intern_string(&name);
            let type_id = factory.type_param(tsz_solver::TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            });
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous));
        }

        // Second pass: refine the visible type-parameter entries with their
        // constraints/defaults and return the final metadata for the signature.
        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };
            let name = self
                .ctx
                .arena
                .get(data.name)
                .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                .map_or_else(|| "T".to_string(), |id_data| id_data.escaped_text.clone());
            let atom = self.ctx.types.intern_string(&name);
            let constraint =
                (data.constraint != NodeIndex::NONE).then(|| self.check(data.constraint));
            let default = if data.default != NodeIndex::NONE {
                let default_type = self.check(data.default);
                (default_type != TypeId::ERROR).then_some(default_type)
            } else {
                None
            };
            let is_const = self
                .ctx
                .arena
                .has_modifier(&data.modifiers, tsz_scanner::SyntaxKind::ConstKeyword);
            let info = tsz_solver::TypeParamInfo {
                name: atom,
                constraint,
                default,
                is_const,
            };
            let type_id = factory.type_param(info);
            self.ctx.type_parameter_scope.insert(name, type_id);
            params.push(info);
        }

        (params, updates)
    }

    pub(super) fn pop_type_parameters_for_type_literal_signature(
        &mut self,
        updates: Vec<(String, Option<TypeId>)>,
    ) {
        for (name, previous) in updates.into_iter().rev() {
            if let Some(prev_type) = previous {
                self.ctx.type_parameter_scope.insert(name, prev_type);
            } else {
                self.ctx.type_parameter_scope.remove(&name);
            }
        }
    }

    /// Check if an index signature parameter type annotation refers to a type parameter
    /// or literal type (TS1337) rather than a plain invalid type (TS1268).
    pub(super) fn is_type_param_or_literal_in_index_sig(
        &self,
        type_annotation_idx: NodeIndex,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_annotation_idx) else {
            return false;
        };

        // Literal types: string/number/boolean literals
        if type_node.kind == syntax_kind_ext::LITERAL_TYPE
            || type_node.kind == SyntaxKind::StringLiteral as u16
            || type_node.kind == SyntaxKind::NumericLiteral as u16
            || type_node.kind == SyntaxKind::TrueKeyword as u16
            || type_node.kind == SyntaxKind::FalseKeyword as u16
        {
            return true;
        }

        // Type references: check if they resolve to type parameters
        if type_node.kind == syntax_kind_ext::TYPE_REFERENCE
            && let Some(type_ref) = self.ctx.arena.get_type_ref(type_node)
            && let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
        {
            // Check the type parameter scope (covers generic type params from
            // type aliases, functions, classes, etc.)
            if self
                .ctx
                .type_parameter_scope
                .contains_key(ident.escaped_text.as_str())
            {
                return true;
            }
        }

        false
    }

    /// Check if a resolved type is an array or tuple type (concrete, not a type parameter).
    /// Used by TS1265/TS1266 checks to distinguish concrete rest elements from variadic
    /// type parameter spreads. Only concrete array/tuple rest elements are subject to
    /// the "rest after rest" and "optional after rest" restrictions.
    pub(super) fn is_array_or_tuple_type(&self, type_id: tsz_solver::TypeId) -> bool {
        tsz_solver::is_array_type(self.ctx.types, type_id)
            || tsz_solver::is_tuple_type(self.ctx.types, type_id)
    }

    pub(super) fn is_this_type_allowed(
        &self,
        this_node_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut child_idx = this_node_idx;
        let mut current = self
            .ctx
            .arena
            .get_extended(this_node_idx)
            .map(|ext| ext.parent);

        while let Some(parent_idx) = current {
            if parent_idx.is_none() {
                break;
            }
            let Some(node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            match node.kind {
                // Nodes that PROVIDE a 'this' type context
                syntax_kind_ext::CLASS_DECLARATION
                | syntax_kind_ext::CLASS_EXPRESSION
                | syntax_kind_ext::INTERFACE_DECLARATION => {
                    return true;
                }

                // Class/Interface members
                syntax_kind_ext::METHOD_DECLARATION
                | syntax_kind_ext::PROPERTY_DECLARATION
                | syntax_kind_ext::GET_ACCESSOR
                | syntax_kind_ext::SET_ACCESSOR
                | syntax_kind_ext::INDEX_SIGNATURE
                | syntax_kind_ext::PROPERTY_SIGNATURE
                | syntax_kind_ext::METHOD_SIGNATURE => {
                    // If it's static, 'this' type is not allowed.
                    // We must check the modifier list (not node.flags, which holds
                    // node_flags — a different namespace where bit 8 is CONTAINS_THIS,
                    // not STATIC).
                    let is_static = self.node_has_static_modifier(parent_idx, node);
                    if is_static {
                        return false;
                    }
                    // Otherwise, it's an instance member, so 'this' type is allowed.
                    // We continue walking up, we will eventually hit the class/interface declaration.
                }

                // Nodes that BLOCK 'this' type context
                syntax_kind_ext::CONSTRUCTOR => {
                    // 'this' type not allowed in constructor parameters or return type,
                    // but it IS allowed in the constructor body.
                    if let Some(c) = self.ctx.arena.get_constructor(node)
                        && child_idx == c.body
                    {
                        return true; // The body provides a 'this' context
                    }
                    return false;
                }

                syntax_kind_ext::FUNCTION_DECLARATION
                | syntax_kind_ext::FUNCTION_EXPRESSION
                | syntax_kind_ext::MODULE_DECLARATION
                | syntax_kind_ext::TYPE_LITERAL
                | syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
                | syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION => {
                    return false;
                }

                // Everything else (ARROW_FUNCTION, MAPPED_TYPE, BLOCK, RETURN_STATEMENT, etc.)
                // just passes through to the parent.
                _ => {}
            }

            child_idx = parent_idx;
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }

        false
    }

    /// Check whether a class/interface member node has the `static` modifier
    /// by inspecting its declaration data's modifier list.
    fn node_has_static_modifier(
        &self,
        _node_idx: tsz_parser::parser::NodeIndex,
        node: &tsz_parser::parser::node::Node,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let modifiers = match node.kind {
            syntax_kind_ext::METHOD_DECLARATION => self
                .ctx
                .arena
                .get_method_decl(node)
                .map(|m| m.modifiers.clone()),
            syntax_kind_ext::PROPERTY_DECLARATION => self
                .ctx
                .arena
                .get_property_decl(node)
                .map(|p| p.modifiers.clone()),
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                .ctx
                .arena
                .get_accessor(node)
                .map(|a| a.modifiers.clone()),
            syntax_kind_ext::INDEX_SIGNATURE => self
                .ctx
                .arena
                .get_index_signature(node)
                .map(|i| i.modifiers.clone()),
            // PROPERTY_SIGNATURE and METHOD_SIGNATURE cannot be static
            // (they are interface members), so return false.
            _ => None,
        };

        if let Some(mods) = modifiers {
            self.ctx
                .arena
                .has_modifier(&mods, SyntaxKind::StaticKeyword)
        } else {
            false
        }
    }
}

pub(crate) fn check_parameter_initializers_in_type(
    ctx: &mut crate::CheckerContext,
    parameters: &tsz_parser::parser::NodeList,
) {
    for &param_idx in &parameters.nodes {
        if let Some(param_node) = ctx.arena.get(param_idx)
            && let Some(param) = ctx.arena.get_parameter(param_node)
        {
            if param.initializer.is_some() {
                // TSC anchors the error at the parameter name, not the initializer
                let name_node = ctx.arena.get(param.name).unwrap_or(param_node);
                ctx.error(
                    name_node.pos,
                    name_node.end - name_node.pos,
                    "A parameter initializer is only allowed in a function or constructor implementation."
                        .to_string(),
                    2371,
                );
            }
            // Also check binding elements inside destructuring patterns for initializers
            // e.g. `type Foo = ({ first = 0 }: T) => void` — TSC reports TS2371 at `first`
            check_binding_pattern_initializers(ctx, param.name);
        }
    }
}

fn check_binding_pattern_initializers(
    ctx: &mut crate::CheckerContext,
    name_idx: tsz_parser::parser::NodeIndex,
) {
    use tsz_parser::parser::syntax_kind_ext;

    let Some(name_node) = ctx.arena.get(name_idx) else {
        return;
    };

    if name_node.kind != syntax_kind_ext::OBJECT_BINDING_PATTERN
        && name_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
    {
        return;
    }

    let Some(pattern) = ctx.arena.get_binding_pattern(name_node) else {
        return;
    };

    let elements = pattern.elements.nodes.clone();
    for &elem_idx in &elements {
        let Some(elem_node) = ctx.arena.get(elem_idx) else {
            continue;
        };
        let Some(elem) = ctx.arena.get_binding_element(elem_node) else {
            continue;
        };
        let has_init = elem.initializer != tsz_parser::parser::NodeIndex::NONE;
        let elem_name = elem.name;
        if has_init {
            let anchor = ctx.arena.get(elem_name).unwrap_or(elem_node);
            ctx.error(
                anchor.pos,
                anchor.end - anchor.pos,
                "A parameter initializer is only allowed in a function or constructor implementation."
                    .to_string(),
                2371,
            );
        }
        // Recurse into nested binding patterns
        check_binding_pattern_initializers(ctx, elem_name);
    }
}
