//! Function, method, and arrow function type resolution.
include!("function_type_large_methods/get_type_of_function_impl_14_1.rs");

mod contextual_arity;
mod function_name_diagnostics;
mod js_prototype;
mod jsx_body_context;

use super::function_type_helpers::{
    ExpressionBodyReturnCheckCtx, FunctionBodyReturnTypeCtx, FunctionFinalReturnTypeCtx,
    GeneratorBodyReturnCheckCtx,
};
use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::query_boundaries::common::ContextualTypeContext;
use crate::query_boundaries::type_checking_utilities as type_query;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeId, TypeParamInfo};
impl<'a> CheckerState<'a> {
    /// Get type of function declaration/expression/arrow.
    pub(crate) fn get_type_of_function(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_function_impl(idx, &TypingRequest::NONE)
    }

    __tsz_split_function_type_get_type_of_function_impl_14_1!();
}

#[cfg(test)]
mod tests;
