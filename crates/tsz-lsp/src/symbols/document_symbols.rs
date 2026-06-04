use std::cell::Cell;

use crate::utils::node_range;

use tsz_common::position::{Position, Range};

use tsz_parser::parser::node::Node;

use tsz_parser::{NodeIndex, node_flags, syntax_kind_ext};

use tsz_scanner::SyntaxKind;

mod expando;

mod imports;

mod model;

mod support;

pub use model::{DocumentSymbol, SymbolKind};

use model::{DocumentSymbolEntry, document_symbols_from_entries};

use support::*;

const MAX_DOCUMENT_SYMBOL_ENTRIES: usize = 3000;

const MAX_DOCUMENT_SYMBOL_DEPTH: usize = 64;

const MORE_DOCUMENT_SYMBOL_NAME: &str = "more...";

thread_local! {
    static DOCUMENT_SYMBOL_REMAINING: Cell<usize> = const { Cell::new(usize::MAX) };
    static DOCUMENT_SYMBOL_DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn with_document_symbol_collection_limit<F>(f: F) -> Vec<DocumentSymbolEntry>
where
    F: FnOnce() -> Vec<DocumentSymbolEntry>,
{
    DOCUMENT_SYMBOL_REMAINING.with(|remaining| {
        DOCUMENT_SYMBOL_DEPTH.with(|depth| {
            let previous_remaining = remaining.replace(MAX_DOCUMENT_SYMBOL_ENTRIES);
            let previous_depth = depth.replace(0);
            let symbols = f();
            remaining.set(previous_remaining);
            depth.set(previous_depth);
            symbols
        })
    })
}

struct DocumentSymbolDepthGuard {
    active: bool,
}

impl Drop for DocumentSymbolDepthGuard {
    fn drop(&mut self) {
        if self.active {
            DOCUMENT_SYMBOL_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }
}

fn document_symbol_depth_guard(kind: u16) -> DocumentSymbolDepthGuard {
    let active = document_symbol_node_may_emit_direct(kind);
    if active {
        DOCUMENT_SYMBOL_DEPTH.with(|depth| depth.set(depth.get() + 1));
    }
    DocumentSymbolDepthGuard { active }
}

fn document_symbol_budget_precheck(kind: u16, range: Range) -> Option<Vec<DocumentSymbolEntry>> {
    let may_emit = document_symbol_node_may_emit_direct(kind);
    let exhausted = DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.get() == 0);
    if exhausted {
        return Some(Vec::new());
    }

    if !may_emit {
        return None;
    }

    let at_depth_limit =
        DOCUMENT_SYMBOL_DEPTH.with(|depth| depth.get() >= MAX_DOCUMENT_SYMBOL_DEPTH);
    let must_emit_more = DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.get() == 1);
    if at_depth_limit || must_emit_more {
        DOCUMENT_SYMBOL_REMAINING
            .with(|remaining| remaining.set(remaining.get().saturating_sub(1)));
        return Some(vec![more_document_symbol(range)]);
    }

    DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.set(remaining.get().saturating_sub(1)));
    None
}

fn document_symbol_budget_account(symbols: &mut Vec<DocumentSymbolEntry>) {
    if symbols.is_empty() {
        DOCUMENT_SYMBOL_REMAINING.with(|remaining| remaining.set(remaining.get() + 1));
        return;
    }

    DOCUMENT_SYMBOL_REMAINING.with(|remaining| {
        let available = remaining.get();
        let extra_symbols = symbols.len().saturating_sub(1);
        if extra_symbols > available {
            let keep = available + 1;
            let sentinel_range = symbols[keep - 1].range;
            symbols.truncate(keep);
            symbols[keep - 1] = more_document_symbol(sentinel_range);
            remaining.set(0);
        } else {
            remaining.set(available - extra_symbols);
        }
    });
}

const fn document_symbol_node_may_emit_direct(kind: u16) -> bool {
    matches!(
        kind,
        k if k == syntax_kind_ext::FUNCTION_DECLARATION
            || k == syntax_kind_ext::FUNCTION_EXPRESSION
            || k == syntax_kind_ext::CLASS_DECLARATION
            || k == syntax_kind_ext::CLASS_EXPRESSION
            || k == syntax_kind_ext::INTERFACE_DECLARATION
            || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
            || k == syntax_kind_ext::VARIABLE_STATEMENT
            || k == syntax_kind_ext::ENUM_DECLARATION
            || k == syntax_kind_ext::ENUM_MEMBER
            || k == syntax_kind_ext::METHOD_DECLARATION
            || k == syntax_kind_ext::PROPERTY_DECLARATION
            || k == syntax_kind_ext::PROPERTY_SIGNATURE
            || k == syntax_kind_ext::CALL_SIGNATURE
            || k == syntax_kind_ext::CONSTRUCT_SIGNATURE
            || k == syntax_kind_ext::INDEX_SIGNATURE
            || k == syntax_kind_ext::METHOD_SIGNATURE
            || k == syntax_kind_ext::CONSTRUCTOR
            || k == syntax_kind_ext::GET_ACCESSOR
            || k == syntax_kind_ext::SET_ACCESSOR
            || k == syntax_kind_ext::MODULE_DECLARATION
            || k == syntax_kind_ext::IMPORT_DECLARATION
            || k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION
            || k == syntax_kind_ext::EXPORT_ASSIGNMENT
            || k == syntax_kind_ext::EXPORT_DECLARATION
    )
}

define_lsp_provider!(minimal DocumentSymbolProvider, "Document symbol provider.");

include!("document_symbols_parts/part1.rs");
include!("document_symbols_parts/part2.rs");

#[cfg(test)]
#[path = "../../tests/document_symbols_tests.rs"]
mod document_symbols_tests;
