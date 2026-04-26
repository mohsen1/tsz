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
use tsz_parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::{FunctionShape, TypeData, TypeId, TypePredicateTarget, visitor};

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
        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
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
        let mut signatures = self.get_signatures_from_type(
            callee_type,
            &checker,
            effective_call_kind,
            &callee_name,
            has_explicit_type_args,
            &explicit_type_arg_texts,
        );

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

    fn signature_help_for_contextual_variable_initializer(
        &self,
        _root: NodeIndex,
        start_node: NodeIndex,
        cursor_offset: u32,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        let mut current = start_node;
        let mut declaration_idx = NodeIndex::NONE;
        if current.is_some() {
            while current.is_some() {
                let node = self.arena.get(current)?;
                if node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                    declaration_idx = current;
                    break;
                }
                current = self.arena.get_extended(current)?.parent;
            }
        } else {
            // When a cursor sits in trivia (whitespace, comments) the AST lookup
            // can return no node. Recover by locating the tightest variable
            // declaration whose initializer span contains the cursor.
            let mut best_len = u32::MAX;
            for (idx, node) in self.arena.nodes.iter().enumerate() {
                if node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
                    continue;
                }
                let Some(decl) = self.arena.get_variable_declaration(node) else {
                    continue;
                };
                if !decl.initializer.is_some() {
                    continue;
                }
                let Some(init_node) = self.arena.get(decl.initializer) else {
                    continue;
                };
                if cursor_offset < init_node.pos || cursor_offset > init_node.end {
                    continue;
                }
                let len = node.end.saturating_sub(node.pos);
                if len < best_len {
                    best_len = len;
                    declaration_idx = NodeIndex(idx as u32);
                }
            }
        }
        if !declaration_idx.is_some() {
            return None;
        }

        let decl_node = self.arena.get(declaration_idx)?;
        let decl = self.arena.get_variable_declaration(decl_node)?;
        if !decl.type_annotation.is_some() || !decl.initializer.is_some() {
            return None;
        }
        let initializer_node = self.arena.get(decl.initializer)?;
        if cursor_offset < initializer_node.pos || cursor_offset > initializer_node.end {
            return None;
        }
        let lower_bound = initializer_node.pos;
        let open_paren = self.find_unmatched_open_paren_before(lower_bound, cursor_offset)?;
        let active_parameter =
            self.count_top_level_commas_in_range((open_paren + 1) as usize, cursor_offset as usize);
        let arg_count = self.textual_argument_count_for_open_paren(open_paren, cursor_offset);

        let var_name = self
            .arena
            .get_identifier_text(decl.name)
            .map(std::string::ToString::to_string)
            .unwrap_or_default();
        if var_name.is_empty() {
            return None;
        }
        let contextual_name = self
            .arena
            .get(decl.type_annotation)
            .and_then(|type_node| {
                if type_node.kind != syntax_kind_ext::TYPE_REFERENCE {
                    return None;
                }
                let type_ref = self.arena.get_type_ref(type_node)?;
                self.arena
                    .get_identifier_text(type_ref.type_name)
                    .map(std::string::ToString::to_string)
            })
            .unwrap_or_else(|| var_name.clone());

        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
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
        self.apply_lib_contexts(&mut checker);

        let contextual_type = checker.get_type_of_node(decl.type_annotation);
        let contextual_type = checker.resolve_lazy_type(contextual_type);
        let mut signatures = self.get_signatures_from_type(
            contextual_type,
            &checker,
            CallKind::Call,
            &contextual_name,
            false,
            &[],
        );
        if signatures.is_empty()
            && initializer_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
        {
            let member_name = self
                .enclosing_object_member_name_within_argument(decl.initializer, cursor_offset)
                .map(|(name, _)| name)
                .or_else(|| {
                    self.object_member_name_from_argument_text(decl.initializer, cursor_offset)
                });
            if let Some(member_name) = member_name {
                if let Some(prop_type) =
                    self.contextual_property_type_from_type(contextual_type, &member_name)
                {
                    signatures = self.get_signatures_from_type(
                        prop_type,
                        &checker,
                        CallKind::Call,
                        &member_name,
                        false,
                        &[],
                    );
                } else if let Some(sig_info) =
                    self.source_contextual_member_signature(&checker, contextual_type, &member_name)
                {
                    signatures = vec![self.signature_candidate_from_info(sig_info)];
                }
            }
        }
        if signatures.is_empty()
            && let Some(type_node) = self.arena.get(decl.type_annotation)
        {
            let start = type_node.pos as usize;
            let end = (type_node.end as usize).min(self.source_text.len());
            if start < end {
                let type_text = self.source_text[start..end]
                    .trim()
                    .trim_end_matches('=')
                    .trim_end();
                if let Some(info) =
                    self.signature_info_from_member_signature_text(&contextual_name, type_text)
                {
                    signatures = vec![self.signature_candidate_from_info(info)];
                }
            }
        }
        for sig in &mut signatures {
            if !sig.type_param_substitutions.is_empty() {
                apply_type_param_substitution(&mut sig.info, &sig.type_param_substitutions);
            }
        }
        if signatures.is_empty() {
            *type_cache = Some(checker.extract_cache());
            return None;
        }
        *type_cache = Some(checker.extract_cache());

        let active_signature =
            self.select_active_signature(&signatures, arg_count, active_parameter, &[]);
        let active_parameter =
            self.clamp_active_parameter(&signatures, active_signature, active_parameter, arg_count);

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: arg_count as u32,
            applicable_span_start: open_paren + 1,
            applicable_span_length: cursor_offset.saturating_sub(open_paren + 1),
        })
    }

    fn contextual_signature_help_from_call_argument(
        &self,
        call_expr: &CallExprData,
        cursor_offset: u32,
        callee_type: TypeId,
        checker: &CheckerState<'_>,
    ) -> Option<SignatureHelp> {
        let (arg_index, arg_node_idx) =
            self.argument_index_and_node_at_cursor(call_expr, cursor_offset)?;
        let (outer_param_type, outer_param_name) =
            self.parameter_type_and_name_at(callee_type, CallKind::Call, arg_index)?;
        let arg_node = self.arena.get(arg_node_idx)?;
        let mut source_signature: Option<SignatureInformation> = None;

        let (context_type, context_name, scan_start) = if let Some((member_name, member_idx)) =
            self.enclosing_object_member_name_within_argument(arg_node_idx, cursor_offset)
        {
            let member_node = self.arena.get(member_idx)?;
            if let Some(prop_type) =
                self.contextual_property_type_from_type(outer_param_type, &member_name)
            {
                (prop_type, member_name, member_node.pos)
            } else if let Some(sig_info) =
                self.source_contextual_member_signature(checker, outer_param_type, &member_name)
            {
                source_signature = Some(sig_info);
                (TypeId::ERROR, member_name, member_node.pos)
            } else {
                return None;
            }
        } else if arg_node.kind == syntax_kind_ext::OBJECT_LITERAL_EXPRESSION
            && let Some(member_name) =
                self.object_member_name_from_argument_text(arg_node_idx, cursor_offset)
        {
            if let Some(prop_type) =
                self.contextual_property_type_from_type(outer_param_type, &member_name)
            {
                (prop_type, member_name, arg_node.pos)
            } else if let Some(sig_info) =
                self.source_contextual_member_signature(checker, outer_param_type, &member_name)
            {
                source_signature = Some(sig_info);
                (TypeId::ERROR, member_name, arg_node.pos)
            } else {
                return None;
            }
        } else {
            let kind = arg_node.kind;
            let looks_like_callback = kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                || kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || kind == syntax_kind_ext::ARROW_FUNCTION;
            if !looks_like_callback {
                return None;
            }
            (
                outer_param_type,
                outer_param_name.unwrap_or_else(|| "callback".to_string()),
                arg_node.pos,
            )
        };

        let mut signatures = if let Some(sig_info) = source_signature {
            vec![self.signature_candidate_from_info(sig_info)]
        } else {
            self.get_signatures_from_type(
                context_type,
                checker,
                CallKind::Call,
                &context_name,
                false,
                &[],
            )
        };
        for sig in &mut signatures {
            if !sig.type_param_substitutions.is_empty() {
                apply_type_param_substitution(&mut sig.info, &sig.type_param_substitutions);
            }
        }
        if signatures.is_empty() {
            return None;
        }

        let open_paren = self.find_unmatched_open_paren_before(scan_start, cursor_offset)?;
        let contextual_active_parameter =
            self.count_top_level_commas_in_range((open_paren + 1) as usize, cursor_offset as usize);
        let arg_count = self.textual_argument_count_for_open_paren(open_paren, cursor_offset);
        let active_signature =
            self.select_active_signature(&signatures, arg_count, contextual_active_parameter, &[]);
        let active_parameter = self.clamp_active_parameter(
            &signatures,
            active_signature,
            contextual_active_parameter,
            arg_count,
        );

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: arg_count as u32,
            applicable_span_start: open_paren + 1,
            applicable_span_length: cursor_offset.saturating_sub(open_paren + 1),
        })
    }

    fn argument_index_and_node_at_cursor(
        &self,
        call_expr: &CallExprData,
        cursor_offset: u32,
    ) -> Option<(usize, NodeIndex)> {
        let args = call_expr.arguments.as_ref()?;
        for (idx, &arg_idx) in args.nodes.iter().enumerate() {
            let node = self.arena.get(arg_idx)?;
            if node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }
            if cursor_offset > node.pos && cursor_offset <= node.end {
                return Some((idx, arg_idx));
            }
        }
        None
    }

    fn object_member_name_from_argument_text(
        &self,
        argument_idx: NodeIndex,
        cursor_offset: u32,
    ) -> Option<String> {
        let arg_node = self.arena.get(argument_idx)?;
        let start = arg_node.pos as usize;
        let end = (cursor_offset as usize)
            .min(arg_node.end as usize)
            .min(self.source_text.len());
        if start >= end {
            return None;
        }
        let prefix = &self.source_text[start..end];
        let colon_candidate = prefix.rfind(':').and_then(|idx| {
            self.identifier_before_offset(prefix, idx)
                .map(|name| (idx, name))
        });
        let paren_candidate = prefix.rfind('(').and_then(|idx| {
            let name = self.identifier_before_offset(prefix, idx)?;
            if name == "function" {
                return None;
            }
            Some((idx, name))
        });

        match (colon_candidate, paren_candidate) {
            (Some((ci, cname)), Some((pi, pname))) => {
                if pi > ci {
                    Some(pname)
                } else {
                    Some(cname)
                }
            }
            (Some((_, cname)), None) => Some(cname),
            (None, Some((_, pname))) => Some(pname),
            (None, None) => None,
        }
    }

    fn identifier_before_offset(&self, text: &str, offset: usize) -> Option<String> {
        if offset == 0 || offset > text.len() {
            return None;
        }
        let bytes = text.as_bytes();
        let mut end = offset;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        if end == 0 {
            return None;
        }
        let mut start = end;
        while start > 0 && Self::is_ascii_identifier_byte(bytes[start - 1]) {
            start -= 1;
        }
        (start < end).then(|| text[start..end].to_string())
    }

    fn parameter_type_and_name_at(
        &self,
        type_id: TypeId,
        call_kind: CallKind,
        arg_index: usize,
    ) -> Option<(TypeId, Option<String>)> {
        if let Some(shape_id) = visitor::function_shape_id(self.interner, type_id) {
            let shape = self.interner.function_shape(shape_id);
            return self.parameter_type_and_name_from_params(&shape.params, arg_index);
        }

        if let Some(shape_id) = visitor::callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(shape_id);
            let signatures = if call_kind == CallKind::New {
                &shape.construct_signatures
            } else {
                &shape.call_signatures
            };
            for sig in signatures {
                if let Some(found) =
                    self.parameter_type_and_name_from_params(&sig.params, arg_index)
                {
                    return Some(found);
                }
            }
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, type_id))
        {
            for &member in self.interner.type_list(list_id).iter() {
                if let Some(found) = self.parameter_type_and_name_at(member, call_kind, arg_index) {
                    return Some(found);
                }
            }
        }

        if let Some(app_id) = visitor::application_id(self.interner, type_id) {
            let app = self.interner.type_application(app_id);
            return self.parameter_type_and_name_at(app.base, call_kind, arg_index);
        }

        None
    }

    fn parameter_type_and_name_from_params(
        &self,
        params: &[tsz_solver::ParamInfo],
        arg_index: usize,
    ) -> Option<(TypeId, Option<String>)> {
        if arg_index < params.len() {
            let param = params[arg_index];
            let name = param.name.map(|atom| self.interner.resolve_atom(atom));
            return Some((param.type_id, name));
        }
        params.last().and_then(|param| {
            if param.rest {
                let name = param.name.map(|atom| self.interner.resolve_atom(atom));
                Some((param.type_id, name))
            } else {
                None
            }
        })
    }

    fn enclosing_object_member_name_within_argument(
        &self,
        argument_idx: NodeIndex,
        cursor_offset: u32,
    ) -> Option<(String, NodeIndex)> {
        let mut current =
            find_node_at_or_before_offset(self.arena, cursor_offset, self.source_text);
        while current.is_some() {
            if current == argument_idx {
                break;
            }
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::PROPERTY_ASSIGNMENT {
                let prop = self.arena.get_property_assignment(node)?;
                let name = self
                    .arena
                    .get_identifier_text(prop.name)
                    .map(std::string::ToString::to_string)?;
                return Some((name, current));
            }
            if node.kind == syntax_kind_ext::METHOD_DECLARATION {
                let method = self.arena.get_method_decl(node)?;
                let name = self
                    .arena
                    .get_identifier_text(method.name)
                    .map(std::string::ToString::to_string)?;
                return Some((name, current));
            }
            current = self.arena.get_extended(current)?.parent;
        }
        None
    }

    fn contextual_property_type_from_type(
        &self,
        container_type_id: TypeId,
        prop_name: &str,
    ) -> Option<TypeId> {
        if let Some(shape_id) = visitor::callable_shape_id(self.interner, container_type_id) {
            let shape = self.interner.callable_shape(shape_id);
            for prop in &shape.properties {
                if self.interner.resolve_atom(prop.name) == prop_name {
                    return Some(prop.type_id);
                }
            }
        }

        if let Some(shape_id) = visitor::object_shape_id(self.interner, container_type_id)
            .or_else(|| visitor::object_with_index_shape_id(self.interner, container_type_id))
        {
            let shape = self.interner.object_shape(shape_id);
            for prop in &shape.properties {
                if self.interner.resolve_atom(prop.name) == prop_name {
                    return Some(prop.type_id);
                }
            }
        }

        if let Some(list_id) = visitor::union_list_id(self.interner, container_type_id)
            .or_else(|| visitor::intersection_list_id(self.interner, container_type_id))
        {
            for &member in self.interner.type_list(list_id).iter() {
                if let Some(member_type) =
                    self.contextual_property_type_from_type(member, prop_name)
                {
                    return Some(member_type);
                }
            }
        }

        if let Some(app_id) = visitor::application_id(self.interner, container_type_id) {
            let app = self.interner.type_application(app_id);
            return self.contextual_property_type_from_type(app.base, prop_name);
        }

        None
    }

    fn source_contextual_member_signature(
        &self,
        checker: &CheckerState<'_>,
        container_type_id: TypeId,
        member_name: &str,
    ) -> Option<SignatureInformation> {
        let container_type_text = checker.format_type(container_type_id);
        let interface_name = container_type_text
            .split('<')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())?;
        let signature_text =
            self.source_interface_member_signature_text(interface_name, member_name)?;
        self.signature_info_from_member_signature_text(member_name, &signature_text)
    }

    fn source_interface_member_signature_text(
        &self,
        interface_name: &str,
        member_name: &str,
    ) -> Option<String> {
        let iface_pattern = format!("interface {interface_name}");
        let iface_start = self.source_text.find(&iface_pattern)?;
        let after_iface = &self.source_text[iface_start..];
        let body_open_rel = after_iface.find('{')?;
        let body_open = iface_start + body_open_rel;
        let body_close = self.find_matching_brace_in_source(body_open)?;
        let body = &self.source_text[body_open + 1..body_close];

        let method_pattern = format!("{member_name}(");
        if let Some(method_idx) = body.find(&method_pattern) {
            let tail = &body[method_idx..];
            let end = tail.find(';').unwrap_or(tail.len());
            return Some(tail[..end].trim().to_string());
        }

        let property_pattern = format!("{member_name}:");
        if let Some(property_idx) = body.find(&property_pattern) {
            let tail = &body[property_idx + property_pattern.len()..];
            let end = tail.find(';').unwrap_or(tail.len());
            return Some(tail[..end].trim().to_string());
        }

        None
    }

    fn signature_info_from_member_signature_text(
        &self,
        member_name: &str,
        signature_text: &str,
    ) -> Option<SignatureInformation> {
        let trimmed = signature_text.trim().trim_end_matches(';').trim();
        let (params_text, return_type) = if trimmed.starts_with(member_name) {
            let open = trimmed.find('(')?;
            let close = Self::find_matching_paren_in_text(trimmed, open)?;
            let params = trimmed[open + 1..close].trim();
            let after = trimmed[close + 1..].trim();
            let return_type = after.strip_prefix(':')?.trim();
            (params, return_type)
        } else {
            let open = trimmed.find('(')?;
            let close = Self::find_matching_paren_in_text(trimmed, open)?;
            let params = trimmed[open + 1..close].trim();
            let after = trimmed[close + 1..].trim();
            let return_type = after.strip_prefix("=>")?.trim();
            (params, return_type)
        };

        let mut parameters = Vec::new();
        for (idx, raw) in Self::split_top_level_text(params_text, ',')
            .into_iter()
            .enumerate()
        {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let is_rest = raw.starts_with("...");
            let lhs = if let Some(colon_idx) = Self::find_top_level_char(raw, ':') {
                raw[..colon_idx].trim()
            } else {
                raw
            };
            let lhs = lhs.trim_start_matches("...").trim();
            let is_optional = !is_rest && lhs.ends_with('?');
            let mut name = lhs.trim_end_matches('?').trim().to_string();
            if name.is_empty() {
                name = format!("arg{idx}");
            }
            parameters.push(ParameterInformation {
                name,
                label: raw.to_string(),
                documentation: None,
                is_optional,
                is_rest,
            });
        }
        let labels: Vec<String> = parameters.iter().map(|p| p.label.clone()).collect();
        let is_variadic = parameters.iter().any(|p| p.is_rest);
        let prefix = format!("{member_name}(");
        let suffix = format!("): {return_type}");
        Some(SignatureInformation {
            label: format!("{prefix}{}{}", labels.join(", "), suffix),
            prefix,
            suffix,
            documentation: None,
            parameters,
            is_variadic,
            is_constructor: false,
            tags: Vec::new(),
        })
    }

    fn signature_candidate_from_info(&self, info: SignatureInformation) -> SignatureCandidate {
        let required_params = info
            .parameters
            .iter()
            .filter(|param| !param.is_optional && !param.is_rest)
            .count();
        let total_params = info.parameters.len();
        let has_rest = info.parameters.iter().any(|param| param.is_rest);
        let param_names = info
            .parameters
            .iter()
            .map(|param| Some(param.name.clone()))
            .collect();
        SignatureCandidate {
            info,
            required_params,
            total_params,
            has_rest,
            param_names,
            type_params: Vec::new(),
            type_param_substitutions: Vec::new(),
        }
    }

    fn find_matching_brace_in_source(&self, open_brace: usize) -> Option<usize> {
        let bytes = self.source_text.as_bytes();
        if open_brace >= bytes.len() || bytes[open_brace] != b'{' {
            return None;
        }
        let mut depth = 0i32;
        for (idx, byte) in bytes.iter().enumerate().skip(open_brace) {
            match *byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn find_matching_paren_in_text(text: &str, open_paren: usize) -> Option<usize> {
        let bytes = text.as_bytes();
        if open_paren >= bytes.len() || bytes[open_paren] != b'(' {
            return None;
        }
        let mut depth = 0i32;
        for (idx, byte) in bytes.iter().enumerate().skip(open_paren) {
            match *byte {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(idx);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn find_unmatched_open_paren_before(
        &self,
        lower_bound: u32,
        cursor_offset: u32,
    ) -> Option<u32> {
        let bytes = self.source_text.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let cursor = cursor_offset.min(bytes.len() as u32);
        if (cursor as usize) < bytes.len() && bytes[cursor as usize] == b'(' {
            return Some(cursor);
        }
        let mut depth = 0i32;
        let min = lower_bound.min(bytes.len() as u32) as i64;
        let mut idx = (cursor as i64).saturating_sub(1);
        while idx >= min && idx >= 0 {
            match bytes[idx as usize] {
                b')' => depth += 1,
                b'(' => {
                    if depth == 0 {
                        return Some(idx as u32);
                    }
                    depth -= 1;
                }
                _ => {}
            }
            idx -= 1;
        }
        None
    }

    fn textual_argument_count_for_open_paren(&self, open_paren: u32, cursor_offset: u32) -> usize {
        let start = (open_paren + 1).min(self.source_text.len() as u32) as usize;
        let end = cursor_offset.min(self.source_text.len() as u32) as usize;
        let text = self.source_text.get(start..end).unwrap_or_default();
        let active = self.count_top_level_commas_in_range(start, end) as usize;
        let trimmed = text.trim_end();
        if trimmed.is_empty() {
            0
        } else if trimmed.ends_with(',') {
            active
        } else {
            active + 1
        }
    }

    fn clamp_active_parameter(
        &self,
        signatures: &[SignatureCandidate],
        active_signature: u32,
        active_parameter: u32,
        arg_count: usize,
    ) -> u32 {
        let mut out = active_parameter;
        if let Some(selected) = signatures.get(active_signature as usize) {
            if selected.info.parameters.is_empty() {
                out = 0;
            } else {
                let has_rest_param = selected.info.parameters.iter().any(|param| param.is_rest);
                let max_index = selected.info.parameters.len().saturating_sub(1);
                if has_rest_param {
                    if out as usize >= arg_count && out as usize > max_index {
                        out = max_index as u32;
                    }
                } else if out as usize > max_index {
                    out = max_index as u32;
                }
            }
        }
        out
    }

    /// For a `super` keyword node, walk up to find the enclosing class, then
    /// return the expression from its `extends` clause (the base class reference).
    /// This lets us resolve the base class symbol for signature help on `super()`.
    fn find_base_class_expression(&self, super_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = super_idx;
        let mut depth = 0;
        while current.is_some() && depth < 100 {
            let node = self.arena.get(current)?;
            if node.is_class_like() {
                let class_data = self.arena.get_class(node)?;
                let heritage_clauses = class_data.heritage_clauses.as_ref()?;
                for &clause_idx in &heritage_clauses.nodes {
                    let heritage = self.arena.get_heritage_clause_at(clause_idx)?;
                    if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                        continue;
                    }
                    let &type_idx = heritage.types.nodes.first()?;
                    // The type in the heritage clause is an ExpressionWithTypeArguments node.
                    // We need the expression inside it (the base class identifier).
                    if let Some(expr_type_args) = self.arena.get_expr_type_args_at(type_idx) {
                        return Some(expr_type_args.expression);
                    }
                    // If not wrapped in ExpressionWithTypeArguments, use directly
                    return Some(type_idx);
                }
                return None;
            }
            if let Some(extended) = self.arena.get_extended(current) {
                current = extended.parent;
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

        let mut seen_non_omitted = 0usize;
        let mut last_non_omitted_end = None;

        for (index, &arg_idx) in args.nodes.iter().enumerate() {
            let Some(arg_node) = self.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                continue;
            }

            // If cursor is before this argument's start, we're between args
            // Treat it as the next argument.
            if cursor_offset <= arg_node.pos {
                return seen_non_omitted as u32;
            }

            // If cursor is within this argument's range, return this index
            if cursor_offset <= arg_node.end {
                return seen_non_omitted as u32;
            }

            let next_start = args
                .nodes
                .iter()
                .skip(index + 1)
                .filter_map(|&next_idx| self.arena.get(next_idx))
                .find(|next| next.kind != syntax_kind_ext::OMITTED_EXPRESSION)
                .map(|next| next.pos);
            if let Some(next_start) = next_start
                && cursor_offset < next_start
            {
                return (seen_non_omitted + 1) as u32;
            }
            last_non_omitted_end = Some(arg_node.end as usize);
            seen_non_omitted += 1;
        }

        if let Some(last_end) = last_non_omitted_end {
            let end = (cursor_offset as usize).min(self.source_text.len());
            if last_end < end
                && seen_non_omitted > 0
                && !self.has_comma_between(last_end as u32, cursor_offset)
            {
                return (seen_non_omitted - 1) as u32;
            }
        }

        if let Some(call_node) = self.arena.get(call_idx)
            && let Some(last_end) = last_non_omitted_end
        {
            let scan_end = cursor_offset.min(call_node.end);
            if self.has_comma_between(last_end as u32, scan_end) {
                return seen_non_omitted as u32;
            }
        }

        seen_non_omitted as u32
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

    fn count_top_level_commas_in_range(&self, start: usize, end: usize) -> u32 {
        if start >= end || start >= self.source_text.len() {
            return 0;
        }
        let end = end.min(self.source_text.len());
        let bytes = self.source_text.as_bytes();
        let mut commas = 0u32;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;
        let mut i = start;
        while i < end {
            match bytes[i] {
                b'(' => paren += 1,
                b')' => paren = paren.saturating_sub(1),
                b'[' => bracket += 1,
                b']' => bracket = bracket.saturating_sub(1),
                b'{' => brace += 1,
                b'}' => brace = brace.saturating_sub(1),
                b'<' => angle += 1,
                b'>' if i == 0 || bytes[i - 1] != b'=' => {
                    angle = angle.saturating_sub(1);
                }
                b',' if paren == 0 && bracket == 0 && brace == 0 && angle == 0 => commas += 1,
                _ => {}
            }
            i += 1;
        }
        commas
    }

    fn type_argument_context_for_call(
        &self,
        call_idx: NodeIndex,
        data: &CallExprData,
        cursor_offset: u32,
    ) -> Option<TypeArgumentContext> {
        data.type_arguments.as_ref()?;

        let call_node = self.arena.get(call_idx)?;
        let call_start = call_node.pos as usize;
        let call_end = (call_node.end as usize).min(self.source_text.len());
        if call_start >= call_end {
            return None;
        }
        let call_text = &self.source_text[call_start..call_end];
        let lt_rel = call_text.find('<')?;
        let lt_abs = call_start + lt_rel;
        if cursor_offset <= lt_abs as u32 {
            return None;
        }

        let bytes = self.source_text.as_bytes();
        let mut depth = 0i32;
        let mut gt_abs = None;
        let mut i = lt_abs;
        while i < call_end {
            match bytes[i] {
                b'<' => depth += 1,
                b'>' if i == 0 || bytes[i - 1] != b'=' => {
                    depth -= 1;
                    if depth == 0 {
                        gt_abs = Some(i);
                        break;
                    }
                }
                _ => {}
            }
            i += 1;
        }

        if let Some(gt) = gt_abs
            && cursor_offset > gt as u32
        {
            return None;
        }

        let scan_end = gt_abs.map_or(cursor_offset as usize, |gt| {
            (cursor_offset as usize).min(gt)
        });
        let scan_start = (lt_abs + 1).min(scan_end);
        let active_parameter = self.count_top_level_commas_in_range(scan_start, scan_end);

        Some(TypeArgumentContext {
            active_parameter,
            span_start: scan_start as u32,
            span_length: (scan_end.saturating_sub(scan_start)) as u32,
        })
    }

    fn rewrite_signatures_for_type_arguments(
        &self,
        signatures: &mut Vec<SignatureCandidate>,
        callee_name: &str,
        active_parameter: u32,
    ) {
        let has_generic_signatures = signatures
            .iter()
            .any(|candidate| !candidate.type_params.is_empty());
        if has_generic_signatures {
            signatures.retain(|candidate| {
                !candidate.type_params.is_empty()
                    && (active_parameter as usize) < candidate.type_params.len()
            });
        }
        for candidate in signatures.iter_mut() {
            let params = if has_generic_signatures {
                candidate.type_params.clone()
            } else {
                Vec::new()
            };
            let function_tail = candidate
                .info
                .label
                .find('(')
                .map(|idx| candidate.info.label[idx..].to_string())
                .unwrap_or_else(|| "()".to_string());
            let prefix = format!("{callee_name}<");
            let suffix = format!(">{function_tail}");
            candidate.info.parameters = params
                .iter()
                .map(|param| {
                    let name = param
                        .split_once(" extends ")
                        .map_or_else(|| param.clone(), |(name, _)| name.to_string());
                    ParameterInformation {
                        name,
                        label: param.clone(),
                        documentation: None,
                        is_optional: false,
                        is_rest: false,
                    }
                })
                .collect();
            candidate.info.prefix = prefix.clone();
            candidate.info.suffix = suffix.clone();
            candidate.info.label = format!("{prefix}{}{}", params.join(", "), suffix);
            candidate.info.is_variadic = false;
            candidate.required_params = params.len();
            candidate.total_params = params.len();
            candidate.has_rest = false;
            candidate.param_names = params
                .iter()
                .map(|param| {
                    Some(
                        param
                            .split_once(" extends ")
                            .map_or_else(|| param.clone(), |(name, _)| name.to_string()),
                    )
                })
                .collect();
        }
    }

    const fn is_ascii_identifier_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
    }

    fn preceded_by_declaration_keyword(&self, probe: usize) -> bool {
        const DECLARATION_KEYWORDS: [&str; 7] = [
            "function",
            "class",
            "interface",
            "type",
            "enum",
            "namespace",
            "module",
        ];
        let bytes = self.source_text.as_bytes();
        DECLARATION_KEYWORDS.iter().any(|keyword| {
            let kw = keyword.as_bytes();
            if probe < kw.len() {
                return false;
            }
            let start = probe - kw.len();
            if &bytes[start..probe] != kw {
                return false;
            }
            start == 0 || !Self::is_ascii_identifier_byte(bytes[start - 1])
        })
    }

    fn find_textual_call_trigger(&self, cursor_offset: u32) -> Option<TextualTypeArgumentTrigger> {
        let bytes = self.source_text.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let cursor = (cursor_offset as usize).min(bytes.len());
        if cursor == 0 {
            return None;
        }

        let mut depth = 0i32;
        let mut paren_idx = None;
        let mut idx = cursor;
        while idx > 0 {
            idx -= 1;
            match bytes[idx] {
                b')' => depth += 1,
                b'(' => {
                    if depth == 0 {
                        paren_idx = Some(idx);
                        break;
                    }
                    depth -= 1;
                }
                b';' | b'\n' | b'\r' if depth == 0 => break,
                _ => {}
            }
        }
        let paren_idx = paren_idx?;

        let mut name_end = paren_idx;
        while name_end > 0 && bytes[name_end - 1].is_ascii_whitespace() {
            name_end -= 1;
        }
        let mut name_start = name_end;
        while name_start > 0 && Self::is_ascii_identifier_byte(bytes[name_start - 1]) {
            name_start -= 1;
        }
        if name_start == name_end {
            return None;
        }
        let callee_name = self.source_text[name_start..name_end].to_string();
        if callee_name.is_empty() {
            return None;
        }

        let mut probe = name_start;
        while probe > 0 && bytes[probe - 1].is_ascii_whitespace() {
            probe -= 1;
        }
        if self.preceded_by_declaration_keyword(probe) {
            return None;
        }
        let call_kind = if probe >= 3 {
            let prefix = &self.source_text[probe - 3..probe];
            let boundary_ok = probe == 3 || !Self::is_ascii_identifier_byte(bytes[probe - 4]);
            if prefix == "new" && boundary_ok {
                CallKind::New
            } else {
                CallKind::Call
            }
        } else {
            CallKind::Call
        };

        let scan_start = (paren_idx + 1).min(cursor);
        let active_parameter = self.count_top_level_commas_in_range(scan_start, cursor);
        Some(TextualTypeArgumentTrigger {
            callee_name,
            callee_offset: name_end.saturating_sub(1) as u32,
            call_kind,
            active_parameter,
            span_start: scan_start as u32,
            span_length: (cursor.saturating_sub(scan_start)) as u32,
        })
    }

    fn find_textual_type_argument_trigger(
        &self,
        cursor_offset: u32,
    ) -> Option<TextualTypeArgumentTrigger> {
        // Robustness audit (PR #F, item 6 in
        // `docs/architecture/ROBUSTNESS_AUDIT_2026-04-26.md`): emit a
        // structured trace at every invocation so the rate at which
        // signature help depends on byte-level source-text scanning is
        // visible. The audit's full solution extracts an
        // `IncompleteCallContext` / `IncompleteCodeQuery` service; this
        // is the visibility-first foothold.
        tracing::trace!(
            site = "signature_help::find_textual_type_argument_trigger",
            cursor_offset = cursor_offset,
            "LSP signature help fell back to text-scanning for type-argument trigger"
        );
        let bytes = self.source_text.as_bytes();
        if bytes.is_empty() {
            return None;
        }
        let cursor = (cursor_offset as usize).min(bytes.len());

        let mut depth = 0i32;
        let mut lt_idx = None;
        let mut idx = cursor;
        while idx > 0 {
            idx -= 1;
            match bytes[idx] {
                b'>' if idx == 0 || bytes[idx - 1] != b'=' => depth += 1,
                b'<' => {
                    if depth == 0 {
                        lt_idx = Some(idx);
                        break;
                    }
                    depth -= 1;
                }
                b';' | b'\n' | b'\r' if depth == 0 => break,
                _ => {}
            }
        }
        let lt_idx = lt_idx?;

        let mut name_end = lt_idx;
        while name_end > 0 && bytes[name_end - 1].is_ascii_whitespace() {
            name_end -= 1;
        }
        let mut name_start = name_end;
        while name_start > 0 && Self::is_ascii_identifier_byte(bytes[name_start - 1]) {
            name_start -= 1;
        }
        if name_start == name_end {
            return None;
        }
        let callee_name = self.source_text[name_start..name_end].to_string();
        if callee_name.is_empty() {
            return None;
        }

        let mut probe = name_start;
        while probe > 0 && bytes[probe - 1].is_ascii_whitespace() {
            probe -= 1;
        }
        if self.preceded_by_declaration_keyword(probe) {
            return None;
        }
        let call_kind = if probe >= 3 {
            let prefix = &self.source_text[probe - 3..probe];
            let boundary_ok = probe == 3 || !Self::is_ascii_identifier_byte(bytes[probe - 4]);
            if prefix == "new" && boundary_ok {
                CallKind::New
            } else {
                CallKind::Call
            }
        } else {
            CallKind::Call
        };

        let scan_start = (lt_idx + 1).min(cursor);
        let active_parameter = self.count_top_level_commas_in_range(scan_start, cursor);
        Some(TextualTypeArgumentTrigger {
            callee_name,
            callee_offset: name_end.saturating_sub(1) as u32,
            call_kind,
            active_parameter,
            span_start: scan_start as u32,
            span_length: (cursor.saturating_sub(scan_start)) as u32,
        })
    }

    fn signature_help_for_textual_call(
        &self,
        root: NodeIndex,
        cursor_offset: u32,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        // Audit PR #F: see `find_textual_type_argument_trigger`.
        tracing::trace!(
            site = "signature_help::signature_help_for_textual_call",
            cursor_offset = cursor_offset,
            "LSP signature help fell back to text-scanning for incomplete call site"
        );
        let trigger = self.find_textual_call_trigger(cursor_offset)?;
        let callee_expr =
            self.find_identifier_node_at_offset(trigger.callee_offset, &trigger.callee_name)?;

        let mut walker = crate::resolver::ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, callee_expr)?;

        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
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
        self.apply_lib_contexts(&mut checker);

        let docs = self.signature_documentation_for_symbol(root, symbol_id, trigger.call_kind);
        let callee_type = checker.get_type_of_symbol(symbol_id);
        let callee_type = checker.resolve_lazy_type(callee_type);
        let mut signatures = self.get_signatures_from_type(
            callee_type,
            &checker,
            trigger.call_kind,
            &trigger.callee_name,
            false,
            &[],
        );

        if let Some(docs) = docs {
            self.apply_signature_docs(&mut signatures, &docs);
        }
        self.apply_source_signature_type_overrides(&mut signatures, symbol_id);
        for sig in &mut signatures {
            if !sig.type_param_substitutions.is_empty() {
                apply_type_param_substitution(&mut sig.info, &sig.type_param_substitutions);
            }
        }
        self.expand_source_rest_tuple_union_signatures(&mut signatures, symbol_id);
        if signatures.is_empty() {
            *type_cache = Some(checker.extract_cache());
            return None;
        }

        *type_cache = Some(checker.extract_cache());

        let span_start = trigger.span_start as usize;
        let span_end = span_start + trigger.span_length as usize;
        let span_text = self
            .source_text
            .get(span_start..span_end)
            .unwrap_or_default();
        let trimmed = span_text.trim_end();
        let arg_count = if trimmed.is_empty() {
            0usize
        } else if trimmed.ends_with(',') {
            trigger.active_parameter as usize
        } else {
            trigger.active_parameter as usize + 1
        };

        let active_signature =
            self.select_active_signature(&signatures, arg_count, trigger.active_parameter, &[]);
        let mut active_parameter = trigger.active_parameter;
        if let Some(selected) = signatures.get(active_signature as usize) {
            if selected.info.parameters.is_empty() {
                active_parameter = 0;
            } else {
                let has_rest_param = selected.info.parameters.iter().any(|param| param.is_rest);
                let max_index = selected.info.parameters.len().saturating_sub(1);
                if has_rest_param {
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

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: arg_count as u32,
            applicable_span_start: trigger.span_start,
            applicable_span_length: trigger.span_length,
        })
    }

    fn find_identifier_node_at_offset(
        &self,
        offset: u32,
        expected_name: &str,
    ) -> Option<NodeIndex> {
        let mut current = find_node_at_or_before_offset(self.arena, offset, self.source_text);
        let mut depth = 0usize;
        while current.is_some() && depth < 128 {
            let node = self.arena.get(current)?;
            if node.kind == SyntaxKind::Identifier as u16
                && self.arena.get_identifier_text(current) == Some(expected_name)
            {
                return Some(current);
            }
            current = self.arena.get_extended(current)?.parent;
            depth += 1;
        }
        None
    }

    fn signature_help_for_textual_type_arguments(
        &self,
        root: NodeIndex,
        cursor_offset: u32,
        type_cache: &mut Option<tsz_checker::TypeCache>,
    ) -> Option<SignatureHelp> {
        // Audit PR #F: see `find_textual_type_argument_trigger`.
        tracing::trace!(
            site = "signature_help::signature_help_for_textual_type_arguments",
            cursor_offset = cursor_offset,
            "LSP signature help fell back to text-scanning for type-argument completion"
        );
        let trigger = self.find_textual_type_argument_trigger(cursor_offset)?;
        let callee_expr =
            self.find_identifier_node_at_offset(trigger.callee_offset, &trigger.callee_name)?;

        let mut walker = crate::resolver::ScopeWalker::new(self.arena, self.binder);
        let symbol_id = walker.resolve_node(root, callee_expr)?;

        let compiler_options = tsz_checker::context::CheckerOptions {
            strict: self.strict,
            no_implicit_any: self.strict,
            no_implicit_returns: false,
            no_implicit_this: self.strict,
            strict_null_checks: self.strict,
            strict_function_types: self.strict,
            strict_property_initialization: self.strict,
            use_unknown_in_catch_variables: self.strict,
            sound_mode: self.sound_mode,
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
        self.apply_lib_contexts(&mut checker);

        let docs = self.signature_documentation_for_symbol(root, symbol_id, trigger.call_kind);
        let callee_type = checker.get_type_of_symbol(symbol_id);
        let callee_type = checker.resolve_lazy_type(callee_type);
        let mut signatures = self.get_signatures_from_type(
            callee_type,
            &checker,
            trigger.call_kind,
            &trigger.callee_name,
            false,
            &[],
        );

        if let Some(docs) = docs {
            self.apply_signature_docs(&mut signatures, &docs);
        }
        self.apply_source_signature_type_overrides(&mut signatures, symbol_id);
        self.rewrite_signatures_for_type_arguments(
            &mut signatures,
            &trigger.callee_name,
            trigger.active_parameter,
        );
        if signatures.is_empty() {
            *type_cache = Some(checker.extract_cache());
            return None;
        }

        *type_cache = Some(checker.extract_cache());

        let active_signature =
            self.select_active_signature(&signatures, 0, trigger.active_parameter, &[]);
        let mut active_parameter = trigger.active_parameter;
        if let Some(selected) = signatures.get(active_signature as usize) {
            if selected.info.parameters.is_empty() {
                active_parameter = 0;
            } else {
                let has_rest_param = selected.info.parameters.iter().any(|param| param.is_rest);
                if !has_rest_param {
                    let max_index = selected.info.parameters.len().saturating_sub(1);
                    if active_parameter as usize > max_index {
                        active_parameter = max_index as u32;
                    }
                }
            }
        }

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature,
            active_parameter,
            argument_count: 0,
            applicable_span_start: trigger.span_start,
            applicable_span_length: trigger.span_length,
        })
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

    /// Extract signature information from a `TypeId`.
    fn get_signatures_from_type(
        &self,
        type_id: TypeId,
        checker: &CheckerState,
        call_kind: CallKind,
        callee_name: &str,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> Vec<SignatureCandidate> {
        if let Some(shape_id) = visitor::function_shape_id(self.interner, type_id) {
            let shape = self.interner.function_shape(shape_id);
            return self.signature_candidates_for_shape(
                &shape,
                checker,
                false,
                callee_name,
                has_explicit_type_args,
                explicit_type_arg_texts,
            );
        }

        if let Some(shape_id) = visitor::callable_shape_id(self.interner, type_id) {
            let shape = self.interner.callable_shape(shape_id);
            let mut sigs = Vec::new();
            let include_call = call_kind == CallKind::Call || call_kind == CallKind::TaggedTemplate;
            let include_construct = call_kind == CallKind::New;

            if include_call {
                // Add call signatures
                for sig in &shape.call_signatures {
                    // Convert CallSignature to FunctionShape for formatting
                    let func_shape = FunctionShape {
                        type_params: sig.type_params.clone(),
                        params: sig.params.clone(),
                        this_type: sig.this_type,
                        return_type: sig.return_type,
                        type_predicate: sig.type_predicate,
                        is_constructor: false,
                        is_method: false,
                    };
                    sigs.extend(self.signature_candidates_for_shape(
                        &func_shape,
                        checker,
                        false,
                        callee_name,
                        has_explicit_type_args,
                        explicit_type_arg_texts,
                    ));
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
                        type_predicate: sig.type_predicate,
                        is_constructor: true,
                        is_method: false,
                    };
                    sigs.extend(self.signature_candidates_for_shape(
                        &func_shape,
                        checker,
                        true,
                        callee_name,
                        has_explicit_type_args,
                        explicit_type_arg_texts,
                    ));
                }
            }
            return sigs;
        }

        // Union of functions
        if let Some(members) = visitor::union_list_id(self.interner, type_id) {
            let members = self.interner.type_list(members);
            let mut sigs = Vec::new();
            for &member in members.iter() {
                sigs.extend(self.get_signatures_from_type(
                    member,
                    checker,
                    call_kind,
                    callee_name,
                    has_explicit_type_args,
                    explicit_type_arg_texts,
                ));
            }
            return sigs;
        }

        vec![]
    }

    fn signature_candidates_for_shape(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
        callee_name: &str,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> Vec<SignatureCandidate> {
        self.expand_rest_tuple_union_variants(shape, checker)
            .into_iter()
            .map(|variant| {
                self.signature_candidate(
                    &variant,
                    checker,
                    is_constructor,
                    callee_name,
                    has_explicit_type_args,
                    explicit_type_arg_texts,
                )
            })
            .collect()
    }

    fn expand_rest_tuple_union_variants(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
    ) -> Vec<FunctionShape> {
        let Some(rest_index) = shape.params.iter().position(|param| param.rest) else {
            return vec![shape.clone()];
        };
        let rest_param = shape.params[rest_index];
        let Some(TypeData::Union(list_id)) = checker.ctx.types.lookup(rest_param.type_id) else {
            return vec![shape.clone()];
        };

        let mut variants = Vec::new();
        for &member in checker.ctx.types.type_list(list_id).iter() {
            if !matches!(checker.ctx.types.lookup(member), Some(TypeData::Tuple(_))) {
                continue;
            }
            let mut variant = shape.clone();
            variant.params[rest_index].type_id = member;
            variants.push(variant);
        }

        if variants.is_empty() {
            vec![shape.clone()]
        } else {
            variants
        }
    }

    /// Format a `FunctionShape` into `SignatureInformation`
    fn format_signature(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
        callee_name: &str,
        has_explicit_type_args: bool,
    ) -> SignatureInformation {
        let mut parameters = Vec::new();

        // Build type parameters string for generics.
        // When no explicit type arguments are provided, hide the type parameter list.
        let type_params_str = if !shape.type_params.is_empty() && has_explicit_type_args {
            let tp_parts: Vec<String> = shape
                .type_params
                .iter()
                .map(|tp| {
                    let name = checker.ctx.types.resolve_atom(tp.name);
                    if let Some(constraint) = tp.constraint {
                        format!("{} extends {}", name, checker.format_type(constraint))
                    } else {
                        name
                    }
                })
                .collect();
            format!("<{}>", tp_parts.join(", "))
        } else {
            String::new()
        };

        // Build parameters
        let mut param_labels = Vec::new();
        // Note: we do NOT include `this` parameter in user-visible params
        // because tsserver also excludes it from the signature help display.

        for param in &shape.params {
            // When a rest parameter has a tuple type, expand the tuple elements
            // as individual parameters. e.g. `...args: [...names: string[], allCaps: boolean]`
            // becomes `...names: string[], allCaps: boolean`.
            if param.rest
                && let Some(TypeData::Tuple(list_id)) = checker.ctx.types.lookup(param.type_id)
            {
                let elements = checker.ctx.types.tuple_list(list_id);
                let param_base_name = param.name.map_or_else(
                    || "arg".to_string(),
                    |atom| checker.ctx.types.resolve_atom(atom),
                );
                for (i, elem) in elements.iter().enumerate() {
                    let elem_name = elem.name.map_or_else(
                        || format!("{param_base_name}_{i}"),
                        |atom| checker.ctx.types.resolve_atom(atom),
                    );
                    let type_str = if elem.type_id == TypeId::UNKNOWN {
                        "any".to_string()
                    } else {
                        checker.format_type(elem.type_id)
                    };
                    let is_optional = elem.optional && !elem.rest;
                    let optional = if is_optional && !self.is_js_like_file() {
                        "?"
                    } else {
                        ""
                    };
                    let rest = if elem.rest { "..." } else { "" };

                    let param_label = format!("{rest}{elem_name}{optional}: {type_str}");
                    parameters.push(ParameterInformation {
                        name: elem_name.clone(),
                        label: param_label.clone(),
                        documentation: None,
                        is_optional,
                        is_rest: elem.rest,
                    });
                    param_labels.push(param_label);
                }
                continue;
            }

            let mut name = param.name.map_or_else(
                || "arg".to_string(),
                |atom| checker.ctx.types.resolve_atom(atom),
            );
            if param.rest && name == "arg" {
                name = "args".to_string();
            }
            let mut type_str = if param.type_id == TypeId::UNKNOWN {
                "any".to_string()
            } else {
                checker.format_type(param.type_id)
            };
            // Rest parameters with bare 'any' type should display as 'any[]'
            if param.rest && type_str == "any" {
                type_str = "any[]".to_string();
            }
            let is_optional = param.optional && !param.rest;
            let optional = if is_optional && !self.is_js_like_file() {
                "?"
            } else {
                ""
            };
            let rest = if param.rest { "..." } else { "" };

            let param_label = format!("{rest}{name}{optional}: {type_str}");
            parameters.push(ParameterInformation {
                name: name.clone(),
                label: param_label.clone(),
                documentation: None,
                is_optional,
                is_rest: param.rest,
            });

            param_labels.push(param_label);
        }

        // Inferred tuple wrappers may degrade `(...a: [])` to `(...a: any[])`.
        // Align with tsserver display by rendering this synthetic single-rest
        // parameter shape as a zero-parameter callable.
        if parameters.len() == 1 {
            let only = &parameters[0];
            if only.is_rest && only.name == "a" && only.label == "...a: any[]" {
                parameters.clear();
                param_labels.clear();
            }
        }

        // Build prefix and suffix
        // For return type display:
        // - Type predicate → "paramName is Type" or "this is Type"
        // - UNKNOWN → "any" (matches TypeScript's display for untyped returns)
        // - Constructor with OBJECT/UNKNOWN → class name (TypeScript shows class name)
        let return_type_str = if is_constructor {
            // For construct signatures with an explicit return type, use it.
            // Otherwise fall back to callee name (class constructors).
            if shape.return_type != TypeId::UNKNOWN {
                checker.format_type(shape.return_type)
            } else {
                callee_name.to_string()
            }
        } else if let Some(ref predicate) = shape.type_predicate {
            // Format type predicate: "x is Type" or "asserts x is Type"
            let target_name = match &predicate.target {
                TypePredicateTarget::This => "this".to_string(),
                TypePredicateTarget::Identifier(atom) => checker.ctx.types.resolve_atom(*atom),
            };
            let type_part = predicate
                .type_id
                .map(|tid| checker.format_type(tid))
                .unwrap_or_default();
            if predicate.asserts {
                if type_part.is_empty() {
                    format!("asserts {target_name}")
                } else {
                    format!("asserts {target_name} is {type_part}")
                }
            } else if type_part.is_empty() {
                target_name
            } else {
                format!("{target_name} is {type_part}")
            }
        } else if shape.return_type == TypeId::UNKNOWN {
            // Functions without return type annotation display as 'any' in TypeScript
            "any".to_string()
        } else {
            checker.format_type(shape.return_type)
        };
        let prefix = format!("{callee_name}{type_params_str}(");
        let suffix = format!("): {return_type_str}");

        // Build full label: prefix + params joined by ", " + suffix
        let label = format!("{}{}{}", prefix, param_labels.join(", "), suffix,);
        let is_variadic = parameters.iter().any(|param| param.is_rest);

        SignatureInformation {
            label,
            prefix,
            suffix,
            documentation: None,
            parameters,
            is_variadic,
            is_constructor,
            tags: Vec::new(),
        }
    }

    fn signature_candidate(
        &self,
        shape: &FunctionShape,
        checker: &CheckerState,
        is_constructor: bool,
        callee_name: &str,
        has_explicit_type_args: bool,
        explicit_type_arg_texts: &[String],
    ) -> SignatureCandidate {
        let type_params = shape
            .type_params
            .iter()
            .map(|tp| {
                let name = checker.ctx.types.resolve_atom(tp.name);
                if let Some(constraint) = tp.constraint {
                    format!("{name} extends {}", checker.format_type(constraint))
                } else {
                    name
                }
            })
            .collect::<Vec<_>>();
        let type_param_substitutions = if !shape.type_params.is_empty() {
            if has_explicit_type_args && !explicit_type_arg_texts.is_empty() {
                // Use the actual explicit type argument text for substitution
                shape
                    .type_params
                    .iter()
                    .enumerate()
                    .map(|(i, tp)| {
                        let name = checker.ctx.types.resolve_atom(tp.name);
                        let substitution = if i < explicit_type_arg_texts.len() {
                            explicit_type_arg_texts[i].clone()
                        } else if let Some(default) = tp.default {
                            checker.format_type(default)
                        } else if let Some(constraint) = tp.constraint {
                            checker.format_type(constraint)
                        } else {
                            "unknown".to_string()
                        };
                        (name, substitution)
                    })
                    .collect()
            } else if !has_explicit_type_args {
                // No explicit type args: use defaults/constraints/unknown
                shape
                    .type_params
                    .iter()
                    .map(|tp| {
                        let name = checker.ctx.types.resolve_atom(tp.name);
                        let substitution = if let Some(default) = tp.default {
                            checker.format_type(default)
                        } else if let Some(constraint) = tp.constraint {
                            checker.format_type(constraint)
                        } else {
                            "unknown".to_string()
                        };
                        (name, substitution)
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        // When explicit type args are provided and we have substitutions,
        // hide the <T, U> prefix since the types are instantiated in params.
        let show_type_params = has_explicit_type_args
            && (explicit_type_arg_texts.is_empty() || type_param_substitutions.is_empty());
        let info = self.format_signature(
            shape,
            checker,
            is_constructor,
            callee_name,
            show_type_params,
        );
        let required_params = info
            .parameters
            .iter()
            .filter(|param| !param.is_optional && !param.is_rest)
            .count();
        let total_params = info.parameters.len();
        let has_rest = info.parameters.iter().any(|param| param.is_rest);
        let param_names = info
            .parameters
            .iter()
            .map(|param| Some(param.name.clone()))
            .collect();
        SignatureCandidate {
            info,
            required_params,
            total_params,
            has_rest,
            param_names,
            type_params,
            type_param_substitutions,
        }
    }

    fn apply_source_signature_type_overrides(
        &self,
        signatures: &mut [SignatureCandidate],
        symbol_id: tsz_binder::SymbolId,
    ) {
        if signatures.len() != 1 {
            return;
        }

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };
        if symbol.declarations.len() != 1 {
            return;
        }

        let Some((param_type_texts, return_type_text)) =
            self.source_signature_type_texts(symbol.declarations[0])
        else {
            return;
        };
        let Some(signature) = signatures.first_mut() else {
            return;
        };
        if param_type_texts.len() != signature.info.parameters.len() {
            return;
        }

        for (param, type_text) in signature.info.parameters.iter_mut().zip(param_type_texts) {
            if let Some(type_text) = type_text {
                let optional = if param.is_optional && !self.is_js_like_file() {
                    "?"
                } else {
                    ""
                };
                let rest = if param.is_rest { "..." } else { "" };
                param.label = format!("{rest}{}{optional}: {type_text}", param.name);
            }
        }

        if let Some(return_type_text) = return_type_text {
            signature.info.suffix = format!("): {return_type_text}");
        }

        let param_labels: Vec<String> = signature
            .info
            .parameters
            .iter()
            .map(|param| param.label.clone())
            .collect();
        signature.info.label = format!(
            "{}{}{}",
            signature.info.prefix,
            param_labels.join(", "),
            signature.info.suffix
        );
    }

    fn expand_source_rest_tuple_union_signatures(
        &self,
        signatures: &mut Vec<SignatureCandidate>,
        symbol_id: tsz_binder::SymbolId,
    ) {
        if signatures.is_empty() {
            return;
        }

        let Some(symbol) = self.binder.symbols.get(symbol_id) else {
            return;
        };
        if symbol.declarations.len() != 1 {
            return;
        }
        let Some((param_type_texts, _)) = self.source_signature_type_texts(symbol.declarations[0])
        else {
            return;
        };
        let Some((rest_param_index, rest_tuple_union_text)) = param_type_texts
            .iter()
            .enumerate()
            .find_map(|(idx, maybe_text)| {
                let text = maybe_text.as_ref()?;
                (Self::tuple_union_variants(text).len() > 1).then_some((idx, text.clone()))
            })
        else {
            return;
        };
        let tuple_variants = Self::tuple_union_variants(&rest_tuple_union_text);
        if tuple_variants.len() <= 1 {
            return;
        }

        let Some(base) = signatures
            .iter()
            .find(|sig| sig.info.parameters.len() >= rest_param_index)
            .cloned()
            .or_else(|| signatures.first().cloned())
        else {
            return;
        };
        let base_rest_name = self
            .arena
            .get(symbol.declarations[0])
            .and_then(|decl_node| self.arena.get_function(decl_node))
            .and_then(|fn_data| fn_data.parameters.nodes.get(rest_param_index).copied())
            .and_then(|param_idx| self.arena.get(param_idx))
            .and_then(|param_node| self.arena.get_parameter(param_node))
            .and_then(|param| self.arena.get_identifier_text(param.name))
            .map(|name| name.to_string())
            .or_else(|| {
                signatures
                    .iter()
                    .flat_map(|sig| sig.info.parameters.iter())
                    .find(|param| param.is_rest)
                    .map(|param| param.name.clone())
            })
            .unwrap_or_else(|| "args".to_string());

        let mut expanded = Vec::new();
        for tuple_variant in tuple_variants {
            let Some(expanded_rest_params) =
                Self::tuple_variant_parameters(&tuple_variant, &base_rest_name)
            else {
                continue;
            };
            let mut info = base.info.clone();
            let prefix_param_count = rest_param_index.min(base.info.parameters.len());
            let mut params = base.info.parameters[..prefix_param_count].to_vec();
            params.extend(expanded_rest_params);
            info.parameters = params;
            let labels: Vec<&str> = info
                .parameters
                .iter()
                .map(|param| param.label.as_str())
                .collect();
            info.label = format!("{}{}{}", info.prefix, labels.join(", "), info.suffix);

            let required_params = info
                .parameters
                .iter()
                .filter(|param| !param.is_optional && !param.is_rest)
                .count();
            let total_params = info.parameters.len();
            let has_rest = info.parameters.iter().any(|param| param.is_rest);
            info.is_variadic = has_rest;
            let param_names = info
                .parameters
                .iter()
                .map(|param| Some(param.name.clone()))
                .collect();

            expanded.push(SignatureCandidate {
                info,
                required_params,
                total_params,
                has_rest,
                param_names,
                type_params: base.type_params.clone(),
                type_param_substitutions: base.type_param_substitutions.clone(),
            });
        }

        if !expanded.is_empty() {
            *signatures = expanded;
        }
    }

    fn tuple_union_variants(text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for part in Self::split_top_level_text(text, '|') {
            let trimmed = part.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    fn tuple_variant_parameters(
        tuple_variant: &str,
        base_name: &str,
    ) -> Option<Vec<ParameterInformation>> {
        let inner = tuple_variant
            .trim()
            .strip_prefix('[')?
            .strip_suffix(']')?
            .trim();
        if inner.is_empty() {
            return Some(Vec::new());
        }

        let mut params = Vec::new();
        for (idx, raw) in Self::split_top_level_text(inner, ',')
            .into_iter()
            .enumerate()
        {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }

            let (name, ty, is_optional, is_rest) =
                if let Some(colon_idx) = Self::find_top_level_char(raw, ':') {
                    let lhs = raw[..colon_idx].trim();
                    let rhs = raw[colon_idx + 1..].trim();
                    let mut name = lhs.trim();
                    let is_rest = name.starts_with("...");
                    if is_rest {
                        name = name.trim_start_matches("...").trim();
                    }
                    let is_optional = name.ends_with('?');
                    if is_optional {
                        name = name.trim_end_matches('?').trim();
                    }
                    let fallback = if is_rest {
                        base_name.to_string()
                    } else {
                        format!("{base_name}_{idx}")
                    };
                    let name = if name.is_empty() {
                        fallback
                    } else {
                        name.to_string()
                    };
                    (name, rhs.to_string(), is_optional, is_rest)
                } else if let Some(rest_ty) = raw.strip_prefix("...") {
                    (
                        base_name.to_string(),
                        rest_ty.trim().to_string(),
                        false,
                        true,
                    )
                } else {
                    (format!("{base_name}_{idx}"), raw.to_string(), false, false)
                };

            if ty.is_empty() {
                continue;
            }
            let optional = if is_optional { "?" } else { "" };
            let rest = if is_rest { "..." } else { "" };
            let label = format!("{rest}{name}{optional}: {ty}");
            params.push(ParameterInformation {
                name,
                label,
                documentation: None,
                is_optional,
                is_rest,
            });
        }

        Some(params)
    }

    fn split_top_level_text(text: &str, separator: char) -> Vec<String> {
        let mut out = Vec::new();
        let mut start = 0usize;
        let bytes = text.as_bytes();
        let sep = separator as u8;
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;

        for (idx, &byte) in bytes.iter().enumerate() {
            match byte {
                b'(' => paren += 1,
                b')' => paren = paren.saturating_sub(1),
                b'[' => bracket += 1,
                b']' => bracket = bracket.saturating_sub(1),
                b'{' => brace += 1,
                b'}' => brace = brace.saturating_sub(1),
                b'<' => angle += 1,
                b'>' if idx == 0 || bytes[idx - 1] != b'=' => {
                    angle = angle.saturating_sub(1);
                }
                _ => {}
            }
            if byte == sep && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                out.push(text[start..idx].trim().to_string());
                start = idx + 1;
            }
        }
        out.push(text[start..].trim().to_string());
        out
    }

    fn find_top_level_char(text: &str, needle: char) -> Option<usize> {
        let bytes = text.as_bytes();
        let mut paren = 0i32;
        let mut bracket = 0i32;
        let mut brace = 0i32;
        let mut angle = 0i32;

        for (idx, &byte) in bytes.iter().enumerate() {
            match byte {
                b'(' => paren += 1,
                b')' => paren = paren.saturating_sub(1),
                b'[' => bracket += 1,
                b']' => bracket = bracket.saturating_sub(1),
                b'{' => brace += 1,
                b'}' => brace = brace.saturating_sub(1),
                b'<' => angle += 1,
                b'>' if idx == 0 || bytes[idx - 1] != b'=' => {
                    angle = angle.saturating_sub(1);
                }
                _ => {}
            }
            if byte == needle as u8 && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                return Some(idx);
            }
        }

        None
    }

    fn source_signature_type_texts(
        &self,
        decl_idx: NodeIndex,
    ) -> Option<(Vec<Option<String>>, Option<String>)> {
        let decl_node = self.arena.get(decl_idx)?;

        if let Some(function) = self.arena.get_function(decl_node) {
            let param_types = function
                .parameters
                .nodes
                .iter()
                .map(|&param_idx| {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    self.type_node_text(param.type_annotation)
                })
                .collect::<Vec<_>>();
            let return_type = self.type_node_text(function.type_annotation).or_else(|| {
                self.inferred_return_type_text_from_body(
                    function.body,
                    &function.parameters.nodes,
                    &param_types,
                )
            });
            return Some((param_types, return_type));
        }

        if let Some(method) = self.arena.get_method_decl(decl_node) {
            let param_types = method
                .parameters
                .nodes
                .iter()
                .map(|&param_idx| {
                    let param_node = self.arena.get(param_idx)?;
                    let param = self.arena.get_parameter(param_node)?;
                    self.type_node_text(param.type_annotation)
                })
                .collect::<Vec<_>>();
            let return_type = self.type_node_text(method.type_annotation).or_else(|| {
                self.inferred_return_type_text_from_body(
                    method.body,
                    &method.parameters.nodes,
                    &param_types,
                )
            });
            return Some((param_types, return_type));
        }

        None
    }

    fn inferred_return_type_text_from_body(
        &self,
        body_idx: NodeIndex,
        parameter_nodes: &[NodeIndex],
        parameter_type_texts: &[Option<String>],
    ) -> Option<String> {
        let body_node = self.arena.get(body_idx)?;
        let block = self.arena.get_block(body_node)?;
        let [statement_idx] = block.statements.nodes.as_slice() else {
            return None;
        };
        let statement_node = self.arena.get(*statement_idx)?;
        let return_stmt = self.arena.get_return_statement(statement_node)?;
        let expr_name = self
            .arena
            .get_identifier_text(return_stmt.expression)?
            .trim();

        parameter_nodes
            .iter()
            .zip(parameter_type_texts.iter())
            .find_map(|(&param_idx, type_text)| {
                let param_node = self.arena.get(param_idx)?;
                let param = self.arena.get_parameter(param_node)?;
                (self.arena.get_identifier_text(param.name)? == expr_name)
                    .then(|| type_text.clone())
                    .flatten()
            })
    }

    fn type_node_text(&self, type_idx: NodeIndex) -> Option<String> {
        if !type_idx.is_some() {
            return None;
        }
        let type_node = self.arena.get(type_idx)?;
        let start = type_node.pos as usize;
        let end = type_node.end.min(self.source_text.len() as u32) as usize;
        (start < end).then(|| Self::normalize_source_type_text(self.source_text[start..end].trim()))
    }

    fn normalize_source_type_text(text: &str) -> String {
        let mut text = text.trim().to_string();
        while let Some(last) = text.chars().last() {
            let should_trim = match last {
                ',' | ';' | '=' => true,
                '(' => Self::has_unmatched_trailing_opener(&text, '(', ')'),
                '[' => Self::has_unmatched_trailing_opener(&text, '[', ']'),
                '{' => Self::has_unmatched_trailing_opener(&text, '{', '}'),
                '<' => Self::has_unmatched_trailing_opener(&text, '<', '>'),
                ')' => Self::has_unmatched_trailing_closer(&text, '(', ')'),
                ']' => Self::has_unmatched_trailing_closer(&text, '[', ']'),
                '}' => Self::has_unmatched_trailing_closer(&text, '{', '}'),
                '>' => Self::has_unmatched_trailing_closer(&text, '<', '>'),
                _ => false,
            };
            if !should_trim {
                break;
            }
            text.pop();
            text = text.trim_end().to_string();
        }

        text
    }

    fn has_unmatched_trailing_closer(text: &str, open: char, close: char) -> bool {
        text.chars().filter(|&ch| ch == close).count()
            > text.chars().filter(|&ch| ch == open).count()
    }

    fn has_unmatched_trailing_opener(text: &str, open: char, close: char) -> bool {
        text.chars().filter(|&ch| ch == open).count()
            > text.chars().filter(|&ch| ch == close).count()
    }

    fn select_active_signature(
        &self,
        signatures: &[SignatureCandidate],
        arg_count: usize,
        active_parameter: u32,
        supplied_argument_types: &[String],
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
        let mut best_type_penalty = usize::MAX;
        let mut best_rest_penalty = usize::MAX;
        let mut best_total_params = usize::MAX;
        let is_trailing_argument_slot = arg_count > 0 && (active_parameter as usize) >= arg_count;

        for (idx, sig) in signatures.iter().enumerate() {
            let min_params = sig.required_params;
            let max_params = if sig.has_rest {
                usize::MAX
            } else {
                sig.total_params
            };
            let mut score = if desired < min_params {
                min_params.saturating_sub(desired)
            } else if desired > max_params {
                desired.saturating_sub(max_params)
            } else {
                0
            };
            if is_trailing_argument_slot {
                let active_index = active_parameter as usize;
                let trailing_slot_penalty = if active_index < sig.total_params {
                    usize::from(sig.has_rest)
                } else if sig.has_rest {
                    0
                } else {
                    2
                };
                score = score.saturating_add(trailing_slot_penalty);
            }
            let rest_penalty = usize::from(sig.has_rest);
            let type_penalty =
                self.argument_type_penalty(sig, active_parameter, supplied_argument_types);

            if score < best_score
                || (score == best_score && type_penalty < best_type_penalty)
                || (score == best_score
                    && type_penalty == best_type_penalty
                    && rest_penalty < best_rest_penalty)
                || (score == best_score
                    && type_penalty == best_type_penalty
                    && rest_penalty == best_rest_penalty
                    && sig.total_params < best_total_params)
            {
                best_idx = idx;
                best_score = score;
                best_type_penalty = type_penalty;
                best_rest_penalty = rest_penalty;
                best_total_params = sig.total_params;
            }
        }

        best_idx as u32
    }

    fn argument_type_texts(
        &self,
        call_site: &CallSite<'_>,
        checker: &mut CheckerState<'_>,
    ) -> Vec<String> {
        let mut types = Vec::new();
        match call_site {
            CallSite::Regular(call_expr) => {
                let Some(args) = call_expr.arguments.as_ref() else {
                    return Vec::new();
                };

                for &arg_idx in &args.nodes {
                    let Some(arg_node) = self.arena.get(arg_idx) else {
                        continue;
                    };
                    if arg_node.kind == syntax_kind_ext::OMITTED_EXPRESSION {
                        continue;
                    }
                    let arg_type_id = checker.get_type_of_node(arg_idx);
                    let arg_type = checker.resolve_lazy_type(arg_type_id);
                    let mut text = checker.format_type(arg_type);
                    let start = arg_node.pos as usize;
                    let end = (arg_node.end as usize).min(self.source_text.len());
                    let raw_literal = if start < end {
                        Some(self.source_text[start..end].trim())
                    } else {
                        None
                    };
                    // Preserve source literal text when available so overload
                    // scoring can distinguish e.g. '' from "hi" | "bye".
                    if let Some(raw) = raw_literal
                        && Self::string_literal_value(raw).is_some()
                    {
                        text = raw.to_string();
                    }
                    if (text == "any" || text == "unknown" || text == "error")
                        && let Some(raw) = raw_literal
                    {
                        if Self::is_numeric_literal_type(raw) {
                            text = "number".to_string();
                        } else if Self::string_literal_value(raw).is_some() {
                            text = raw.to_string();
                        } else if raw == "true" || raw == "false" {
                            text = "boolean".to_string();
                        }
                    }
                    types.push(text);
                }
            }
            CallSite::TaggedTemplate(tagged) => {
                // Tagged template signatures always receive a template strings array as
                // the first argument followed by `${}` expression values.
                types.push("TemplateStringsArray".to_string());
                let Some(tmpl_node) = self.arena.get(tagged.template) else {
                    return types;
                };
                let Some(tmpl_expr) = self.arena.get_template_expr(tmpl_node) else {
                    return types;
                };
                for &span_idx in &tmpl_expr.template_spans.nodes {
                    let Some(span_node) = self.arena.get(span_idx) else {
                        continue;
                    };
                    let Some(span_data) = self.arena.get_template_span(span_node) else {
                        continue;
                    };
                    let expr_idx = span_data.expression;
                    let Some(expr_node) = self.arena.get(expr_idx) else {
                        continue;
                    };
                    let expr_type_id = checker.get_type_of_node(expr_idx);
                    let expr_type = checker.resolve_lazy_type(expr_type_id);
                    let mut text = checker.format_type(expr_type);
                    let start = expr_node.pos as usize;
                    let end = (expr_node.end as usize).min(self.source_text.len());
                    let raw_literal = if start < end {
                        Some(self.source_text[start..end].trim())
                    } else {
                        None
                    };
                    if let Some(raw) = raw_literal
                        && Self::string_literal_value(raw).is_some()
                    {
                        text = raw.to_string();
                    }
                    if (text == "any" || text == "unknown" || text == "error")
                        && let Some(raw) = raw_literal
                    {
                        if Self::is_numeric_literal_type(raw) {
                            text = "number".to_string();
                        } else if Self::string_literal_value(raw).is_some() {
                            text = raw.to_string();
                        } else if raw == "true" || raw == "false" {
                            text = "boolean".to_string();
                        }
                    }
                    types.push(text);
                }
            }
        }
        types
    }

    fn infer_type_param_substitutions_from_arguments(
        &self,
        signatures: &mut [SignatureCandidate],
        supplied_argument_types: &[String],
    ) {
        if supplied_argument_types.is_empty() {
            return;
        }

        for sig in signatures.iter_mut() {
            if (sig.type_params.is_empty() && sig.type_param_substitutions.is_empty())
                || sig.info.parameters.is_empty()
            {
                continue;
            }

            let substitution_pairs = sig.type_param_substitutions.clone();
            let mut repeated_identifier_type_counts: FxHashMap<String, usize> =
                FxHashMap::default();
            for param in &sig.info.parameters {
                if let Some((_, ty)) = param.label.rsplit_once(':') {
                    let ty = ty.trim();
                    if Self::is_identifier_like_type_name(ty) {
                        *repeated_identifier_type_counts
                            .entry(ty.to_string())
                            .or_insert(0) += 1;
                    }
                }
            }
            let mut inferred: FxHashMap<String, String> = FxHashMap::default();
            for (arg_index, arg_type_text) in supplied_argument_types.iter().enumerate() {
                if arg_type_text.is_empty()
                    || arg_type_text == "error"
                    || arg_type_text == "unknown"
                {
                    continue;
                }

                let param_idx = if arg_index < sig.info.parameters.len() {
                    arg_index
                } else if sig.has_rest {
                    sig.info.parameters.len().saturating_sub(1)
                } else {
                    continue;
                };

                let Some((_, param_ty)) = sig.info.parameters[param_idx].label.rsplit_once(':')
                else {
                    continue;
                };
                let param_ty = param_ty.trim();
                if sig.type_params.iter().any(|tp| tp == param_ty)
                    || sig
                        .type_param_substitutions
                        .iter()
                        .any(|(name, _)| name == param_ty)
                {
                    inferred
                        .entry(param_ty.to_string())
                        .or_insert_with(|| arg_type_text.clone());
                    continue;
                }

                if Self::is_literal_type_text(arg_type_text)
                    && Self::is_identifier_like_type_name(param_ty)
                    && repeated_identifier_type_counts
                        .get(param_ty)
                        .copied()
                        .unwrap_or(0)
                        >= 2
                {
                    inferred
                        .entry(param_ty.to_string())
                        .or_insert_with(|| arg_type_text.clone());
                    continue;
                }

                for (type_param_name, current_substitution) in &substitution_pairs {
                    if param_ty == current_substitution
                        && Self::is_literal_narrowing_for_base_type(
                            arg_type_text,
                            current_substitution,
                        )
                    {
                        inferred
                            .entry(type_param_name.clone())
                            .or_insert_with(|| arg_type_text.clone());
                    }
                }
            }

            if inferred.is_empty() {
                continue;
            }

            for (name, substitution) in inferred {
                if let Some(existing) = sig
                    .type_param_substitutions
                    .iter_mut()
                    .find(|(existing_name, _)| *existing_name == name)
                {
                    existing.1 = substitution;
                } else {
                    sig.type_param_substitutions.push((name, substitution));
                }
            }
        }
    }

    fn is_identifier_like_type_name(type_name: &str) -> bool {
        if type_name.is_empty() || type_name.contains('.') || type_name.contains('<') {
            return false;
        }
        let mut chars = type_name.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        if !(first.is_ascii_alphabetic() || first == '_') {
            return false;
        }
        if !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
            return false;
        }
        !matches!(
            type_name,
            "string"
                | "number"
                | "boolean"
                | "bigint"
                | "symbol"
                | "any"
                | "unknown"
                | "never"
                | "void"
                | "object"
                | "null"
                | "undefined"
                | "true"
                | "false"
        )
    }

    fn is_literal_type_text(type_text: &str) -> bool {
        let trimmed = type_text.trim();
        Self::string_literal_value(trimmed).is_some()
            || Self::is_numeric_literal_type(trimmed)
            || Self::is_bigint_literal_type(trimmed)
            || trimmed == "true"
            || trimmed == "false"
    }

    fn is_literal_narrowing_for_base_type(arg_type_text: &str, base_type_text: &str) -> bool {
        let base = base_type_text.trim();
        let arg = arg_type_text.trim();
        if Self::string_literal_value(arg).is_some() {
            return base == "string";
        }
        if Self::is_bigint_literal_type(arg) {
            return base == "bigint";
        }
        if arg == "true" || arg == "false" {
            return base == "boolean";
        }
        if Self::is_numeric_literal_type(arg) {
            return base == "number";
        }
        false
    }

    fn argument_type_penalty(
        &self,
        sig: &SignatureCandidate,
        _active_parameter: u32,
        supplied_argument_types: &[String],
    ) -> usize {
        if supplied_argument_types.is_empty() || sig.info.parameters.is_empty() {
            return 0;
        }

        let mut penalty = 0usize;
        for (arg_index, arg_type_text) in supplied_argument_types.iter().enumerate() {
            if arg_type_text.is_empty() || arg_type_text == "error" {
                continue;
            }
            let Some(arg_kind) = Self::primitive_kind_from_type_text(arg_type_text) else {
                continue;
            };

            let param_idx = if arg_index < sig.info.parameters.len() {
                arg_index
            } else if sig.has_rest {
                sig.info.parameters.len().saturating_sub(1)
            } else {
                penalty += 1;
                continue;
            };

            let Some((_, param_ty)) = sig.info.parameters[param_idx].label.rsplit_once(':') else {
                continue;
            };
            let param_ty = param_ty.trim();
            if let Some(arg_string_literal) = Self::string_literal_value(arg_type_text)
                && let Some(matches) =
                    Self::string_literal_union_contains(param_ty, arg_string_literal)
            {
                if !matches {
                    penalty += 1;
                }
                continue;
            }
            let Some(param_mask) = Self::primitive_kind_mask_from_type_text(param_ty) else {
                continue;
            };
            let arg_mask = Self::primitive_mask(arg_kind);
            if (param_mask & arg_mask) == 0 {
                penalty += 1;
            }
        }

        penalty
    }

    const fn primitive_mask(kind: PrimitiveKind) -> u8 {
        match kind {
            PrimitiveKind::String => 0b0001,
            PrimitiveKind::Number => 0b0010,
            PrimitiveKind::Boolean => 0b0100,
            PrimitiveKind::BigInt => 0b1000,
        }
    }

    fn primitive_kind_from_type_text(text: &str) -> Option<PrimitiveKind> {
        let mask = Self::primitive_kind_mask_from_type_text(text)?;
        if mask & Self::primitive_mask(PrimitiveKind::Number) != 0 {
            return Some(PrimitiveKind::Number);
        }
        if mask & Self::primitive_mask(PrimitiveKind::String) != 0 {
            return Some(PrimitiveKind::String);
        }
        if mask & Self::primitive_mask(PrimitiveKind::Boolean) != 0 {
            return Some(PrimitiveKind::Boolean);
        }
        if mask & Self::primitive_mask(PrimitiveKind::BigInt) != 0 {
            return Some(PrimitiveKind::BigInt);
        }
        None
    }

    fn primitive_kind_mask_from_type_text(text: &str) -> Option<u8> {
        let normalized = text.trim().trim_matches(|c| c == '(' || c == ')');
        if normalized.is_empty() {
            return None;
        }

        let mut mask = 0u8;
        for part in normalized
            .split('|')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            if part == "string" || part.starts_with('"') || part.starts_with('\'') {
                mask |= Self::primitive_mask(PrimitiveKind::String);
                continue;
            }
            if part == "number" || Self::is_numeric_literal_type(part) {
                mask |= Self::primitive_mask(PrimitiveKind::Number);
                continue;
            }
            if part == "boolean" || part == "true" || part == "false" {
                mask |= Self::primitive_mask(PrimitiveKind::Boolean);
                continue;
            }
            if part == "bigint" || Self::is_bigint_literal_type(part) {
                mask |= Self::primitive_mask(PrimitiveKind::BigInt);
            }
        }

        (mask != 0).then_some(mask)
    }

    fn is_numeric_literal_type(text: &str) -> bool {
        let mut chars = text.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        let rest = if first == '-' { chars.as_str() } else { text };
        if rest.is_empty() {
            return false;
        }
        rest.chars()
            .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == 'e' || ch == 'E' || ch == '+')
    }

    fn is_bigint_literal_type(text: &str) -> bool {
        let Some(stripped) = text.strip_suffix('n') else {
            return false;
        };
        !stripped.is_empty() && stripped.chars().all(|ch| ch.is_ascii_digit() || ch == '-')
    }

    fn string_literal_value(text: &str) -> Option<&str> {
        let trimmed = text.trim();
        if trimmed.len() < 2 {
            return None;
        }
        let bytes = trimmed.as_bytes();
        let quote = bytes[0];
        if (quote == b'"' || quote == b'\'') && bytes[trimmed.len() - 1] == quote {
            return Some(&trimmed[1..trimmed.len() - 1]);
        }
        None
    }

    // Returns:
    // - Some(true) when parameter type is a pure union of string literals
    //   and includes the argument literal
    // - Some(false) when parameter type is a pure union of string literals
    //   and does not include the argument literal
    // - None when parameter type is not a pure string-literal union
    fn string_literal_union_contains(
        param_type_text: &str,
        arg_literal_value: &str,
    ) -> Option<bool> {
        let mut seen = false;
        for part in param_type_text
            .split('|')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            let literal = Self::string_literal_value(part)?;
            seen = true;
            if literal == arg_literal_value {
                return Some(true);
            }
        }
        seen.then_some(false)
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

        // Copy non-param tags
        if (overwrite || sig.info.tags.is_empty()) && !parsed.tags.is_empty() {
            sig.info.tags = parsed.tags.clone();
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
        symbol_id: tsz_binder::SymbolId,
        call_kind: CallKind,
    ) -> Option<SignatureDocs> {
        let symbol = self.binder.get_symbol(symbol_id)?;
        let decls = symbol.all_declarations();

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
                // For new expressions, only use docs from explicit constructors
                // (collected above), not from the class declaration itself.
                // TypeScript does not propagate class-level JSDoc to implicit constructors.
                continue;
            }
            let doc = jsdoc_for_node(self.arena, root, decl, self.source_text);
            let mut parsed = if doc.is_empty() {
                ParsedJsdoc {
                    summary: None,
                    params: FxHashMap::default(),
                    tags: Vec::new(),
                }
            } else {
                parse_jsdoc(&doc)
            };

            // Merge inline parameter JSDoc comments (e.g. /** comment */ before param)
            let inline_docs = inline_param_jsdocs(self.arena, root, decl, self.source_text);
            for (name, doc) in inline_docs {
                // Inline docs take precedence over @param tags only when @param is absent
                parsed.params.entry(name).or_insert(doc);
            }

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

        for &member in &class_data.members.nodes {
            let Some(member_node) = self.arena.get(member) else {
                continue;
            };
            if self.arena.get_constructor(member_node).is_none() {
                continue;
            }

            let doc = jsdoc_for_node(self.arena, root, member, self.source_text);
            let mut parsed = if doc.is_empty() {
                ParsedJsdoc {
                    summary: None,
                    params: FxHashMap::default(),
                    tags: Vec::new(),
                }
            } else {
                parse_jsdoc(&doc)
            };

            // Merge inline parameter JSDoc comments
            let inline_docs = inline_param_jsdocs(self.arena, root, member, self.source_text);
            for (name, doc) in inline_docs {
                parsed.params.entry(name).or_insert(doc);
            }

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
        let access_node = self.arena.get(access_idx)?;
        let access = self.arena.get_access_expr(access_node)?;
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

            for &member in &class_data.members.nodes {
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
                let mut parsed = if doc.is_empty() {
                    ParsedJsdoc {
                        summary: None,
                        params: FxHashMap::default(),
                        tags: Vec::new(),
                    }
                } else {
                    parse_jsdoc(&doc)
                };

                // Merge inline parameter JSDoc comments
                let inline_docs = inline_param_jsdocs(self.arena, root, member, self.source_text);
                for (name, doc) in inline_docs {
                    parsed.params.entry(name).or_insert(doc);
                }

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
        let expr_node = self.arena.get(expr)?;
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
        sym_id: tsz_binder::SymbolId,
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

    fn class_decls_from_symbol(&self, sym_id: tsz_binder::SymbolId) -> Vec<NodeIndex> {
        let Some(symbol) = self.binder.get_symbol(sym_id) else {
            return Vec::new();
        };
        let mut class_decls = Vec::new();
        for decl in symbol.all_declarations() {
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

    fn class_decls_from_variable_symbol(&self, symbol: &tsz_binder::Symbol) -> Vec<NodeIndex> {
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
        if var_decl.initializer.is_some() {
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

        for &stmt in &sf.statements.nodes {
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
        class_data: &tsz_parser::parser::node::ClassData,
        property_name: &str,
    ) -> bool {
        for &member in &class_data.members.nodes {
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

    fn is_static_method(&self, method: &tsz_parser::parser::node::MethodDeclData) -> bool {
        let Some(modifiers) = method.modifiers.as_ref() else {
            return false;
        };
        for &mod_idx in &modifiers.nodes {
            let Some(mod_node) = self.arena.get(mod_idx) else {
                continue;
            };
            if mod_node.kind == SyntaxKind::StaticKeyword as u16 {
                return true;
            }
        }
        false
    }

    fn resolve_symbol_for_identifier(&self, ident_idx: NodeIndex) -> Option<tsz_binder::SymbolId> {
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

        for &param_idx in &params.nodes {
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
mod signature_help_internal_tests {
    use super::SignatureHelpProvider;
    use tsz_binder::BinderState;
    use tsz_common::position::{LineMap, Position};
    use tsz_parser::ParserState;
    use tsz_solver::TypeInterner;

    #[test]
    fn split_top_level_text_keeps_function_type_commas_grouped() {
        let parts = SignatureHelpProvider::<'_>::split_top_level_text(
            "(err: Error) => void, ...object[]",
            ',',
        );
        assert_eq!(parts, vec!["(err: Error) => void", "...object[]"]);
    }

    #[test]
    fn tuple_variant_parameters_names_unlabeled_entries() {
        let params = SignatureHelpProvider::<'_>::tuple_variant_parameters(
            "[object, (err: Error) => void]",
            "rest",
        )
        .expect("tuple should parse");
        let labels: Vec<String> = params.into_iter().map(|param| param.label).collect();
        assert_eq!(
            labels,
            vec![
                "rest_0: object".to_string(),
                "rest_1: (err: Error) => void".to_string(),
            ]
        );
    }

    #[test]
    fn textual_nested_incomplete_call_prefers_inner_callee() {
        let source = "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar(";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let offset = source.find("bar(").expect("bar(") + "bar(".len();
        let trigger = provider
            .find_textual_call_trigger(offset as u32)
            .expect("textual call trigger");
        assert_eq!(trigger.callee_name, "bar");
        assert_eq!(trigger.active_parameter, 0);

        let mut cache = None;
        let help = provider
            .signature_help_for_textual_call(root, offset as u32, &mut cache)
            .expect("textual call help");
        assert!(
            !help.signatures.is_empty(),
            "Should provide signatures for incomplete inner call"
        );
        assert_eq!(
            help.signatures[help.active_signature as usize].label,
            "bar(x: unknown, y: unknown): unknown"
        );
    }

    #[test]
    fn nested_call_with_outer_unclosed_context_still_has_inner_signature_help() {
        let source = "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar()";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);

        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;
        let help = provider
            .get_signature_help(root, Position::new(2, 8), &mut cache)
            .expect("nested incomplete call should return signature help");
        let typed_pos = source.find("bar(").expect("bar(") + "bar".len();
        assert_eq!(
            help.applicable_span_start as usize,
            typed_pos + 1,
            "Applicable span should start immediately after inner call '('"
        );
        assert!(
            help.signatures[help.active_signature as usize]
                .label
                .starts_with("bar("),
            "Expected active signature for inner callee `bar`"
        );
    }

    #[test]
    fn trigger_sequence_for_nested_generic_call_keeps_signature_help_available() {
        let cases = [
            (
                "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar()",
                Position::new(2, 8),
                "bar(x: unknown, y: unknown): unknown",
            ),
            (
                "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar<)",
                Position::new(2, 8),
                "bar<U>(x: U, y: U): U",
            ),
            (
                "declare function foo<T>(x: T, y: T): T;\ndeclare function bar<U>(x: U, y: U): U;\nfoo(bar,)",
                Position::new(2, 8),
                "foo(x: <U>(x: U, y: U) => U, y: <U>(x: U, y: U) => U): <U>(x: U, y: U) => U",
            ),
        ];

        for (source, position, expected_label) in cases {
            let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
            let root = parser.parse_source_file();

            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);

            let interner = TypeInterner::new();
            let line_map = LineMap::build(source);
            let provider = SignatureHelpProvider::new(
                parser.get_arena(),
                &binder,
                &line_map,
                &interner,
                source,
                "test.ts".to_string(),
            );

            let mut cache = None;
            let help = provider
                .get_signature_help(root, position, &mut cache)
                .expect("signature help should be available");
            let actual = &help.signatures[help.active_signature as usize].label;
            assert_eq!(actual, expected_label);
        }
    }

    #[test]
    fn contextual_object_member_signature_preferred_over_outer_call() {
        let source = "interface I { m(n: number, s: string): void; }\ndeclare function takesObj(i: I): void;\ntakesObj({ m: () });";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );
        let mut cache = None;
        let help = provider
            .get_signature_help(root, Position::new(2, 15), &mut cache)
            .expect("contextual signature help should be available");
        let active = &help.signatures[help.active_signature as usize].label;
        assert_eq!(active, "m(n: number, s: string): void");
    }

    #[test]
    fn contextual_variable_initializer_type_alias_and_function_type() {
        let source = "type Cb = () => void;\nconst cb: Cb = ();\nconst cb2: () => void = ();";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;
        let alias_help = provider
            .get_signature_help(root, Position::new(1, 16), &mut cache)
            .expect("alias contextual signature help");
        assert_eq!(
            alias_help.signatures[alias_help.active_signature as usize].label,
            "Cb(): void"
        );

        let fn_type_help = provider
            .get_signature_help(root, Position::new(2, 24), &mut cache)
            .expect("function type contextual signature help");
        assert_eq!(
            fn_type_help.signatures[fn_type_help.active_signature as usize].label,
            "cb2(): void"
        );
    }

    #[test]
    fn contextual_variable_initializer_gives_function_type_help() {
        // Cursor sits inside the empty parens of a parenthesized expression that
        // is the initializer of a variable with a contextual function type.
        let source = "const cb2: () => void = ()";
        let cursor_offset = (source.rfind('(').expect("open paren") + 1) as u32;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let position = line_map.offset_to_position(cursor_offset, source);
        let mut cache = None;
        let help = provider
            .get_signature_help(root, position, &mut cache)
            .expect("function type contextual signature help");
        assert_eq!(
            help.signatures[help.active_signature as usize].label,
            "cb2(): void"
        );
    }

    #[test]
    fn textual_type_argument_trigger_skips_function_declaration_name() {
        // After `<` in a function declaration head we are naming a new type
        // parameter, which must not trigger signature help.
        let source = "function f<\nx";
        let cursor_offset = (source.find('<').expect("less than") + 1) as u32;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let position = line_map.offset_to_position(cursor_offset, source);
        let mut cache = None;
        let help = provider.get_signature_help(root, position, &mut cache);
        assert!(
            help.is_none(),
            "type parameter declaration position should not produce signature help"
        );
    }

    #[test]
    fn contextual_object_literal_method_from_typed_initializer() {
        // Cursor sits inside the parameter list of a method in an object literal
        // whose contextual type has a matching method signature.
        let source = "interface Obj { optionalMethod?: (current: any) => any; }\nconst o: Obj = {\n  optionalMethod() { return {}; }\n};";
        let cursor_offset =
            (source.find("optionalMethod()").expect("call") + "optionalMethod(".len()) as u32;
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let position = line_map.offset_to_position(cursor_offset, source);
        let mut cache = None;
        let help = provider
            .get_signature_help(root, position, &mut cache)
            .expect("contextual object literal method signature help");
        assert_eq!(
            help.signatures[help.active_signature as usize].label,
            "optionalMethod(current: any): any"
        );
        assert_eq!(help.active_parameter, 0);
    }

    #[test]
    fn overload_selection_prefers_matching_string_literal_signatures() {
        let source = "function x1(x: \"hi\");\nfunction x1(y: \"bye\");\nfunction x1(z: string);\nfunction x1(a: any) {}\nx1('');\nx1('hi');\nx1('bye');";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;

        let empty_call = provider
            .get_signature_help(root, Position::new(4, 4), &mut cache)
            .expect("signature help for x1('')");
        assert_eq!(
            empty_call.signatures[empty_call.active_signature as usize].parameters[0].name,
            "z"
        );

        let hi_call = provider
            .get_signature_help(root, Position::new(5, 6), &mut cache)
            .expect("signature help for x1('hi')");
        assert_eq!(
            hi_call.signatures[hi_call.active_signature as usize].parameters[0].name,
            "x"
        );

        let bye_call = provider
            .get_signature_help(root, Position::new(6, 7), &mut cache)
            .expect("signature help for x1('bye')");
        assert_eq!(
            bye_call.signatures[bye_call.active_signature as usize].parameters[0].name,
            "y"
        );
    }

    #[test]
    fn generic_inference_uses_argument_literal_for_signature_display() {
        let source = "declare function f<T extends string>(a: T, b: T, c: T): void;\nf(\"x\", );";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;
        let help = provider
            .get_signature_help(root, Position::new(1, 7), &mut cache)
            .expect("signature help for generic inference");
        assert_eq!(
            help.signatures[help.active_signature as usize].label,
            "f(a: \"x\", b: \"x\", c: \"x\"): void"
        );
    }

    #[test]
    fn no_signature_help_while_editing_identifier_before_call_open_paren() {
        let source = "/**\n * @param start The start\n * @param end The end\n * More text\n */\ndeclare function foo(start: number, end?: number);\n\nfo";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;
        let help = provider.get_signature_help(root, Position::new(7, 2), &mut cache);
        assert!(
            help.is_none(),
            "expected no help before '(' while editing identifier, got {}",
            help.as_ref()
                .map(|h| h.signatures[h.active_signature as usize].label.as_str())
                .unwrap_or_default()
        );
    }

    #[test]
    fn no_signature_help_after_closing_paren() {
        let source = "declare function foo(start: number, end?: number): void;\nfoo(10)";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;
        let help = provider.get_signature_help(root, Position::new(1, 7), &mut cache);
        assert!(
            help.is_none(),
            "expected no help after closing paren, got {}",
            help.as_ref()
                .map(|h| h.signatures[h.active_signature as usize].label.as_str())
                .unwrap_or_default()
        );
    }

    #[test]
    fn no_signature_help_for_private_constructor_new_call() {
        let source = "class A { private constructor() {} }\nnew A(";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;
        let help = provider.get_signature_help(root, Position::new(1, 6), &mut cache);
        assert!(
            help.is_none(),
            "expected no help for private constructor call, got {}",
            help.as_ref()
                .map(|h| h.signatures[h.active_signature as usize].label.as_str())
                .unwrap_or_default()
        );
    }

    #[test]
    fn no_signature_help_for_protected_constructor_new_call() {
        let source = "class A { protected constructor() {} }\nnew A(";
        let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
        let root = parser.parse_source_file();

        let mut binder = BinderState::new();
        binder.bind_source_file(parser.get_arena(), root);

        let interner = TypeInterner::new();
        let line_map = LineMap::build(source);
        let provider = SignatureHelpProvider::new(
            parser.get_arena(),
            &binder,
            &line_map,
            &interner,
            source,
            "test.ts".to_string(),
        );

        let mut cache = None;
        let help = provider.get_signature_help(root, Position::new(1, 6), &mut cache);
        assert!(
            help.is_none(),
            "expected no help for protected constructor call, got {}",
            help.as_ref()
                .map(|h| h.signatures[h.active_signature as usize].label.as_str())
                .unwrap_or_default()
        );
    }
}

#[cfg(test)]
#[path = "../tests/signature_help_tests.rs"]
mod signature_help_tests;
