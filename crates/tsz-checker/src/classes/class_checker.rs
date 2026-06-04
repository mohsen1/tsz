use std::borrow::Cow;

use crate::classes_domain::class_summary::ClassChainSummary;

use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

use crate::query_boundaries::class::{
    should_report_member_type_mismatch, should_report_member_type_mismatch_bivariant,
};

use crate::query_boundaries::common::TypeSubstitution;

use crate::state::CheckerState;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeAccess;

use tsz_parser::parser::syntax_kind_ext;

use tsz_scanner::SyntaxKind;

use tsz_solver::TypeId;

struct OverloadCompatCtx<'a> {
    member_name: &'a str,
    member_type: TypeId,
    member_name_idx: NodeIndex,
    is_static: bool,
    derived_class_name: &'a str,
    base_class_name: &'a str,
    base_info: &'a ClassMemberInfo,
    base_chain_summary: &'a ClassChainSummary,
    derived_overloads: &'a rustc_hash::FxHashMap<String, TypeId>,
    substitution: &'a TypeSubstitution,
    overload_compat_checked: &'a mut rustc_hash::FxHashSet<(String, bool)>,
}

/// Format a property name for error messages.
///
/// If the property name is not a valid identifier (e.g., `2.0`, `my-prop`),
/// it gets wrapped in single quotes. TSC does this to match the original
/// source syntax for string literal property names.
pub(crate) fn format_property_name_for_diagnostic(name: &str) -> String {
    if needs_property_name_quotes(name) {
        format!("'{name}'")
    } else {
        name.to_string()
    }
}

/// Returns `true` if a property name needs to be quoted in diagnostics
/// (i.e., it is not a valid JS identifier or pure numeric literal).
fn needs_property_name_quotes(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Computed property names wrapped in brackets (e.g., [Symbol.asyncIterator])
    // are displayed as-is without quotes.
    if name.starts_with('[') && name.ends_with(']') {
        return false;
    }
    // Pure numeric property names (e.g., "0", "42") don't need quotes
    if name.chars().all(|ch| ch.is_ascii_digit()) {
        return false;
    }
    // Check if it's a valid identifier
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' || first == '$' => {
            !chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '$')
        }
        _ => true,
    }
}

pub(crate) const fn base_class_name_for_diagnostic(name: &str) -> Cow<'_, str> {
    Cow::Borrowed(name)
}

/// Extracted info about a single class member (property, method, or accessor).
#[derive(Clone)]
pub(crate) struct ClassMemberInfo {
    pub(crate) name: String,
    pub(crate) type_id: TypeId,
    pub(crate) name_idx: NodeIndex,
    pub(crate) visibility: MemberVisibility,
    pub(crate) is_method: bool,
    pub(crate) is_static: bool,
    pub(crate) is_accessor: bool,
    /// True when this entry comes from a `SET_ACCESSOR` declaration (always
    /// implies `is_accessor`). Used to recognize the setter half of an accessor
    /// pair: tsc treats an accessor pair as one property whose type is the
    /// getter return type, so override-compat (TS2416/TS2417) must run once
    /// per pair instead of independently on the setter parameter type.
    pub(crate) is_setter: bool,
    pub(crate) is_abstract: bool,
    pub(crate) has_override: bool,
    /// True when `override` comes from a JSDoc `@override` tag (not the keyword).
    /// Used to emit TS4118-4123 (JSDoc variants) instead of TS4112-4117.
    pub(crate) is_jsdoc_override: bool,
    pub(crate) has_dynamic_name: bool,
    /// True when the member name is a computed property whose expression is NOT
    /// a direct string/number literal. tsc uses this (`isComputedNonLiteralName`)
    /// to skip `noImplicitOverride` checks for computed names like `[someVar]`.
    pub(crate) has_computed_non_literal_name: bool,
    /// True when the member comes from a merged interface declaration (not a class
    /// property declaration). Used to skip TS2610/TS2611 accessor/property mismatch
    /// checks, since interface-sourced members can be freely overridden by accessors.
    pub(crate) from_interface: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemberVisibility {
    Public,
    Protected,
    Private,
}

/// Build the elaboration line tsc appends to TS2415 (class incorrectly extends
/// base class) when the conflict is purely a visibility/branding mismatch on a
/// single member.
///
/// Returns `None` when the conflict is not a pure visibility one (callers fall
/// back to the bare TS2415 message).
///
/// tsc message catalog:
/// - both Private (different declarations): "Types have separate declarations of a private property '{name}'."
/// - base Private, derived Public/Protected: "Property '{name}' is private in type '{base}' but not in type '{derived}'."
/// - base Public, derived Private/Protected: "Property '{name}' is {vis} in type '{derived}' but not in type '{base}'."
/// - base Public, derived Protected: "Property '{name}' is protected in type '{derived}' but public in type '{base}'."
/// - both Protected (different declarations): "Types have separate declarations of a protected property '{name}'."
pub(crate) fn visibility_conflict_elaboration(
    derived_visibility: MemberVisibility,
    base_visibility: MemberVisibility,
    display_name: &str,
    derived_class_name: &str,
    base_class_name: &str,
) -> Option<String> {
    use MemberVisibility::*;
    match (derived_visibility, base_visibility) {
        (Private, Private) => Some(format!(
            "Types have separate declarations of a private property '{display_name}'."
        )),
        (Protected, Protected) => Some(format!(
            "Types have separate declarations of a protected property '{display_name}'."
        )),
        (_, Private) => Some(format!(
            "Property '{display_name}' is private in type '{base_class_name}' but not in type '{derived_class_name}'."
        )),
        (Private, _) => Some(format!(
            "Property '{display_name}' is private in type '{derived_class_name}' but not in type '{base_class_name}'."
        )),
        (Protected, Public) => Some(format!(
            "Property '{display_name}' is protected in type '{derived_class_name}' but public in type '{base_class_name}'."
        )),
        (Public, Protected) => Some(format!(
            "Property '{display_name}' is protected in type '{base_class_name}' but public in type '{derived_class_name}'."
        )),
        (Public, Public) => None,
    }
}

include!("class_checker_parts/part1.rs");
include!("class_checker_parts/part2.rs");
