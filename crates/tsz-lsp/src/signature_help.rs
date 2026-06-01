//! Signature Help implementation for LSP.
//!
//! Provides function signature information and active parameter highlighting
//! when typing arguments in a call expression.

use rustc_hash::FxHashMap;

use crate::jsdoc::{JsdocTag, ParsedJsdoc, inline_param_jsdocs, jsdoc_for_node, parse_jsdoc};
use crate::resolver::{ScopeCache, ScopeCacheStats};
use crate::utils::find_node_at_or_before_offset;
use tsz_binder::symbol_flags;
use tsz_checker::state::CheckerState;
use tsz_common::position::Position;
use tsz_parser::parser::node::{CallExprData, NodeAccess};
use tsz_parser::{
    NodeIndex, NodeList, count_top_level_commas, find_incomplete_angle_call,
    find_incomplete_paren_call, has_comma_between_offsets, syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;
use tsz_solver::{
    FunctionShape, ParamInfo, TypeId, TypePredicateTarget, apparent_intrinsic_kind, visitor,
};

use crate::intrinsic_params::{
    IntrinsicParamSpec, IntrinsicParamTypeHint, bigint_intrinsic_method_params,
    boolean_intrinsic_method_params, number_intrinsic_method_params,
    string_intrinsic_method_params,
};
mod contextual;
mod docs;
mod selection;
mod shapes;
#[cfg(test)]
#[path = "signature_help/internal_tests.rs"]
mod signature_help_internal_tests;

#[cfg(test)]
fn parse_test_source(source: &str) -> (tsz_parser::ParserState, tsz_parser::parser::NodeIndex) {
    let mut parser = tsz_parser::ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();
    (parser, root)
}

/// Represents a parameter in a signature.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParameterInformation {
    /// The name of this parameter (e.g., "x")
    pub name: String,
    /// The display label of this parameter (e.g., "x: number")
    pub label: String,
    /// The documentation for this parameter
    pub documentation: Option<String>,
    /// Whether this parameter is optional
    pub is_optional: bool,
    /// Whether this parameter is a rest parameter
    pub is_rest: bool,
}

/// Represents a single signature (overload).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SignatureInformation {
    /// The full label of the signature (e.g., "add(x: number, y: number): number")
    pub label: String,
    /// The prefix display text (e.g., "add(" or "add<T>(")
    pub prefix: String,
    /// The suffix display text (e.g., "): number")
    pub suffix: String,
    /// The documentation for this signature
    pub documentation: Option<String>,
    /// The parameters of this signature
    pub parameters: Vec<ParameterInformation>,
    /// Whether this signature is variadic (has rest parameter)
    pub is_variadic: bool,
    /// Whether this is a constructor signature (affects display part kinds)
    pub is_constructor: bool,
    /// `JSDoc` tags (non-param tags like @returns, @mytag, etc.)
    pub tags: Vec<JsdocTag>,
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
    /// The total number of arguments at the call site
    pub argument_count: u32,
    /// The byte offset of the applicable span start (after opening delimiter)
    pub applicable_span_start: u32,
    /// The length of the applicable span
    pub applicable_span_length: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CallKind {
    Call,
    New,
    TaggedTemplate,
}

/// Abstraction over regular calls and tagged template expressions.
enum CallSite<'a> {
    Regular(&'a CallExprData),
    TaggedTemplate(&'a tsz_parser::parser::node::TaggedTemplateData),
}

impl<'a> CallSite<'a> {
    const fn expression(&self) -> NodeIndex {
        match self {
            CallSite::Regular(data) => data.expression,
            CallSite::TaggedTemplate(data) => data.tag,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrimitiveKind {
    String,
    Number,
    Boolean,
    BigInt,
}

#[derive(Clone)]
struct SignatureCandidate {
    info: SignatureInformation,
    required_params: usize,
    total_params: usize,
    has_rest: bool,
    param_names: Vec<Option<String>>,
    type_params: Vec<String>,
    /// Type parameter (name, substitution) pairs from the function signature,
    /// used for substitution when no explicit type arguments are provided at
    /// the call site. The substitution is the default type, constraint type,
    /// or "unknown" (in that priority order).
    type_param_substitutions: Vec<(String, String)>,
}

#[derive(Clone, Copy)]
struct TypeArgumentContext {
    active_parameter: u32,
    span_start: u32,
    span_length: u32,
}

struct TextualTypeArgumentTrigger {
    callee_name: String,
    callee_offset: u32,
    call_kind: CallKind,
    active_parameter: u32,
    span_start: u32,
    span_length: u32,
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
    const fn is_empty(&self) -> bool {
        self.candidates.is_empty() && self.fallback.is_none()
    }
}

define_lsp_provider!(full SignatureHelpProvider, "Signature help provider.");

impl<'a> SignatureHelpProvider<'a> {
    fn is_js_like_file(&self) -> bool {
        self.file_name.ends_with(".js")
            || self.file_name.ends_with(".jsx")
            || self.file_name.ends_with(".mjs")
            || self.file_name.ends_with(".cjs")
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
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        self.get_signature_help_internal(root, position, type_cache, None, None)
    }

    pub fn get_signature_help_with_scope_cache(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
        scope_cache: &mut ScopeCache,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SignatureHelp> {
        self.get_signature_help_internal(root, position, type_cache, Some(scope_cache), scope_stats)
    }

    fn get_signature_help_internal(
        &self,
        root: NodeIndex,
        position: Position,
        type_cache: &mut Option<tsz_checker::TypeCache>,
        scope_cache: Option<&mut ScopeCache>,
        scope_stats: Option<&mut ScopeCacheStats>,
    ) -> Option<SignatureHelp> {
        let offset = self
            .line_map
            .position_to_offset(position, self.source_text)?;

        // In incomplete generic invocations like `foo(bar<|)`, the parser may
        // bind us to an outer call expression. Prefer explicit textual
        // type-argument handling when we can detect an unclosed `<...` span.
        if self.find_textual_type_argument_trigger(offset).is_some()
            && let Some(help) =
                self.signature_help_for_textual_type_arguments(root, offset, type_cache)
        {
            return Some(help);
        }

        // 1. Find the deepest node at the cursor
        let leaf_node = find_node_at_or_before_offset(self.arena, offset, self.source_text);

        // 2. Walk up to find the nearest CallExpression, NewExpression, or TaggedTemplateExpression
        let Some((call_node_idx, call_site, call_kind)) =
            self.find_containing_call(leaf_node, offset)
        else {
            if let Some(help) = self.signature_help_for_contextual_variable_initializer(
                root, leaf_node, offset, type_cache,
            ) {
                return Some(help);
            }
            if let Some(help) = self.signature_help_for_textual_call(root, offset, type_cache) {
                return Some(help);
            }
            return self.signature_help_for_textual_type_arguments(root, offset, type_cache);
        };
        let type_argument_context = match &call_site {
            CallSite::Regular(data) => {
                self.type_argument_context_for_call(call_node_idx, data, offset)
            }
            CallSite::TaggedTemplate(_) => None,
        };
        let in_type_argument_list = type_argument_context.is_some();

        // 3. Determine active parameter
        let mut active_parameter = if let Some(ctx) = type_argument_context {
            ctx.active_parameter
        } else {
            match &call_site {
                CallSite::Regular(call_expr) => {
                    self.determine_active_parameter(call_node_idx, call_expr, offset)
                }
                CallSite::TaggedTemplate(tagged) => {
                    self.determine_tagged_template_active_param(tagged, offset)
                }
            }
        };

        let callee_expr = call_site.expression();

        // 4. Check if this is a super() call — need special handling
        let is_super_call = self
            .arena
            .get(callee_expr)
            .is_some_and(|n| n.kind == SyntaxKind::SuperKeyword as u16);

        // 5. Resolve the symbol being called using ScopeWalker
        let mut walker = crate::resolver::ScopeWalker::new(self.arena, self.binder);
        let symbol_id = if is_super_call {
            // For super(), resolve the base class expression instead
            self.find_base_class_expression(callee_expr)
                .and_then(|base_expr| walker.resolve_node(root, base_expr))
        } else if let Some(scope_cache) = scope_cache {
            walker.resolve_node_cached(root, callee_expr, scope_cache, scope_stats)
        } else {
            walker.resolve_node(root, callee_expr)
        };

        // 6. Create checker with persistent cache if available
        let compiler_options = self.checker_options();
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
        self.apply_lib_contexts(&mut checker);

        let access_docs = if call_kind == CallKind::Call {
            self.signature_documentation_for_property_access(root, callee_expr)
        } else {
            None
        };

        // Interfaces and type aliases are type-only declarations — don't provide
        // signature help when they're used as call targets (e.g. `C()`).
        if let Some(symbol_id) = symbol_id
            && let Some(symbol) = self.binder.get_symbol(symbol_id)
            && symbol.flags & symbol_flags::INTERFACE != 0
            && symbol.flags & symbol_flags::VALUE == 0
        {
            return None;
        }

        let (callee_type, docs) = if let Some(symbol_id) = symbol_id {
            (
                checker.get_type_of_symbol(symbol_id),
                access_docs.or_else(|| {
                    self.signature_documentation_for_symbol(root, symbol_id, call_kind)
                }),
            )
        } else {
            (checker.get_type_of_node(callee_expr), access_docs)
        };
        let callee_type = checker.resolve_lazy_type(callee_type);
        if !in_type_argument_list
            && call_kind == CallKind::Call
            && let CallSite::Regular(call_expr) = &call_site
            && let Some(help) = self.contextual_signature_help_from_call_argument(
                call_expr,
                offset,
                callee_type,
                &checker,
            )
        {
            *type_cache = Some(checker.extract_cache());
            return Some(help);
        }
        // `new` on private/protected constructors should not offer signature help
        // from out-of-scope locations.
        if call_kind == CallKind::New
            && !is_super_call
            && (checker.is_private_ctor(callee_type) || checker.is_protected_ctor(callee_type))
        {
            *type_cache = Some(checker.extract_cache());
            return None;
        }

        // 6. Resolve the callee name for display
        let callee_name = if is_super_call {
            // For super(), use the base class name from the extends clause
            self.find_base_class_expression(callee_expr)
                .and_then(|base_expr| {
                    self.arena
                        .get_identifier_text(base_expr)
                        .map(|s| s.to_string())
                })
                .unwrap_or_else(|| "super".to_string())
        } else {
            self.resolve_callee_name(callee_expr, call_kind)
        };

        // 7. Extract signatures from the type
        // For super() calls, extract construct signatures (since super invokes the base constructor)
        let effective_call_kind = if is_super_call {
            CallKind::New
        } else {
            call_kind
        };
        let has_explicit_type_args = if in_type_argument_list {
            false
        } else {
            match &call_site {
                CallSite::Regular(data) => data.type_arguments.is_some(),
                CallSite::TaggedTemplate(_) => false,
            }
        };
        // Extract source text for each explicit type argument node
        let explicit_type_arg_texts: Vec<String> = if has_explicit_type_args {
            if let CallSite::Regular(data) = &call_site {
                if let Some(ref type_args) = data.type_arguments {
                    type_args
                        .nodes
                        .iter()
                        .map(|&node_idx| {
                            if let Some(node) = self.arena.get(node_idx) {
                                let start = node.pos as usize;
                                let end = (node.end as usize).min(self.source_text.len());
                                if start < end {
                                    self.source_text[start..end].trim().to_string()
                                } else {
                                    "unknown".to_string()
                                }
                            } else {
                                "unknown".to_string()
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        // For primitive intrinsic methods resolved via the no-lib fallback the type
        // system synthesizes `(...args: any[]) => ReturnType`.  Try to build directly
        // from the intrinsic parameter table first so we never pay the cost of
        // `get_signatures_from_type` when the result would be discarded.
        let intrinsic_sigs = self.try_build_intrinsic_signatures(
            callee_expr,
            callee_type,
            &mut checker,
            &callee_name,
            has_explicit_type_args,
            &explicit_type_arg_texts,
        );
        let mut signatures = if let Some(sigs) = intrinsic_sigs {
            sigs
        } else {
            self.get_signatures_from_type(
                callee_type,
                &checker,
                effective_call_kind,
                &callee_name,
                has_explicit_type_args,
                &explicit_type_arg_texts,
            )
        };

        if let Some(docs) = docs {
            self.apply_signature_docs(&mut signatures, &docs);
        }
        if let Some(symbol_id) = symbol_id {
            self.apply_source_signature_type_overrides(&mut signatures, symbol_id);
        }

        // Substitute type parameter names in the displayed signature. This must
        // happen after apply_source_signature_type_overrides since that can
        // overwrite labels with raw source text containing type parameter names.
        // When explicit type arguments are provided, we substitute with the
        // actual type argument text; otherwise we use defaults/constraints/unknown.
        let supplied_argument_types = self.argument_type_texts(&call_site, &mut checker);

        if !in_type_argument_list {
            self.infer_type_param_substitutions_from_arguments(
                &mut signatures,
                &supplied_argument_types,
            );
            for sig in &mut signatures {
                if !sig.type_param_substitutions.is_empty() {
                    apply_type_param_substitution(&mut sig.info, &sig.type_param_substitutions);
                }
            }
        }
        if let Some(symbol_id) = symbol_id
            && !in_type_argument_list
        {
            self.expand_source_rest_tuple_union_signatures(&mut signatures, symbol_id);
        }
        if in_type_argument_list {
            self.rewrite_signatures_for_type_arguments(
                &mut signatures,
                &callee_name,
                active_parameter,
            );
        }

        // Extract and save the updated cache for future queries
        *type_cache = Some(checker.extract_cache());

        if signatures.is_empty() {
            if let Some(help) = self.signature_help_for_textual_call(root, offset, type_cache) {
                return Some(help);
            }
            if let Some(help) =
                self.signature_help_for_textual_type_arguments(root, offset, type_cache)
            {
                return Some(help);
            }
            return None;
        }

        let arg_count = if in_type_argument_list {
            0
        } else {
            match &call_site {
                CallSite::Regular(call_expr) => call_expr.arguments.as_ref().map_or(0, |args| {
                    args.nodes
                        .iter()
                        .filter(|&&arg_idx| {
                            self.arena.get(arg_idx).is_some_and(|node| {
                                node.kind != syntax_kind_ext::OMITTED_EXPRESSION
                            })
                        })
                        .count()
                }),
                CallSite::TaggedTemplate(tagged) => {
                    // For tagged templates, arg count = 1 (templateStrings) + number of ${} expressions
                    if let Some(tmpl_node) = self.arena.get(tagged.template) {
                        if let Some(tmpl_expr) = self.arena.get_template_expr(tmpl_node) {
                            1 + tmpl_expr.template_spans.nodes.len()
                        } else {
                            1 // NoSubstitutionTemplateLiteral = just templateStrings
                        }
                    } else {
                        1
                    }
                }
            }
        };
        let active_signature = self.select_active_signature(
            &signatures,
            arg_count,
            active_parameter,
            &supplied_argument_types,
        );
        if let Some(selected) = signatures.get(active_signature as usize) {
            if selected.info.parameters.is_empty() {
                active_parameter = 0;
            } else {
                let has_rest_param = selected.info.parameters.iter().any(|param| param.is_rest);
                let max_index = selected.info.parameters.len().saturating_sub(1);
                if has_rest_param {
                    // Keep active_parameter advancing across concrete rest arguments,
                    // but clamp trailing-comma empty slots back to the rest parameter.
                    if active_parameter as usize >= arg_count
                        && active_parameter as usize > max_index
                    {
                        active_parameter = max_index as u32;
                    }
                } else if active_parameter as usize > max_index {
                    active_parameter = max_index as u32;
                }
            }
        }

        // Compute applicable span (byte offsets for the argument region)
        let (span_start, span_length) = if let Some(ctx) = type_argument_context {
            (ctx.span_start, ctx.span_length)
        } else {
            match &call_site {
                CallSite::Regular(call_expr) => {
                    self.compute_applicable_span(call_node_idx, call_expr)
                }
                CallSite::TaggedTemplate(tagged) => {
                    // For tagged templates, span covers the template
                    if let Some(tmpl_node) = self.arena.get(tagged.template) {
                        let tmpl_start = tmpl_node.pos as usize;
                        let tmpl_end = (tmpl_node.end as usize).min(self.source_text.len());
                        let tmpl_text = &self.source_text[tmpl_start..tmpl_end];
                        if let Some(bt) = tmpl_text.find('`') {
                            ((tmpl_start + bt + 1) as u32, 0)
                        } else {
                            (tmpl_node.pos, 0)
                        }
                    } else {
                        (offset, 0)
                    }
                }
            }
        };

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: arg_count as u32,
            applicable_span_start: span_start,
            applicable_span_length: span_length,
        })
    }

    /// Resolve the name of the callee for display in signature help.
    /// For `foo(...)` returns "foo", for `obj.method(...)` returns "method",
    /// for `new Foo(...)` returns "Foo".
    fn resolve_callee_name(&self, expr_idx: NodeIndex, _call_kind: CallKind) -> String {
        // Try to get identifier text directly (handles simple identifiers)
        if let Some(name) = self.arena.get_identifier_text(expr_idx)
            && !name.is_empty()
        {
            return name.to_string();
        }
        if let Some(node) = self.arena.get(expr_idx) {
            // Property access: obj.method(...)
            if let Some(access) = self.arena.get_access_expr(node) {
                if let Some(name) = self.arena.get_identifier_text(access.name_or_argument)
                    && !name.is_empty()
                {
                    return name.to_string();
                }
                // Source text fallback for property name
                if let Some(pn) = self.arena.get(access.name_or_argument) {
                    let s = pn.pos as usize;
                    let e = pn.end as usize;
                    if s < e && e <= self.source_text.len() {
                        let text = self.source_text[s..e].trim();
                        if !text.is_empty()
                            && text
                                .chars()
                                .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                        {
                            return text.to_string();
                        }
                    }
                }
            }
        }
        // Fallback: try to extract name from source text
        if let Some(node) = self.arena.get(expr_idx) {
            let start = node.pos as usize;
            let end = node.end as usize;
            if start < end && end <= self.source_text.len() {
                let text = &self.source_text[start..end];
                // For dotted access, take the last segment
                if let Some(dot_pos) = text.rfind('.') {
                    let name = text[dot_pos + 1..].trim();
                    if !name.is_empty()
                        && name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    {
                        return name.to_string();
                    }
                }
                // For simple identifier, use the whole text
                let trimmed = text.trim();
                if !trimmed.is_empty()
                    && trimmed
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                {
                    return trimmed.to_string();
                }
            }
        }
        String::new()
    }

    /// Walk up the AST to find the call expression or tagged template containing the cursor.
    fn find_containing_call(
        &self,
        start_node: NodeIndex,
        cursor_offset: u32,
    ) -> Option<(NodeIndex, CallSite<'a>, CallKind)> {
        let mut current = start_node;

        // Safety limit to prevent infinite loops
        let mut depth = 0;
        while current.is_some() && depth < 100 {
            if let Some(node) = self.arena.get(current) {
                if (node.kind == syntax_kind_ext::CALL_EXPRESSION
                    || node.kind == syntax_kind_ext::NEW_EXPRESSION)
                    && let Some(data) = self.arena.get_call_expr(node)
                {
                    // Only provide signature help if cursor is after the opening
                    // `(` or `<` of the call. We find the delimiter by scanning
                    // the source text within the call node range.
                    let call_start = node.pos as usize;
                    let call_end = (node.end as usize).min(self.source_text.len());
                    let call_text = &self.source_text[call_start..call_end];
                    let delimiter = if data.type_arguments.is_some() {
                        call_text.find('<').or_else(|| call_text.find('('))
                    } else {
                        call_text.find('(').or_else(|| call_text.find('<'))
                    };
                    if let Some(delim_offset) = delimiter {
                        let delim_pos = (call_start + delim_offset) as u32;
                        if cursor_offset > delim_pos
                            && !self.cursor_after_closed_call_delimiter(
                                call_start,
                                call_text,
                                delim_offset,
                                cursor_offset,
                            )
                        {
                            let kind = if node.kind == syntax_kind_ext::NEW_EXPRESSION {
                                CallKind::New
                            } else {
                                CallKind::Call
                            };
                            return Some((current, CallSite::Regular(data), kind));
                        }
                    }
                }

                // Check for tagged template expression
                if node.kind == syntax_kind_ext::TAGGED_TEMPLATE_EXPRESSION
                    && let Some(data) = self.arena.get_tagged_template(node)
                {
                    // Cursor must be strictly inside the template backticks.
                    // tmpl_node.pos may include leading trivia, so find the
                    // actual opening backtick position in the source text.
                    if let Some(tmpl_node) = self.arena.get(data.template) {
                        let tmpl_start = tmpl_node.pos as usize;
                        let tmpl_end = (tmpl_node.end as usize).min(self.source_text.len());
                        let tmpl_text = &self.source_text[tmpl_start..tmpl_end];
                        if let Some(backtick_rel) = tmpl_text.find('`') {
                            let backtick_pos = (tmpl_start + backtick_rel) as u32;
                            // Cursor must be strictly after opening backtick
                            // and strictly before closing backtick.
                            // For incomplete templates (missing closing backtick),
                            // the parser sets tmpl_node.end before the cursor,
                            // so relax the upper bound check.
                            let template_incomplete = tmpl_end <= tmpl_start
                                || self.source_text.as_bytes()[tmpl_end - 1] != b'`';
                            if cursor_offset > backtick_pos
                                && (template_incomplete || cursor_offset < tmpl_node.end)
                            {
                                return Some((
                                    current,
                                    CallSite::TaggedTemplate(data),
                                    CallKind::TaggedTemplate,
                                ));
                            }
                        }
                    }
                }

                // Stop at function boundaries — if the cursor is inside a nested
                // function body (arrow, function expression, method), don't provide
                // signature help for the outer call expression.
                if node.kind == syntax_kind_ext::ARROW_FUNCTION
                    || node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                    || node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                {
                    // Only stop if the cursor is inside a multi-line function BODY.
                    // For single-line bodies like `foo(() => {/**/})`, still show
                    // signature help since the user is effectively still at the argument.
                    if let Some(fn_data) = self.arena.get_function(node)
                        && let Some(body_node) = self.arena.get(fn_data.body)
                        && cursor_offset >= body_node.pos
                        && cursor_offset <= body_node.end
                    {
                        let body_text =
                            &self.source_text[body_node.pos as usize..body_node.end as usize];
                        if body_text.contains('\n') {
                            return None;
                        }
                    }
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

    fn cursor_after_closed_call_delimiter(
        &self,
        call_start: usize,
        call_text: &str,
        open_rel: usize,
        cursor_offset: u32,
    ) -> bool {
        let bytes = call_text.as_bytes();
        if open_rel >= bytes.len() || bytes[open_rel] != b'(' {
            return false;
        }

        let mut depth = 1i32;
        let mut cursor = open_rel + 1;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let close_pos = (call_start + cursor) as u32;
                        return cursor_offset > close_pos;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }

        false
    }

    /// Compute the applicable span for a regular call expression.
    /// Returns (`start_offset`, length) as byte offsets in the source text.
    fn compute_applicable_span(&self, call_idx: NodeIndex, data: &CallExprData) -> (u32, u32) {
        let call_node = match self.arena.get(call_idx) {
            Some(n) => n,
            None => return (0, 0),
        };
        let call_start = call_node.pos as usize;
        let call_end = (call_node.end as usize).min(self.source_text.len());
        let call_text = &self.source_text[call_start..call_end];

        // Find opening paren
        let paren_rel = match call_text.find('(') {
            Some(p) => p,
            None => return (call_node.pos, 0),
        };
        let after_paren = (call_start + paren_rel + 1) as u32;

        // If there are arguments, span from after '(' to before ')'
        if let Some(ref args) = data.arguments
            && !args.nodes.is_empty()
        {
            let first_start = args
                .nodes
                .first()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(after_paren, |n| n.pos);
            let last_end = args
                .nodes
                .last()
                .and_then(|&idx| self.arena.get(idx))
                .map_or(after_paren, |n| n.end);
            return (first_start, last_end.saturating_sub(first_start));
        }

        // No arguments - zero-length span at after-paren position
        (after_paren, 0)
    }

    /// Determine the active parameter for a tagged template expression.
    ///
    /// For tagged templates like ``tag`text ${expr1} text ${expr2} text``:
    /// - Parameter 0 is always the templateStrings array
    /// - Parameter N (1-based) corresponds to the Nth ${} expression
    /// - Cursor in static template text maps to parameter 0
    /// - Cursor inside ${expr} maps to the corresponding parameter index
    fn determine_tagged_template_active_param(
        &self,
        tagged: &tsz_parser::parser::node::TaggedTemplateData,
        cursor_offset: u32,
    ) -> u32 {
        let Some(tmpl_node) = self.arena.get(tagged.template) else {
            return 0;
        };

        // If the template is a NoSubstitutionTemplateLiteral, active param is always 0
        let Some(tmpl_expr) = self.arena.get_template_expr(tmpl_node) else {
            return 0;
        };

        // Use head/literal boundaries to determine active parameter.
        // The head token covers `text${` - cursor before head.end is in template text (param 0).
        // Each span's literal covers `}text${` or `}text` - cursor in literal is in template text (param 0).
        // Everything between head.end and span[i].literal.pos is the expression area (param i+1).
        // This avoids gaps caused by trivia between AST node boundaries.
        let Some(head_node) = self.arena.get(tmpl_expr.head) else {
            return 0;
        };

        // Cursor in head (before the first ${) → param 0 (templateStrings)
        if cursor_offset < head_node.end {
            return 0;
        }

        // Walk spans: region from head.end/prev-literal.end to this literal.pos is expression area
        for (i, &span_idx) in tmpl_expr.template_spans.nodes.iter().enumerate() {
            let Some(span_node) = self.arena.get(span_idx) else {
                continue;
            };
            if let Some(span_data) = self.arena.get_template_span(span_node)
                && let Some(lit_node) = self.arena.get(span_data.literal)
            {
                // Cursor at or before the literal's `}` → in expression area → param i+1
                // The literal starts with `}` which closes the expression; cursor there
                // is still conceptually "at the expression" (matches TypeScript behavior).
                if cursor_offset <= lit_node.pos {
                    return (i + 1) as u32;
                }
                // Cursor within the literal (template text after `}`) → param 0
                if cursor_offset < lit_node.end {
                    return 0;
                }
                // Cursor past this literal → continue to next span
            }
        }

        0
    }
}

/// Apply type parameter substitution to a `SignatureInformation`, replacing each
/// type parameter name with its resolved substitution (default type, constraint
/// type, or `unknown`) in parameter labels, prefix, suffix, and the full label.
fn apply_type_param_substitution(
    info: &mut SignatureInformation,
    type_param_substitutions: &[(String, String)],
) {
    // Substitute in each parameter label
    for param in &mut info.parameters {
        param.label = substitute_type_params(&param.label, type_param_substitutions);
    }
    // Substitute in suffix (contains return type)
    info.suffix = substitute_type_params(&info.suffix, type_param_substitutions);
    // Rebuild full label from prefix + substituted param labels + substituted suffix
    let param_labels: Vec<&str> = info.parameters.iter().map(|p| p.label.as_str()).collect();
    info.label = format!("{}{}{}", info.prefix, param_labels.join(", "), info.suffix);
}

/// Substitute occurrences of type parameter names with their resolved
/// substitution text in a formatted type string. Uses word-boundary-aware
/// replacement so that e.g. type param `T` does not replace the `T` inside
/// `Tuple`.
fn substitute_type_params(s: &str, type_param_substitutions: &[(String, String)]) -> String {
    let mut result = s.to_string();
    for (name, substitution) in type_param_substitutions {
        // Replace whole-word occurrences of the type parameter name with its
        // substitution. A "word boundary" here means the character before/after
        // is not alphanumeric or underscore (matching TypeScript identifier
        // characters).
        let mut out = String::with_capacity(result.len());
        let name_len = name.len();
        let bytes = result.as_bytes();
        let len = bytes.len();
        let mut i = 0;
        while i < len {
            if i + name_len <= len && &result[i..i + name_len] == name.as_str() {
                let before_ok = i == 0 || !is_ident_char(bytes[i - 1]);
                let after_ok = i + name_len == len || !is_ident_char(bytes[i + name_len]);
                if before_ok && after_ok {
                    out.push_str(substitution);
                    i += name_len;
                    continue;
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        result = out;
    }
    result
}

#[inline]
const fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
#[path = "../tests/signature_help_tests.rs"]
mod signature_help_tests;
