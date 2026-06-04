#[path = "namespace_es5_ir_const_enum.rs"]
mod namespace_es5_ir_const_enum;

#[path = "namespace_es5_ir_helpers.rs"]
mod namespace_es5_ir_helpers;

#[path = "namespace_es5_ir_import_alias.rs"]
mod namespace_es5_ir_import_alias;

#[path = "namespace_es5_ir_source.rs"]
mod namespace_es5_ir_source;

use namespace_es5_ir_helpers::*;

use std::cell::{Cell, RefCell};

use crate::emitter::ScopedConstEnum;

use crate::enums::evaluator::EnumValue;

use crate::transforms::async_es5_ir::AsyncES5Transformer;

use crate::transforms::class_es5_ir::{AstToIr, ES5ClassTransformer};

use crate::transforms::enum_es5_ir::transform_enum_to_ir;

use crate::transforms::ir::{EnumMemberValue, IRCatchClause, IRNode, IRParam, IRPropertyKey};

use crate::transforms::ir_printer::IRPrinter;

use rustc_hash::FxHashMap;

use tsz_common::common::ModuleKind;

use tsz_parser::parser::node::{Node, NodeArena};

use tsz_parser::parser::node_flags;

use tsz_parser::parser::syntax_kind_ext;

use tsz_parser::parser::{NodeIndex, NodeList};

use tsz_scanner::SyntaxKind;

fn starts_with_keyword_token(text: &str, keyword: &str) -> bool {
    text.strip_prefix(keyword).is_some_and(|tail| {
        tail.chars()
            .next()
            .is_none_or(|ch| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
    })
}

const fn is_identifier_continue(byte: u8) -> bool {
    byte == b'_' || byte == b'$' || byte.is_ascii_alphanumeric()
}

fn previous_identifier_token(text: &str, mut end: usize) -> Option<(&str, usize)> {
    let bytes = text.as_bytes();
    while end > 0 && matches!(bytes[end - 1], b' ' | b'\t' | b'\r' | b'\n') {
        end -= 1;
    }
    let token_end = end;
    while end > 0 && is_identifier_continue(bytes[end - 1]) {
        end -= 1;
    }
    (end < token_end).then(|| (&text[end..token_end], end))
}

/// ES5 Namespace Transformer
///
/// Transforms TypeScript namespace declarations into ES5-compatible IIFE patterns.
/// This is the primary entry point for namespace IR transformations.
///
/// # Example
///
/// ```ignore
/// use crate::transforms::namespace_es5_ir::NamespaceES5Transformer;
/// use crate::transforms::ir_printer::IRPrinter;
///
/// let transformer = NamespaceES5Transformer::new(&arena);
/// if let Some(ir) = transformer.transform_namespace(ns_idx) {
///     let output = IRPrinter::emit_to_string(&ir);
/// }
/// ```
pub struct NamespaceES5Transformer<'a> {
    arena: &'a NodeArena,
    is_commonjs: bool,
    module_kind: ModuleKind,
    target_es5: bool,
    source_text: Option<&'a str>,
    comment_ranges: Vec<tsz_common::comments::CommentRange>,
    /// Exported variable names from prior blocks of the same namespace.
    /// Used for cross-block export substitution (e.g., `x` → `M.x` in block 2
    /// when `export var x` was declared in block 1).
    prior_exported_vars: std::collections::HashSet<String>,
    /// Whether legacy decorators are enabled (experimentalDecorators)
    legacy_decorators: bool,
    /// Whether `__metadata` calls should be emitted in `__decorate` arrays.
    /// Mirrors `--emitDecoratorMetadata`. Forwarded to nested
    /// `ES5ClassTransformer` so metadata is emitted for classes that live
    /// inside a namespace IIFE.
    emit_decorator_metadata: bool,
    /// Whether `--useDefineForClassFields` is enabled. Mirrors the top-level
    /// class lowering option and is forwarded to the nested
    /// `ES5ClassTransformer` so classes that live inside a namespace/module
    /// IIFE get the same field/method lowering as top-level classes (fields
    /// become `Object.defineProperty(this, ...)`; static methods become
    /// `Object.defineProperty(C, ...)`).
    use_define_for_class_fields: bool,
    /// Hoisted temp variable names collected from expression conversions
    /// (e.g., from computed property lowering inside object literals)
    hoisted_temps: RefCell<Vec<String>>,
    disposable_env_counter: Cell<u32>,
    generated_disposable_env_names: RefCell<Vec<String>>,
    active_namespace_using_env: RefCell<Option<(String, bool)>>,
    default_exported_func_names: std::collections::HashSet<String>,
    commonjs_export_names: Vec<String>,
    const_enum_values: FxHashMap<String, Vec<ScopedConstEnum>>,
    const_enum_import_aliases: FxHashMap<String, String>,
    remove_comments: bool,
    /// Monotonic per-namespace-name suffix counter for IIFE parameter renames.
    /// `tsc` renames the IIFE parameter with an incrementing suffix across
    /// reopenings of the same namespace (`schema` → `schema_1`, `schema_2`,
    /// ...). Each namespace block is transformed by a fresh transformer, so the
    /// dispatcher seeds this from its shared counter before transforming a block
    /// and reads it back afterwards to persist the increment.
    iife_param_rename_counter: RefCell<FxHashMap<String, u32>>,
}

include!("namespace_es5_ir_parts/part1.rs");
include!("namespace_es5_ir_parts/part2.rs");

fn mark_invalid_namespace_static(node: &mut IRNode) {
    match node {
        IRNode::EnumIIFE {
            invalid_namespace_static,
            ..
        }
        | IRNode::NamespaceIIFE {
            invalid_namespace_static,
            ..
        } => *invalid_namespace_static = true,
        IRNode::Sequence(items) => {
            if let Some(first) = items.first_mut() {
                mark_invalid_namespace_static(first);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "../../tests/namespace_es5_ir.rs"]
mod tests;
