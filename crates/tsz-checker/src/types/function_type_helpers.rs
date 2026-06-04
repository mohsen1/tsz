use crate::computation::complex::{
    expression_needs_contextual_return_type, is_contextually_sensitive,
};

use crate::context::TypingRequest;

use crate::context::speculation::DiagnosticSpeculationSnapshot;

use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

use crate::query_boundaries::common::ContextualTypeContext;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::{FunctionShape, ParamInfo, TypeId, TypeParamInfo};

/// Context for TS2366/TS2355/TS7030 function return completeness checks.
pub(crate) struct FunctionReturnCheckCtx {
    /// Whether this is a function declaration (checked separately).
    pub(crate) is_function_declaration: bool,
    /// The function body node.
    pub(crate) body: NodeIndex,
    /// The function node itself.
    pub(crate) func_idx: NodeIndex,
    /// The annotated return type, if any.
    pub(crate) annotated_return_type: Option<TypeId>,
    /// The inferred or annotated return type.
    pub(crate) return_type: TypeId,
    /// Whether an explicit return type annotation is present.
    pub(crate) has_type_annotation: bool,
    /// The type annotation node (used as error anchor).
    pub(crate) type_annotation: NodeIndex,
    /// Whether this function is a generator.
    pub(crate) function_is_generator: bool,
    /// Optional name node for TS7030 (implicit return) anchoring.
    pub(crate) name_node: Option<NodeIndex>,
    /// The overall expression/declaration index used for diagnostics.
    pub(crate) idx: NodeIndex,
}

pub(crate) struct FunctionFinalReturnTypeCtx {
    pub(crate) has_type_annotation: bool,
    pub(crate) function_is_async: bool,
    pub(crate) function_is_generator: bool,
    pub(crate) annotated_return_type: Option<TypeId>,
    pub(crate) return_type: TypeId,
    pub(crate) final_generator_yield_type: Option<TypeId>,
    pub(crate) early_gen_return_type: Option<TypeId>,
    pub(crate) early_gen_next_type: Option<TypeId>,
}

pub(crate) struct GeneratorBodyReturnCheckCtx<'b> {
    pub(crate) is_generator: bool,
    pub(crate) has_type_annotation: bool,
    pub(crate) annotated_return_type: Option<TypeId>,
    pub(crate) return_type: TypeId,
    pub(crate) type_annotation: NodeIndex,
    pub(crate) idx: NodeIndex,
    pub(crate) function_is_async: bool,
    pub(crate) early_yield_type: Option<TypeId>,
    pub(crate) name_node: Option<NodeIndex>,
    pub(crate) name_for_error: Option<&'b str>,
}

pub(crate) struct FunctionBodyReturnTypeCtx {
    pub(crate) idx: NodeIndex,
    pub(crate) is_generator: bool,
    pub(crate) has_type_annotation: bool,
    pub(crate) annotated_return_type: Option<TypeId>,
    pub(crate) return_type: TypeId,
    pub(crate) type_annotation: NodeIndex,
    pub(crate) is_async_for_context: bool,
    pub(crate) has_contextual_return: bool,
    pub(crate) contextual_void_return_exception: bool,
    pub(crate) return_context_for_circularity: Option<TypeId>,
    pub(crate) jsdoc_return_context: Option<TypeId>,
    pub(crate) early_gen_return_type: Option<TypeId>,
}

pub(crate) struct ExpressionBodyReturnCheckCtx {
    pub(crate) idx: NodeIndex,
    pub(crate) body: NodeIndex,
    pub(crate) is_closure: bool,
    pub(crate) has_type_annotation: bool,
    pub(crate) is_async_for_context: bool,
    pub(crate) contextual_void_return_exception: bool,
    pub(crate) expected_expression_return_type: Option<TypeId>,
    pub(crate) jsdoc_return_context: Option<TypeId>,
}

struct DirectExpressionBodyReturnMismatchCtx {
    idx: NodeIndex,
    body: NodeIndex,
    expected_return_type: TypeId,
    actual_return: TypeId,
    actual_return_node: NodeIndex,
    actual_return_uses_jsdoc_cast: bool,
    is_closure: bool,
    is_async_for_context: bool,
}

include!("function_type_helpers_parts/part1.rs");
include!("function_type_helpers_parts/part2.rs");
