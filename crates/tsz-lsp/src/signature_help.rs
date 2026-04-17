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
            && let Some(help) = self.signature_help_for_textual_type_arguments(
                root, offset, type_cache,
            )
        {
            return Some(help);
        }

        // 1. Find the deepest node at the cursor
        let leaf_node = find_node_at_or_before_offset(self.arena, offset, self.source_text);

        // 2. Walk up to find the nearest CallExpression, NewExpression, or TaggedTemplateExpression
        let Some((call_node_idx, call_site, call_kind)) =
            self.find_containing_call(leaf_node, offset)
        else {
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
        if !in_type_argument_list {
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

        let supplied_argument_types = self.argument_type_texts(&call_site, &mut checker);

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
                        if cursor_offset > delim_pos {
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

    /// For a `super` keyword node, walk up to find the enclosing class, then
    /// return the expression from its `extends` clause (the base class reference).
    /// This lets us resolve the base class symbol for signature help on `super()`.
    fn find_base_class_expression(&self, super_idx: NodeIndex) -> Option<NodeIndex> {
        let mut current = super_idx;
        let mut depth = 0;
        while current.is_some() && depth < 100 {
            let node = self.arena.get(current)?;
            if node.kind == syntax_kind_ext::CLASS_DECLARATION
                || node.kind == syntax_kind_ext::CLASS_EXPRESSION
            {
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
            if last_end < end && seen_non_omitted > 0 {
                if !self.has_comma_between(last_end as u32, cursor_offset) {
                    return (seen_non_omitted - 1) as u32;
                }
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
        if data.type_arguments.is_none() {
            return None;
        }

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
        let span_text = self.source_text.get(span_start..span_end).unwrap_or_default();
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

        for (param, type_text) in signature
            .info
            .parameters
            .iter_mut()
            .zip(param_type_texts.into_iter())
        {
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
                    if text == "any" || text == "unknown" || text == "error" {
                        let start = arg_node.pos as usize;
                        let end = (arg_node.end as usize).min(self.source_text.len());
                        if start < end {
                            let raw = self.source_text[start..end].trim();
                            if Self::is_numeric_literal_type(raw) {
                                text = "number".to_string();
                            } else if raw.starts_with('"') || raw.starts_with('\'') {
                                text = "string".to_string();
                            } else if raw == "true" || raw == "false" {
                                text = "boolean".to_string();
                            }
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
                    if text == "any" || text == "unknown" || text == "error" {
                        let start = expr_node.pos as usize;
                        let end = (expr_node.end as usize).min(self.source_text.len());
                        if start < end {
                            let raw = self.source_text[start..end].trim();
                            if Self::is_numeric_literal_type(raw) {
                                text = "number".to_string();
                            } else if raw.starts_with('"') || raw.starts_with('\'') {
                                text = "string".to_string();
                            } else if raw == "true" || raw == "false" {
                                text = "boolean".to_string();
                            }
                        }
                    }
                    types.push(text);
                }
            }
        }
        types
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

    fn primitive_mask(kind: PrimitiveKind) -> u8 {
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
        let mut decls = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !decls.contains(&symbol.value_declaration) {
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
        let mut decls = symbol.declarations.clone();
        if symbol.value_declaration.is_some() && !decls.contains(&symbol.value_declaration) {
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
                "foo(x: unknown, y: unknown): unknown",
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
}

#[cfg(test)]
#[path = "../tests/signature_help_tests.rs"]
mod signature_help_tests;
