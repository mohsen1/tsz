//! JS-parameter optionality signals (#17227).
//!
//! `tsc` keeps two independent facts about a JavaScript function parameter that
//! `tsz` had previously collapsed into a single `optional` bit:
//!
//! - weak **call-arity** leniency (`minArgumentCount`): a bare, unannotated JS
//!   parameter may be omitted at a call site, and
//! - the **displayed** optionality: `tsc` still renders such a bare parameter
//!   as required — `(tree: any) => void`, never `(tree?: any)`.
//!
//! Collapsing them made every bare JS parameter render with a spurious `?`.
//! This module computes both signals so the checker can keep `ParamInfo::optional`
//! (which drives arity and structural subtyping) while setting
//! `ParamInfo::suppress_display_optional` for the bare case, which only the
//! printer consults via `ParamInfo::displays_optional`.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ParameterData;

impl<'a> CheckerState<'a> {
    /// Compute the two independent JS-parameter optionality signals `tsc` keeps
    /// separate, returning `(js_implicit_optional, suppress_display_optional)`.
    ///
    /// - `js_implicit_optional`: a bare, unannotated JS parameter is optional
    ///   for *weak call-arity* (`minArgumentCount`) unless a JSDoc `@param`
    ///   type tag or a `@type` function annotation marks it required.
    /// - `suppress_display_optional`: `true` only when that implicit rule is the
    ///   *sole* source of optionality — no real `?`, no initializer, and no
    ///   JSDoc bracket/`=`-optional tag — so the printer renders the parameter
    ///   as required (`tree: any`, never `tree?: any`), exactly as `tsc` does.
    pub(crate) fn js_param_optionality_signals(
        &self,
        idx: NodeIndex,
        param: &ParameterData,
        has_jsdoc_type_function: bool,
        func_jsdoc: Option<&str>,
        jsdoc_param_names: &[String],
        contextual_index: usize,
    ) -> (bool, bool) {
        if !self.is_js_file() || param.type_annotation.is_some() {
            return (false, false);
        }
        let pname =
            self.effective_jsdoc_param_name(param.name, jsdoc_param_names, contextual_index);
        // `func_jsdoc` falls back to a re-scan so a detached comment is still
        // found, matching the original inline computation.
        let jsdoc_required = func_jsdoc
            .map(str::to_owned)
            .or_else(|| self.find_jsdoc_for_function(idx))
            .is_some_and(|jsdoc| Self::jsdoc_has_required_param_tag(&jsdoc, &pname));
        let js_implicit_optional = !has_jsdoc_type_function && !jsdoc_required;
        let suppress_display_optional = js_implicit_optional
            && !param.question_token
            && param.initializer.is_none()
            && !func_jsdoc.is_some_and(|jsdoc| {
                Self::is_jsdoc_param_optional_by_brackets(jsdoc, &pname)
                    || Self::extract_jsdoc_param_type_string(jsdoc, &pname)
                        .is_some_and(|t| t.trim().ends_with('='))
            });
        (js_implicit_optional, suppress_display_optional)
    }
}
