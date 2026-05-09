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
        let expr_node = self.arena.get(expr_idx)?;
        let access = self.arena.get_access_expr(expr_node)?;
        let base_name = self.get_identifier_text(access.expression)?;
        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let member_name = self.get_identifier_text(access.name_or_argument)?;
            return Some(format!("{base_name}.{member_name}"));
        }

        if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let member_node = self.arena.get(access.name_or_argument)?;
            let member_text = self.get_source_slice(member_node.pos, member_node.end)?;
            return Some(format!("{base_name}[{member_text}]"));
        }

        None
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
        let sym_id = self
            .value_reference_symbol(expr_idx)
            .or_else(|| self.entity_access_chain_symbol(expr_idx))?;
        let symbol = binder.symbols.get(sym_id)?;
        if symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER == 0 {
            return None;
        }

        self.get_source_slice_no_semi(expr_node.pos, expr_node.end)
    }

    fn entity_access_chain_symbol(&self, expr_idx: NodeIndex) -> Option<SymbolId> {
        let binder = self.binder?;
        let (root_idx, parts) = self.entity_access_chain_parts(expr_idx)?;
        let root_name = self.get_identifier_text(root_idx)?;
        let root_sym_id = self.resolve_identifier_symbol(root_idx, &root_name)?;
        let root_symbol = binder.symbols.get(root_sym_id)?;

        if !parts.is_empty()
            && root_symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)
            && let Some(module_specifier) = root_symbol.import_module.as_deref()
            && let Some(current_path) = self.current_file_path.as_deref()
        {
            for module_path in
                self.matching_module_export_paths(binder, current_path, module_specifier)
            {
                let Some(exports) = binder.module_exports.get(module_path) else {
                    continue;
                };
                let export_name = root_symbol.import_name.as_deref();
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

        if expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let member_name = self.get_identifier_text(access.name_or_argument)?;
            return Some(format!("{base_name}.{member_name}"));
        }

        if expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION {
            let member_node = self.arena.get(access.name_or_argument)?;
            let member_text = self.get_source_slice(member_node.pos, member_node.end)?;
            return Some(format!("{base_name}[{member_text}]"));
        }

        None
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

    pub(crate) fn const_asserted_enum_access_member_text(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let expr_idx = self.skip_parenthesized_non_null_and_comma(expr_idx);
        let expr_node = self.arena.get(expr_idx)?;
        let assertion = self.arena.get_type_assertion(expr_node)?;
        let type_node = self.arena.get(assertion.type_node)?;
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
