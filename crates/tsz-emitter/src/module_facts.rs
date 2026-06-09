//! Shared, AST-only module/import/export detection facts.
//!
//! Module-detection facts decide whether a source file is an external module,
//! whether it carries `import.meta`, and whether a dynamic `import(...)` call
//! must drive wrapper/runtime lowering. The same questions are asked by the
//! lowering pass (when scheduling the module wrapper) and by the source-file
//! emitter (when deciding `"use strict"`, the `__esModule` marker, and bundle
//! dependency discovery).
//!
//! These three helpers used to be duplicated verbatim in both layers, which let
//! the lowering and emit phases drift apart. Centralizing them here keeps the
//! facts in a single owner. They are pure functions of `(NodeArena, options,
//! statements)` with no emit side effects, so either phase can call them.

use crate::emitter::{JsxEmit, PrinterOptions};
use tsz_parser::parser::node::{NodeAccess, NodeArena};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

/// True when any statement subtree contains a dynamic `import(...)` call.
///
/// AMD/UMD/System lower dynamic import through the module wrapper runtime, so a
/// file that only contains `import(expr)` still needs that wrapper (and is a
/// module under those targets). The walk is intentionally whole-subtree because
/// dynamic imports can appear anywhere an expression is legal.
#[must_use]
pub(crate) fn source_has_dynamic_import_call(arena: &NodeArena, statements: &NodeList) -> bool {
    let mut stack: Vec<NodeIndex> = statements.nodes.clone();
    while let Some(idx) = stack.pop() {
        if idx.is_none() {
            continue;
        }
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::CALL_EXPRESSION
            && let Some(call) = arena.get_call_expr(node)
            && let Some(expr_node) = arena.get(call.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
        {
            return true;
        }
        for child in arena.get_children(idx) {
            stack.push(child);
        }
    }
    false
}

/// True when any statement subtree contains an `import.meta` expression.
///
/// `import.meta` is ESM-only syntax, so its presence promotes a file to an
/// external module. The AST shape is a property access whose `expression` is the
/// `import` keyword and whose member name is `meta`.
#[must_use]
pub(crate) fn contains_import_meta(arena: &NodeArena, statements: &NodeList) -> bool {
    let mut stack: Vec<NodeIndex> = statements.nodes.clone();
    while let Some(idx) = stack.pop() {
        if idx.is_none() {
            continue;
        }
        let Some(node) = arena.get(idx) else {
            continue;
        };
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = arena.get_access_expr(node)
            && let Some(expr_node) = arena.get(access.expression)
            && expr_node.kind == SyntaxKind::ImportKeyword as u16
            && arena
                .get(access.name_or_argument)
                .and_then(|name_node| arena.get_identifier(name_node))
                .is_some_and(|ident| ident.escaped_text.as_str() == "meta")
        {
            return true;
        }
        for child in arena.get_children(idx) {
            stack.push(child);
        }
    }
    false
}

/// True when the JSX automatic runtime (`react-jsx` / `react-jsxdev`) promotes a
/// file to a module because it contains JSX.
///
/// The automatic runtime injects a `jsx-runtime` import, so any JSX element in
/// the file makes it an external module — unless `moduleDetection: legacy`
/// opts out of that promotion.
#[must_use]
pub(crate) fn jsx_automatic_runtime_makes_module(
    arena: &NodeArena,
    options: &PrinterOptions,
) -> bool {
    if options.module_detection_legacy {
        return false;
    }
    if !matches!(options.jsx, JsxEmit::ReactJsx | JsxEmit::ReactJsxDev) {
        return false;
    }
    (0..arena.len()).any(|idx| {
        arena.get(NodeIndex(idx as u32)).is_some_and(|node| {
            node.kind == syntax_kind_ext::JSX_ELEMENT
                || node.kind == syntax_kind_ext::JSX_SELF_CLOSING_ELEMENT
                || node.kind == syntax_kind_ext::JSX_FRAGMENT
        })
    })
}

#[cfg(test)]
#[path = "../tests/module_facts.rs"]
mod tests;
