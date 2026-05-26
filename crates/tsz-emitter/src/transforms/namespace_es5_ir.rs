//! ES5 Namespace Transform (IR-based)
//!
//! Transforms TypeScript namespaces to ES5 IIFE patterns, producing IR nodes.
//!
//! # Architecture
//!
//! This module provides `NamespaceES5Transformer`, the main transformer struct
//! that produces IR nodes for namespace IIFE emission.
//!
//! # Examples
//!
//! Simple namespace:
//! ```typescript
//! namespace foo {
//!     export class Provide { }
//! }
//! ```
//!
//! Becomes IR that prints as:
//! ```javascript
//! var foo;
//! (function (foo) {
//!     var Provide = /** @class */ (function () {
//!         function Provide() { }
//!         return Provide;
//!     }());
//!     foo.Provide = Provide;
//! })(foo || (foo = {}));
//! ```
//!
//! Qualified namespace name (A.B.C) produces nested IIFEs:
//! ```typescript
//! namespace A.B.C {
//!     export const x = 1;
//! }
//! ```
//!
//! Becomes:
//! ```javascript
//! var A;
//! (function (A) {
//!     var B;
//!     (function (B) {
//!         var C;
//!         (function (C) {
//!             var x = 1;
//!             C.x = x;
//!         })(C = B.C || (B.C = {}));
//!     })(B = A.B || (A.B = {}));
//! })(A || (A = {}));
//! ```

#[path = "namespace_es5_ir_const_enum.rs"]
mod namespace_es5_ir_const_enum;
#[path = "namespace_es5_ir_helpers.rs"]
mod namespace_es5_ir_helpers;
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

// =============================================================================
// NamespaceES5Transformer - Main transformer struct
// =============================================================================

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
}

impl<'a> NamespaceES5Transformer<'a> {
    /// Create a new namespace transformer
    pub fn new(arena: &'a NodeArena) -> Self {
        Self {
            arena,
            is_commonjs: false,
            module_kind: ModuleKind::None,
            source_text: None,
            comment_ranges: Vec::new(),
            prior_exported_vars: std::collections::HashSet::new(),
            legacy_decorators: false,
            emit_decorator_metadata: false,
            hoisted_temps: RefCell::new(Vec::new()),
            disposable_env_counter: Cell::new(1),
            generated_disposable_env_names: RefCell::new(Vec::new()),
            active_namespace_using_env: RefCell::new(None),
            default_exported_func_names: std::collections::HashSet::new(),
            commonjs_export_names: Vec::new(),
            const_enum_values: FxHashMap::default(),
            const_enum_import_aliases: FxHashMap::default(),
            remove_comments: false,
        }
    }

    /// Create a namespace transformer with `CommonJS` mode enabled
    pub fn with_commonjs(arena: &'a NodeArena, is_commonjs: bool) -> Self {
        Self {
            arena,
            is_commonjs,
            module_kind: ModuleKind::None,
            source_text: None,
            comment_ranges: Vec::new(),
            prior_exported_vars: std::collections::HashSet::new(),
            legacy_decorators: false,
            emit_decorator_metadata: false,
            hoisted_temps: RefCell::new(Vec::new()),
            disposable_env_counter: Cell::new(1),
            generated_disposable_env_names: RefCell::new(Vec::new()),
            active_namespace_using_env: RefCell::new(None),
            default_exported_func_names: std::collections::HashSet::new(),
            commonjs_export_names: Vec::new(),
            const_enum_values: FxHashMap::default(),
            const_enum_import_aliases: FxHashMap::default(),
            remove_comments: false,
        }
    }

    /// Set whether legacy decorators are enabled
    pub const fn set_legacy_decorators(&mut self, enabled: bool) {
        self.legacy_decorators = enabled;
    }

    /// Set whether `__metadata` calls should be emitted in `__decorate`
    /// arrays for classes inside this namespace.
    pub const fn set_emit_decorator_metadata(&mut self, enabled: bool) {
        self.emit_decorator_metadata = enabled;
    }

    pub fn set_disposable_env_context(&self, next_env_id: u32) {
        self.disposable_env_counter.set(next_env_id);
    }

    pub const fn disposable_env_counter(&self) -> u32 {
        self.disposable_env_counter.get()
    }

    pub fn take_generated_disposable_env_names(&self) -> Vec<String> {
        self.generated_disposable_env_names
            .borrow_mut()
            .drain(..)
            .collect()
    }

    /// Set source text for comment extraction
    pub fn set_source_text(&mut self, text: &'a str) {
        self.comment_ranges = tsz_common::comments::get_comment_ranges(text);
        self.source_text = Some(text);
    }

    /// Set `CommonJS` mode
    pub const fn set_commonjs(&mut self, is_commonjs: bool) {
        self.is_commonjs = is_commonjs;
    }

    pub const fn set_module_kind(&mut self, kind: ModuleKind) {
        self.module_kind = kind;
    }

    pub fn set_default_exported_func_names(&mut self, names: std::collections::HashSet<String>) {
        self.default_exported_func_names = names;
    }

    pub fn set_commonjs_export_name(&mut self, name: Option<String>) {
        self.commonjs_export_names = name.into_iter().collect();
    }

    pub fn set_commonjs_export_names(&mut self, names: Vec<String>) {
        self.commonjs_export_names = names;
    }

    pub(crate) fn set_const_enum_facts(
        &mut self,
        values: FxHashMap<String, Vec<ScopedConstEnum>>,
        import_aliases: FxHashMap<String, String>,
    ) {
        self.const_enum_values = values;
        self.const_enum_import_aliases = import_aliases;
    }

    pub const fn set_remove_comments(&mut self, remove_comments: bool) {
        self.remove_comments = remove_comments;
    }

    /// Set exported variable names from prior blocks of the same namespace.
    /// These will be merged with locally-collected exports for rewriting references.
    pub fn set_prior_exported_vars(&mut self, vars: std::collections::HashSet<String>) {
        self.prior_exported_vars = vars;
    }

    /// Collect exported variable names from a namespace declaration without emitting.
    /// Used by the Printer to accumulate cross-block exports.
    pub fn collect_exported_var_names(
        &self,
        ns_idx: NodeIndex,
    ) -> std::collections::HashSet<String> {
        let Some((_parts, innermost_body)) = self.collect_all_namespace_parts(ns_idx) else {
            return std::collections::HashSet::new();
        };
        collect_runtime_exported_var_names(self.arena, innermost_body)
    }

    pub fn collect_namespace_rewrite_var_names(
        &self,
        ns_idx: NodeIndex,
    ) -> Option<(String, std::collections::HashSet<String>)> {
        let (parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        let ns_name = parts.last()?.clone();
        let mut names = collect_runtime_exported_var_names(self.arena, innermost_body);
        if !self.prior_exported_vars.is_empty() {
            names.extend(self.prior_exported_vars.iter().cloned());
            let local_names = collect_local_var_names(self.arena, innermost_body);
            for name in &local_names {
                names.remove(name);
            }
        }
        Some((ns_name, names))
    }

    pub fn collect_namespace_block_scope_shadowed_names(
        &self,
        ns_idx: NodeIndex,
    ) -> std::collections::HashSet<String> {
        let Some((_parts, innermost_body)) = self.collect_all_namespace_parts(ns_idx) else {
            return std::collections::HashSet::new();
        };
        collect_namespace_function_scope_reference_names(self.arena, innermost_body)
    }

    /// Transform a namespace declaration to IR
    ///
    /// Returns `Some(IRNode::NamespaceIIFE { ... })` for valid namespaces,
    /// or `None` for ambient namespaces (declare namespace) or invalid nodes.
    ///
    /// # Arguments
    ///
    /// * `ns_idx` - `NodeIndex` of the namespace declaration
    ///
    /// # Returns
    ///
    /// `Option<IRNode>` - The transformed namespace as an IR node, or None if skipped
    pub fn transform_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        self.transform_namespace_with_flags(ns_idx, false, true)
    }

    /// Transform a namespace declaration with explicit control over var declaration
    pub fn transform_namespace_with_var_flag(
        &self,
        ns_idx: NodeIndex,
        should_declare_var: bool,
    ) -> Option<IRNode> {
        self.transform_namespace_with_flags(ns_idx, false, should_declare_var)
    }

    /// Transform a namespace declaration that is known to be exported
    ///
    /// Use this when the namespace is wrapped in an `EXPORT_DECLARATION`.
    pub fn transform_exported_namespace(&self, ns_idx: NodeIndex) -> Option<IRNode> {
        self.transform_namespace_with_flags(ns_idx, true, true)
    }

    /// Transform an exported namespace declaration with explicit control over
    /// whether the namespace binding declaration should be emitted.
    pub fn transform_exported_namespace_with_var_flag(
        &self,
        ns_idx: NodeIndex,
        should_declare_var: bool,
    ) -> Option<IRNode> {
        self.transform_namespace_with_flags(ns_idx, true, should_declare_var)
    }

    /// Transform a namespace declaration with explicit export and var flags
    fn transform_namespace_with_flags(
        &self,
        ns_idx: NodeIndex,
        force_exported: bool,
        should_declare_var: bool,
    ) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient namespaces (declare namespace)
        if self
            .arena
            .has_modifier(&ns_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return None;
        }

        // Collect all namespace parts for qualified names (A.B.C)
        // The parser creates nested MODULE_DECLARATION nodes for qualified names:
        // MODULE_DECLARATION "A" -> body: MODULE_DECLARATION "B" -> body: MODULE_DECLARATION "C" -> body: MODULE_BLOCK
        let (name_parts, innermost_body) = self.collect_all_namespace_parts(ns_idx)?;
        if name_parts.is_empty() {
            return None;
        }

        // Check if exported from modifiers OR if forced (when wrapped in EXPORT_DECLARATION)
        let is_exported = force_exported
            || self
                .arena
                .has_modifier(&ns_data.modifiers, SyntaxKind::ExportKeyword);

        let using_region = self.namespace_body_using_region(innermost_body);
        if let Some((env_name, using_async, _, _)) = using_region.as_ref() {
            self.active_namespace_using_env
                .replace(Some((env_name.clone(), *using_async)));
        }

        // Transform the innermost body - use the last name part for member exports
        let mut body = self.transform_namespace_body(innermost_body, &name_parts);
        self.active_namespace_using_env.replace(None);
        if let Some((env_name, _using_async, error_name, result_name)) = using_region {
            body = self.wrap_namespace_using_region(body, env_name, error_name, result_name);
        }
        self.rewrite_const_enum_accesses(&mut body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        // A namespace is instantiated if it has any value declarations
        // (variables, functions, classes, enums, sub-namespaces),
        // even if the body produces no IR output (e.g., uninitialized exports).
        // Comments alone don't make a namespace instantiated.
        let has_code = body.iter().any(|n| !is_comment_node(n));
        if !has_code && !self.has_value_declarations(innermost_body) {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        // Root name is the first part
        let name = name_parts.first().cloned().unwrap_or_default();

        let merges_with_default_func = self.is_commonjs
            && (self.default_exported_func_names.contains(&name)
                || self.source_file_has_default_exported_function(ns_idx, &name));
        let default_export_merge = !is_exported && merges_with_default_func;

        Some(IRNode::NamespaceIIFE {
            name: name.into(),
            name_parts: name_parts.into_iter().map(Into::into).collect(),
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs && !merges_with_default_func,
            commonjs_export_names: self
                .commonjs_export_names
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            system_export_names: Vec::new(),
            should_declare_var,
            default_export_merge,
            parent_name: None,
            param_name: param_name.map(Into::into),
            skip_sequence_indent: false,
            trailing_comment: self
                .extract_namespace_trailing_comment(innermost_body)
                .map(Into::into),
            invalid_namespace_static: self.has_invalid_namespace_static_modifier(ns_idx),
        })
    }

    fn namespace_body_using_region(
        &self,
        body_idx: NodeIndex,
    ) -> Option<(String, bool, String, String)> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_module_block(body_node)?;
        let statements = block.statements.as_ref()?;
        let mut has_using = false;
        let mut using_async = false;
        for &stmt_idx in &statements.nodes {
            let Some(stmt_node) = self.arena.get(stmt_idx) else {
                continue;
            };
            let flags = self.variable_statement_source_using_flags(stmt_node);
            if flags & node_flags::USING != 0 {
                has_using = true;
                using_async |= node_flags::is_await_using(flags);
                continue;
            }
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                let flags = decl_list
                    .declarations
                    .nodes
                    .iter()
                    .fold(decl_list_node.flags as u32, |flags, &decl_idx| {
                        flags | self.arena.get_variable_declaration_flags(decl_idx)
                    });
                if flags & node_flags::USING != 0 {
                    has_using = true;
                    using_async |= node_flags::is_await_using(flags);
                }
            }
        }
        if !has_using {
            return None;
        }
        let id = self.disposable_env_counter.get();
        self.disposable_env_counter.set(id + 1);
        let env_name = format!("env_{id}");
        let error_name = format!("e_{id}");
        let result_name = format!("result_{id}");
        self.generated_disposable_env_names.borrow_mut().extend([
            env_name.clone(),
            error_name.clone(),
            result_name.clone(),
        ]);
        Some((env_name, using_async, error_name, result_name))
    }

    fn variable_statement_source_using_flags(&self, node: &Node) -> u32 {
        if node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return 0;
        }
        let Some(source_text) = self.source_text else {
            return 0;
        };
        let start = (node.pos as usize).min(source_text.len());
        let end = (node.end as usize).min(source_text.len());
        let text = source_text[start..end].trim_start();
        if text.starts_with("await using") {
            return node_flags::AWAIT_USING;
        }
        if text.starts_with("using") {
            return node_flags::USING;
        }
        0
    }

    fn disposable_env_initializer_ir() -> IRNode {
        IRNode::ObjectLiteral {
            properties: vec![
                crate::transforms::ir::IRProperty {
                    key: IRPropertyKey::Identifier("stack".into()),
                    value: IRNode::ArrayLiteral(Vec::new()),
                    kind: crate::transforms::ir::IRPropertyKind::Init,
                },
                crate::transforms::ir::IRProperty {
                    key: IRPropertyKey::Identifier("error".into()),
                    value: IRNode::Undefined,
                    kind: crate::transforms::ir::IRPropertyKind::Init,
                },
                crate::transforms::ir::IRProperty {
                    key: IRPropertyKey::Identifier("hasError".into()),
                    value: IRNode::BooleanLiteral(false),
                    kind: crate::transforms::ir::IRPropertyKind::Init,
                },
            ],
            source_range: None,
            extra_indent: 0,
        }
    }

    fn wrap_namespace_using_region(
        &self,
        mut body: Vec<IRNode>,
        env_name: String,
        error_name: String,
        result_name: String,
    ) -> Vec<IRNode> {
        let mut prefix = Vec::new();
        while matches!(body.first(), Some(IRNode::VarDeclList(_))) {
            prefix.push(body.remove(0));
        }
        prefix.push(IRNode::VarDecl {
            name: env_name.clone().into(),
            initializer: Some(Box::new(Self::disposable_env_initializer_ir())),
        });
        prefix.push(IRNode::TryStatement {
            try_block: Box::new(IRNode::Block(body)),
            catch_clause: Some(IRCatchClause {
                param: Some(error_name.clone().into()),
                body: vec![
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::id(env_name.clone()), "error"),
                        IRNode::id(error_name),
                    )),
                    IRNode::expr_stmt(IRNode::assign(
                        IRNode::prop(IRNode::id(env_name.clone()), "hasError"),
                        IRNode::BooleanLiteral(true),
                    )),
                ],
            }),
            finally_block: Some(Box::new(IRNode::Block(vec![IRNode::expr_stmt(
                IRNode::CallExpr {
                    callee: Box::new(IRNode::RuntimeHelper("__disposeResources".into())),
                    arguments: vec![IRNode::id(env_name)],
                },
            )]))),
        });
        let _ = result_name;
        prefix
    }

    fn has_invalid_namespace_static_modifier(&self, node_idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(node_idx) else {
            return false;
        };
        let (modifiers, probe_pos) = match node.kind {
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                let Some(module) = self.arena.get_module(node) else {
                    return false;
                };
                let probe_pos = self
                    .arena
                    .get(module.name)
                    .map_or(node.pos, |name| name.pos);
                (module.modifiers.as_ref(), probe_pos)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                let Some(enum_data) = self.arena.get_enum(node) else {
                    return false;
                };
                let probe_pos = self
                    .arena
                    .get(enum_data.name)
                    .map_or(node.pos, |name| name.pos);
                (enum_data.modifiers.as_ref(), probe_pos)
            }
            _ => (None, node.pos),
        };
        if modifiers.is_some_and(|mods| {
            mods.nodes.iter().any(|&mod_idx| {
                self.arena
                    .get(mod_idx)
                    .is_some_and(|mod_node| mod_node.kind == SyntaxKind::StaticKeyword as u16)
            })
        }) {
            return true;
        }

        let Some(source_text) = self.source_text else {
            return false;
        };
        let token_start = self.skip_trivia_forward(probe_pos, node.end) as usize;
        let Some(remaining) =
            source_text.get(token_start..(node.end as usize).min(source_text.len()))
        else {
            return false;
        };
        if starts_with_keyword_token(remaining, "static") {
            return true;
        }

        let Some((previous, previous_start)) = previous_identifier_token(source_text, token_start)
        else {
            return false;
        };
        if previous == "static" {
            return true;
        }
        if matches!(previous, "namespace" | "module" | "enum")
            && let Some((before_keyword, _)) =
                previous_identifier_token(source_text, previous_start)
        {
            return before_keyword == "static";
        }
        false
    }

    fn source_file_has_default_exported_function(&self, ns_idx: NodeIndex, name: &str) -> bool {
        let mut current = ns_idx;
        while current != NodeIndex::NONE {
            let Some(node) = self.arena.get(current) else {
                return false;
            };
            if node.kind == syntax_kind_ext::SOURCE_FILE {
                let Some(source_file) = self.arena.get_source_file(node) else {
                    return false;
                };
                return source_file.statements.nodes.iter().any(|&stmt_idx| {
                    self.default_exported_function_name(stmt_idx)
                        .as_deref()
                        .is_some_and(|func_name| func_name == name)
                });
            }
            current = self.arena.parent_of(current).unwrap_or(NodeIndex::NONE);
        }
        false
    }

    fn default_exported_function_name(&self, stmt_idx: NodeIndex) -> Option<String> {
        let stmt = self.arena.get(stmt_idx)?;
        if stmt.kind == syntax_kind_ext::EXPORT_DECLARATION {
            let export = self.arena.get_export_decl(stmt)?;
            if export.is_type_only || !export.is_default_export || export.module_specifier.is_some()
            {
                return None;
            }
            let clause = self.arena.get(export.export_clause)?;
            if clause.kind != syntax_kind_ext::FUNCTION_DECLARATION {
                return None;
            }
            let func = self.arena.get_function(clause)?;
            if self.arena.is_declare(&func.modifiers) || func.body.is_none() {
                return None;
            }
            return get_identifier_text(self.arena, func.name);
        }

        if stmt.kind == syntax_kind_ext::FUNCTION_DECLARATION {
            let func = self.arena.get_function(stmt)?;
            if !self
                .arena
                .has_modifier(&func.modifiers, SyntaxKind::DefaultKeyword)
                || self.arena.is_declare(&func.modifiers)
                || func.body.is_none()
            {
                return None;
            }
            return get_identifier_text(self.arena, func.name);
        }

        None
    }

    /// Check if a namespace body contains any value declarations
    fn has_value_declarations(&self, body_idx: NodeIndex) -> bool {
        body_has_value_declarations(self.arena, body_idx)
    }

    fn declaration_keyword_from_var_declarations(&self, declarations: &NodeList) -> &'static str {
        declaration_keyword_from_var_declarations(self.arena, declarations)
    }

    fn namespace_member_ast_ref_if_non_empty(&self, member_idx: NodeIndex) -> Option<IRNode> {
        if let Some(source_text) = self.source_text
            && let Some(member_node) = self.arena.get(member_idx)
        {
            let start = member_node.pos as usize;
            let end = (member_node.end as usize).min(source_text.len());
            if start < end {
                let raw = &source_text[start..end];
                if raw.trim().is_empty() {
                    return None;
                }
            }
        }

        Some(IRNode::ASTRef(member_idx))
    }

    fn is_uninitialized_exported_var_member(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let var_idx = if member_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_data) = self.arena.get_export_decl(member_node)
        {
            export_data.export_clause
        } else {
            member_idx
        };

        let Some(var_node) = self.arena.get(var_idx) else {
            return false;
        };
        if var_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
            return false;
        }
        let Some(var_data) = self.arena.get_variable(var_node) else {
            return false;
        };
        let is_exported = member_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            || self
                .arena
                .has_modifier(&var_data.modifiers, SyntaxKind::ExportKeyword);
        if !is_exported {
            return false;
        }

        var_data.declarations.nodes.iter().all(|&decl_list_idx| {
            self.arena
                .get_variable_at(decl_list_idx)
                .is_none_or(|decl_list| {
                    decl_list.declarations.nodes.iter().all(|&decl_idx| {
                        self.arena
                            .get_variable_declaration_at(decl_idx)
                            .is_none_or(|decl| decl.initializer.is_none())
                    })
                })
        })
    }

    fn is_stray_export_keyword_member(&self, member_idx: NodeIndex) -> bool {
        self.arena
            .get(member_idx)
            .is_some_and(|node| node.kind == SyntaxKind::ExportKeyword as u16)
    }

    fn is_class_like_member(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let inner_idx = if member_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            && let Some(export_data) = self.arena.get_export_decl(member_node)
        {
            export_data.export_clause
        } else {
            member_idx
        };

        self.arena
            .get(inner_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::CLASS_DECLARATION)
    }

    /// Flatten a module name into parts (handles both identifiers and qualified names)
    ///
    /// For qualified names like `A.B.C` (parsed as nested `MODULE_DECLARATIONs`), returns `["A", "B", "C"]`.
    /// For simple identifiers like `foo`, returns `["foo"]`.
    ///
    /// Note: The parser creates nested `MODULE_DECLARATION` nodes for qualified namespace names,
    /// where each level has a single identifier name and the body points to the next level.
    pub fn flatten_module_name(&self, name_idx: NodeIndex) -> Option<Vec<String>> {
        let mut parts = Vec::new();
        self.collect_name_parts(name_idx, &mut parts);
        if parts.is_empty() { None } else { Some(parts) }
    }

    /// Recursively collect name parts from qualified names
    ///
    /// Handles both:
    /// 1. `QUALIFIED_NAME` nodes (left.right structure)
    /// 2. Simple identifier nodes
    fn collect_name_parts(&self, idx: NodeIndex, parts: &mut Vec<String>) {
        let Some(node) = self.arena.get(idx) else {
            return;
        };

        if node.kind == syntax_kind_ext::QUALIFIED_NAME {
            // QualifiedName has left and right - recurse into both
            if let Some(qn_data) = self.arena.qualified_names.get(node.data_index as usize) {
                self.collect_name_parts(qn_data.left, parts);
                self.collect_name_parts(qn_data.right, parts);
            }
        } else if node.kind == SyntaxKind::Identifier as u16
            && let Some(ident) = self.arena.get_identifier(node)
        {
            parts.push(ident.escaped_text.clone());
        }
    }

    /// Collect all name parts by walking through nested `MODULE_DECLARATION` chain
    ///
    /// For `namespace A.B.C {}`, the parser creates:
    /// `MODULE_DECLARATION` "A" -> body: `MODULE_DECLARATION` "B" -> body: `MODULE_DECLARATION` "C" -> body: `MODULE_BLOCK`
    ///
    /// This method walks through all levels and returns (["A", "B", "C"], `innermost_body_idx`)
    fn collect_all_namespace_parts(&self, ns_idx: NodeIndex) -> Option<(Vec<String>, NodeIndex)> {
        let mut parts = Vec::new();
        let mut current_idx = ns_idx;

        loop {
            let node = self.arena.get(current_idx)?;
            if node.kind != syntax_kind_ext::MODULE_DECLARATION {
                // We've reached a non-namespace node (likely MODULE_BLOCK)
                break;
            }

            let ns_data = self.arena.get_module(node)?;

            // Get the name of this level
            let name_node = self.arena.get(ns_data.name)?;
            if let Some(ident) = self.arena.get_identifier(name_node) {
                parts.push(ident.escaped_text.clone());
            }

            // Check if body is another MODULE_DECLARATION (nested namespace) or MODULE_BLOCK
            let body_node = self.arena.get(ns_data.body)?;
            if body_node.kind == syntax_kind_ext::MODULE_DECLARATION {
                // Continue walking nested declarations
                current_idx = ns_data.body;
            } else {
                // We've reached the innermost body (MODULE_BLOCK)
                return Some((parts, ns_data.body));
            }
        }

        if parts.is_empty() {
            None
        } else {
            Some((parts, current_idx))
        }
    }

    /// Transform namespace body into IR nodes
    fn transform_namespace_body(&self, body_idx: NodeIndex, name_parts: &[String]) -> Vec<IRNode> {
        let mut result = Vec::new();
        let mut runtime_exported_vars = collect_runtime_exported_var_names(self.arena, body_idx);
        // Merge in exported vars from prior blocks of the same namespace.
        // This enables cross-block substitution: `x` → `M.x` when `export var x`
        // was declared in a prior block.
        if !self.prior_exported_vars.is_empty() {
            runtime_exported_vars.extend(self.prior_exported_vars.iter().cloned());
            // Remove locally-declared non-exported names — they shadow prior exports
            let local_names = collect_local_var_names(self.arena, body_idx);
            for name in &local_names {
                runtime_exported_vars.remove(name);
            }
        }

        // The innermost namespace name (last part) is used for member exports
        let ns_name = name_parts.last().map_or("", |s| s.as_str());

        let Some(body_node) = self.arena.get(body_idx) else {
            return result;
        };

        // Track names declared by classes, functions, enums so that subsequent
        // namespace declarations merging with them don't re-emit `var`.
        let mut declared_names = std::collections::HashSet::new();

        // First pass: collect declared names from classes, functions, enums
        if let Some(block_data) = self.arena.get_module_block(body_node)
            && let Some(stmts) = block_data.statements.as_ref()
        {
            for &stmt_idx in &stmts.nodes {
                if let Some(stmt_node) = self.arena.get(stmt_idx) {
                    match stmt_node.kind {
                        k if k == syntax_kind_ext::CLASS_DECLARATION => {
                            if let Some(class_data) = self.arena.get_class(stmt_node)
                                && let Some(name) = get_identifier_text(self.arena, class_data.name)
                            {
                                declared_names.insert(name);
                            }
                        }
                        k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                            if let Some(func_data) = self.arena.get_function(stmt_node)
                                && let Some(name) = get_identifier_text(self.arena, func_data.name)
                            {
                                declared_names.insert(name);
                            }
                        }
                        k if k == syntax_kind_ext::ENUM_DECLARATION => {
                            if let Some(enum_data) = self.arena.get_enum(stmt_node)
                                && let Some(name) = get_identifier_text(self.arena, enum_data.name)
                            {
                                declared_names.insert(name);
                            }
                        }
                        k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                            if let Some(export_data) = self.arena.get_export_decl(stmt_node)
                                && let Some(inner) = self.arena.get(export_data.export_clause)
                            {
                                match inner.kind {
                                    k if k == syntax_kind_ext::CLASS_DECLARATION => {
                                        if let Some(class_data) = self.arena.get_class(inner)
                                            && let Some(name) =
                                                get_identifier_text(self.arena, class_data.name)
                                        {
                                            declared_names.insert(name);
                                        }
                                    }
                                    k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                                        if let Some(func_data) = self.arena.get_function(inner)
                                            && let Some(name) =
                                                get_identifier_text(self.arena, func_data.name)
                                        {
                                            declared_names.insert(name);
                                        }
                                    }
                                    k if k == syntax_kind_ext::ENUM_DECLARATION => {
                                        if let Some(enum_data) = self.arena.get_enum(inner)
                                            && let Some(name) =
                                                get_identifier_text(self.arena, enum_data.name)
                                        {
                                            declared_names.insert(name);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Find the position of the closing '}' of the module block.
        // The last statement's node.end may extend into this brace, so we
        // constrain ASTRef nodes to not include it.
        let body_close_pos = self
            .find_module_block_close_pos(body_node)
            .unwrap_or_else(|| body_node.end.saturating_sub(1));

        // Check if it's a module block
        if let Some(block_data) = self.arena.get_module_block(body_node)
            && let Some(stmts) = block_data.statements.as_ref()
        {
            // Track cursor for comment extraction between statements.
            // Start after the opening brace of the module block.
            let mut prev_end = body_node.pos + 1; // skip past '{'
            let mut prev_stmt_pos = body_node.pos + 1;
            let mut pending_static_modifier = false;

            for &stmt_idx in &stmts.nodes {
                let stmt_node = match self.arena.get(stmt_idx) {
                    Some(n) => n,
                    None => continue,
                };
                if stmt_node.kind == SyntaxKind::StaticKeyword as u16 {
                    pending_static_modifier = true;
                    prev_end = stmt_node.end;
                    prev_stmt_pos = stmt_node.pos;
                    continue;
                }

                // Some statements have trailing trivia that includes standalone comments
                // before the next declaration. Capture those comments here so they can
                // be emitted immediately after the current statement.
                let code_end = self.find_code_end_of_erased_stmt(stmt_node.pos, stmt_node.end);
                let is_class_like = self.is_class_like_member(stmt_idx);
                let is_namespace_like_stmt = is_namespace_like(self.arena, stmt_node);
                let next_erases_runtime = stmts
                    .nodes
                    .iter()
                    .copied()
                    .skip_while(|&idx| idx != stmt_idx)
                    .nth(1)
                    .is_some_and(|next_idx| self.namespace_statement_erases_runtime(next_idx));
                let trailing_standalone = if is_class_like || is_namespace_like_stmt {
                    Vec::new()
                } else {
                    self.extract_standalone_comments_in_range(code_end, stmt_node.end)
                };

                // Extract leading comments between previous end and this statement.
                // We compute them up-front but defer pushing until we know whether
                // this statement actually produces IR. If the statement is erased
                // (e.g., a non-instantiated namespace, interface, type alias), its
                // leading comments belong to the erasure too and must be dropped.
                let actual_start = self.skip_trivia_forward(stmt_node.pos, stmt_node.end);
                let leading_comments = if !self.is_uninitialized_exported_var_member(stmt_idx)
                    && !self.is_stray_export_keyword_member(stmt_idx)
                {
                    if prev_end <= actual_start {
                        self.extract_comments_in_range(prev_end, actual_start)
                    } else if prev_end == stmt_node.pos && prev_stmt_pos <= actual_start {
                        // Parser trivia-skipping can move `stmt_node.end` to the next statement token,
                        // which can skip standalone comments on blank lines. Recover those comments
                        // by probing from the previous statement start as a fallback.
                        self.extract_comments_in_range(prev_stmt_pos, actual_start)
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                };

                let mut ir = self.transform_namespace_member_with_declared(
                    ns_name,
                    stmt_idx,
                    &declared_names,
                );
                if pending_static_modifier {
                    if let Some(ir_node) = ir.as_mut() {
                        mark_invalid_namespace_static(ir_node);
                    }
                    pending_static_modifier = false;
                }
                if ir.is_some() {
                    for c in leading_comments {
                        result.push(c);
                    }
                }

                if let Some(ir) = ir {
                    // Constrain ASTRef nodes so their source text doesn't extend
                    // into the module block's closing brace.
                    let ir = if let IRNode::ASTRef(idx) = ir {
                        IRNode::ASTRefRange(idx, body_close_pos)
                    } else {
                        ir
                    };

                    // Check for trailing comment on the same line as this statement.
                    // Skip namespace/class declarations since their sub-emitters handle
                    // internal comments.
                    let export_clause_kind =
                        if stmt_node.kind == syntax_kind_ext::EXPORT_DECLARATION {
                            self.arena
                                .get_export_decl(stmt_node)
                                .and_then(|d| self.arena.get(d.export_clause))
                                .map(|n| n.kind)
                        } else {
                            None
                        };
                    let skip = is_namespace_like_stmt
                        || stmt_node.kind == syntax_kind_ext::CLASS_DECLARATION
                        || matches!(export_clause_kind, Some(k) if k == syntax_kind_ext::CLASS_DECLARATION || k == syntax_kind_ext::MODULE_DECLARATION);
                    let trailing =
                        self.extract_trailing_comment_in_stmt(stmt_node.pos, stmt_node.end);
                    let mut ir = ir;
                    let mut trailing_attached_in_sequence = false;
                    // For exported function declarations inside namespaces, attach trailing
                    // comments to the function declaration, not the namespace export assignment.
                    if let IRNode::Sequence(items) = &mut ir
                        && let Some(comment_text) = trailing.clone()
                        && items.len() > 1
                        && matches!(items.first(), Some(IRNode::FunctionDecl { .. }))
                    {
                        items.insert(1, IRNode::TrailingComment(comment_text.into()));
                        trailing_attached_in_sequence = true;
                    }
                    result.push(ir);
                    if !skip
                        && !trailing_attached_in_sequence
                        && let Some(comment_text) = trailing
                    {
                        result.push(IRNode::TrailingComment(comment_text.into()));
                    }
                } else {
                    // Erased statement (interface/type alias).
                    // (Standalone trailing comments are now emitted above for all
                    // statement kinds.)
                }

                if !next_erases_runtime {
                    for c in trailing_standalone {
                        result.push(c);
                    }
                }

                // For class-like members the class sub-emitter handles its own
                // internal comments and we don't extract `trailing_standalone`
                // for them — but we still need to surface standalone comments
                // sitting between the class's `}` and the next statement.
                // Stop the cursor at `code_end` (after `}`, before any trailing
                // trivia / inter-statement comments) so the next statement's
                // leading-comment extraction picks them up. For other members
                // `trailing_standalone` already drained that gap, so advancing
                // to `stmt_node.end` is safe and keeps current behavior.
                prev_end = if is_class_like || is_namespace_like_stmt {
                    if is_namespace_like_stmt {
                        self.trailing_same_line_comment_end_after(code_end)
                            .unwrap_or(code_end)
                    } else {
                        code_end
                    }
                } else {
                    stmt_node.end
                };
                prev_stmt_pos = stmt_node.pos;
            }

            // Extract standalone comments after the last statement but before the closing brace.
            // Since node.end includes trailing trivia, these are comments NOT part of any
            // statement's trivia — they appear on their own lines before `}`.
            if let Some(last_stmt) = stmts.nodes.last()
                && let Some(last_node) = self.arena.get(*last_stmt)
            {
                let code_end = self.find_code_end_of_erased_stmt(last_node.pos, last_node.end);
                let standalone_comments =
                    self.extract_standalone_comments_in_range(code_end, body_close_pos);
                for c in standalone_comments {
                    result.push(c);
                }
            }
        }

        if !runtime_exported_vars.is_empty() {
            for node in &mut result {
                rewrite_exported_var_refs(node, ns_name, &runtime_exported_vars);
            }
        }

        // Insert `var _a;` declarations at the top for hoisted temp variables
        // collected during expression conversion (e.g., computed property lowering
        // in object literals: `{ [expr]: val }` → `(_a = {}, _a[expr] = val, _a)`).
        let extra_temps: Vec<String> = std::mem::take(&mut *self.hoisted_temps.borrow_mut());
        if !extra_temps.is_empty() {
            let var_decls: Vec<IRNode> = extra_temps
                .into_iter()
                .map(|name| IRNode::VarDecl {
                    name: name.into(),
                    initializer: None,
                })
                .collect();
            result.insert(0, IRNode::VarDeclList(var_decls));
        }

        result
    }

    /// Transform a namespace member, considering already-declared names for `should_declare_var`
    fn transform_namespace_member_with_declared(
        &self,
        ns_name: &str,
        member_idx: NodeIndex,
        declared_names: &std::collections::HashSet<String>,
    ) -> Option<IRNode> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                // Check if a class/function/enum already declared this name
                let ns_data = self.arena.get_module(member_node)?;
                let name = get_identifier_text(self.arena, ns_data.name)?;
                let should_declare_var = !declared_names.contains(&name);
                self.transform_nested_namespace_core(ns_name, member_idx, should_declare_var, false)
            }
            k if k == syntax_kind_ext::EXPORT_DECLARATION => {
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    if let Some(inner) = self.arena.get(export_data.export_clause)
                        && inner.kind == syntax_kind_ext::MODULE_DECLARATION
                    {
                        let ns_data = self.arena.get_module(inner)?;
                        let name = get_identifier_text(self.arena, ns_data.name)?;
                        let should_declare_var = !declared_names.contains(&name);
                        return self.transform_nested_namespace_core(
                            ns_name,
                            export_data.export_clause,
                            should_declare_var,
                            true,
                        );
                    }
                    self.transform_namespace_member(ns_name, export_data.export_clause, true)
                } else {
                    None
                }
            }
            _ => self.transform_namespace_member(ns_name, member_idx, false),
        }
    }

    /// Transform a namespace member to IR. When `force_export` is true, the member
    /// is always treated as exported (used for `export { decl }` wrappers).
    fn transform_namespace_member(
        &self,
        ns_name: &str,
        member_idx: NodeIndex,
        force_export: bool,
    ) -> Option<IRNode> {
        let member_node = self.arena.get(member_idx)?;

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION && !force_export => {
                // Handle export declarations by extracting the inner declaration
                if let Some(export_data) = self.arena.get_export_decl(member_node) {
                    if self.namespace_statement_erases_runtime(export_data.export_clause) {
                        return None;
                    }
                    self.transform_namespace_member(ns_name, export_data.export_clause, true)
                } else {
                    None
                }
            }
            k if k == SyntaxKind::ExportKeyword as u16 => None,
            k if k == syntax_kind_ext::FUNCTION_DECLARATION => {
                self.transform_function_in_namespace(ns_name, member_idx, force_export)
            }
            k if k == syntax_kind_ext::CLASS_DECLARATION => {
                self.transform_class_in_namespace(ns_name, member_idx, force_export)
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                self.transform_variable_in_namespace(ns_name, member_idx, force_export)
            }
            k if k == syntax_kind_ext::MODULE_DECLARATION => {
                self.transform_nested_namespace_core(ns_name, member_idx, true, force_export)
            }
            k if k == syntax_kind_ext::ENUM_DECLARATION => {
                self.transform_enum_in_namespace(ns_name, member_idx, force_export)
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                if self.import_equals_uses_external_module_ref(member_idx) {
                    return None;
                }
                if force_export {
                    self.transform_import_equals_exported(ns_name, member_idx)
                } else {
                    self.transform_import_equals_in_namespace(ns_name, member_idx)
                }
            }
            k if k == syntax_kind_ext::IMPORT_DECLARATION => None,
            k if k == syntax_kind_ext::NAMED_EXPORTS => None,
            k if !force_export
                && (k == syntax_kind_ext::INTERFACE_DECLARATION
                    || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION) =>
            {
                None
            }
            _ if !force_export => self.namespace_member_ast_ref_if_non_empty(member_idx),
            _ => None,
        }
    }

    fn namespace_statement_erases_runtime(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return true;
        };

        match member_node.kind {
            k if k == syntax_kind_ext::EXPORT_DECLARATION => self
                .arena
                .get_export_decl(member_node)
                .is_none_or(|export_data| {
                    self.namespace_statement_erases_runtime(export_data.export_clause)
                }),
            k if k == syntax_kind_ext::INTERFACE_DECLARATION
                || k == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || k == syntax_kind_ext::IMPORT_DECLARATION
                || k == syntax_kind_ext::NAMED_EXPORTS =>
            {
                true
            }
            k if k == syntax_kind_ext::IMPORT_EQUALS_DECLARATION => {
                self.import_equals_uses_external_module_ref(member_idx)
            }
            _ => false,
        }
    }

    fn import_equals_uses_external_module_ref(&self, import_idx: NodeIndex) -> bool {
        let Some(import) = self.arena.get_import_decl_at(import_idx) else {
            return false;
        };
        self.arena.get(import.module_specifier).is_some_and(|node| {
            node.kind == syntax_kind_ext::EXTERNAL_MODULE_REFERENCE
                || node.kind == SyntaxKind::StringLiteral as u16
        })
    }

    fn transform_import_equals_in_namespace(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        if !self.import_equals_target_has_runtime_value(import_idx, import.module_specifier) {
            return None;
        }

        let alias = get_identifier_text(self.arena, import.import_clause)?;
        if !self.import_equals_alias_is_referenced_after_node(import_idx, import) {
            return None;
        }

        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);
        let is_exported = self
            .arena
            .has_modifier(&import.modifiers, SyntaxKind::ExportKeyword);

        if is_exported {
            Some(IRNode::NamespaceExport {
                namespace: ns_name.to_string().into(),
                name: alias.into(),
                value: Box::new(target_expr),
            })
        } else {
            Some(IRNode::VarDecl {
                name: alias.into(),
                initializer: Some(Box::new(target_expr)),
            })
        }
    }

    fn transform_import_equals_exported(
        &self,
        ns_name: &str,
        import_idx: NodeIndex,
    ) -> Option<IRNode> {
        let import = self.arena.get_import_decl_at(import_idx)?;
        let alias = get_identifier_text(self.arena, import.import_clause)?;

        if !self.import_equals_target_has_runtime_value(import_idx, import.module_specifier) {
            return None;
        }

        let target_expr = AstToIr::new(self.arena).convert_expression(import.module_specifier);

        Some(IRNode::NamespaceExport {
            namespace: ns_name.to_string().into(),
            name: alias.into(),
            value: Box::new(target_expr),
        })
    }

    fn import_equals_target_has_runtime_value(
        &self,
        import_idx: NodeIndex,
        target_idx: NodeIndex,
    ) -> bool {
        let Some(target_parts) = collect_qualified_name_parts(self.arena, target_idx) else {
            return true;
        };

        let namespace_parts = self.containing_namespace_parts(import_idx);
        if !namespace_parts.is_empty() {
            let mut relative_parts = namespace_parts;
            relative_parts.extend(target_parts.iter().cloned());
            if let Some(has_runtime) = entity_path_has_runtime_value(self.arena, &relative_parts) {
                return has_runtime;
            }
        }

        entity_path_has_runtime_value(self.arena, &target_parts).unwrap_or(true)
    }

    fn import_equals_alias_is_referenced_after_node(
        &self,
        import_idx: NodeIndex,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> bool {
        let Some(alias) = get_identifier_text(self.arena, import.import_clause) else {
            return true;
        };
        let Some(source_text) = self.source_text else {
            return true;
        };
        let Some(import_node) = self.arena.get(import_idx) else {
            return true;
        };
        let full_haystack = self.source_after_import_equals(import_node, import);
        let haystack = if let Some(scope_end) = self.namespace_import_scope_end(import_idx) {
            let full_start_in_source = source_text.len().saturating_sub(full_haystack.len());
            let scope_end = scope_end as usize;
            if scope_end <= full_start_in_source {
                ""
            } else {
                let end_in_full = scope_end - full_start_in_source;
                &full_haystack[..end_in_full.min(full_haystack.len())]
            }
        } else {
            full_haystack
        };
        let stripped = crate::import_usage::strip_type_only_content(haystack);
        crate::import_usage::contains_identifier_occurrence_before_shadow(&stripped, &alias)
    }

    fn source_after_import_equals(
        &self,
        import_node: &Node,
        import: &tsz_parser::parser::node::ImportDeclData,
    ) -> &'a str {
        let Some(source_text) = self.source_text else {
            return "";
        };
        let mut start = self
            .arena
            .get(import.module_specifier)
            .map_or(import_node.end as usize, |module_node| {
                module_node.end as usize
            });
        start = start.min(source_text.len());
        let bytes = source_text.as_bytes();
        while start < bytes.len() {
            match bytes[start] {
                b'\n' => {
                    start += 1;
                    break;
                }
                b'\r' => {
                    start += 1;
                    if start < bytes.len() && bytes[start] == b'\n' {
                        start += 1;
                    }
                    break;
                }
                _ => start += 1,
            }
        }
        &source_text[start..]
    }

    fn namespace_import_scope_end(&self, import_idx: NodeIndex) -> Option<u32> {
        let block_idx = self.containing_module_block(import_idx)?;
        let block_node = self.arena.get(block_idx)?;
        let block = self.arena.get_module_block(block_node)?;
        let statements = block.statements.as_ref()?;
        let last_stmt = statements
            .nodes
            .last()
            .and_then(|last_idx| self.arena.get(*last_idx))?;
        Some(self.find_code_end_of_erased_stmt(last_stmt.pos, last_stmt.end))
    }

    fn containing_module_block(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = self.arena.parent_of(node_idx).unwrap_or(NodeIndex::NONE);
        while current != NodeIndex::NONE {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::MODULE_BLOCK {
                return Some(current);
            }
            current = self.arena.parent_of(current).unwrap_or(NodeIndex::NONE);
        }
        None
    }

    fn containing_namespace_parts(&self, node_idx: NodeIndex) -> Vec<String> {
        let mut groups = Vec::new();
        let mut current = self.arena.parent_of(node_idx).unwrap_or(NodeIndex::NONE);

        while current != NodeIndex::NONE {
            let Some(node) = self.arena.get(current) else {
                break;
            };
            if node.kind == syntax_kind_ext::MODULE_DECLARATION
                && let Some(module) = self.arena.get_module(node)
                && let Some(parts) = self.flatten_module_name(module.name)
            {
                groups.push(parts);
            }
            current = self.arena.parent_of(current).unwrap_or(NodeIndex::NONE);
        }

        groups.reverse();
        groups.into_iter().flatten().collect()
    }

    /// Transform a function in namespace. When `force_export` is true, the function
    /// is always treated as exported (used for `export { function }` wrappers).
    fn transform_function_in_namespace(
        &self,
        ns_name: &str,
        func_idx: NodeIndex,
        force_export: bool,
    ) -> Option<IRNode> {
        let func_data = self.arena.get_function_at(func_idx)?;

        if let Some(body_node) = self.arena.get(func_data.body)
            && body_node.kind != syntax_kind_ext::BLOCK
        {
            // Malformed `function f() => expr` declarations keep `expr` as the
            // recovery body. TypeScript emits that expression as a statement,
            // not as a function declaration or namespace export.
            return Some(IRNode::ExpressionStatement(Box::new(
                AstToIr::new(self.arena).convert_expression(func_data.body),
            )));
        }

        // Skip declaration-only functions (no body)
        if func_data.body.is_none() {
            return None;
        }

        let func_name = get_identifier_text(self.arena, func_data.name)?;
        let is_exported = force_export
            || self
                .arena
                .has_modifier(&func_data.modifiers, SyntaxKind::ExportKeyword);

        let func_decl = if func_data.is_async && !func_data.asterisk_token {
            let mut async_transformer = AsyncES5Transformer::new(self.arena);
            async_transformer.set_module_kind(self.module_kind);
            if let Some(src) = self.source_text {
                async_transformer.set_source_text(src);
            }
            async_transformer.transform_async_function(func_idx)
        } else {
            let body_source_range = self.arena.pos_end_at(func_data.body);

            // Convert function to IR (stripping type annotations)
            let mut parameters =
                convert_function_parameters(self.arena, &func_data.parameters, self.source_text);
            if parameters.is_empty()
                && let Some(recovered) = recover_empty_function_parameters_from_header(
                    self.arena,
                    self.source_text,
                    func_idx,
                    func_data.body,
                )
            {
                parameters = recovered;
            }

            IRNode::FunctionDecl {
                name: func_name.clone().into(),
                parameters,
                body: convert_function_body(self.arena, func_data.body),
                body_source_range,
                leading_comment: None,
            }
        };

        if is_exported {
            Some(IRNode::Sequence(vec![
                func_decl,
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string().into(),
                    name: func_name.clone().into(),
                    value: Box::new(IRNode::Identifier(func_name.into())),
                },
            ]))
        } else {
            Some(func_decl)
        }
    }

    /// Transform a class in namespace. When `force_export` is true, the class
    /// is always treated as exported (used for `export { class }` wrappers).
    fn transform_class_in_namespace(
        &self,
        ns_name: &str,
        class_idx: NodeIndex,
        force_export: bool,
    ) -> Option<IRNode> {
        let class_data = self.arena.get_class_at(class_idx)?;

        let class_name = get_identifier_text(self.arena, class_data.name)?;
        let is_exported = force_export
            || self
                .arena
                .has_modifier(&class_data.modifiers, SyntaxKind::ExportKeyword);

        // Transform the class to ES5 using the class transformer
        let mut class_transformer = ES5ClassTransformer::new(self.arena);
        // Classes in namespace are nested one level deeper than top-level
        class_transformer.set_indent_base(1);
        // Forward `--emitDecoratorMetadata` so namespace-scoped decorated
        // classes still emit `__metadata("design:type", T)` etc.
        class_transformer.set_emit_decorator_metadata(self.emit_decorator_metadata);

        // Pass legacy decorator info so __decorate calls are emitted inside the IIFE
        if self.legacy_decorators {
            let has_member_decorators = class_data.members.nodes.iter().any(|&m_idx| {
                let Some(m_node) = self.arena.get(m_idx) else {
                    return false;
                };
                let mods = match m_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(m_node)
                        .and_then(|m| m.modifiers.as_ref()),
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(m_node)
                        .and_then(|p| p.modifiers.as_ref()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(m_node)
                            .and_then(|a| a.modifiers.as_ref())
                    }
                    _ => None,
                };
                mods.is_some_and(|m| {
                    m.nodes.iter().any(|&mod_idx| {
                        self.arena
                            .get(mod_idx)
                            .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                    })
                })
            });
            if has_member_decorators {
                class_transformer.set_legacy_decorators(true);
            }
            // Collect class-level decorators
            let class_decorators: Vec<NodeIndex> = class_data
                .modifiers
                .as_ref()
                .map(|mods| {
                    mods.nodes
                        .iter()
                        .copied()
                        .filter(|&mod_idx| {
                            self.arena
                                .get(mod_idx)
                                .is_some_and(|n| n.kind == syntax_kind_ext::DECORATOR)
                        })
                        .collect()
                })
                .unwrap_or_default();
            if !class_decorators.is_empty() {
                class_transformer.set_class_decorators(class_decorators);
            }
        }

        if let Some(text) = self.source_text {
            class_transformer.set_source_text(text);
        }

        let class_ir = class_transformer.transform_class_to_ir(class_idx)?;

        if is_exported {
            Some(IRNode::Sequence(vec![
                class_ir,
                IRNode::NamespaceExport {
                    namespace: ns_name.to_string().into(),
                    name: class_name.clone().into(),
                    value: Box::new(IRNode::Identifier(class_name.into())),
                },
            ]))
        } else {
            Some(class_ir)
        }
    }

    /// Transform a variable statement in namespace. When `force_export` is true, the variable
    /// is always treated as exported (used for `export { variable }` wrappers).
    fn transform_variable_in_namespace(
        &self,
        ns_name: &str,
        var_idx: NodeIndex,
        force_export: bool,
    ) -> Option<IRNode> {
        let var_data = self.arena.get_variable_at(var_idx)?;
        if self.arena.is_declare(&var_data.modifiers) {
            return None;
        }

        let is_exported = force_export
            || self
                .arena
                .has_modifier(&var_data.modifiers, SyntaxKind::ExportKeyword);

        if let Some((env_name, using_async)) = self.active_namespace_using_env.borrow().clone() {
            let source_flags = self
                .arena
                .get(var_idx)
                .map_or(0, |node| self.variable_statement_source_using_flags(node));
            let mut decls = Vec::new();
            let mut temps = Vec::new();
            for &decl_list_idx in &var_data.declarations.nodes {
                let Some(decl_list_node) = self.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.arena.get_variable(decl_list_node) else {
                    continue;
                };
                let flags = source_flags
                    | decl_list.declarations.nodes.iter().fold(
                        decl_list_node.flags as u32,
                        |flags, &decl_idx| {
                            flags | self.arena.get_variable_declaration_flags(decl_idx)
                        },
                    );
                if flags & node_flags::USING == 0 {
                    continue;
                }
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl) = self.arena.get_variable_declaration_at(decl_idx) else {
                        continue;
                    };
                    let Some(name) = get_identifier_text(self.arena, decl.name) else {
                        continue;
                    };
                    let initializer = if decl.initializer.is_some() {
                        let converter = AstToIr::new(self.arena);
                        let expr = converter.convert_expression(decl.initializer);
                        temps.extend(converter.take_hoisted_temps());
                        expr
                    } else {
                        IRNode::void_0()
                    };
                    decls.push(IRNode::VarDecl {
                        name: name.into(),
                        initializer: Some(Box::new(IRNode::CallExpr {
                            callee: Box::new(IRNode::RuntimeHelper(
                                "__addDisposableResource".into(),
                            )),
                            arguments: vec![
                                IRNode::id(env_name.clone()),
                                initializer,
                                IRNode::BooleanLiteral(using_async),
                            ],
                        })),
                    });
                }
            }
            if !decls.is_empty() {
                self.hoisted_temps.borrow_mut().extend(temps);
                return Some(IRNode::Sequence(decls));
            }
        }

        if is_exported {
            // For exported variables, emit directly as namespace property assignments:
            // `Namespace.X = initializer;` instead of `var X = initializer; Namespace.X = X;`
            let (decls, temps) =
                convert_exported_variable_declarations(self.arena, &var_data.declarations, ns_name);
            self.hoisted_temps.borrow_mut().extend(temps);
            if decls.is_empty() {
                None
            } else {
                Some(IRNode::Sequence(decls))
            }
        } else {
            if self.variable_statement_has_binding_pattern(var_idx) {
                return Some(IRNode::ASTRef(var_idx));
            }

            let empty_decl_keyword =
                self.declaration_keyword_from_var_declarations(&var_data.declarations);
            let (decls, temps) = convert_variable_declarations(
                self.arena,
                &var_data.declarations,
                empty_decl_keyword,
            );
            self.hoisted_temps.borrow_mut().extend(temps);
            Some(IRNode::Sequence(decls))
        }
    }

    fn variable_statement_has_binding_pattern(&self, var_idx: NodeIndex) -> bool {
        let Some(var_data) = self.arena.get_variable_at(var_idx) else {
            return false;
        };

        var_data.declarations.nodes.iter().any(|&decl_list_idx| {
            self.arena
                .get_variable_at(decl_list_idx)
                .is_some_and(|decl_list| {
                    decl_list.declarations.nodes.iter().any(|&decl_idx| {
                        let Some(decl) = self.arena.get_variable_declaration_at(decl_idx) else {
                            return false;
                        };
                        self.arena.get(decl.name).is_some_and(|name| {
                            name.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN
                                || name.kind == syntax_kind_ext::OBJECT_BINDING_PATTERN
                        })
                    })
                })
        })
    }

    /// Transform an enum in namespace. When `force_export` is true, the enum
    /// is always treated as exported (used for `export { enum }` wrappers).
    fn transform_enum_in_namespace(
        &self,
        ns_name: &str,
        enum_idx: NodeIndex,
        force_export: bool,
    ) -> Option<IRNode> {
        let is_exported = force_export || {
            let enum_node = self.arena.get(enum_idx)?;
            let enum_data = self.arena.get_enum(enum_node)?;
            self.arena
                .has_modifier(&enum_data.modifiers, SyntaxKind::ExportKeyword)
        };

        let mut enum_ir = transform_enum_to_ir(self.arena, enum_idx)?;
        let invalid_namespace_static = self.has_invalid_namespace_static_modifier(enum_idx);

        // For exported enums, fold the namespace export into the IIFE closing:
        // `(Color = A.Color || (A.Color = {}))` instead of separate `A.Color = Color;`
        if is_exported
            && let IRNode::EnumIIFE {
                namespace_export, ..
            } = &mut enum_ir
        {
            *namespace_export = Some(ns_name.to_string().into());
        }
        if invalid_namespace_static
            && let IRNode::EnumIIFE {
                invalid_namespace_static,
                ..
            } = &mut enum_ir
        {
            *invalid_namespace_static = true;
        }

        Some(enum_ir)
    }

    /// Core implementation for nested namespace transforms. When `force_export` is true,
    /// the namespace is always treated as exported (used for `export { namespace }` wrappers).
    fn transform_nested_namespace_core(
        &self,
        parent_ns: &str,
        ns_idx: NodeIndex,
        should_declare_var: bool,
        force_export: bool,
    ) -> Option<IRNode> {
        let ns_data = self.arena.get_module_at(ns_idx)?;

        // Skip ambient nested namespaces
        if self
            .arena
            .has_modifier(&ns_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return None;
        }

        let name_parts = self.flatten_module_name(ns_data.name)?;
        if name_parts.is_empty() {
            return None;
        }

        let is_exported = force_export
            || self
                .arena
                .has_modifier(&ns_data.modifiers, SyntaxKind::ExportKeyword);

        // Transform body
        let mut body = self.transform_namespace_body(ns_data.body, &name_parts);
        self.rewrite_const_enum_accesses(&mut body, &name_parts);

        // Skip non-instantiated namespaces (only contain types).
        if !body.iter().any(|n| !is_comment_node(n)) && !self.has_value_declarations(ns_data.body) {
            return None;
        }

        // Detect collision: if a member name matches the innermost namespace name,
        // rename the IIFE parameter (e.g., A -> A_1)
        let innermost_name = name_parts.last().map_or("", |s| s.as_str());
        let param_name = detect_and_apply_param_rename(&mut body, innermost_name);

        let name = name_parts.first().cloned().unwrap_or_default();

        Some(IRNode::NamespaceIIFE {
            name: name.into(),
            name_parts: name_parts.into_iter().map(Into::into).collect(),
            body,
            is_exported,
            attach_to_exports: is_exported && self.is_commonjs,
            commonjs_export_names: self
                .commonjs_export_names
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
            system_export_names: Vec::new(),
            should_declare_var,
            default_export_merge: false,
            parent_name: is_exported.then(|| parent_ns.to_string().into()),
            param_name: param_name.map(Into::into),
            skip_sequence_indent: true, // Nested namespace IIFEs need to skip indent when in sequence
            trailing_comment: self
                .extract_namespace_trailing_comment(ns_data.body)
                .map(Into::into),
            invalid_namespace_static: self.has_invalid_namespace_static_modifier(ns_idx),
        })
    }
}

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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../../tests/namespace_es5_ir.rs"]
mod tests;
