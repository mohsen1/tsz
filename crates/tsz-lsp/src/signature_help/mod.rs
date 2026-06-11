//! Signature Help implementation for LSP.
//!
//! Provides function signature information and active parameter highlighting
//! when typing arguments in a call expression.
//!
//! # Phase structure
//!
//! | Module         | Responsibility                                               |
//! |----------------|--------------------------------------------------------------|
//! | `trigger`      | Locate the containing call site; resolve callee name;        |
//! |                | determine active parameter index.                            |
//! | `phases`       | `SignatureHelpTriggerContext`, `TypeArgumentContext`,         |
//! |                | `SignatureHelpDisplaySelection`; top-level orchestration     |
//! |                | helpers (collect candidates, select display, span).          |
//! | `candidates`   | Build `SignatureCandidate` lists from solver types           |
//! |                | (`shapes`, intrinsic fallback).                              |
//! | `overload`     | Active-signature selection and argument-type scoring         |
//! |                | (`selection`).                                               |
//! | `display`      | Compute the applicable span; apply type-param substitutions. |
//! | `contextual`   | Textual / contextual fallback paths.                         |
//! | `docs`         | `JSDoc` documentation enrichment.                            |

use rustc_hash::FxHashMap;

use crate::jsdoc::{JsdocTag, ParsedJsdoc, inline_param_jsdocs, jsdoc_for_node, parse_jsdoc};
use crate::resolver::{ScopeCache, ScopeCacheStats};
use crate::utils::find_node_at_or_before_offset;
use tsz_binder::symbol_flags;
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
mod display;
mod docs;
mod phases;
mod selection;
mod shapes;
#[cfg(test)]
#[path = "internal_tests.rs"]
mod signature_help_internal_tests;
mod trigger;
mod unicode_identifier;

pub(super) use display::apply_type_param_substitution;
use phases::TypeArgumentContext;

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
        let trigger = self.signature_help_trigger_context(position)?;
        let offset = trigger.offset;

        // In incomplete generic invocations like `foo(bar<|)`, the parser may
        // bind us to an outer call expression. Prefer explicit textual
        // type-argument handling when we can detect an unclosed `<...` span.
        if self.find_textual_type_argument_trigger(offset).is_some()
            && let Some(help) =
                self.signature_help_for_textual_type_arguments(root, offset, type_cache)
        {
            return Some(help);
        }

        let Some((call_node_idx, call_site, call_kind)) =
            self.find_containing_call(trigger.leaf_node, offset)
        else {
            if let Some(help) = self.signature_help_for_contextual_variable_initializer(
                root,
                trigger.leaf_node,
                offset,
                type_cache,
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

        let active_parameter = if let Some(ctx) = type_argument_context {
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

        let is_super_call = self
            .arena
            .get(callee_expr)
            .is_some_and(|n| n.kind == SyntaxKind::SuperKeyword as u16);

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
        let mut checker = self.checker_with_cache(type_cache);

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

        // For super() calls, extract construct signatures (since super invokes the base constructor)
        let effective_call_kind = if is_super_call {
            CallKind::New
        } else {
            call_kind
        };
        let has_explicit_type_args =
            !in_type_argument_list && Self::call_site_has_explicit_type_args(&call_site);
        let explicit_type_arg_texts =
            self.explicit_type_argument_texts(&call_site, has_explicit_type_args);
        let mut signatures = self.collect_signature_candidates_for_call(
            callee_expr,
            callee_type,
            &mut checker,
            &callee_name,
            effective_call_kind,
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

        let display = self.select_signature_help_display(
            call_node_idx,
            &call_site,
            type_argument_context,
            offset,
            &signatures,
            active_parameter,
            &supplied_argument_types,
        );

        Some(SignatureHelp {
            signatures: signatures.into_iter().map(|sig| sig.info).collect(),
            active_signature: display.active_signature,
            active_parameter: display.active_parameter,
            argument_count: display.argument_count as u32,
            applicable_span_start: display.span_start,
            applicable_span_length: display.span_length,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/signature_help_tests.rs"]
mod signature_help_tests;
