//! Enum and const-enum access inference for declaration emit.
//!
//! Extracted from `type_inference.rs` for file-size reasons; behavior is unchanged.

#[allow(unused_imports)]
use super::super::{DeclarationEmitter, ImportPlan, PlannedImportModule, PlannedImportSymbol};
#[allow(unused_imports)]
use crate::emitter::type_printer::TypePrinter;
#[allow(unused_imports)]
use crate::output::source_writer::{SourcePosition, SourceWriter, source_position_from_offset};
#[allow(unused_imports)]
use rustc_hash::{FxHashMap, FxHashSet};
#[allow(unused_imports)]
use std::sync::Arc;
#[allow(unused_imports)]
use tracing::debug;
#[allow(unused_imports)]
use tsz_binder::{BinderState, SymbolId, symbol_flags};
#[allow(unused_imports)]
use tsz_common::comments::{get_jsdoc_content, is_jsdoc_comment};
#[allow(unused_imports)]
use tsz_parser::parser::ParserState;
#[allow(unused_imports)]
use tsz_parser::parser::node::{Node, NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn semantic_simple_enum_access(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if !self.is_simple_enum_access(expr_node) {
            return None;
        }

        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;

        if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
            && symbol.flags & tsz_binder::symbol_flags::ENUM != 0
            && symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0
        {
            return Some(expr_idx);
        }

        let source_file_idx = self.current_source_file_idx?;
        let source_file_node = self.arena.get(source_file_idx)?;
        let source_file = self.arena.get_source_file(source_file_node)?;
        for &stmt_idx in &source_file.statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                continue;
            }
            if let Some(enum_data) = self.arena.get_enum(stmt_node)
                && self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
            {
                return Some(expr_idx);
            }
        }
        None
    }

    pub(crate) fn simple_enum_access_member_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        self.enum_member_access_canonical_text(expr_idx)
    }

    /// Canonical `Base.Member` declaration text for an enum access that resolves
    /// to a specific enum-member *literal*, or `None` when the access is a
    /// reverse mapping (numeric / computed / non-member index → `string`) or is
    /// otherwise not a member literal.
    ///
    /// Mirrors `tsc`'s declaration emit (`createLiteralConstValue`): only an
    /// enum-member literal type is preserved as a member reference, and it is
    /// always rendered in property-access spelling — so `E["B"]` normalizes to
    /// `E.B` and a reverse mapping like `E[0]` / `E[E.B]` (type `string`) yields
    /// `None` so the caller falls back to a `: string` type annotation. The base
    /// and member are reconstructed from structured entity-name nodes, never a
    /// raw source slice, so parentheses/non-null punctuation cannot leak into the
    /// output (e.g. `(E.B)` renders as `E.B`, not `E.B)`).
    pub(in crate::declaration_emitter) fn enum_member_access_canonical_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            // `E.B` / `ns.E.B` — a dotted entity name already names the member.
            return self.entity_name_text(expr_idx);
        }

        if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let base = self.entity_name_text(access.expression)?;
            // Only a string-literal member name names an enum-member literal type.
            // Numeric / computed indices are reverse mappings whose type is
            // `string`, so they must not be preserved as a member reference.
            let member_name = self.string_literal_member_name(access.name_or_argument)?;
            // tsc normalizes `E["valid"]` to property spelling `E.valid`, but a
            // member name that is not a valid identifier (e.g. `"hyphen-member"`)
            // must stay in bracket form `E["hyphen-member"]`.
            if crate::transforms::emit_utils::is_valid_identifier_name(&member_name) {
                return Some(format!("{base}.{member_name}"));
            }
            let escaped = super::escape_string_for_double_quote(&member_name);
            return Some(format!("{base}[\"{escaped}\"]"));
        }

        None
    }

    /// The (unquoted) member name when `expr_idx` is a string-literal element
    /// access argument, e.g. the `B` in `E["B"]`. Returns `None` for any other
    /// argument (numeric / computed / asserted), which is a reverse mapping.
    fn string_literal_member_name(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_node = self.arena.get(expr_idx)?;
        if expr_node.kind != SyntaxKind::StringLiteral as u16 {
            return None;
        }
        self.arena
            .get_literal(expr_node)
            .map(|lit| lit.text.clone())
    }

    pub(crate) fn enum_member_access_initializer_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let is_access = expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;
        if !is_access {
            return None;
        }

        let binder = self.binder?;
        let sym_id = self.access_reference_symbol(expr_idx)?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0 {
            return None;
        }

        // Render in canonical `Base.Member` spelling rather than a raw source
        // slice: this normalizes `E["B"]` to `E.B` and prevents parenthesis /
        // non-null punctuation from leaking into the declaration output.
        self.enum_member_access_canonical_text(expr_idx)
    }

    /// Resolve a value reference or namespace-qualified property-access chain
    /// (`N.deep`, `N.M.deep`) to its value symbol, trying the binder-tracked
    /// node symbol first and falling back to walking the namespace export chain.
    /// Shared by the enum-member-access and namespace-const-member paths.
    pub(in crate::declaration_emitter) fn access_reference_symbol(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<SymbolId> {
        self.value_reference_symbol(expr_idx)
            .or_else(|| self.entity_access_chain_symbol(expr_idx))
    }

    fn entity_access_chain_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let binder = self.binder?;
        let (root_idx, parts) = self.entity_access_chain_parts(expr_idx)?;
        let root_name = self.get_identifier_text(root_idx)?;
        let root_sym_id = self.resolve_identifier_symbol(root_idx, &root_name)?;
        let root_symbol = binder.symbols.get(root_sym_id)?;

        if !parts.is_empty()
            && root_symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && let Some(module_specifier) = root_symbol.import_module()
            && let Some(current_path) = self.current_file_path.as_deref()
        {
            for module_path in
                self.matching_module_export_paths(binder, current_path, module_specifier)
            {
                let Some(exports) = binder.module_exports.get(module_path) else {
                    continue;
                };
                let export_name = root_symbol.import_name();
                let (mut current, start_index) = match export_name {
                    Some("*") | Some("export=") | None => {
                        let Some(current) = exports.get(&parts[0]) else {
                            continue;
                        };
                        (current, 1)
                    }
                    Some(name) => {
                        let Some(current) = exports.get(name) else {
                            continue;
                        };
                        (current, 0)
                    }
                };
                let mut resolved_all_parts = true;
                for part in parts.iter().skip(start_index) {
                    let Some(next) = self.symbol_member(current, part, binder) else {
                        resolved_all_parts = false;
                        break;
                    };
                    current = next;
                }
                if !resolved_all_parts {
                    continue;
                }
                return Some(current);
            }
        }

        let mut current = self.resolve_portability_symbol(root_sym_id, binder);
        for part in parts {
            current = self.symbol_member(current, &part, binder)?;
        }
        Some(current)
    }

    fn entity_access_chain_parts(&self, expr_idx: NodeIndex) -> Option<(NodeIndex, Vec<String>)> {
        let mut current = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let mut reversed_parts = Vec::new();

        for _ in 0..32 {
            let node = self.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16 {
                reversed_parts.reverse();
                return Some((current, reversed_parts));
            }

            if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
                let access = self.arena.get_access_expr(node)?;
                reversed_parts.push(self.get_identifier_text(access.name_or_argument)?);
                current = access.expression;
                continue;
            }

            return None;
        }

        None
    }

    fn symbol_member(
        &self,
        sym_id: SymbolId,
        member_name: &str,
        binder: &BinderState,
    ) -> Option<SymbolId> {
        let resolved = self.resolve_portability_symbol(sym_id, binder);
        let symbol = binder.symbols.get(resolved)?;
        symbol
            .exports
            .as_ref()
            .and_then(|exports| exports.get(member_name))
            .or_else(|| {
                symbol
                    .members
                    .as_ref()
                    .and_then(|members| members.get(member_name))
            })
    }

    pub(crate) fn simple_const_enum_access_member_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        if !self.is_simple_enum_access(expr_node) {
            return None;
        }
        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;
        let is_const_enum = self
            .current_source_file_idx
            .and_then(|source_file_idx| self.arena.get(source_file_idx))
            .and_then(|source_file_node| self.arena.get_source_file(source_file_node))
            .is_some_and(|source_file| {
                source_file
                    .statements
                    .nodes
                    .iter()
                    .any(|&stmt_idx| self.enum_declaration_is_const_named(stmt_idx, &base_name))
            })
            || self.arena.nodes.iter().enumerate().any(|(idx, node)| {
                node.kind == syntax_kind_ext::ENUM_DECLARATION
                    && self.enum_declaration_is_const_named(NodeIndex(idx as u32), &base_name)
            });

        if !is_const_enum {
            return None;
        }

        self.enum_member_access_canonical_text(expr_idx)
    }

    fn enum_declaration_is_const_named(&self, stmt_idx: NodeIndex, base_name: &str) -> bool {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return false;
        };
        if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
            return false;
        }
        let Some(enum_data) = self.arena.get_enum(stmt_node) else {
            return false;
        };
        self.get_identifier_text(enum_data.name).as_deref() == Some(base_name)
            && self
                .arena
                .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
    }

    pub(crate) fn simple_enum_access_base_name_text(&self, expr_idx: NodeIndex) -> Option<String> {
        let expr_idx = self.semantic_simple_enum_access(expr_idx)?;
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_node = self.arena.get(access.expression)?;
        self.get_source_slice(base_node.pos, base_node.end)
    }

    /// Widen a single returned enum-member access (`return E.A`, `() => E.A`) to
    /// its parent enum name (`E`) for an inferred declaration return type.
    ///
    /// tsc's `getReturnTypeFromBody` runs the aggregated return type through
    /// `getWidenedType`, which widens a fresh enum-member literal to its parent
    /// enum exactly as a fresh primitive literal widens to its base. The checker
    /// already does this at the type level; this is the declaration-emit analog
    /// for the AST-text return paths (class methods, arrow expression bodies,
    /// returned local callables) that render the returned expression directly
    /// rather than printing the solver return type.
    ///
    /// The gate is the returned expression's solver type being a *literal* enum
    /// member (`is_literal_enum_member`), so a reverse mapping (`E[0]`, type
    /// `string`) is excluded and only a genuine member literal widens. Returns
    /// `None` when the expression is not a widenable enum member, so const
    /// initializers and explicit annotations (handled by their own paths) are
    /// never reached here.
    pub(in crate::declaration_emitter) fn returned_enum_member_widened_base_text(
        &self,
        return_expr: NodeIndex,
    ) -> Option<String> {
        // Cheap syntactic gate first: only an `Enum.Member` / `Enum["Member"]`
        // access can widen. `simple_enum_access_base_name_text` rejects every
        // other return shape (literals, calls, objects) before the solver type
        // lookup below runs, so the common non-enum return pays no type query.
        let base_name = self.simple_enum_access_base_name_text(return_expr)?;
        let interner = self.type_interner?;
        let type_id = self.get_node_type_or_names(&[return_expr])?;
        // Confirm a *literal* member, so a reverse mapping (`E[0]`, type `string`)
        // is excluded and only a genuine member literal widens to its parent enum.
        tsz_solver::type_queries::is_literal_enum_member(interner, type_id).then_some(base_name)
    }

    /// Inferred return-type text for a single returned expression, widening a
    /// returned enum-member literal to its parent enum before the general
    /// declaration-summary / fallback inference. Shared by the block-body,
    /// arrow-body, and returned-local-callable return paths so the widen-then-
    /// fallback order stays in one place.
    pub(in crate::declaration_emitter) fn return_expression_type_text_with_enum_widening(
        &self,
        return_expr: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        self.returned_enum_member_widened_base_text(return_expr)
            .or_else(|| self.declaration_summary_primitive_expression_type_text(return_expr, depth))
            .or_else(|| self.infer_fallback_type_text_at(return_expr, depth))
    }

    pub(crate) fn const_asserted_enum_access_member_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let assertion = self.arena.get_type_assertion(expr_node)?;
        let type_node = self.arena.get(assertion.type_node)?;
        // Source-syntax check, not a rendered-type predicate (#14142): `type_text`
        // is the verbatim source slice of the assertion's type node, so this
        // recognizes the `as const` assertion spelling itself, not a computed type.
        let type_text = self.get_source_slice(type_node.pos, type_node.end)?;
        if type_text != "const" {
            return None;
        }

        self.simple_enum_access_member_text(assertion.expression)
    }

    pub(in crate::declaration_emitter) fn invalid_const_enum_object_access(
        &self,
        expr_idx: NodeIndex,
    ) -> bool {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let Some(expr_node) = self.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.arena.get_access_expr(expr_node) else {
            return false;
        };
        let Some(base_name) = self.get_identifier_text(access.expression) else {
            return false;
        };

        let is_const_enum = if let Some(binder) = self.binder
            && let Some(symbol_id) = binder.get_node_symbol(access.expression)
            && let Some(symbol) = binder.symbols.get(symbol_id)
        {
            symbol.flags & tsz_binder::symbol_flags::CONST_ENUM != 0
        } else if let Some(source_file_idx) = self.current_source_file_idx
            && let Some(source_file_node) = self.arena.get(source_file_idx)
            && let Some(source_file) = self.arena.get_source_file(source_file_node)
        {
            source_file
                .statements
                .nodes
                .iter()
                .copied()
                .any(|stmt_idx| {
                    let Some(stmt_node) = self.arena.get(stmt_idx) else {
                        return false;
                    };
                    if stmt_node.kind != syntax_kind_ext::ENUM_DECLARATION {
                        return false;
                    }
                    let Some(enum_data) = self.arena.get_enum(stmt_node) else {
                        return false;
                    };
                    self.get_identifier_text(enum_data.name).as_deref() == Some(base_name.as_str())
                        && self
                            .arena
                            .has_modifier(&enum_data.modifiers, SyntaxKind::ConstKeyword)
                })
        } else {
            false
        };
        if !is_const_enum {
            return false;
        }

        let argument_idx = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(access.name_or_argument);
        self.arena
            .get(argument_idx)
            .is_some_and(|arg| arg.kind != SyntaxKind::StringLiteral as u16)
    }
}
