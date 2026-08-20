//! Helper functions and secondary impl blocks for type node checking.
//!
//! Extracted from `type_node.rs` to keep module size manageable.

use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

use super::type_node::{TypeLiteralSignatureScopeUpdates, TypeNodeChecker};
use crate::query_boundaries::indexed_access_key_space;
use crate::query_boundaries::signature_building as signature_building_boundary;
use crate::query_boundaries::type_construction;

/// Extract the string literal text from a type-level index (e.g., `'y'` from `T['y']`).
/// In type position, the index is a `LiteralType` node wrapping a string literal.
pub(crate) fn get_string_literal_from_type_index(
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
pub(crate) fn is_typeof_global_this_type_node(
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

pub(crate) fn is_type_query_in_non_flow_sensitive_signature_parameter(
    arena: &tsz_parser::parser::NodeArena,
    idx: NodeIndex,
) -> bool {
    let mut current = idx;
    while let Some(ext) = arena.get_extended(current) {
        let parent = ext.parent;
        if parent.is_none() {
            break;
        }

        let Some(parent_node) = arena.get(parent) else {
            break;
        };

        match parent_node.kind {
            k if k == syntax_kind_ext::CALL_SIGNATURE
                || k == syntax_kind_ext::CONSTRUCT_SIGNATURE
                || k == syntax_kind_ext::METHOD_SIGNATURE
                || k == syntax_kind_ext::FUNCTION_TYPE
                || k == syntax_kind_ext::CONSTRUCTOR_TYPE =>
            {
                return true;
            }
            k if k == syntax_kind_ext::FUNCTION_DECLARATION
                || k == syntax_kind_ext::FUNCTION_EXPRESSION
                || k == syntax_kind_ext::ARROW_FUNCTION
                || k == syntax_kind_ext::METHOD_DECLARATION
                || k == syntax_kind_ext::CONSTRUCTOR
                || k == syntax_kind_ext::GET_ACCESSOR
                || k == syntax_kind_ext::SET_ACCESSOR =>
            {
                return false;
            }
            _ => {}
        }

        current = parent;
    }

    false
}

/// TS1229: A type predicate is only allowed in return type position for
/// functions and methods. Reports the diagnostic on the predicate when it
/// appears as the return-type annotation of a `CONSTRUCTOR_TYPE` node.
pub(crate) fn report_type_predicate_in_constructor_type(
    ctx: &mut crate::CheckerContext,
    node_kind: u16,
    type_annotation: tsz_parser::parser::NodeIndex,
) {
    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
    if node_kind != syntax_kind_ext::CONSTRUCTOR_TYPE {
        return;
    }
    let Some(tn) = ctx.arena.get(type_annotation) else {
        return;
    };
    if tn.kind != syntax_kind_ext::TYPE_PREDICATE {
        return;
    }
    ctx.error(
        tn.pos,
        tn.end - tn.pos,
        diagnostic_messages::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO.to_string(),
        diagnostic_codes::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHO,
    );
}

// Check duplicate parameters from a TypeNodeChecker context.
//
// `tsc` runs `checkGrammarParameterList` for every function-like signature
// written in type position — a function/constructor type and every call,
// construct, and method signature of an object type literal — and reports
// `TS2300` on **every** occurrence of a repeated parameter name, exactly as it
// does for a function declaration or an interface member. The `seen` map keeps
// the first occurrence's node so the second occurrence can retroactively blame
// it, matching `CheckerState::check_duplicate_parameters`.
pub(crate) fn check_duplicate_parameters_in_type(
    ctx: &mut crate::CheckerContext,
    parameters: &tsz_parser::parser::NodeList,
) {
    let mut seen_names = rustc_hash::FxHashMap::default();
    for &param_idx in &parameters.nodes {
        if let Some(param_node) = ctx.arena.get(param_idx)
            && let Some(param) = ctx.arena.get_parameter(param_node)
        {
            collect_names_in_type(ctx, param.name, &mut seen_names);
        }
    }
}

/// Property-name node kinds that can form the "renamed-from" side of a
/// destructuring rename in a signature binding pattern (TS2842). `tsc` flags
/// the rename regardless of how the property name is written, so identifier,
/// string-literal, numeric-literal, and computed names all qualify.
const fn is_renaming_source_property_kind(kind: u16) -> bool {
    kind == SyntaxKind::Identifier as u16
        || kind == SyntaxKind::StringLiteral as u16
        || kind == SyntaxKind::NumericLiteral as u16
        || kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
}

/// Render the "renamed-from" property name for a TS2842 message. An identifier
/// uses its escaped text; every other kind uses its verbatim source text so the
/// message matches `tsc` with quotes and brackets preserved (`"a"`, `2`,
/// `["a"]`, `[2]`).
fn renaming_property_name_text(
    ctx: &crate::CheckerContext,
    prop_idx: NodeIndex,
    prop_kind: u16,
) -> String {
    if prop_kind == SyntaxKind::Identifier as u16 {
        return ctx
            .arena
            .get(prop_idx)
            .and_then(|n| ctx.arena.get_identifier(n))
            .map(|i| i.escaped_text.trim_end_matches(':').trim().to_string())
            .unwrap_or_default();
    }
    node_source_text(ctx, prop_idx)
        .map(|t| t.trim().to_string())
        .unwrap_or_default()
}

/// Verbatim source text of `idx`, resolving the source file that owns the node
/// so multi-file programs slice from the correct buffer (mirrors the checker's
/// `owning_source_file` walk).
fn node_source_text(ctx: &crate::CheckerContext, idx: NodeIndex) -> Option<String> {
    let node = ctx.arena.get(idx)?;
    let start = node.pos as usize;
    let end = node.end as usize;
    let mut current = idx;
    let text = loop {
        let n = ctx.arena.get(current)?;
        if n.kind == syntax_kind_ext::SOURCE_FILE {
            break ctx.arena.get_source_file(n)?.text.clone();
        }
        let info = ctx.arena.node_info(current)?;
        if info.parent.is_none() {
            return None;
        }
        current = info.parent;
    };
    text.get(start..end).map(str::to_string)
}

fn collect_names_in_type(
    ctx: &mut crate::CheckerContext,
    name_idx: tsz_parser::parser::NodeIndex,
    seen: &mut rustc_hash::FxHashMap<String, NodeIndex>,
) {
    use tsz_scanner::SyntaxKind;
    let Some(node) = ctx.arena.get(name_idx) else {
        return;
    };
    if node.kind == SyntaxKind::Identifier as u16 {
        // Capture the current occurrence's span before any `&mut ctx` call so no
        // arena borrow is live across the diagnostic emission.
        let cur_pos = node.pos;
        let cur_len = node.end - node.pos;
        if let Some(name) = ctx
            .arena
            .get_identifier(node)
            .map(|i| i.escaped_text.to_string())
        {
            use std::collections::hash_map::Entry;
            let msg = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::DUPLICATE_IDENTIFIER,
                &[&name],
            );
            let code = crate::diagnostics::diagnostic_codes::DUPLICATE_IDENTIFIER;
            match seen.entry(name) {
                Entry::Occupied(mut entry) => {
                    // Blame the first occurrence exactly once (mark it consumed
                    // with `NONE`), then blame this one. tsc anchors TS2300 on
                    // every occurrence of the repeated name.
                    let first_idx = *entry.get();
                    if first_idx != NodeIndex::NONE {
                        if let Some((first_pos, first_len)) =
                            ctx.arena.get(first_idx).map(|n| (n.pos, n.end - n.pos))
                        {
                            ctx.error(first_pos, first_len, msg.clone(), code);
                        }
                        entry.insert(NodeIndex::NONE);
                    }
                    ctx.error(cur_pos, cur_len, msg, code);
                }
                Entry::Vacant(entry) => {
                    entry.insert(name_idx);
                }
            }
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
                    let prop_idx = elem.property_name;
                    let name_idx = elem.name;
                    // TS2842 reports a renaming that is *unused*. The renamed
                    // local is in scope for the signature's own type
                    // positions, so a `typeof` query naming it makes the
                    // renaming used and `tsc` stays silent. The property side of
                    // the rename may be spelled as an identifier (`a`), a string
                    // literal (`"a"`), a numeric literal (`2`), or a computed
                    // name (`["a"]`, `[2]`) — tsc flags all of them, so the gate
                    // matches every property-name kind, not just identifiers.
                    if prop_idx.is_some()
                        && !ctx.is_declaration_file()
                        && let Some(prop_node) = ctx.arena.get(prop_idx)
                        && is_renaming_source_property_kind(prop_node.kind)
                        && let Some(name_node) = ctx.arena.get(name_idx)
                        && name_node.kind == SyntaxKind::Identifier as u16
                        && !ctx.arena.get_identifier_text(name_idx).is_some_and(|n| {
                            crate::types_domain::signature_binding_scope::binding_is_referenced_by_type_query(
                                ctx, elem_idx, n,
                            )
                        })
                    {
                        let prop_kind = prop_node.kind;
                        let name_pos = name_node.pos;
                        let name_len = name_node.end - name_node.pos;
                        let prop_name = renaming_property_name_text(ctx, prop_idx, prop_kind);
                        let name_str = ctx
                            .arena
                            .get(name_idx)
                            .and_then(|n| ctx.arena.get_identifier(n))
                            .map(|i| i.escaped_text.clone())
                            .unwrap_or_default();
                        let msg = crate::diagnostics::format_message(crate::diagnostics::diagnostic_messages::IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION, &[&name_str, &prop_name]);
                        ctx.error(name_pos, name_len, msg, crate::diagnostics::diagnostic_codes::IS_AN_UNUSED_RENAMING_OF_DID_YOU_INTEND_TO_USE_IT_AS_A_TYPE_ANNOTATION);
                    }
                    collect_names_in_type(ctx, name_idx, seen);
                }
            }
        }
    }
}

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Shared rest-element grammar check for a tuple type's `...X` element,
    /// covering both bare `RestType` and dot-dot-dot `NamedTupleMember` forms.
    ///
    /// Mirrors the variadic arm of `tsc`'s `checkTupleType`: if the resolved
    /// element type is not array-like, emit TS2574; otherwise, when it is a
    /// variable-length array/tuple, track it for the TS1265 "rest after rest"
    /// check. `type_node` is the inner (unwrapped) element type node, used only
    /// to recognise the `...T[]` / `...Array<T>` surface forms that are always
    /// variable-length. Once any ordering diagnostic has fired for the tuple
    /// (`grammar_broke`), further ordering diagnostics are suppressed to match
    /// `tsc`'s single-`break` loop.
    pub(super) fn check_tuple_rest_element_grammar(
        &mut self,
        elem_type: TypeId,
        type_node: NodeIndex,
        pos: u32,
        end: u32,
        seen_rest: &mut bool,
        grammar_broke: &mut bool,
    ) {
        if self.rest_type_node_is_unconstrained_infer(type_node) {
            if *seen_rest && !*grammar_broke {
                self.ctx.error(
                    pos,
                    end.saturating_sub(pos),
                    crate::diagnostics::diagnostic_messages::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT.to_string(),
                    crate::diagnostics::diagnostic_codes::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT,
                );
                *grammar_broke = true;
            }
            *seen_rest = true;
            return;
        }
        if !self.rest_element_type_is_array_like(elem_type) {
            if !*grammar_broke {
                self.emit_rest_element_type_must_be_array(pos, end);
                *grammar_broke = true;
            }
            return;
        }

        let is_variadic = self.is_variadic_array_or_tuple(elem_type)
            || Self::ast_kind_is_obviously_array_or_tuple(self.ctx.arena, type_node);
        if is_variadic {
            if *seen_rest && !*grammar_broke {
                self.ctx.error(
                    pos,
                    end.saturating_sub(pos),
                    crate::diagnostics::diagnostic_messages::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT.to_string(),
                    crate::diagnostics::diagnostic_codes::A_REST_ELEMENT_CANNOT_FOLLOW_ANOTHER_REST_ELEMENT,
                );
                *grammar_broke = true;
            }
            *seen_rest = true;
        }
    }

    pub(super) fn emit_rest_element_type_must_be_array(&mut self, pos: u32, end: u32) {
        self.ctx.error(
            pos,
            end.saturating_sub(pos),
            crate::diagnostics::diagnostic_messages::A_REST_ELEMENT_TYPE_MUST_BE_AN_ARRAY_TYPE
                .to_string(),
            crate::diagnostics::diagnostic_codes::A_REST_ELEMENT_TYPE_MUST_BE_AN_ARRAY_TYPE,
        );
    }

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
            .map_or((0, 0), |n| (self.full_start_of(n.pos), n.end));
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
            .map_or((0, 0), |n| (self.full_start_of(n.pos), n.end));
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
            if is_first_in_intersection
                && let Some(gp_ext) = self.ctx.arena.get_extended(parent)
                && let Some(gp_node) = self.ctx.arena.get(gp_ext.parent)
                && gp_node.kind == syntax_kind_ext::UNION_TYPE
            {
                self.ctx.error(
                                full_start,
                                length,
                                "Function type notation must be parenthesized when used in a union type.".to_string(),
                                crate::diagnostics::diagnostic_codes::FUNCTION_TYPE_NOTATION_MUST_BE_PARENTHESIZED_WHEN_USED_IN_A_UNION_TYPE,
                            );
            }
        }
    }

    /// Recursively scan a type node subtree for unparenthesized function/constructor
    /// types in union/intersection contexts. This is needed because function type
    /// return types are processed through `TypeLowering`, which doesn't trigger the
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
                .map_or_else(
                    || "T".to_string(),
                    |id_data| id_data.escaped_text.to_string(),
                );
            let atom = self.ctx.types.intern_string(&name);
            let info = signature_building_boundary::user_type_param_info(atom, None, None, false);
            let type_id = signature_building_boundary::user_type_param(self.ctx.types, info);
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
                .map_or_else(
                    || "T".to_string(),
                    |id_data| id_data.escaped_text.to_string(),
                );
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
            let info = signature_building_boundary::user_type_param_info(
                atom, constraint, default, is_const,
            );
            let type_id = signature_building_boundary::user_type_param(self.ctx.types, info);
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

        // Composite types: recurse into union/intersection members so that
        // a generic type parameter or literal nested in a composite (e.g.
        // `T & string`, `"a" | "b"`) is detected. Without this, the AST
        // fallback would accept `T & string` as a valid intersection and
        // suppress TS1268 entirely.
        if type_node.kind == syntax_kind_ext::UNION_TYPE
            || type_node.kind == syntax_kind_ext::INTERSECTION_TYPE
        {
            if let Some(composite) = self.ctx.arena.get_composite_type(type_node) {
                return composite
                    .types
                    .nodes
                    .iter()
                    .any(|&m| self.is_type_param_or_literal_in_index_sig(m));
            }
            return false;
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

    /// Returns `true` for variadic (variable-length) array/tuple AST nodes: `T[]`,
    /// `Array<T>`, `ReadonlyArray<T>`, or a tuple that itself contains a rest element.
    /// Fixed-length tuple spreads (`...[1, 2]`) return `false`; they inline as individual
    /// elements and are not subject to the TS1265 "rest after rest" restriction.
    pub(super) fn ast_kind_is_obviously_array_or_tuple(
        arena: &tsz_parser::parser::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        match node.kind {
            k if k == syntax_kind_ext::ARRAY_TYPE => true,
            k if k == syntax_kind_ext::TUPLE_TYPE => {
                let Some(tuple_data) = arena.get_tuple_type(node) else {
                    return false;
                };
                tuple_data.elements.nodes.iter().any(|&elem_idx| {
                    let Some(elem) = arena.get(elem_idx) else {
                        return false;
                    };
                    elem.kind == syntax_kind_ext::REST_TYPE
                        || (elem.kind == syntax_kind_ext::NAMED_TUPLE_MEMBER
                            && arena
                                .get_named_tuple_member(elem)
                                .is_some_and(|d| d.dot_dot_dot_token))
                })
            }
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                let Some(type_ref) = arena.get_type_ref(node) else {
                    return false;
                };
                let has_type_args = type_ref
                    .type_arguments
                    .as_ref()
                    .is_some_and(|args| !args.nodes.is_empty());
                if !has_type_args {
                    return false;
                }
                let Some(name_node) = arena.get(type_ref.type_name) else {
                    return false;
                };
                let Some(ident) = arena.get_identifier(name_node) else {
                    return false;
                };
                matches!(ident.escaped_text.as_str(), "Array" | "ReadonlyArray")
            }
            _ => false,
        }
    }

    /// Returns `true` for arrays and variable-length tuples (tuples with a rest element).
    /// Fixed-length tuples return `false`. Used by TS1265/TS1266 to decide whether a
    /// spread counts as a "rest" element for "rest after rest" / "optional after rest".
    pub(super) fn is_variadic_array_or_tuple(&self, type_id: tsz_solver::TypeId) -> bool {
        let mut type_id = type_id;
        while let Some(inner) =
            crate::query_boundaries::common::unwrap_readonly_or_noinfer(self.ctx.types, type_id)
        {
            type_id = inner;
        }

        crate::query_boundaries::common::is_array_type(self.ctx.types, type_id)
            || (crate::query_boundaries::common::is_tuple_type(self.ctx.types, type_id)
                && crate::query_boundaries::common::get_fixed_tuple_length(self.ctx.types, type_id)
                    .is_none())
    }

    /// Decide the rest-element grammar check (TS2574) the way `tsc`'s
    /// `isArrayLikeType` does — from the *resolved* type rather than the AST
    /// shape — but conservatively: return `true` (legal rest) unless the type is
    /// *definitely* not array-like.
    ///
    /// `tsc` runs `isArrayLikeType` on a fully instantiated type, so utility
    /// spreads (`[...Tuple<I, E>]`, `[...NTuple<A>]`) and parameters constrained
    /// to them (`<S extends Selector[]> [...S]`) resolve to arrays before the
    /// check and are accepted. tsz's solver resolver is a no-op for `Lazy` defs,
    /// so such types may stay opaque here; treating opaque/instantiable types as
    /// "indeterminate" (legal) avoids false TS2574 on them while still flagging
    /// the concrete non-array cases (`[...string]`, `[...{a:1}]`, unconstrained
    /// `[...T]`, `[...unknown]`).
    pub(super) fn rest_element_type_is_array_like(&self, type_id: tsz_solver::TypeId) -> bool {
        !self.rest_element_type_is_definitely_not_array_like(type_id, 0)
    }

    /// Recursive classifier backing [`Self::rest_element_type_is_array_like`].
    /// Returns `true` only when the resolved type is *definitely* not array-like
    /// (so TS2574 should fire). Array/tuple, `any`/`never`, and types that remain
    /// instantiable after resolution (applications, conditionals, mapped types,
    /// and parameters constrained to them) all return `false`.
    fn rest_element_type_is_definitely_not_array_like(
        &self,
        type_id: tsz_solver::TypeId,
        depth: u32,
    ) -> bool {
        use crate::query_boundaries::{common as q, type_checking_utilities as type_utils};
        // Bound the constraint/wrapper/union recursion; an undecided type is not
        // "definitely" non-array-like, so give up in the legal direction.
        if depth > 8 {
            return false;
        }
        let t = self.resolve_type_for_rest_element_check(type_id);

        // Concrete array/tuple shapes are array-like.
        if q::is_array_type(self.ctx.types, t) || q::is_tuple_type(self.ctx.types, t) {
            return false;
        }
        // `any`/`never`/error are assignable to `readonly any[]`.
        if matches!(t, TypeId::ANY | TypeId::NEVER | TypeId::ERROR) {
            return false;
        }
        // Error-type contagion: a rest element rooted at an *unresolved imported
        // alias* (e.g. `[...TupleParts<T>["suffix"]]` where `TupleParts` comes
        // from a module that failed to resolve — already flagged TS2307) has the
        // permissive `error` apparent type in tsc, which is assignable to
        // `readonly any[]`, so the spread is legal. Treat it as array-like
        // (indeterminate) rather than emitting a spurious TS2574.
        if self.ctx.type_references_unresolved_import(t) {
            return false;
        }
        // tsc's nullable guard: bare `null`/`undefined` are rejected.
        if q::is_nullish_type(self.ctx.types, t) {
            return true;
        }
        // `unknown` is not array-like (`tsc` flags `[...unknown]`).
        if t == TypeId::UNKNOWN {
            return true;
        }
        // Look through `readonly` / `NoInfer` wrappers.
        if let Some(inner) = q::unwrap_readonly_or_noinfer(self.ctx.types, t) {
            return self.rest_element_type_is_definitely_not_array_like(inner, depth + 1);
        }
        // Type parameter: classify by its constraint. An unconstrained parameter
        // has constraint `unknown`, which is not array-like (matches `tsc`).
        if let Some(info) = q::type_param_info(self.ctx.types, t) {
            return match info.constraint {
                None => true,
                Some(constraint) => {
                    self.rest_element_type_is_definitely_not_array_like(constraint, depth + 1)
                }
            };
        }
        // Conditional true-branch substitutions present their base identity for
        // display/inference, but rest grammar needs their structural constraint.
        let base_constraint = q::get_base_constraint_of_type(self.ctx.types, t);
        if base_constraint != t && base_constraint != TypeId::UNKNOWN {
            return self.rest_element_type_is_definitely_not_array_like(base_constraint, depth + 1);
        }
        // Types that remain instantiable *and* still reference free type
        // parameters are deferred generics whose array-like-ness `tsc` decides
        // from the (usually array-like) constraint; tsz frequently cannot
        // resolve them here, so treat them as indeterminate rather than risk a
        // false TS2574 (`[...Tuple<I, E>]`, `[...NTuple<A>]`). Concrete
        // applications/conditionals (no free type parameters, e.g.
        // `[...Cond<number>]`) fall through to the relation, which reduces them.
        if (q::application_info(self.ctx.types, t).is_some()
            || q::is_conditional_type(self.ctx.types, t)
            || q::is_mapped_type(self.ctx.types, t))
            && q::contains_free_type_parameters(self.ctx.types, t)
        {
            return false;
        }
        // Union is array-like only if every member is, so it is definitely not
        // array-like as soon as one member is definitely not.
        if let Some(members) = q::union_members(self.ctx.types, t) {
            return members
                .iter()
                .any(|&m| self.rest_element_type_is_definitely_not_array_like(m, depth + 1));
        }
        // Intersection is array-like if any member is, so it is definitely not
        // array-like only when every member is.
        if let Some(members) = q::intersection_members(self.ctx.types, t) {
            return members
                .iter()
                .all(|&m| self.rest_element_type_is_definitely_not_array_like(m, depth + 1));
        }
        // Fully-resolved concrete type that is neither array nor tuple (primitive,
        // literal, object, function, …). Defer to assignability so array-like
        // object shapes (numeric index + `length`) are still accepted.
        let readonly_any_array = type_construction::type_node_readonly_any_array(self.ctx.types);
        let env = self.ctx.type_environment.borrow();
        !type_utils::rest_element_array_like_relation_outcome(
            self.ctx.types,
            &*env,
            t,
            readonly_any_array,
        )
        .related
    }

    /// Resolve a rest-element type to its inspectable shape for the TS2574
    /// array-like check: alternately resolve `Lazy(DefId)` alias references
    /// through the checker's `TypeEnvironment` (the solver's own resolver is a
    /// no-op for lazy defs) and evaluate pending application/conditional types,
    /// until a fixpoint. This lets alias chains and utility-type spreads be
    /// classified by their resolved array/tuple shape.
    fn resolve_type_for_rest_element_check(
        &self,
        type_id: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        // Bounded to avoid spinning on pathological recursive aliases; a handful
        // of rounds is enough for realistic alias/utility nesting.
        const MAX_REST_ELEMENT_RESOLVE_ROUNDS: usize = 16;
        let mut current = type_id;
        for _ in 0..MAX_REST_ELEMENT_RESOLVE_ROUNDS {
            // Resolve a bare `Lazy(DefId)` alias reference, evaluate any resulting
            // application, and reduce a now-concrete conditional / indexed-access
            // result so it is classified by its reduced shape rather than left
            // opaque.
            let next = self.resolve_alias_and_application_round(current);
            // A *deferred* indexed access whose base is a deferred conditional
            // (`Cond<T>['suffix']`) or a type parameter (`T[0]`) stays opaque
            // here, but tsc classifies its array-like-ness from its apparent
            // type: the base's resolved constraint indexed by the inner key.
            // Reduce to that apparent type so a tuple-valued element (`{ suffix:
            // [] }`, `[unknown[], unknown[]]`) is accepted while a non-array
            // element (`{ s: string }`, `[string, …]`) still fires TS2574.
            // Mirrors `getConstraintOfIndexedAccessType`.
            let next = self
                .deferred_indexed_access_apparent_array_like_type(next)
                .unwrap_or(next);
            if next == current {
                break;
            }
            current = next;
        }
        current
    }

    /// One round of env-backed alias/application resolution for the TS2574
    /// rest-element check: resolve a bare `Lazy(DefId)` alias reference, then
    /// evaluate any resulting application through the env-backed
    /// `ApplicationEvaluator` (which can resolve lazy application heads such as
    /// `Cond<number[]>`, where the plain solver evaluator cannot), then reduce
    /// the result via `evaluate_type` (e.g. `number extends infer U ? U : never`
    /// from `Cond<number>`). Unlike [`Self::resolve_type_for_rest_element_check`]
    /// this performs no indexed-access apparent-type reduction, so it is safe to
    /// reuse when resolving a base/constraint without re-entering that logic.
    fn resolve_alias_and_application_round(
        &self,
        type_id: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        let next = {
            let env = self.ctx.type_environment.borrow();
            let resolved = crate::query_boundaries::flow::resolve_lazy_def_with_env(
                self.ctx.types,
                Some(&env),
                type_id,
            );
            crate::query_boundaries::flow_analysis::evaluate_application_type(
                self.ctx.types,
                &env,
                resolved,
            )
        };
        self.ctx.types.evaluate_type(next)
    }

    /// Resolve `type_id` to a fixpoint through env-backed alias/application
    /// resolution (no indexed-access reduction). Used to resolve the constraint
    /// of an indexed-access base (`T`'s constraint in `T[K]`) to its concrete
    /// tuple/array shape.
    fn resolve_alias_and_application(&self, type_id: tsz_solver::TypeId) -> tsz_solver::TypeId {
        const MAX_ROUNDS: usize = 16;
        let mut current = type_id;
        for _ in 0..MAX_ROUNDS {
            let next = self.resolve_alias_and_application_round(current);
            if next == current {
                break;
            }
            current = next;
        }
        current
    }

    /// The indexable base constraint of an indexed-access base `B` for the
    /// TS2574 rest-element check: a deferred conditional yields its branch-union
    /// constraint (tsc's `getConstraintOfIndexedAccessType`), while a (readonly-
    /// unwrapped) tuple/array yields itself. Any other shape is not concretely
    /// indexable here, so the caller treats the access as indeterminate (legal).
    fn indexable_base_constraint(&self, base: tsz_solver::TypeId) -> Option<tsz_solver::TypeId> {
        use crate::query_boundaries::common as q;
        use crate::query_boundaries::conditional_constraints as conditional_query;
        if q::is_conditional_type(self.ctx.types, base) {
            return conditional_query::conditional_branch_union_constraint(self.ctx.types, base);
        }
        let base = q::unwrap_readonly_or_noinfer(self.ctx.types, base).unwrap_or(base);
        (q::is_tuple_type(self.ctx.types, base) || q::is_array_type(self.ctx.types, base))
            .then_some(base)
    }

    /// Apparent type of a deferred indexed access `B[K1]` for the TS2574
    /// rest-element array-like check. Mirrors tsc's
    /// `getConstraintOfIndexedAccessType`: reduce the base `B` to its apparent
    /// type and index it with `K1`, then classify the resulting property type.
    ///
    /// Apparent-base shapes covered here:
    /// * a deferred conditional base — the apparent base is the conditional's
    ///   branch-union constraint (`Cond<T>['suffix']`);
    /// * an object / mapped / interface base — the apparent base is the resolved
    ///   object shape itself, so the property type is read directly
    ///   (`Parts<T>['suffix']` where `Parts<T>` reduces to `{ suffix: T }`);
    /// * a type parameter base — the apparent base is the parameter's declared
    ///   constraint resolved through any generic alias to a concrete tuple/array
    ///   (`T extends Pair<unknown[], unknown[]>` -> `[unknown[], unknown[]]`).
    ///
    /// An alias-wrapped base (`Application(Lazy(_), [T])`) is expanded to its
    /// resolved body first so both the aliased and inline forms are covered.
    /// Returns `None` when `type_id` is not such a deferred indexed access or the
    /// apparent type cannot be reduced concretely, leaving the caller to treat the
    /// type as indeterminate (legal).
    fn deferred_indexed_access_apparent_array_like_type(
        &self,
        type_id: tsz_solver::TypeId,
    ) -> Option<tsz_solver::TypeId> {
        use crate::query_boundaries::common as q;
        use crate::query_boundaries::conditional_constraints as conditional_query;
        let (base, index) = q::index_access_types(self.ctx.types, type_id)?;
        // Concrete indexed accesses already reduce through `evaluate_type` above;
        // here the base is still generic. Expand a generic alias/interface
        // application (`Application(Lazy(_), [T])`) to its resolved body so both
        // the aliased and inline forms are covered.
        let resolved_base = if q::is_generic_application(self.ctx.types, base) {
            let env = self.ctx.type_environment.borrow();
            crate::query_boundaries::flow_analysis::evaluate_application_type(
                self.ctx.types,
                &env,
                base,
            )
        } else {
            base
        };

        // The apparent base to index: a conditional reduces to its branch-union
        // constraint (`getConstraintOfIndexedAccessType`); an object/mapped/
        // interface body is its own apparent type, so it is indexed directly.
        let apparent_base = if q::is_conditional_type(self.ctx.types, resolved_base) {
            let base_constraint = conditional_query::conditional_branch_union_constraint(
                self.ctx.types,
                resolved_base,
            )?;
            // The branch-union collapsed to `error` — a branch references an
            // unresolved import (already flagged TS2307), so its apparent type is
            // `error`, which tsc treats as `any` (assignable to `readonly any[]`).
            // Propagate `error` so the caller classifies the spread as array-like
            // rather than emitting a spurious TS2574.
            if base_constraint == tsz_solver::TypeId::ERROR {
                return Some(tsz_solver::TypeId::ERROR);
            }
            if q::is_conditional_type(self.ctx.types, base_constraint)
                || q::is_index_access_type(self.ctx.types, base_constraint)
            {
                return None;
            }
            base_constraint
        } else if q::is_object_or_mapped_type(self.ctx.types, resolved_base) {
            resolved_base
        } else {
            // A type parameter (`T[K]`) indexes its declared constraint; if the
            // candidate is already an application/alias, index its resolved
            // shape. This also covers the case where `evaluate_type` substituted
            // `T` with its constraint application, e.g.
            // `Pair<unknown[], unknown[]>[0]`.
            let candidate = match q::type_param_info(self.ctx.types, base) {
                Some(info) => info.constraint?,
                None => base,
            };
            let resolved = self.resolve_alias_and_application(candidate);
            self.indexable_base_constraint(resolved)?
        };

        // Read property `K1` off the apparent base, reducing through the env so a
        // property contributed by a nested deferred application is materialized.
        let apparent = self.reduce_apparent_indexed_property(apparent_base, index);
        // An `error` property type is error contagion from an unresolved import;
        // tsc treats it as `any`, so surface it as the (legal) apparent type.
        if apparent == tsz_solver::TypeId::ERROR {
            return Some(tsz_solver::TypeId::ERROR);
        }
        if q::is_index_access_type(self.ctx.types, apparent)
            || q::is_conditional_type(self.ctx.types, apparent)
        {
            return None;
        }
        Some(apparent)
    }

    /// Reduce `apparent_base[K1]` to a concrete property type for the TS2574
    /// check, mirroring tsc's `getConstraintOfIndexedAccessType`. The apparent
    /// base may be an object, a homomorphic mapped type (`Simplify<X>`), or a
    /// union/intersection of those. A homomorphic mapped type is reduced to its
    /// template instantiated at `K1` (so `Simplify<X>[K1]` becomes `X[K1]`); the
    /// resulting object — which can still embed a deferred application that
    /// supplies the property — is expanded through the env-backed evaluator and
    /// re-indexed. Bounded; leaves opaque fallbacks to the caller after the
    /// resulting property type is classified.
    fn reduce_apparent_indexed_property(
        &self,
        apparent_base: tsz_solver::TypeId,
        index: tsz_solver::TypeId,
    ) -> tsz_solver::TypeId {
        use crate::query_boundaries::common as q;
        let mut base = self.fully_expand_apparent_base(apparent_base);
        // Unwrap homomorphic mapped types to the property type at `K1` a bounded
        // number of times (`Simplify<Simplify<…>>` nesting), expanding the base
        // each round so the eventual index reads a materialized shape.
        for _ in 0..8 {
            let Some(reduced) = q::mapped_property_type(self.ctx.types, base, index) else {
                break;
            };
            // `Simplify<X>[K1]` reduces to `X[K1]`; recover `X` to keep expanding
            // it, falling back to the reduced property type when it is no longer a
            // `_[K1]` indexed access.
            let next_base =
                q::index_access_types(self.ctx.types, reduced).map_or(reduced, |(obj, _)| obj);
            let expanded = self.fully_expand_apparent_base(next_base);
            if expanded == base {
                break;
            }
            base = expanded;
        }
        let indexed = indexed_access_key_space::indexed_access_type(self.ctx.types, base, index);
        {
            let env = self.ctx.type_environment.borrow();
            let expanded = crate::query_boundaries::flow_analysis::evaluate_application_type(
                self.ctx.types,
                &env,
                indexed,
            );
            self.ctx.types.evaluate_type(expanded)
        }
    }

    /// Recursively env-expand the apparent base of a deferred indexed access so a
    /// property contributed by a nested deferred application can be read. A
    /// branch-union member such as `Simplify<{ … } & Inner<T>>` leaves the
    /// supplying `Inner<T>` application unexpanded after the top-level
    /// `ApplicationEvaluator` pass; descend into union/intersection members and
    /// re-run the env-backed evaluator on each so the eventual `[K1]` index reads
    /// the resolved property. Bounded; returns the input unchanged when nothing
    /// further reduces.
    fn fully_expand_apparent_base(&self, type_id: tsz_solver::TypeId) -> tsz_solver::TypeId {
        use crate::query_boundaries::common as q;
        use crate::query_boundaries::conditional_constraints as conditional_query;
        let expand_once = |ty: tsz_solver::TypeId| -> tsz_solver::TypeId {
            let env = self.ctx.type_environment.borrow();
            let expanded = crate::query_boundaries::flow_analysis::evaluate_application_type(
                self.ctx.types,
                &env,
                ty,
            );
            let evaluated = self.ctx.types.evaluate_type(expanded);
            // A member that resolves to a deferred conditional (e.g. an
            // intersection part `Inner<T>` whose body is `T extends … ? … :
            // { suffix: Suffix }`) is reduced to its branch-union constraint —
            // tsc's `getBaseConstraintOfType` of a conditional — so the property
            // it contributes becomes readable. Leave non-conditionals untouched.
            if q::is_conditional_type(self.ctx.types, evaluated)
                && let Some(constraint) = conditional_query::conditional_branch_union_constraint(
                    self.ctx.types,
                    evaluated,
                )
                && !q::is_conditional_type(self.ctx.types, constraint)
                && !q::is_index_access_type(self.ctx.types, constraint)
            {
                return constraint;
            }
            evaluated
        };
        let mut current = expand_once(type_id);
        // Distribute into union/intersection members and expand each, then
        // recombine through the interner so a member application that supplies the
        // indexed property is materialized. A bounded handful of rounds covers
        // realistic nesting (`Simplify<A & Inner<T>>`).
        for _ in 0..8 {
            let members = q::union_members(self.ctx.types, current)
                .map(|m| (true, m))
                .or_else(|| q::intersection_members(self.ctx.types, current).map(|m| (false, m)));
            let Some((is_union, members)) = members else {
                break;
            };
            let expanded_members: Vec<_> = members.iter().map(|&m| expand_once(m)).collect();
            let next = if is_union {
                type_construction::type_node_union(self.ctx.types, expanded_members)
            } else {
                type_construction::type_node_intersection(self.ctx.types, expanded_members)
            };
            let next = expand_once(next);
            if next == current {
                break;
            }
            current = next;
        }
        current
    }

    pub(super) fn fixed_tuple_spread_elements(
        &self,
        type_id: tsz_solver::TypeId,
    ) -> Option<Vec<tsz_solver::TupleElement>> {
        let mut type_id = type_id;
        while let Some(inner) =
            crate::query_boundaries::common::unwrap_readonly_or_noinfer(self.ctx.types, type_id)
        {
            type_id = inner;
        }
        crate::query_boundaries::common::get_fixed_tuple_length(self.ctx.types, type_id)?;
        crate::query_boundaries::common::tuple_elements(self.ctx.types, type_id)
    }

    /// If `idx` is a direct, unshadowed reference to the lib's `Array` /
    /// `ReadonlyArray`, return the canonical `array(elem)` (or `readonly array(elem)`)
    /// representation. Returns `None` for any other type node, including aliases that
    /// happen to resolve to `Array<T>`.
    ///
    /// This canonicalization mirrors the path in `type_literal_checker.rs:308` and
    /// ensures that downstream consumers which inspect the solver array variant
    /// directly (e.g. tuple rest element extraction in `array_element_type`) see
    /// the array shape instead of `Application(Lazy(Array_DefId), [T])`. Without
    /// it, `[T, ...Array<U>]` and `[T, ...Array]` store the entire array type as
    /// the rest element, causing false TS2322 on tuple initialization and false
    /// TS2339 on destructured rest elements.
    ///
    /// When the reference has no type argument (e.g. bare `...Array`), we fall back
    /// to `any` to match tsc's recovery behaviour after TS2314.
    pub(super) fn try_canonicalize_array_type_reference(
        &mut self,
        idx: NodeIndex,
    ) -> Option<TypeId> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind != syntax_kind_ext::TYPE_REFERENCE {
            return None;
        }
        let type_ref = self.ctx.arena.get_type_ref(node)?;
        let name_node = self.ctx.arena.get(type_ref.type_name)?;
        let ident = self.ctx.arena.get_identifier(name_node)?;
        let name = ident.escaped_text.as_str();
        if name != "Array" && name != "ReadonlyArray" {
            return None;
        }

        if self.ctx.type_parameter_scope.contains_key(name) {
            return None;
        }

        // Skip canonicalization if the user has declared their own `Array` /
        // `ReadonlyArray` symbol that shadows the lib's. Declaration merging with
        // the lib counts as the lib symbol, so `interface Array<T> { custom: T }`
        // still hits the canonicalization path.
        let shadowed_by_user = self
            .ctx
            .binder
            .file_locals
            .get(name)
            .is_some_and(|sym_id| !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id));
        if shadowed_by_user {
            return None;
        }

        let elem_type = type_ref
            .type_arguments
            .as_ref()
            .and_then(|args| args.nodes.first().copied())
            .map_or(TypeId::ANY, |arg_idx| self.check(arg_idx));

        Some(if name == "ReadonlyArray" {
            type_construction::type_node_readonly_array(self.ctx.types, elem_type)
        } else {
            type_construction::type_node_array(self.ctx.types, elem_type)
        })
    }

    pub(super) fn check_tuple_rest_type_node(
        &mut self,
        idx: NodeIndex,
        canonicalize_array_ref: bool,
    ) -> TypeId {
        if canonicalize_array_ref
            && let Some(array_type) = self.try_canonicalize_array_type_reference(idx)
        {
            array_type
        } else {
            self.check(idx)
        }
    }

    fn rest_type_node_is_unconstrained_infer(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::INFER_TYPE {
            return false;
        }
        let Some(infer_data) = self.ctx.arena.get_infer_type(node) else {
            return false;
        };
        self.ctx
            .arena
            .get(infer_data.type_parameter)
            .and_then(|tp_node| self.ctx.arena.get_type_parameter(tp_node))
            .is_some_and(|tp_data| tp_data.constraint == NodeIndex::NONE)
    }

    pub(crate) fn is_this_type_allowed(
        &self,
        this_node_idx: tsz_parser::parser::NodeIndex,
    ) -> bool {
        is_this_type_allowed_in(self.ctx, this_node_idx)
    }
}

/// Walk up from `this_node_idx` looking for the node that supplies a `this`
/// type. Shared by the TS-syntax `THIS_TYPE` branch and the JSDoc
/// `@return {this}` gate, which both need the same answer from a
/// `CheckerContext` alone.
pub(crate) fn is_this_type_allowed_in(
    ctx: &crate::CheckerContext,
    this_node_idx: tsz_parser::parser::NodeIndex,
) -> bool {
    {
        use tsz_parser::parser::syntax_kind_ext;

        let mut child_idx = this_node_idx;
        let mut current = ctx.arena.get_extended(this_node_idx).map(|ext| ext.parent);

        while let Some(parent_idx) = current {
            if parent_idx.is_none() {
                break;
            }
            let Some(node) = ctx.arena.get(parent_idx) else {
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
                    let is_static = node_has_static_modifier_in(ctx, node);
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
                    if let Some(c) = ctx.arena.get_constructor(node)
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
            current = ctx.arena.get_extended(parent_idx).map(|ext| ext.parent);
        }

        false
    }
}

/// Check whether a class/interface member node has the `static` modifier
/// by inspecting its declaration data's modifier list.
pub(crate) fn node_has_static_modifier_in(
    ctx: &crate::CheckerContext,
    node: &tsz_parser::parser::node::Node,
) -> bool {
    {
        use tsz_parser::parser::syntax_kind_ext;

        let modifiers = match node.kind {
            syntax_kind_ext::METHOD_DECLARATION => {
                ctx.arena.get_method_decl(node).map(|m| m.modifiers.clone())
            }
            syntax_kind_ext::PROPERTY_DECLARATION => ctx
                .arena
                .get_property_decl(node)
                .map(|p| p.modifiers.clone()),
            syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                ctx.arena.get_accessor(node).map(|a| a.modifiers.clone())
            }
            syntax_kind_ext::INDEX_SIGNATURE => ctx
                .arena
                .get_index_signature(node)
                .map(|i| i.modifiers.clone()),
            // PROPERTY_SIGNATURE and METHOD_SIGNATURE cannot be static
            // (they are interface members), so return false.
            _ => None,
        };

        if let Some(mods) = modifiers {
            ctx.arena.is_static(&mods)
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

/// Emit TS2371 for every parameter-default nested in a destructuring binding
/// pattern (`{ mult = 1 }`, `[a = 1]`, arbitrarily nested). The caller checks
/// the top-level parameter initializer; this walks the pattern tree and, like
/// tsc, anchors each diagnostic at the offending binding element's name.
pub(crate) fn check_binding_pattern_initializers(
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

/// Returns true when a type annotation syntactically mentions `undefined`.
///
/// Exact-optional property declarations need this surface distinction:
/// `a?: string` has a different declared write surface from
/// `a?: string | undefined`, even though optional reads may include
/// `undefined`.
pub(crate) fn type_node_includes_explicit_undefined(
    arena: &tsz_parser::parser::NodeArena,
    idx: NodeIndex,
) -> bool {
    let Some(node) = arena.get(idx) else {
        return false;
    };

    if node.kind == SyntaxKind::UndefinedKeyword as u16 {
        return true;
    }

    if node.kind == syntax_kind_ext::TYPE_REFERENCE
        && let Some(type_ref) = arena.get_type_ref(node)
    {
        let has_type_args = type_ref
            .type_arguments
            .as_ref()
            .is_some_and(|args| !args.nodes.is_empty());
        if !has_type_args
            && let Some(name_node) = arena.get(type_ref.type_name)
            && let Some(ident) = arena.get_identifier(name_node)
        {
            return ident.escaped_text.as_str() == "undefined";
        }
    }

    if matches!(
        node.kind,
        k if k == syntax_kind_ext::UNION_TYPE || k == syntax_kind_ext::INTERSECTION_TYPE
    ) && let Some(composite) = arena.get_composite_type(node)
    {
        return composite
            .types
            .nodes
            .iter()
            .copied()
            .any(|member| type_node_includes_explicit_undefined(arena, member));
    }

    if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
        && let Some(wrapped) = arena.get_wrapped_type(node)
    {
        return type_node_includes_explicit_undefined(arena, wrapped.type_node);
    }

    false
}
