//! Shared JSDoc data structures.
//!
//! These types represent parsed JSDoc annotations and are used across
//! all JSDoc subsystem modules (parsing, resolution, params, diagnostics).

/// Parsed `@typedef` or `@callback` definition from a JSDoc comment.
#[derive(Clone)]
pub(crate) struct JsdocTypedefInfo {
    pub(crate) base_type: Option<String>,
    pub(crate) properties: Vec<JsdocPropertyTagInfo>,
    pub(crate) template_params: Vec<JsdocTemplateParamInfo>,
    /// If this is a `@callback` definition, holds the parsed parameter and return info.
    pub(crate) callback: Option<JsdocCallbackInfo>,
}
#[derive(Clone)]
pub(crate) struct JsdocTemplateParamInfo {
    pub(crate) name: String,
    pub(crate) constraint: Option<String>,
}
#[derive(Clone)]
pub(crate) struct JsdocPropertyTagInfo {
    pub(crate) name: String,
    pub(crate) type_expr: String,
    pub(crate) optional: bool,
}
#[derive(Clone)]
pub(crate) struct JsdocParamTagInfo {
    pub(crate) name: String,
    pub(crate) type_expr: Option<String>,
    pub(crate) optional: bool,
    pub(crate) rest: bool,
}
/// Parsed `@callback` information: parameter names/types and return type/predicate.
#[derive(Clone)]
pub(crate) struct JsdocCallbackInfo {
    pub(crate) params: Vec<JsdocParamTagInfo>,
    pub(crate) return_type: Option<String>, // raw return type expression
    /// Parsed type predicate from `@return {x is Type}`.
    pub(crate) predicate: Option<(bool, String, Option<String>)>, // (is_asserts, param_name, type_str)
}
