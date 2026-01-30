//! Signature Help implementation for LSP.
//!
//! Provides function signature information and active parameter highlighting
//! when typing arguments in a call expression.

use crate::binder::BinderState;
use crate::binder::symbol_flags;
use crate::checker::state::CheckerState;
use crate::lsp::jsdoc::{ParsedJsdoc, jsdoc_for_node, parse_jsdoc};
use crate::lsp::position::{LineMap, Position};
use crate::lsp::resolver::{ScopeCache, ScopeCacheStats};
use crate::lsp::utils::find_node_at_or_before_offset;
use crate::parser::node::{CallExprData, NodeAccess, NodeArena};
use crate::parser::{NodeIndex, NodeList, syntax_kind_ext};
use crate::scanner::SyntaxKind;
use crate::solver::{FunctionShape, TypeId, TypeInterner, TypeKey};

/// Represents a parameter in a signature.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParameterInformation {
    /// The label of this parameter (e.g., "x: number")
    pub label: String,
    /// The documentation for this parameter
    pub documentation: Option<String>,
}

/// Represents a single signature (overload).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignatureInformation {
    /// The label of the signature (e.g., "add(x: number, y: number): number")
    pub label: String,
    /// The documentation for this signature
    pub documentation: Option<String>,
    /// The parameters of this signature
    pub parameters: Vec<ParameterInformation>,
}

/// The response for a signature help request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignatureHelp {
    /// One or more signatures (for overloads)
    pub signatures: Vec<SignatureInformation>,
    /// The active signature (usually 0, or based on best match)
    pub active_signature: u32,
    /// The active parameter index based on cursor position
    pub active_parameter: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallKind {
    Call,
    New,
}

struct SignatureCandidate {
    info: SignatureInformation,
    required_params: usize,
    total_params: usize,
    has_rest: bool,
    param_names: Vec<Option<String>>,
}

struct SignatureDocCandidate {
    doc: ParsedJsdoc,
    required_params: usize,
    total_params: usize,
    has_rest: bool,
}

struct SignatureDocs {
    candidates: Vec<SignatureDocCandidate>,
    fallback: Option<ParsedJsdoc>,
}

impl SignatureDocs {
    fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.fallback.is_none()
    }
}

pub struct SignatureHelpProvider<'a> {
    arena: &'a NodeArena,
    binder: &'a BinderState,
    line_map: &'a LineMap,
    interner: &'a TypeInterner,
    source_text: &'a str,
    file_name: String,
    strict: bool,
}

impl<'a> SignatureHelpProvider<'a> {
    pub fn new(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            interner,
            source_text,
            file_name,
            strict: false,
        }
    }

    pub fn with_strict(
        arena: &'a NodeArena,
        binder: &'a BinderState,
        line_map: &'a LineMap,
        interner: &'a TypeInterner,
        source_text: &'a str,
        file_name: String,
        strict: bool,
    ) -> Self {
        Self {
            arena,
            binder,
            line_map,
            interner,
            source_text,
            file_name,
            strict,
        }
    }

    /// Get signature help at the given position.
    ///
    /// # Arguments
    /// * `root` - The root node of the AST
    /// * `position` - The cursor position
    /// * `type_cache` - Mutable reference to the persistent type cache (for performance)
    pub fn get_signature_help(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<crate::checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        self.get_signature_help_internal(root, position, type_cache, None, None)
    }

    pub fn get_signature_help_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<crate::checker::TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SignatureHelp> {
        self.get_signature_help_internal(root, position, type_cache, Some(scope_cache), scope_stats)
    }

    fn get_signature_help_internal(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<crate::checker::TypeCache>,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SignatureHelp> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // 1. Find the deepest node at the cursor
        let leaf_node = find_node_at_or_before_offset(self.arena, offset, self.source_text);

        // 2. Walk up to find the nearest CallExpression or NewExpression
        let (call_node_idx, call_expr, call_kind) = self.find_containing_call(leaf_node)?;

        // 3. Determine active parameter by counting commas
        let active_parameter = self.determine_active_parameter(call_node_idx, call_expr, offset);

        // 4. Resolve the symbol being called using ScopeWalker
        let mut walker = crate::lsp::resolver::ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, call_expr.expression, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, call_expr.expression)
        };

        // 5. Create checker with persistent cache if available
        let compiler_options = crate::checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            isolated_modules: false,
            ..Default::default()
        };
        let mut checker = if let Some(cache) = type_cache.take() {
            CheckerState::with_cache(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                cache,
                compiler_options,
            )
        } else {
            CheckerState::new(
                self.arena,
                self.binder,
                self.interner,
                self.file_name.clone(),
                compiler_options,
            )
        };

        let access_docs = if call_kind == CallKind::Call {
            self.signature_documentation_for_property_access(root, call_expr.expression)
        } else {
            None
        };

        let (callee_type, docs) = if let Some(symbol_id) = symbol_id {
            (
                checker.get_type_of_symbol(symbol_id),
                access_docs.or_else(|| {
                    self.signature_documentation_for_symbol(root, symbol_id, call_kind)
                }),
            )
        } else {
            (checker.get_type_of_node(call_expr.expression), access_docs)
        };

        // 6. Extract signatures from the type
        let mut signatures = self.get_signatures_from_type(callee_type, &checker, call_kind);

        if let Some(docs) = docs {
            self.apply_signature_docs(&mut signatures, &docs);
        }

        // Extract and save the updated cache for future queries
        *type_cache = Some(checker.extract_cache());

        if signatures.is_empty() {
            return None;
        }

        let arg_count = call_expr
            .arguments
            .as_ref()
            .map(|args| args.nodes.len())
            .unwrap_or(0);
        let active_signature =
            self.select_active_signature(&signatures, arg_count, active_parameter);

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
        })
    }

    /// Walk up the AST to find the call expression containing the cursor.
    fn find_containing_call(
        &self,
        start_node: NodeIndex,
    ) -> Option<(NodeIndex, &'a CallExprData, CallKind)> {
        let mut current = start_node;

        // Safety limit to prevent infinite loops
        let mut depth = 0;
        while !current.is_none() && depth < 100 {
            if let Some(node) = self.arena.get(current) {
                if (node.kind == syntax_kind_ext::CALL_EXPRESSION
                    || node.kind == syntax_kind_ext::NEW_EXPRESSION)
                    && let Some(data) = self.arena.get_call_expr(node)
                {
                    let kind = if node.kind == syntax_kind_ext::NEW_EXPRESSION {
                        CallKind::New
                    } else {
                        CallKind::Call
                    };
                    return Some((current, data, kind));
                }

                // Move up to parent
                if let Some(extended) = self.arena.get_extended(current) {
                    current = extended.parent;
                } else {
                    break;
                }
            } else {
                break;
            }
            depth += 1;
        }

        None
    }

    /// Determine active parameter by scanning for commas, respecting nesting.
    /// This is more robust than AST analysis for incomplete code.
    fn determine_active_parameter(
        &self,
        call_idx: NodeIndex,
        data: &CallExprData,
        cursor_offset: u32,
    ) -> u32 {
        // Use AST-based approach instead of token scanning to handle edge cases:
        // - Generic type arguments with angle brackets: Set<string, number>
        // - Nested calls: foo(bar(x, y), z)
        // - Complex expressions with comparison operators: a < b

        // If there are no arguments, return 0
        let Some(ref args) = data.arguments else {
            return 0;
        };

        // Check if cursor is before the first argument
        if args.nodes.is_empty() {
            return 0;
        }

        // Find which argument contains or precedes the cursor
        for (index, &arg_idx) in args.nodes.iter().enumerate() {
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };

            // If cursor is before this argument's start, we're between args
            // Treat it as the next argument.
            if cursor_offset < arg_node.pos {
                return index as u32;
            }

            // If cursor is within this argument's range, return this index
            if cursor_offset >= arg_node.pos && cursor_offset < arg_node.end {
                return index as u32;
            }
        }

        if let Some(&last_arg_idx) = args.nodes.last()
            && let Some(last_arg_node) = self.arena.get(last_arg_idx)
        {
            let call_end = self
                .arena
                .get(call_idx)
                .map(|node| node.end)
                .unwrap_or(cursor_offset);
            let scan_end = cursor_offset.min(call_end);
            if scan_end > last_arg_node.end && self.has_comma_between(last_arg_node.end, scan_end) {
                return args.nodes.len() as u32;
            }
        }

        // Cursor is after all arguments - return the last argument index.
        (args.nodes.len().saturating_sub(1)) as u32
    }

    fn has_comma_between(&self, start: u32, end: u32) -> bool {
        if start >= end {
            return false;
        }

        let max_len = self.source_text.len() as u32;
        let start = start.min(max_len) as usize;
        let end = end.min(max_len) as usize;
        if start >= end {
            return false;
        }

        let bytes = self.source_text.as_bytes();
        let mut i = start;
        while i < end {
            match bytes[i] {
                b',' => return true,
                b'/' if i + 1 < end && bytes[i + 1] == b'/' => {
                    i += 2;
                    while i < end && bytes[i] != b'\n' && bytes[i] != b'\r' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < end && bytes[i + 1] == b'*' => {
                    i += 2;
                    while i + 1 < end {
                        if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                            i += 2;
                            break;
                        }
                        i += 1;
                    }
                    if i + 1 >= end {
                        i = end;
                    }
                }
                _ => i += 1,
            }
        }

        false
    }

    /// Extract signature information from a TypeId.
    fn get_signatures_from_type(
        &self,
        type_id: TypeId,
        checker: &CheckerState,
        call_kind: CallKind,
    ) -> Vec<SignatureCandidate> {
        let key = match self.interner.lookup(type_id) {
            Some(k) => k,
            None => return vec![],
        };

        match key {
            // Single function signature
            TypeKey::Function(shape_id) => {
                let shape = self.interner.function_shape(shape_id);
                vec![self.signature_candidate(&shape, checker, false)]
            }
            // Overloaded signatures
            TypeKey::Callable(shape_id) => {
                let shape = self.interner.callable_shape(shape_id);
                let mut sigs = Vec::new();
                let include_call =
                    call_kind == CallKind::Call || shape.construct_signatures.is_empty();
                let include_construct =
                    call_kind == CallKind::New || shape.call_signatures.is_empty();

                if include_call {
                    // Add call signatures
                    for sig in &shape.call_signatures {
                        // Convert CallSignature to FunctionShape for formatting
                        let func_shape = FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate.clone(),
                            is_constructor: false,
                            is_method: false,
                        };
                        sigs.push(self.signature_candidate(&func_shape, checker, false));
                    }
                }
                if include_construct {
                    // Add construct signatures
                    for sig in &shape.construct_signatures {
                        let func_shape = FunctionShape {
                            type_params: sig.type_params.clone(),
                            params: sig.params.clone(),
                            this_type: sig.this_type,
                            return_type: sig.return_type,
                            type_predicate: sig.type_predicate.clone(),
                            is_constructor: true,
                            is_method: false,
                        };
                        sigs.push(self.signature_candidate(&func_shape, checker, true));
                    }
                }
                sigs
            }
            // Union of functions
            TypeKey::Union(members) => {
                let members = self.interner.type_list(members);
                let mut sigs = Vec::new();
                for &member in members.iter() {
                    sigs.extend(self.get_signatures_from_type(member, checker, call_kind));
                }
                sigs
            }
            _ => vec![],
        }
    }

    /// Format a FunctionShape into SignatureInformation
    fn format_signature(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
    ) -> SignatureInformation {
        let mut parameters = Vec::new();

        // 1. Parameters
        let mut param_labels = Vec::new();
        if let Some(this_type) = shape.this_type {
            param_labels.push(format!("this: {}", checker.format_type(this_type)));
        }

        for param in &shape.params {
            let name = param
                .name
                .map(|atom| checker.ctx.types.resolve_atom(atom))
                .unwrap_or_else(|| "arg".to_string());
            let type_str = checker.format_type(param.type_id);
            let optional = if param.optional { "?" } else { "" };
            let rest = if param.rest { "..." } else { "" };

            let param_label = format!("{}{}{}: {}", rest, name, optional, type_str);
            parameters.push(ParameterInformation {
                label: param_label.clone(),
                documentation: None,
            });

            param_labels.push(param_label);
        }

        // 3. Return Type
        let return_type_str = checker.format_type(shape.return_type);
        let prefix = if is_constructor { "new (" } else { "(" };
        let label = format!(
            "{}{}): {}",
            prefix,
            param_labels.join(", "),
            return_type_str
        );

        SignatureInformation {
            label,
            documentation: None,
            parameters,
        }
    }

    fn signature_candidate(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
    ) -> SignatureCandidate {
        let (required_params, total_params, has_rest) = self.signature_meta(&shape.params);
        let param_names = shape
            .params
            .iter()
            .map(|param| param.name.map(|atom| checker.ctx.types.resolve_atom(atom)))
            .collect();
        SignatureCandidate {
            info: self.format_signature(shape, checker, is_constructor),
            required_params,
            total_params,
            has_rest,
            param_names,
        }
    }

    fn signature_meta(&self, params: &[crate::solver::ParamInfo]) -> (usize, usize, bool) {
        let required_params = params
            .iter()
            .filter(|param| !param.optional && !param.rest)
            .count();
        let total_params = params.len();
        let has_rest = params.iter().any(|param| param.rest);
        (required_params, total_params, has_rest)
    }

    fn select_active_signature(
        &self,
        signatures: &[SignatureCandidate],
        arg_count: usize,
        active_parameter: u32,
    ) -> u32 {
        if signatures.is_empty() {
            return 0;
        }

        let desired = if arg_count == 0 {
            0
        } else {
            arg_count.max(active_parameter as usize + 1)
        };

        let mut best_idx = 0usize;
        let mut best_score = usize::MAX;
        let mut best_rest_penalty = usize::MAX;
        let mut best_total_params = usize::MAX;

        for (idx, sig) in signatures.iter().enumerate() {
            let min_params = sig.required_params;
            let max_params = if sig.has_rest {
                usize::MAX
            } else {
                sig.total_params
            };
            let score = if desired < min_params {
                min_params.saturating_sub(desired)
            } else if desired > max_params {
                desired.saturating_sub(max_params)
            } else {
                0
            };
            let rest_penalty = if sig.has_rest { 1 } else { 0 };

            if score < best_score
                || (score == best_score && rest_penalty < best_rest_penalty)
                || (score == best_score
                    && rest_penalty == best_rest_penalty
                    && sig.total_params < best_total_params)
            {
                best_idx = idx;
                best_score = score;
                best_rest_penalty = rest_penalty;
                best_total_params = sig.total_params;
            }
        }

        best_idx as u32
    }

    fn apply_signature_docs(&self, signatures: &mut [SignatureCandidate], docs: &SignatureDocs) {
        if signatures.is_empty() || docs.is_empty() {
            return;
        }

        if docs.candidates.len() == 1 {
            let doc = &docs.candidates[0].doc;
            for sig in signatures {
                self.apply_jsdoc_to_signature(sig, doc, true);
            }
            return;
        }

        if docs.candidates.is_empty() {
            if let Some(fallback) = docs.fallback.as_ref() {
                for sig in signatures {
                    self.apply_jsdoc_to_signature(sig, fallback, true);
                }
            }
            return;
        }

        let mut used = vec![false; docs.candidates.len()];
        for sig in signatures {
            if let Some(idx) = Self::match_doc_candidate(sig, &docs.candidates, &mut used) {
                let doc = &docs.candidates[idx].doc;
                self.apply_jsdoc_to_signature(sig, doc, true);
            } else if let Some(fallback) = docs.fallback.as_ref() {
                self.apply_jsdoc_to_signature(sig, fallback, false);
            }
        }
    }

    fn apply_jsdoc_to_signature(
        &self,
        sig: &mut SignatureCandidate,
        parsed: &ParsedJsdoc,
        overwrite: bool,
    ) {
        if overwrite || sig.info.documentation.is_none() {
            sig.info.documentation = parsed.summary.clone();
        }

        for (idx, name) in sig.param_names.iter().enumerate() {
            let Some(name) = name else {
                continue;
            };
            let Some(param_doc) = parsed.params.get(name) else {
                continue;
            };
            if let Some(param_info) = sig.info.parameters.get_mut(idx)
                && (overwrite || param_info.documentation.is_none())
            {
                param_info.documentation = Some(param_doc.clone());
            }
        }
    }

    fn match_doc_candidate(
        sig: &SignatureCandidate,
        candidates: &[SignatureDocCandidate],
        used: &mut [bool],
    ) -> Option<usize> {
        for (idx, candidate) in candidates.iter().enumerate() {
            if used[idx] {
                continue;
            }
            if candidate.required_params == sig.required_params
                && candidate.total_params == sig.total_params
                && candidate.has_rest == sig.has_rest
            {
                used[idx] = true;
                return Some(idx);
            }
        }
        None
    }

    fn signature_documentation_for_symbol(
        &self,
        root: NodeIndex,
        symbol_id: crate::binder::SymbolId,
        call_kind: CallKind,
    ) -> Option<SignatureDocs> {
        let symbol = self.binder.get_symbol(symbol_id)?;
        let mut decls = symbol.declarations.clone();
        if !symbol.value_declaration.is_none() && !decls.contains(&symbol.value_declaration) {
            decls.insert(0, symbol.value_declaration);
        }

        let mut candidates = Vec::new();
        let mut fallback = None;

        for decl in decls {
            if decl.is_none() {
                continue;
            }
            if call_kind == CallKind::New {
                self.collect_constructor_docs_from_class(
                    root,
                    decl,
                    &mut candidates,
                    &mut fallback,
                );
            }
            let doc = jsdoc_for_node(self.arena, root, decl, self.source_text);
            if doc.is_empty() {
                continue;
            }
            let parsed = parse_jsdoc(&doc);
            if parsed.is_empty() {
                continue;
            }

            if let Some((required_params, total_params, has_rest)) =
                self.signature_meta_from_decl(decl)
            {
                candidates.push(SignatureDocCandidate {
                    doc: parsed,
                    required_params,
                    total_params,
                    has_rest,
                });
            } else if fallback.is_none() {
                fallback = Some(parsed);
            }
        }

        let docs = SignatureDocs {
            candidates,
            fallback,
        };
        if docs.is_empty() { None } else { Some(docs) }
    }

    fn collect_constructor_docs_from_class(
        &self,
        root: NodeIndex,
        decl: NodeIndex,
        candidates: &mut Vec<SignatureDocCandidate>,
        fallback: &mut Option<ParsedJsdoc>,
    ) {
        let Some(node) = self.arena.get(decl) else {
            return;
        };
        let Some(class_data) = self.arena.get_class(node) else {
            return;
        };

        for &member in class_data.members.nodes.iter() {
            let Some(member_node) = self.arena.get(member) else {
                continue;
            };
            if self.arena.get_constructor(member_node).is_none() {
                continue;
            }

            let doc = jsdoc_for_node(self.arena, root, member, self.source_text);
            if doc.is_empty() {
                continue;
            }
            let parsed = parse_jsdoc(&doc);
            if parsed.is_empty() {
                continue;
            }

            if let Some((required_params, total_params, has_rest)) =
                self.signature_meta_from_decl(member)
            {
                candidates.push(SignatureDocCandidate {
                    doc: parsed,
                    required_params,
                    total_params,
                    has_rest,
                });
            } else if fallback.is_none() {
                *fallback = Some(parsed);
            }
        }
    }

    fn signature_documentation_for_property_access(
        &self,
        root: NodeIndex,
        access_idx: NodeIndex,
    ) -> Option<SignatureDocs> {
        let Some(access_node) = self.arena.get(access_idx) else {
            return None;
        };
        let Some(access) = self.arena.get_access_expr(access_node) else {
            return None;
        };
        let property_name = self
            .arena
            .get_identifier_text(access.name_or_argument)
            .or_else(|| self.arena.get_literal_text(access.name_or_argument))?;

        let (class_decls, static_only) = if let Some(result) =
            self.class_decls_for_expression(access.expression)
        {
            result
        } else if let Some(decls) = self.class_decls_for_property_name_in_file(root, property_name)
        {
            (decls, false)
        } else {
            return None;
        };
        let mut candidates = Vec::new();
        let mut fallback = None;

        for class_decl in class_decls {
            let Some(class_node) = self.arena.get(class_decl) else {
                continue;
            };
            let Some(class_data) = self.arena.get_class(class_node) else {
                continue;
            };

            for &member in class_data.members.nodes.iter() {
                let Some(member_node) = self.arena.get(member) else {
                    continue;
                };
                let Some(method) = self.arena.get_method_decl(member_node) else {
                    continue;
                };
                let Some(member_name) = self
                    .arena
                    .get_identifier_text(method.name)
                    .or_else(|| self.arena.get_literal_text(method.name))
                else {
                    continue;
                };
                if member_name != property_name {
                    continue;
                }

                let is_static = self.is_static_method(method);
                if static_only && !is_static {
                    continue;
                }
                if !static_only && is_static {
                    continue;
                }

                let doc = jsdoc_for_node(self.arena, root, member, self.source_text);
                if doc.is_empty() {
                    continue;
                }
                let parsed = parse_jsdoc(&doc);
                if parsed.is_empty() {
                    continue;
                }

                if let Some((required_params, total_params, has_rest)) =
                    self.signature_meta_from_decl(member)
                {
                    candidates.push(SignatureDocCandidate {
                        doc: parsed,
                        required_params,
                        total_params,
                        has_rest,
                    });
                } else if fallback.is_none() {
                    fallback = Some(parsed);
                }
            }
        }

        let docs = SignatureDocs {
            candidates,
            fallback,
        };
        if docs.is_empty() { None } else { Some(docs) }
    }

    fn class_decls_for_expression(&self, expr: NodeIndex) -> Option<(Vec<NodeIndex>, bool)> {
        let Some(expr_node) = self.arena.get(expr) else {
            return None;
        };
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            let sym_id = self.resolve_symbol_for_identifier(expr)?;
            return self.class_decls_for_symbol(sym_id);
        }
        if expr_node.kind == syntax_kind_ext::NEW_EXPRESSION {
            let decls = self.class_decls_from_new_expression(expr);
            if !decls.is_empty() {
                return Some((decls, false));
            }
        }
        None
    }

    fn class_decls_for_symbol(
        &self,
        sym_id: crate::binder::SymbolId,
    ) -> Option<(Vec<NodeIndex>, bool)> {
        let symbol = self.binder.get_symbol(sym_id)?;
        if symbol.flags & symbol_flags::CLASS != 0 {
            let decls = self.class_decls_from_symbol(sym_id);
            if decls.is_empty() {
                None
            } else {
                Some((decls, true))
            }
        } else if symbol.flags
            & (symbol_flags::BLOCK_SCOPED_VARIABLE | symbol_flags::FUNCTION_SCOPED_VARIABLE)
            != 0
        {
            let decls = self.class_decls_from_variable_symbol(symbol);
            if decls.is_empty() {
                None
            } else {
                Some((decls, false))
            }
        } else {
            None
        }
    }

    fn class_decls_from_symbol(&self, sym_id: crate::binder::SymbolId) -> Vec<NodeIndex> {
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return Vec::new();
        };
        let mut decls = symbol.declarations.clone();
        if !symbol.value_declaration.is_none() && !decls.contains(&symbol.value_declaration) {
            decls.push(symbol.value_declaration);
        }

        let mut class_decls = Vec::new();
        for decl in decls {
            if decl.is_none() {
                continue;
            }
            let Some(node) = self.arena.get(decl) else {
                continue;
            };
            if self.arena.get_class(node).is_some() {
                class_decls.push(decl);
            }
        }
        class_decls
    }

    fn class_decls_from_variable_symbol(&self, symbol: &crate::binder::Symbol) -> Vec<NodeIndex> {
        let mut decls = Vec::new();
        let decl_idx = symbol.value_declaration;
        if decl_idx.is_none() {
            return decls;
        }
        let Some(node) = self.arena.get(decl_idx) else {
            return decls;
        };
        let Some(var_decl) = self.arena.get_variable_declaration(node) else {
            return decls;
        };
        if !var_decl.initializer.is_none() {
            decls.extend(self.class_decls_from_new_expression(var_decl.initializer));
        }
        decls
    }

    fn class_decls_from_new_expression(&self, expr: NodeIndex) -> Vec<NodeIndex> {
        let Some(node) = self.arena.get(expr) else {
            return Vec::new();
        };
        if node.kind != syntax_kind_ext::NEW_EXPRESSION {
            return Vec::new();
        }
        let Some(call) = self.arena.get_call_expr(node) else {
            return Vec::new();
        };
        let callee_idx = call.expression;
        let Some(callee_node) = self.arena.get(callee_idx) else {
            return Vec::new();
        };
        if callee_node.kind != SyntaxKind::Identifier as u16 {
            return Vec::new();
        }
        let Some(sym_id) = self.resolve_symbol_for_identifier(callee_idx) else {
            return Vec::new();
        };
        self.class_decls_from_symbol(sym_id)
    }

    fn class_decls_for_property_name_in_file(
        &self,
        root: NodeIndex,
        property_name: &str,
    ) -> Option<Vec<NodeIndex>> {
        let root_node = self.arena.get(root)?;
        let sf = self.arena.get_source_file(root_node)?;
        let mut matches = Vec::new();

        for &stmt in sf.statements.nodes.iter() {
            let Some(node) = self.arena.get(stmt) else {
                continue;
            };
            let Some(class_data) = self.arena.get_class(node) else {
                continue;
            };
            if self.class_has_method_named(class_data, property_name) {
                matches.push(stmt);
                if matches.len() > 1 {
                    return None;
                }
            }
        }

        if matches.is_empty() {
            None
        } else {
            Some(matches)
        }
    }

    fn class_has_method_named(
        &self,
        class_data: &crate::parser::node::ClassData,
        property_name: &str,
    ) -> bool {
        for &member in class_data.members.nodes.iter() {
            let Some(member_node) = self.arena.get(member) else {
                continue;
            };
            let Some(method) = self.arena.get_method_decl(member_node) else {
                continue;
            };
            let Some(member_name) = self
                .arena
                .get_identifier_text(method.name)
                .or_else(|| self.arena.get_literal_text(method.name))
            else {
                continue;
            };
            if member_name == property_name {
                return true;
            }
        }
        false
    }

    fn is_static_method(&self, method: &crate::parser::node::MethodDeclData) -> bool {
        let Some(modifiers) = method.modifiers.as_ref() else {
            return false;
        };
        for &mod_idx in modifiers.nodes.iter() {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                return true;
            }
        }
        false
    }

    fn resolve_symbol_for_identifier(
        &self,
        ident_idx: NodeIndex,
    ) -> Option<crate::binder::SymbolId> {
        self.binder
            .resolve_identifier(self.arena, ident_idx)
            .or_else(|| {
                let name = self.arena.get_identifier_text(ident_idx)?;
                self.binder.file_locals.get(name)
            })
            .or_else(|| {
                let name = self.arena.get_identifier_text(ident_idx)?;
                self.binder.get_symbols().find_by_name(name)
            })
    }

    fn signature_meta_from_decl(&self, decl: NodeIndex) -> Option<(usize, usize, bool)> {
        let node = self.arena.get(decl)?;
        if let Some(func) = self.arena.get_function(node) {
            return self.signature_meta_from_params(&func.parameters);
        }
        if let Some(method) = self.arena.get_method_decl(node) {
            return self.signature_meta_from_params(&method.parameters);
        }
        if let Some(ctor) = self.arena.get_constructor(node) {
            return self.signature_meta_from_params(&ctor.parameters);
        }
        None
    }

    fn signature_meta_from_params(&self, params: &NodeList) -> Option<(usize, usize, bool)> {
        let mut required_params = 0;
        let mut total_params = 0;
        let mut has_rest = false;

        for &param_idx in params.nodes.iter() {
            let Some(param_node) = self.arena.get(param_idx) else {
                continue;
            };
            let Some(param_data) = self.arena.get_parameter(param_node) else {
                continue;
            };
            if let Some(name_node) = self.arena.get(param_data.name) {
                if name_node.kind == SyntaxKind::ThisKeyword as u16 {
                    continue;
                }
                if let Some(ident) = self.arena.get_identifier(name_node)
                    && ident.escaped_text == "this"
                {
                    continue;
                }
            }

            total_params += 1;
            if param_data.dot_dot_dot_token {
                has_rest = true;
                continue;
            }
            if !param_data.question_token && param_data.initializer.is_none() {
                required_params += 1;
            }
        }

        Some((required_params, total_params, has_rest))
    }
}

#[cfg(test)]
#[path = "tests/signature_help_tests.rs"]
mod signature_help_tests;
