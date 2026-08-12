//! JS parameter optionality classification for signature building (#17227).
//!
//! In a JS file, a parameter with no type annotation is implicitly `optional`
//! so call-arity checking stays lenient — but tsc never DISPLAYS that
//! leniency as `?`. Only a written `?`, an initializer, or a JSDoc optional
//! marker (`@param [a]` / `@param {T=} a`) keeps its `?` in rendered
//! signatures. This module classifies each parameter into those two
//! independent bits; the arity-only bit feeds the solver's display mask (see
//! `function_with_arity_optional_mask`) and never touches
//! `required_param_count` or subtyping.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;

/// The two optionality signals of one JS-file parameter.
pub(crate) struct JsParamOptionality {
    /// Parameter is implicitly optional for call-arity purposes.
    pub(crate) implicit_optional: bool,
    /// `optional` would exist ONLY for arity leniency: display it required.
    pub(crate) arity_only_optional: bool,
}

impl<'a> CheckerState<'a> {
    /// Classify one parameter's JS implicit optionality. Both bits are
    /// `false` for TS files, annotated parameters, and `@type`-function
    /// annotated signatures.
    pub(crate) fn js_param_optionality(
        &mut self,
        func_idx: NodeIndex,
        param: &tsz_parser::parser::node::ParameterData,
        has_jsdoc_type_function: bool,
        func_jsdoc: Option<&str>,
        jsdoc_param_names: &[String],
        contextual_index: usize,
    ) -> JsParamOptionality {
        if !self.is_js_file() || has_jsdoc_type_function || param.type_annotation.is_some() {
            return JsParamOptionality {
                implicit_optional: false,
                arity_only_optional: false,
            };
        }
        let jsdoc = func_jsdoc
            .map(str::to_owned)
            .or_else(|| self.find_jsdoc_for_function(func_idx));
        let pname =
            self.effective_jsdoc_param_name(param.name, jsdoc_param_names, contextual_index);
        let implicit_optional = !jsdoc
            .as_deref()
            .is_some_and(|jsdoc| Self::jsdoc_has_required_param_tag(jsdoc, &pname));
        let arity_only_optional = implicit_optional
            && !param.question_token
            && param.initializer.is_none()
            && !jsdoc.as_deref().is_some_and(|jsdoc| {
                Self::is_jsdoc_param_optional_by_brackets(jsdoc, &pname)
                    || Self::extract_jsdoc_param_type_string(jsdoc, &pname)
                        .is_some_and(|t| t.trim().ends_with('='))
            });
        JsParamOptionality {
            implicit_optional,
            arity_only_optional,
        }
    }

    /// JS files: a function with no AST parameters whose body references
    /// `arguments` synthesizes a call signature from its JSDoc `@param` tags
    /// so calls are checked against the declared JSDoc parameter types.
    /// Mirrors tsc's `getSignatureFromDeclaration` JSDoc fallback.
    pub(crate) fn jsdoc_arguments_fallback_params(
        &mut self,
        func_idx: NodeIndex,
        jsdoc: &str,
        params: &mut Vec<tsz_solver::ParamInfo>,
    ) {
        let function_has_name = self.function_has_effective_name(func_idx);
        let comment_pos = self.get_jsdoc_comment_pos_for_function(func_idx);
        for (pname, _) in Self::extract_jsdoc_param_names(jsdoc) {
            if pname == "this" {
                continue;
            }
            let is_rest = Self::jsdoc_param_is_rest(jsdoc, &pname);
            // tsc only promotes {...T} → T[] when the function has an
            // effective name; anonymous expressions leave the type as T
            // and TS8029 is emitted by check_jsdoc_param_tag_names.
            if is_rest && !function_has_name {
                continue;
            }
            let is_optional = Self::is_jsdoc_param_optional_by_brackets(jsdoc, &pname)
                || Self::extract_jsdoc_param_type_string(jsdoc, &pname)
                    .is_some_and(|t| t.trim().ends_with('='));
            let Some(type_id) = self.resolve_jsdoc_param_type_with_pos(jsdoc, &pname, comment_pos)
            else {
                continue;
            };
            // `resolve_jsdoc_param_type_with_pos` already strips the `...`
            // prefix and returns the element type. For rest params we store
            // the element type and set `rest: true`, matching the AST-param
            // JSDoc path.
            let name = self.ctx.types.intern_string(&pname);
            params.push(crate::query_boundaries::signature_building::param_info(
                Some(name),
                type_id,
                is_optional,
                is_rest,
            ));
        }
    }
}
