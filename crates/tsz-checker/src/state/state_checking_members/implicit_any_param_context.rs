//! Deferral of implicit-`any` (TS7006) parameter diagnostics for function
//! expressions whose parameters are contextually typed by an enclosing
//! variable-declaration annotation.
//!
//! Split out of `implicit_any_checks` to keep each source file under the
//! checker's 2000-line cap.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

impl<'a> CheckerState<'a> {
    pub(super) fn enclosing_function_for_parameter_name(
        &self,
        param_name: NodeIndex,
    ) -> Option<NodeIndex> {
        let param_idx = self.ctx.arena.get_extended(param_name)?.parent;
        let func_idx = self.ctx.arena.get_extended(param_idx)?.parent;
        let func_node = self.ctx.arena.get(func_idx)?;
        (func_node.kind == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
            || func_node.kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION)
            .then_some(func_idx)
    }

    pub(super) fn parameter_has_deferred_explicit_context(
        &mut self,
        param_name: NodeIndex,
    ) -> bool {
        let Some(func_idx) = self.enclosing_function_for_parameter_name(param_name) else {
            return false;
        };
        // Walk up to the nearest enclosing variable declaration, capturing its
        // (Copy) type-annotation node. Stop at an enclosing function — a
        // parameter whose function is itself nested inside another function is
        // not directly contextually typed by an outer variable annotation.
        let mut current = self.ctx.arena.get_extended(func_idx).map(|ext| ext.parent);
        let mut annotation: Option<NodeIndex> = None;
        while let Some(parent_idx) = current {
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };
            let parent_kind = parent_node.kind;
            if parent_kind == tsz_parser::parser::syntax_kind_ext::VARIABLE_DECLARATION {
                annotation = self
                    .ctx
                    .arena
                    .get_variable_declaration(parent_node)
                    .map(|var_decl| var_decl.type_annotation)
                    .filter(|type_annotation| type_annotation.is_some());
                break;
            }
            if parent_kind == tsz_parser::parser::syntax_kind_ext::ARROW_FUNCTION
                || parent_kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_kind == tsz_parser::parser::syntax_kind_ext::FUNCTION_DECLARATION
            {
                return false;
            }
            current = self
                .ctx
                .arena
                .get_extended(parent_idx)
                .map(|ext| ext.parent);
        }
        let Some(annotation) = annotation else {
            return false;
        };
        if !self.explicit_annotation_can_defer_implicit_any_context(annotation) {
            return false;
        }
        // The annotation only defers the implicit-any check for a parameter it can
        // actually supply a contextual type for, matching tsc's contextual typing:
        //   1. When the function expression declares more *required* parameters
        //      than the annotated signature accepts (and that signature has no
        //      rest parameter), contextual typing is discarded for *every*
        //      parameter — they are all genuinely implicit `any`.
        //   2. Otherwise, contextual typing is applied per position: a parameter
        //      whose position is beyond the signature's parameter count (with no
        //      rest parameter) — e.g. a trailing optional param the signature does
        //      not declare — still receives no contextual type and is implicit
        //      `any`.
        // Both are expressed via `contextual_signature_accepts_required_callback_params`,
        // which reports whether the signature covers a given (1-based) arity.
        self.annotation_defers_implicit_any_for_param(func_idx, annotation, param_name)
    }

    /// Whether the variable-declaration `annotation` supplies a contextual type
    /// for the parameter named `param_name` of the function at `func_idx`. It
    /// does when the signature both accepts the expression's required arity and
    /// covers this parameter's own position; otherwise the parameter is implicit
    /// `any` and its diagnostic must not be deferred.
    fn annotation_defers_implicit_any_for_param(
        &mut self,
        func_idx: NodeIndex,
        annotation: NodeIndex,
        param_name: NodeIndex,
    ) -> bool {
        let counts = {
            let Some(func_node) = self.ctx.arena.get(func_idx) else {
                return true;
            };
            let Some(func) = self.ctx.arena.get_function(func_node) else {
                return true;
            };
            let mut required = 0usize;
            // Position of `param_name` among the non-`this` parameters (0-based),
            // mirroring `get_type_of_function`'s `contextual_index`.
            let mut contextual_index = 0usize;
            let mut found_index: Option<usize> = None;
            for &param_idx in &func.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if self.is_this_parameter_name(param.name) {
                    continue;
                }
                if param.name == param_name {
                    found_index = Some(contextual_index);
                }
                if !param.question_token && param.initializer.is_none() && !param.dot_dot_dot_token
                {
                    required += 1;
                }
                contextual_index += 1;
            }
            found_index.map(|index| (required, index))
        };
        // Parameter not found among the function's own parameters (e.g. a binding
        // element): keep the prior deferral behavior.
        let Some((required_non_this_param_count, contextual_index)) = counts else {
            return true;
        };
        // The annotation supplies a contextual type for this parameter only when
        // its signature both accepts the expression's required arity and covers
        // this parameter's own position. Since a signature covers a `(1-based)`
        // arity `n` exactly when it has a rest parameter or at least `n`
        // parameters, "covers both" reduces to covering the larger of the two
        // arities — a single query (avoiding a redundant signature normalization).
        let required_arity = required_non_this_param_count.max(contextual_index + 1);
        let annotation_type = self.get_type_from_type_node(annotation);
        self.contextual_signature_accepts_required_callback_params(annotation_type, required_arity)
    }
}
