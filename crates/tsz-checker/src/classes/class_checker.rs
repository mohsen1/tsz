//! Class/interface declaration checking (inheritance, implements, abstract members).

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

// =============================================================================
// Class and Interface Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Report explicit/implicit override errors for constructor parameter properties.
    pub(crate) fn check_constructor_parameter_property_overrides(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        base_class_idx: Option<NodeIndex>,
        base_chain_summary: Option<&ClassChainSummary>,
        base_class_name: &str,
        derived_class_name: &str,
        base_instance_member_names: &rustc_hash::FxHashSet<String>,
        no_implicit_override: bool,
    ) {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }

            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if !self.has_parameter_property_modifier(&param.modifiers) {
                    continue;
                }
                let Some(param_name) = self.get_property_name(param.name) else {
                    continue;
                };

                let has_override = self.has_override_modifier(&param.modifiers)
                    || self.has_jsdoc_override_tag(param_idx);
                let base_member = match (base_class_idx, base_chain_summary) {
                    (Some(base_idx), Some(summary)) => {
                        let _ = base_idx;
                        summary.lookup(&param_name, false, true).cloned()
                    }
                    (Some(base_idx), None) => {
                        self.find_member_in_class_chain(base_idx, &param_name, false, 0, true)
                    }
                    (None, _) => None,
                };

                if has_override {
                    if base_class_idx.is_none() {
                        // tsc points at the parameter declaration (starting at the
                        // first modifier like 'public'), not just the identifier name.
                        // Use ctx.error() directly to bypass normalized_anchor_span
                        // which would strip modifiers and point at just the name.
                        self.ctx.error(
                            param_node.pos,
                            param_node.end - param_node.pos,
                            crate::diagnostics::format_message(
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                                &[base_class_name],
                            ),
                            diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_CONTAINING_CLASS_DOES_N,
                        );
                        continue;
                    }

                    if base_member.is_none() {
                        // tsc points at the parameter declaration (starting at the
                        // first modifier like 'public'), not just the identifier name.
                        if let Some(suggestion) = self
                            .find_override_name_suggestion(base_instance_member_names, &param_name)
                        {
                            self.ctx.error(
                                param_node.pos,
                                param_node.end - param_node.pos,
                                crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                    &[base_class_name, &suggestion],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                            );
                        } else {
                            self.ctx.error(
                                param_node.pos,
                                param_node.end - param_node.pos,
                                crate::diagnostics::format_message(
                                    diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                    &[base_class_name],
                                ),
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                            );
                        }
                    }
                } else if no_implicit_override && base_member.is_some() {
                    // tsc points TS4115 at the parameter declaration (starting at the
                    // first modifier like 'public'), not just the identifier name.
                    self.ctx.error(
                        param_node.pos,
                        param_node.end - param_node.pos,
                        crate::diagnostics::format_message(
                            diagnostic_messages::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                            &[base_class_name],
                        ),
                        diagnostic_codes::THIS_PARAMETER_PROPERTY_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_ME,
                    );
                }

                // TS2610: constructor parameter property overrides a base accessor
                // A parameter property like `constructor(public p: string)` acts as an
                // instance property. If the base class defines the same name as an
                // accessor (get/set), this is an accessor/property kind mismatch.
                if let Some(ref base_info) = base_member
                    && base_info.is_accessor
                    && !base_info.is_abstract
                {
                    self.error_at_node(
                            param.name,
                            &format!(
                                "'{param_name}' is defined as an accessor in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an instance property."
                            ),
                            diagnostic_codes::IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP,
                        );
                }
            }
        }
    }

    // =========================================================================
    // Inheritance Checking
    // =========================================================================

    /// Check that property types in derived class are compatible with base class (error 2416).
    /// For each property/accessor in the derived class, checks if there's a corresponding
    /// member in the base class with incompatible type.
    pub(crate) fn check_property_inheritance_compatibility(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};

        // Find base class from heritage clauses (extends, not implements)
        // If there are no heritage clauses, we still need to check for
        // invalid `override` members (TS4112) since override requires extends.
        let heritage_clauses = match class_data.heritage_clauses {
            Some(ref hc) => hc,
            None => {
                // No heritage clauses — still check for override members (TS4112)
                let derived_class_name = if class_data.name.is_some() {
                    self.ctx
                        .arena
                        .get(class_data.name)
                        .and_then(|n| self.ctx.arena.get_identifier(n))
                        .map_or_else(
                            || String::from("(Anonymous class)"),
                            |id| id.escaped_text.clone(),
                        )
                } else {
                    String::from("(Anonymous class)")
                };
                self.report_overrides_without_base(
                    class_data,
                    &derived_class_name,
                    self.ctx.no_implicit_override(),
                );
                return;
            }
        };

        let mut base_class_idx: Option<NodeIndex> = None;
        let mut base_class_name = String::new();
        let mut base_type_argument_nodes: Option<Vec<NodeIndex>> = None;
        // Save heritage expression info for type-level fallback when AST resolution fails
        let mut heritage_expr_idx: Option<NodeIndex> = None;
        let mut heritage_type_idx: Option<NodeIndex> = None;
        let resolve_heritage_type_args = |checker: &Self, type_idx: NodeIndex| {
            checker
                .ctx
                .arena
                .get_expr_type_args(checker.ctx.arena.get(type_idx)?)
                .and_then(|expr_type_args| expr_type_args.type_arguments.as_ref())
        };
        // Track the base class symbol for namespace-merged static type check (TS2417).
        // Set when the heritage clause resolves to a class symbol. The actual TS2417
        // check only fires when the *derived* class has a merged namespace.
        let mut base_sym_for_ns_static_check: Option<tsz_binder::SymbolId> = None;

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (token = ExtendsKeyword = 96)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            // Get the first type in the extends clause (the base class)
            if let Some(&type_idx) = heritage.types.nodes.first()
                && let Some(type_node) = self.ctx.arena.get(type_idx)
            {
                // Handle both cases:
                // 1. ExpressionWithTypeArguments (e.g., Base<T>)
                // 2. Simple Identifier (e.g., Base)
                let (expr_idx, type_arguments, heritage_expr_for_type_idx) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                            expr_type_args.expression,
                        )
                    } else if let Some(call_expr) = self.ctx.arena.get_call_expr(type_node) {
                        (call_expr.expression, None, type_idx)
                    } else {
                        // For simple identifiers without type arguments, the type_node itself is the identifier
                        (type_idx, None, type_idx)
                    };
                heritage_expr_idx = Some(heritage_expr_for_type_idx);
                heritage_type_idx = Some(type_idx);
                if let Some(args) = type_arguments {
                    base_type_argument_nodes = Some(args.nodes.clone());
                }

                // Unwrap parenthesized expressions to find the actual base expression.
                // e.g., `class E extends (class { ... })` — the inner expr is a class expression.
                let mut resolved_expr_idx = expr_idx;
                while let Some(rn) = self.ctx.arena.get(resolved_expr_idx) {
                    if rn.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION
                        && let Some(paren) = self.ctx.arena.get_parenthesized(rn)
                    {
                        resolved_expr_idx = paren.expression;
                        continue;
                    }
                    break;
                }

                // Check if the base expression is a class expression directly
                let resolved_node = self.ctx.arena.get(resolved_expr_idx);
                let is_class_expr =
                    resolved_node.is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION);

                if is_class_expr {
                    // Direct class expression as base — use it directly
                    base_class_idx = Some(resolved_expr_idx);
                    if let Some(rn) = resolved_node
                        && let Some(cls) = self.ctx.arena.get_class(rn)
                        && cls.name.is_some()
                        && let Some(name_node) = self.ctx.arena.get(cls.name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    {
                        base_class_name = ident.escaped_text.clone();
                    } else {
                        base_class_name = String::from("(Anonymous class)");
                    }
                } else {
                    // Get the class name from the expression (identifier or property access)
                    if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
                        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                            base_class_name = ident.escaped_text.clone();
                        } else if let Some(access) = self.ctx.arena.get_access_expr(expr_node)
                            && let Some(name_node) = self.ctx.arena.get(access.name_or_argument)
                            && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
                        {
                            // e.g., `extends React.Component` — show rightmost name.
                            base_class_name = name_ident.escaped_text.clone();
                        }
                    }

                    // Find the base class declaration via heritage symbol resolution
                    // This handles namespace scoping correctly
                    if let Some(sym_id) = self.resolve_heritage_symbol(expr_idx) {
                        // Track the base symbol for the namespace-merged static check (TS2417).
                        // Always store the base symbol here; the check at line ~1731 only
                        // fires when the *derived* class has a merged namespace (which is the
                        // condition that can make `typeof Derived` incompatible with
                        // `typeof Base`). Previously we only stored the symbol when the
                        // *base* had a namespace, but tsc also reports TS2417 when the
                        // derived class's namespace introduces conflicting static members
                        // even if the base class has no namespace at all.
                        base_sym_for_ns_static_check = Some(sym_id);
                        // Resolve to an in-arena class declaration when possible.
                        // Cross-file/module heritage often resolves to symbols whose
                        // declaration nodes live in another arena; returning `None`
                        // here is intentional so the type-level fallback path can
                        // handle the base class structurally.
                        base_class_idx = self.get_class_declaration_from_symbol(sym_id);
                    }
                }
            }
            break; // Only one extends clause is valid
        }

        let derived_class_name = if class_data.name.is_some() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name) {
                if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
                    let mut name = ident.escaped_text.clone();
                    // Append type parameters for tsc parity: "Foo<T, U>"
                    self.append_type_param_names(&mut name, &class_data.type_parameters);
                    name
                } else {
                    String::from("(Anonymous class)")
                }
            } else {
                String::from("(Anonymous class)")
            }
        } else {
            String::from("(Anonymous class)")
        };
        // tsc does not enforce noImplicitOverride in ambient/declare class declarations.
        let is_ambient_class = self.has_declare_modifier(&class_data.modifiers);
        let no_implicit_override = self.ctx.no_implicit_override() && !is_ambient_class;

        let Some(base_idx) = base_class_idx else {
            // No AST-level class declaration found. Try type-level fallback for complex
            // heritage expressions (function calls, intersection types, etc.).
            if let Some(h_expr_idx) = heritage_expr_idx {
                let type_arguments =
                    heritage_type_idx.and_then(|tidx| resolve_heritage_type_args(self, tidx));
                if self.heritage_call_has_invalid_mixin_constructor_constraint(h_expr_idx)
                    && let Some(base_static_type) =
                        self.base_constructor_type_from_expression(h_expr_idx, type_arguments)
                {
                    self.error_at_node(
                        class_data.name,
                        &format!(
                            "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side '{}'.",
                            self.format_type(base_static_type)
                        ),
                        diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                    );
                }
                if let Some(instance_type) =
                    self.base_instance_type_from_expression(h_expr_idx, type_arguments)
                {
                    let heritage_sym_id = self.resolve_heritage_symbol(h_expr_idx);
                    let is_actual_lib_iterator =
                        self.heritage_reference_is_actual_lib_iterator(h_expr_idx);
                    let heritage_sym_id_for_display = self
                        .heritage_symbol_id_for_expression_display(
                            heritage_sym_id,
                            is_actual_lib_iterator,
                        );
                    // Use intersection display name if available (preserves "I1 & I2"
                    // instead of showing merged "{ m1: ...; m2: ... }")
                    let type_base_name = if is_actual_lib_iterator {
                        self.format_builtin_iterator_reference_with_type_arguments(type_arguments)
                    } else {
                        None
                    }
                    .or_else(|| {
                        self.format_heritage_class_symbol_reference(
                            heritage_sym_id_for_display,
                            type_arguments,
                        )
                    })
                    .or_else(|| self.intersection_instance_display_name(h_expr_idx, type_arguments))
                    .unwrap_or_else(|| {
                        self.format_heritage_instance_display(
                            instance_type,
                            h_expr_idx,
                            type_arguments,
                        )
                    });
                    let base_instance_member_names =
                        self.collect_property_names_from_type(instance_type);
                    let base_static_type =
                        self.base_constructor_type_from_expression(h_expr_idx, type_arguments);
                    let base_static_member_names = if let Some(static_type) = base_static_type {
                        self.collect_property_names_from_type(static_type)
                    } else {
                        rustc_hash::FxHashSet::default()
                    };
                    self.check_non_public_member_inheritance_conflicts_against_type(
                        class_data,
                        instance_type,
                        &derived_class_name,
                        &type_base_name,
                    );

                    self.check_override_members_against_type(
                        class_data,
                        &derived_class_name,
                        &type_base_name,
                        &base_instance_member_names,
                        &base_static_member_names,
                        no_implicit_override,
                        (Some(instance_type), base_static_type),
                    );
                    return;
                }
            }

            // True fallback: no extends clause resolved at all — emit TS4112
            self.report_overrides_without_base(
                class_data,
                &derived_class_name,
                no_implicit_override,
            );

            return;
        };

        // Get the base class data. If the resolved node is not a class declaration
        // (e.g., variable typed as intersection of constructors), use type-level fallback.
        let Some(base_node) = self.ctx.arena.get(base_idx) else {
            return;
        };

        let Some(base_class) = self.ctx.arena.get_class(base_node) else {
            // base_idx points to a non-class node (e.g., variable declaration).
            // Fall back to type-level resolution via base_instance_type_from_expression.
            if let Some(h_expr_idx) = heritage_expr_idx {
                let type_arguments =
                    heritage_type_idx.and_then(|tidx| resolve_heritage_type_args(self, tidx));
                if let Some(instance_type) =
                    self.base_instance_type_from_expression(h_expr_idx, type_arguments)
                {
                    let heritage_sym_id = self.resolve_heritage_symbol(h_expr_idx);
                    let is_actual_lib_iterator =
                        self.heritage_reference_is_actual_lib_iterator(h_expr_idx);
                    let heritage_sym_id_for_display = self
                        .heritage_symbol_id_for_expression_display(
                            heritage_sym_id,
                            is_actual_lib_iterator,
                        );
                    let type_base_name = if is_actual_lib_iterator {
                        self.format_builtin_iterator_reference_with_type_arguments(type_arguments)
                    } else {
                        None
                    }
                    .or_else(|| {
                        self.format_heritage_class_symbol_reference(
                            heritage_sym_id_for_display,
                            type_arguments,
                        )
                    })
                    .or_else(|| self.intersection_instance_display_name(h_expr_idx, type_arguments))
                    .unwrap_or_else(|| {
                        self.format_heritage_instance_display(
                            instance_type,
                            h_expr_idx,
                            type_arguments,
                        )
                    });
                    let base_instance_member_names =
                        self.collect_property_names_from_type(instance_type);
                    let base_static_type =
                        self.base_constructor_type_from_expression(h_expr_idx, type_arguments);
                    let base_static_member_names = if let Some(static_type) = base_static_type {
                        self.collect_property_names_from_type(static_type)
                    } else {
                        rustc_hash::FxHashSet::default()
                    };

                    self.check_override_members_against_type(
                        class_data,
                        &derived_class_name,
                        &type_base_name,
                        &base_instance_member_names,
                        &base_static_member_names,
                        no_implicit_override,
                        (Some(instance_type), base_static_type),
                    );
                    return;
                }
            }
            return;
        };

        // Append type parameters to base class name for tsc parity: "A<T>"
        self.append_type_param_names(&mut base_class_name, &base_class.type_parameters);

        let (_derived_type_params, derived_type_param_updates) =
            self.push_type_parameters(&class_data.type_parameters);

        let mut type_args = Vec::new();
        if let Some(nodes) = base_type_argument_nodes.as_ref() {
            for arg_idx in nodes {
                type_args.push(self.get_type_from_type_node(*arg_idx));
            }
        }

        let (base_type_params, base_type_param_updates) =
            self.push_type_parameters(&base_class.type_parameters);
        let base_is_actual_lib_iterator = heritage_expr_idx
            .is_some_and(|expr_idx| self.heritage_reference_is_actual_lib_iterator(expr_idx));
        if type_args.len() < base_type_params.len() {
            for (param_index, param) in base_type_params.iter().enumerate().skip(type_args.len()) {
                let fallback = if base_is_actual_lib_iterator && param_index == 1 {
                    TypeId::UNDEFINED
                } else if base_is_actual_lib_iterator && param_index == 2 {
                    TypeId::UNKNOWN
                } else {
                    param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN)
                };
                type_args.push(fallback);
            }
        }
        if type_args.len() > base_type_params.len() {
            type_args.truncate(base_type_params.len());
        }
        let mut substitution =
            TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);

        // When the extends clause has explicit type arguments, rebuild the base class name
        // with formatted type arguments (e.g., `Base<{ bar: string; }>`) instead of
        // generic parameter names (`Base<T>`). tsc shows the supplied type arguments.
        if base_type_argument_nodes.is_some() && !type_args.is_empty() {
            // Strip the previously appended type params (e.g., remove "<T>" from "Base<T>")
            if let Some(lt_pos) = base_class_name.find('<') {
                base_class_name.truncate(lt_pos);
            }
            let mut display_type_args = type_args.clone();
            if base_is_actual_lib_iterator {
                if display_type_args.len() < 2 {
                    display_type_args.push(TypeId::UNDEFINED);
                }
                if display_type_args.len() < 3 {
                    display_type_args.push(TypeId::UNKNOWN);
                }
            }
            let arg_strs: Vec<String> = display_type_args
                .iter()
                .map(|&t| self.format_type(t))
                .collect();
            base_class_name.push('<');
            base_class_name.push_str(&arg_strs.join(", "));
            base_class_name.push('>');
        }

        // Base type parameters are only needed to build the extends-clause substitution here.
        self.pop_type_parameters(base_type_param_updates);

        // Compose substitutions through the entire inheritance chain.
        // The chain summary stores raw (uninstantiated) member types from ancestor classes.
        // For example, if L<RT> extends T<RT[RT['a']]> and T<A> has member a: A,
        // the chain summary stores a: A (T's raw type param). The initial substitution
        // only maps RT -> X_type, leaving A unresolved. We need to also map A -> the
        // instantiated extends clause type arg, so A maps to the correct concrete type.
        self.compose_ancestor_substitutions(base_idx, &mut substitution);

        let base_chain_summary = self.summarize_class_chain(base_idx);
        let base_instance_member_names: rustc_hash::FxHashSet<String> = base_chain_summary
            .visible_instance_names()
            .cloned()
            .collect();
        let base_static_member_names: rustc_hash::FxHashSet<String> =
            base_chain_summary.visible_static_names().cloned().collect();

        self.check_constructor_parameter_property_overrides(
            class_data,
            Some(base_idx),
            Some(&base_chain_summary),
            &base_class_name,
            &derived_class_name,
            &base_instance_member_names,
            no_implicit_override,
        );

        // Track names that already had TS2610/TS2611 emitted (avoid duplicate for get+set pairs)
        let mut accessor_mismatch_reported: rustc_hash::FxHashSet<String> =
            rustc_hash::FxHashSet::default();
        let mut class_extends_error_reported = false;

        // For overloaded methods, the implementation signature is internal and
        // may be intentionally wider than the visible overload set. The
        // per-node TS2416 check below would compare the impl against the base
        // and falsely flag it; instead we compare the combined CallableShapes.
        let (derived_instance_method_overloads, derived_static_method_overloads) =
            self.build_class_method_overload_types(class_data);
        let mut overload_compat_checked: rustc_hash::FxHashSet<(String, bool)> =
            rustc_hash::FxHashSet::default();

        // When the derived class declares BOTH a getter and a setter for the
        // same name+static-ness, tsc treats them as one accessor pair whose
        // property type is the getter return type (see TS2416 override-compat).
        // The getter's per-node iteration runs the compat check against the
        // pair's canonical type; the setter must NOT independently relate its
        // parameter type against the base or it produces a false TS2416 when
        // the setter parameter type differs from the getter return type even
        // though the accessor property type matches.
        let derived_accessor_pair_types = self.class_accessor_pair_getter_types(class_data);

        // Check each member in the derived class
        for &member_idx in &class_data.members.nodes {
            let Some(info) = self.extract_class_member_info(member_idx, false) else {
                continue;
            };
            let (
                member_name,
                member_type,
                member_name_idx,
                member_visibility,
                is_method,
                is_static,
                is_accessor,
                is_setter,
                has_override,
                is_jsdoc_override,
                has_dynamic_name,
                is_abstract,
                has_computed_non_literal_name,
            ) = (
                info.name,
                info.type_id,
                info.name_idx,
                info.visibility,
                info.is_method,
                info.is_static,
                info.is_accessor,
                info.is_setter,
                info.has_override,
                info.is_jsdoc_override,
                info.has_dynamic_name,
                info.is_abstract,
                info.has_computed_non_literal_name,
            );

            // Skip override checking for private identifiers (#foo)
            // Private fields are scoped to the class that declares them and
            // do NOT participate in the inheritance hierarchy
            if member_name.starts_with('#') {
                continue;
            }

            // Detect overload signatures (method declarations without body) so we
            // can skip the type compatibility check for them later.  We do NOT
            // skip the entire loop iteration because override / accessor / kind
            // mismatch checks still need to run for bodyless method declarations.
            let is_overload_signature = is_method && {
                self.ctx
                    .arena
                    .get(member_idx)
                    .and_then(|n| self.ctx.arena.get_method_decl(n))
                    .is_some_and(|m| m.body.is_none())
            };

            let base_info = base_chain_summary
                .lookup(&member_name, is_static, true)
                .cloned();

            if has_override {
                // Cannot use `override` when name is computed dynamically.
                if has_dynamic_name {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                            &[],
                        ),
                        diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_ITS_NAME_IS_DYNAMIC,
                    );
                    continue;
                }
            }

            if has_dynamic_name {
                // Dynamic names are allowed regardless of `noImplicitOverride`; they cannot
                // satisfy normal override checks because their exact identity cannot be
                // statically proven as an inherited symbol.
            } else if has_override {
                // `override` requires a matching visible base member.
                if base_info.is_none() {
                    let suggestion_names = if is_static {
                        &base_static_member_names
                    } else {
                        &base_instance_member_names
                    };
                    if let Some(suggestion) =
                        self.find_override_name_suggestion(suggestion_names, &member_name)
                    {
                        // TS4117 (keyword) or TS4123 (JSDoc): "Did you mean ...?"
                        let (msg, code) = if is_jsdoc_override {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D_2,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D_2,
                            )
                        } else {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B_2,
                            )
                        };
                        self.error_at_node(
                            member_name_idx,
                            &crate::diagnostics::format_message(
                                msg,
                                &[&base_class_name, &suggestion],
                            ),
                            code,
                        );
                    } else {
                        // TS4113 (keyword) or TS4122 (JSDoc): not declared in base
                        let (msg, code) = if is_jsdoc_override {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_IS_NOT_D,
                            )
                        } else {
                            (
                                diagnostic_messages::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                                diagnostic_codes::THIS_MEMBER_CANNOT_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_IS_NOT_DECLARED_IN_THE_B,
                            )
                        };
                        self.error_at_node(
                            member_name_idx,
                            &crate::diagnostics::format_message(msg, &[&base_class_name]),
                            code,
                        );
                    }
                    continue;
                }
            } else if no_implicit_override && base_info.is_some() && !has_computed_non_literal_name
            {
                // tsc does not require `override` for `declare` property re-declarations.
                // A `declare property: T` in a derived class is a type-only ambient annotation
                // (no runtime effect) and is not considered a true override.
                let is_declare_property = !is_method
                    && !is_accessor
                    && self
                        .ctx
                        .arena
                        .get(member_idx)
                        .and_then(|n| self.ctx.arena.get_property_decl(n))
                        .is_some_and(|prop| self.has_declare_modifier(&prop.modifiers));
                if is_declare_property {
                    continue;
                }
                // tsc does not require `override` when a concrete member implements an
                // abstract base method. Abstract members MUST be implemented, so
                // providing a concrete implementation is not an "accidental" override —
                // only abstract-to-abstract re-declarations require the `override` keyword.
                let base_is_abstract_method = base_info
                    .as_ref()
                    .is_some_and(|base| base.is_abstract && base.is_method);
                if !is_abstract && base_is_abstract_method {
                    continue;
                }
                if base_info
                    .as_ref()
                    .is_some_and(|base| base.is_abstract && base.is_method)
                {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            if self.ctx.is_js_file() {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                            } else {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH
                            },
                            &[&base_class_name],
                        ),
                        if self.ctx.is_js_file() {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                        } else {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_AN_ABSTRACT_METH
                        },
                    );
                } else {
                    self.error_at_node(
                        member_name_idx,
                        &crate::diagnostics::format_message(
                            if self.ctx.is_js_file() {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                            } else {
                                diagnostic_messages::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE
                            },
                            &[&base_class_name],
                        ),
                        if self.ctx.is_js_file() {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_A_JSDOC_COMMENT_WITH_AN_OVERRIDE_TAG_BECAUSE_IT_OVERRIDES
                        } else {
                            diagnostic_codes::THIS_MEMBER_MUST_HAVE_AN_OVERRIDE_MODIFIER_BECAUSE_IT_OVERRIDES_A_MEMBER_IN_THE
                        },
                    );
                }
                continue;
            }

            let base_scope = self.push_type_parameters(&base_class.type_parameters);

            // Find matching member including private/protected members to detect
            // class-level visibility/branding incompatibilities (TS2415).
            let base_any_info = base_chain_summary
                .lookup(&member_name, is_static, false)
                .cloned();
            if let Some(ref base_any_info) = base_any_info
                && self
                    .class_member_visibility_conflicts(member_visibility, base_any_info.visibility)
            {
                // When both derived and base members are private, tsc checks type
                // compatibility and emits TS2416 if the types differ, rather than
                // emitting TS2415 (branding conflict). Only emit TS2415 when the
                // types are compatible or when visibility differs.
                if member_visibility == MemberVisibility::Private
                    && base_any_info.visibility == MemberVisibility::Private
                {
                    let base_type =
                        instantiate_type(self.ctx.types, base_any_info.type_id, &substitution);
                    if member_type != TypeId::ANY
                        && base_type != TypeId::ANY
                        && should_report_member_type_mismatch(
                            self,
                            member_type,
                            base_type,
                            member_name_idx,
                        )
                    {
                        // TS2416: Private member type incompatibility
                        self.pop_type_parameters(base_scope.1);
                        let display_name = format_property_name_for_diagnostic(&member_name);
                        let base_class_display_name =
                            base_class_name_for_diagnostic(&base_class_name);
                        self.error_at_node(
                            member_name_idx,
                            &format!(
                                "Property '{display_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_display_name}'."
                            ),
                            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                        );
                        let member_type_str = self.format_type(member_type);
                        let base_type_str = self.format_type(base_type);
                        self.report_type_not_assignable_detail(
                            member_name_idx,
                            &member_type_str,
                            &base_type_str,
                            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                        );
                        continue;
                    }
                }
                self.pop_type_parameters(base_scope.1);
                if !class_extends_error_reported {
                    let display_name = format_property_name_for_diagnostic(&member_name);
                    let elaboration = visibility_conflict_elaboration(
                        member_visibility,
                        base_any_info.visibility,
                        &display_name,
                        &derived_class_name,
                        &base_class_name,
                    );
                    if is_static {
                        let message = match elaboration {
                            Some(detail) => format!(
                                "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'.\n  {detail}"
                            ),
                            None => format!(
                                "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                            ),
                        };
                        self.error_at_node(
                            class_data.name,
                            &message,
                            diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                        );
                    } else {
                        let message = match elaboration {
                            Some(detail) => format!(
                                "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'.\n  {detail}"
                            ),
                            None => format!(
                                "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
                            ),
                        };
                        self.error_at_node(
                            class_data.name,
                            &message,
                            diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                        );
                    }
                    class_extends_error_reported = true;
                }
                continue;
            }

            // Look for a matching member in the base class hierarchy (skip private members)
            // First check direct members of the base class, then walk up the chain
            let base_info = base_chain_summary
                .lookup(&member_name, is_static, true)
                .cloned();

            self.pop_type_parameters(base_scope.1);

            let Some(base_info) = base_info else {
                continue;
            };

            let base_type = instantiate_type(self.ctx.types, base_info.type_id, &substitution);

            // TS2610/TS2611: Check accessor/property kind mismatch
            // Only applies to non-method, non-static members. Fires regardless of types (even ANY).
            // Static members are allowed to override accessors with properties and vice versa.
            // Members from merged interface declarations are not subject to this check — only
            // actual class property declarations trigger the accessor/property mismatch.
            if !is_method
                && !is_static
                && !base_info.is_method
                && !base_info.is_abstract
                && !base_info.from_interface
                && !accessor_mismatch_reported.contains(&member_name)
            {
                // Note: do NOT `continue` after emitting TS2610/TS2611 — tsc treats the
                // property/accessor kind mismatch and the override type-incompatibility
                // (TS2416) as independent diagnostics, so the assignability gate below must
                // still run when the overriding type is not assignable to the base type.
                if !is_accessor && base_info.is_accessor {
                    // TS2610: derived property overrides base accessor
                    accessor_mismatch_reported.insert(member_name.clone());
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "'{member_name}' is defined as an accessor in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an instance property."
                        ),
                        diagnostic_codes::IS_DEFINED_AS_AN_ACCESSOR_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_INSTANCE_PROP,
                    );
                } else if is_accessor && !base_info.is_accessor {
                    // TS2611: derived accessor overrides base property
                    accessor_mismatch_reported.insert(member_name.clone());
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "'{member_name}' is defined as a property in class '{base_class_name}', but is overridden here in '{derived_class_name}' as an accessor."
                        ),
                        diagnostic_codes::IS_DEFINED_AS_A_PROPERTY_IN_CLASS_BUT_IS_OVERRIDDEN_HERE_IN_AS_AN_ACCESSOR,
                    );
                }
            }

            // TS2423/TS2425/TS2426: Check for method/property/accessor kind mismatch (INSTANCE members only)
            // Static members use TS2417 instead
            if !is_static {
                // TS2423: Base has method, derived has accessor
                if is_accessor && !is_method && base_info.is_method {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member function '{member_name}', but extended class '{derived_class_name}' defines it as instance member accessor."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_FUNCTION_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }

                // TS2425: Base has property (not method, not accessor), derived has method
                if is_method && !base_info.is_method && !base_info.is_accessor {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member property '{member_name}', but extended class '{derived_class_name}' defines it as instance member function."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_PROPERTY_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }

                // TS2423: Base has method, derived has accessor
                if is_accessor && base_info.is_method {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member function '{member_name}', but extended class '{derived_class_name}' defines it as instance member accessor."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_FUNCTION_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                    continue;
                }

                // TS2426: Base has accessor, derived has method
                // Note: do NOT `continue` here — tsc also emits TS2416 for type incompatibility
                // alongside the kind mismatch error, so the type check below must still run.
                if is_method && base_info.is_accessor {
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class '{base_class_name}' defines instance member accessor '{member_name}', but extended class '{derived_class_name}' defines it as instance member function."
                        ),
                        diagnostic_codes::CLASS_DEFINES_INSTANCE_MEMBER_ACCESSOR_BUT_EXTENDED_CLASS_DEFINES_IT_AS_INSTANCE,
                    );
                }
            }

            // Accessor pair canonicalization: the GET_ACCESSOR's iteration
            // already runs the override-compat check against the pair's
            // canonical property type (the getter return type). Skip the
            // SET_ACCESSOR to avoid duplicate or false TS2416 diagnostics.
            if is_setter
                && derived_accessor_pair_types.contains_key(&(member_name.clone(), is_static))
            {
                continue;
            }

            // Skip type compatibility check if either type is ANY
            if member_type == TypeId::ANY || base_type == TypeId::ANY {
                continue;
            }

            // Overloaded-method compat: when either side has multiple
            // declarations of this name, swap the impl/overload-sig node's
            // type for the externally-visible `CallableShape` and run the
            // compat check once per name.
            if is_method
                && self.check_overloaded_method_compat(OverloadCompatCtx {
                    member_name: &member_name,
                    member_type,
                    member_name_idx,
                    is_static,
                    derived_class_name: &derived_class_name,
                    base_class_name: &base_class_name,
                    base_info: &base_info,
                    base_chain_summary: &base_chain_summary,
                    derived_overloads: if is_static {
                        &derived_static_method_overloads
                    } else {
                        &derived_instance_method_overloads
                    },
                    substitution: &substitution,
                    overload_compat_checked: &mut overload_compat_checked,
                })
            {
                continue;
            }

            // Skip type compatibility for overload signatures. tsc checks
            // inheritance using the combined overloaded type from the symbol,
            // not individual AST declarations.  Individual overloads may be
            // narrower than the base method's type, producing false TS2416.
            if is_overload_signature {
                continue;
            }

            // Resolve TypeQuery types (typeof) before comparison
            let resolved_member_type = self.resolve_type_query_type(member_type);
            let resolved_base_type = self.resolve_type_query_type(base_type);

            // Check type compatibility through centralized mismatch policy.
            // Methods always use bivariant relation checks.
            // Static properties also use bivariant checks — tsc checks the static
            // side structurally (typeof Derived vs typeof Base) with the normal
            // assignability relation, which without strictFunctionTypes is bivariant.
            // Only instance property overrides use strict assignability (TS2416).
            let should_report_mismatch = if is_method || is_static {
                should_report_member_type_mismatch_bivariant(
                    self,
                    resolved_member_type,
                    resolved_base_type,
                    member_name_idx,
                )
            } else {
                should_report_member_type_mismatch(
                    self,
                    resolved_member_type,
                    resolved_base_type,
                    member_name_idx,
                )
            };

            if should_report_mismatch {
                let member_type_str = self.format_type(member_type);
                let base_type_str = self.format_type(base_type);

                // TS2417: Static members use different error message and code
                // TS2416: Instance members use standard property incompatibility error
                if is_static {
                    // TS2417: Class static side '{0}' incorrectly extends base class static side '{1}'.
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                        ),
                        diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                    );
                } else {
                    // TS2416: Instance member incompatibility
                    let display_name = format_property_name_for_diagnostic(&member_name);
                    let base_class_display_name = base_class_name_for_diagnostic(&base_class_name);
                    self.error_at_node(
                        member_name_idx,
                        &format!(
                            "Property '{display_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_display_name}'."
                        ),
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                    self.report_type_not_assignable_detail(
                        member_name_idx,
                        &member_type_str,
                        &base_type_str,
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                }
            }
        }

        // Check constructor parameter properties for type/visibility compatibility
        // with base class members. The main member loop above only handles
        // PROPERTY_DECLARATION/METHOD_DECLARATION/ACCESSOR nodes. Parameter properties
        // (e.g., `constructor(public p?: number)`) are syntactic sugar for class properties
        // but live inside the constructor node, so they need separate handling.
        if !class_extends_error_reported {
            self.check_parameter_property_compatibility(
                class_data,
                &base_chain_summary,
                &derived_class_name,
                &base_class_name,
                &substitution,
            );
        }

        // Check index signature compatibility between derived and base classes (TS2415)
        self.check_class_index_signature_compatibility(
            class_data,
            base_class,
            &derived_class_name,
            &base_class_name,
            &substitution,
            class_extends_error_reported,
        );

        // TS2417: Whole-type static side check for namespace-merged classes.
        //
        // The member-by-member loop above only examines AST class body members.
        // When the base class has a merged namespace (e.g.,
        // `namespace Shape.Utils { export function convert(): Shape { ... } }`),
        // `typeof Shape` includes those namespace exports. If the derived class
        // also has a merged namespace with conflicting exports, `typeof Derived`
        // is structurally incompatible with `typeof Base` — tsc reports this as TS2417.
        //
        // We only check when the DERIVED class also has a namespace (NAMESPACE_MODULE
        // flag), since a derived class without any namespace cannot have conflicting
        // namespace exports. This avoids false positives for classes that simply
        // don't replicate namespace exports from their base class.
        if !class_extends_error_reported && let Some(base_sym) = base_sym_for_ns_static_check {
            let derived_sym = self.ctx.binder.get_node_symbol(class_idx);
            if let Some(derived_sym) = derived_sym {
                let derived_symbol_flags = self
                    .ctx
                    .binder
                    .get_symbol(derived_sym)
                    .map_or(0, |s| s.flags);
                let derived_has_namespace = derived_symbol_flags
                    & (tsz_binder::symbol_flags::NAMESPACE_MODULE
                        | tsz_binder::symbol_flags::VALUE_MODULE)
                    != 0;
                if derived_has_namespace {
                    let derived_ctor_type = self.get_type_of_symbol(derived_sym);
                    let base_ctor_type = self.get_type_of_symbol(base_sym);
                    // Only flag TS2417 when derived's namespace exports a name
                    // that already exists on base's static (constructor) side.
                    // Without such overlap, the namespace merge cannot shadow
                    // a base static member, so the structural check would
                    // otherwise over-fire on self-referential clodule generics
                    // (e.g. `class C extends B<typeof C.X> { } ;
                    // namespace C { export const X = ... }`) where
                    // is_assignable_to rejects a technically-compatible
                    // constructor pair purely because of the self-reference.
                    let derived_ns_names =
                        self.collect_namespace_export_names_for_symbol(derived_sym);
                    let base_static_names = self.collect_property_names_from_type(base_ctor_type);
                    let has_name_overlap = derived_ns_names
                        .iter()
                        .any(|n| base_static_names.contains(n));
                    if has_name_overlap
                        && derived_ctor_type != TypeId::UNKNOWN
                        && derived_ctor_type != TypeId::ERROR
                        && base_ctor_type != TypeId::UNKNOWN
                        && base_ctor_type != TypeId::ERROR
                        && !self.is_assignable_to(derived_ctor_type, base_ctor_type)
                    {
                        self.error_at_node(
                                class_data.name,
                                &format!(
                                    "Class static side 'typeof {derived_class_name}' incorrectly extends base class static side 'typeof {base_class_name}'."
                                ),
                                diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
                            );
                    }
                }
            }
        }

        self.pop_type_parameters(derived_type_param_updates);
    }

    /// TS2416 type compat for overloaded methods: when this method has
    /// multiple declarations on either the derived or base side, swap the
    /// per-AST-node signature for the externally-visible `CallableShape`
    /// (the overload sigs, or — when no bodyless overload sigs exist — the
    /// single implementation signature). Returns `true` if the method was
    /// recognized as overloaded and handled here, so the per-node compat
    /// check should be skipped for every declaration of this name in the
    /// derived class.
    fn check_overloaded_method_compat(&mut self, ctx: OverloadCompatCtx<'_>) -> bool {
        let OverloadCompatCtx {
            member_name,
            member_type,
            member_name_idx,
            is_static,
            derived_class_name,
            base_class_name,
            base_info,
            base_chain_summary,
            derived_overloads,
            substitution,
            overload_compat_checked,
        } = ctx;
        use crate::query_boundaries::common::instantiate_type;

        let derived_overload_type = derived_overloads.get(member_name).copied();
        let base_overload_type = base_chain_summary.method_overload_type(member_name, is_static);
        if derived_overload_type.is_none() && base_overload_type.is_none() {
            return false;
        }
        if !overload_compat_checked.insert((member_name.to_string(), is_static)) {
            return true;
        }

        let derived_combined = derived_overload_type.unwrap_or(member_type);
        let base_combined = base_overload_type.unwrap_or(base_info.type_id);
        let base_combined = instantiate_type(self.ctx.types, base_combined, substitution);
        let resolved_member_type = self.resolve_type_query_type(derived_combined);
        let resolved_base_type = self.resolve_type_query_type(base_combined);
        if resolved_member_type == TypeId::ANY
            || resolved_base_type == TypeId::ANY
            || !should_report_member_type_mismatch_bivariant(
                self,
                resolved_member_type,
                resolved_base_type,
                member_name_idx,
            )
        {
            return true;
        }

        let member_type_str = self.format_type(resolved_member_type);
        let base_type_str = self.format_type(resolved_base_type);
        let display_name = format_property_name_for_diagnostic(member_name);
        let base_class_display_name = base_class_name_for_diagnostic(base_class_name);
        self.error_at_node(
            member_name_idx,
            &format!(
                "Property '{display_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_display_name}'."
            ),
            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
        );
        self.report_type_not_assignable_detail(
            member_name_idx,
            &member_type_str,
            &base_type_str,
            diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
        );
        true
    }

    /// Check constructor parameter properties against base class members for
    /// type and visibility compatibility (TS2415).
    ///
    /// tsc emits TS2415 at the class name when a parameter property (e.g.,
    /// `constructor(public p?: number)`) is incompatible with the corresponding
    /// base class member. This can be due to:
    /// - Visibility conflict: derived public vs base private
    /// - Type incompatibility: derived `number | undefined` vs base `number`
    fn check_parameter_property_compatibility(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        base_chain_summary: &ClassChainSummary,
        derived_class_name: &str,
        base_class_name: &str,
        substitution: &TypeSubstitution,
    ) {
        use crate::query_boundaries::common::instantiate_type;

        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };

            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                if !self.has_parameter_property_modifier(&param.modifiers) {
                    continue;
                }
                let Some(param_name) = self.get_property_name(param.name) else {
                    continue;
                };

                let derived_visibility = if self.has_private_modifier(&param.modifiers) {
                    MemberVisibility::Private
                } else if self.has_protected_modifier(&param.modifiers) {
                    MemberVisibility::Protected
                } else {
                    MemberVisibility::Public
                };

                // Find matching member in base class (including private, for visibility checks)
                let base_any_info = base_chain_summary
                    .lookup(&param_name, false, false)
                    .cloned();

                // Check visibility conflict (TS2415)
                if let Some(ref base_any_info) = base_any_info
                    && self.class_member_visibility_conflicts(
                        derived_visibility,
                        base_any_info.visibility,
                    )
                {
                    self.error_at_node(
                        class_data.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
                        ),
                        diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
                    return; // Only one TS2415 per class
                }

                // Check type compatibility — find visible base member
                let base_info = base_chain_summary.lookup(&param_name, false, true).cloned();
                let Some(base_info) = base_info else {
                    continue;
                };
                let base_type = instantiate_type(self.ctx.types, base_info.type_id, substitution);

                // Get the parameter property type, accounting for optionality
                let mut prop_type = if param.type_annotation.is_some() {
                    self.get_type_from_type_node(param.type_annotation)
                } else {
                    TypeId::ANY
                };

                // Optional parameter properties (`p?: T`) have type `T | undefined`
                // under strictNullChecks
                if param.question_token && self.ctx.strict_null_checks() {
                    let factory = self.ctx.types.factory();
                    prop_type = factory.union2(prop_type, TypeId::UNDEFINED);
                }

                // Skip if either type is ANY
                if prop_type == TypeId::ANY || base_type == TypeId::ANY {
                    continue;
                }

                // Check type compatibility through centralized mismatch policy
                if should_report_member_type_mismatch(self, prop_type, base_type, param.name) {
                    // tsc emits TS2415 at the class name for parameter property
                    // type incompatibility (not TS2416 at the member)
                    self.error_at_node(
                        class_data.name,
                        &format!(
                            "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
                        ),
                        diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
                    );
                    return; // Only one TS2415 per class
                }
            }
        }
    }

    fn check_non_public_member_inheritance_conflicts_against_type(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        base_instance_type: TypeId,
        derived_class_name: &str,
        base_class_name: &str,
    ) {
        use tsz_solver::Visibility;

        let mut class_extends_error_reported = false;
        let tsz_solver::objects::PropertyCollectionResult::Properties { properties, .. } =
            tsz_solver::objects::collect_properties(
                self.resolve_lazy_type(base_instance_type),
                self.ctx.types,
                &self.ctx,
            )
        else {
            return;
        };

        for &member_idx in &class_data.members.nodes {
            let Some(info) = self.extract_class_member_info(member_idx, false) else {
                continue;
            };
            if info.name.starts_with('#') || info.is_static {
                continue;
            }

            let Some(base_prop) = properties
                .iter()
                .find(|prop| self.ctx.types.resolve_atom_ref(prop.name).as_ref() == info.name)
            else {
                continue;
            };
            let base_visibility = match base_prop.visibility {
                Visibility::Public => MemberVisibility::Public,
                Visibility::Protected => MemberVisibility::Protected,
                Visibility::Private => MemberVisibility::Private,
            };

            if !self.class_member_visibility_conflicts(info.visibility, base_visibility) {
                continue;
            }

            if info.visibility == MemberVisibility::Private
                && base_visibility == MemberVisibility::Private
            {
                let base_type = base_prop.type_id;
                if info.type_id != TypeId::ANY
                    && base_type != TypeId::ANY
                    && should_report_member_type_mismatch(
                        self,
                        info.type_id,
                        base_type,
                        info.name_idx,
                    )
                {
                    let display_name = format_property_name_for_diagnostic(&info.name);
                    let base_class_display_name = base_class_name_for_diagnostic(base_class_name);
                    self.error_at_node(
                        info.name_idx,
                        &format!(
                            "Property '{display_name}' in type '{derived_class_name}' is not assignable to the same property in base type '{base_class_display_name}'."
                        ),
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                    let member_type_str = self.format_type(info.type_id);
                    let base_type_str = self.format_type(base_type);
                    self.report_type_not_assignable_detail(
                        info.name_idx,
                        &member_type_str,
                        &base_type_str,
                        diagnostic_codes::PROPERTY_IN_TYPE_IS_NOT_ASSIGNABLE_TO_THE_SAME_PROPERTY_IN_BASE_TYPE,
                    );
                    continue;
                }
            }

            if class_extends_error_reported {
                continue;
            }

            self.error_at_node(
                class_data.name,
                &format!(
                    "Class '{derived_class_name}' incorrectly extends base class '{base_class_name}'."
                ),
                diagnostic_codes::CLASS_INCORRECTLY_EXTENDS_BASE_CLASS,
            );
            class_extends_error_reported = true;
        }
    }

    /// Append type parameter names (e.g., `<T, U>`) to a class/interface name string.
    /// This matches tsc's display format for TS2415/TS2430 error messages.
    pub(crate) fn append_type_param_names(
        &self,
        name: &mut String,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) {
        let Some(list) = type_parameters else {
            return;
        };
        let mut param_names = Vec::new();
        for &param_idx in &list.nodes {
            let Some(node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(data) = self.ctx.arena.get_type_parameter(node) else {
                continue;
            };
            if let Some(name_node) = self.ctx.arena.get(data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                param_names.push(ident.escaped_text.as_str());
            }
        }
        if !param_names.is_empty() {
            name.push('<');
            name.push_str(&param_names.join(", "));
            name.push('>');
        }
    }

    pub(crate) fn class_symbol_is_actual_lib_iterator(&self, sym_id: tsz_binder::SymbolId) -> bool {
        self.get_symbol_globally(sym_id).is_some_and(|symbol| {
            symbol.escaped_name == "Iterator"
                && self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id)
        })
    }

    pub(crate) fn heritage_reference_is_actual_lib_iterator(&self, expr_idx: NodeIndex) -> bool {
        let resolved_sym = self.resolve_heritage_symbol(expr_idx);
        let Some(name) = self.ctx.arena.get_identifier_text(expr_idx) else {
            return false;
        };
        if name != "Iterator" {
            return false;
        }
        if self.current_file_import_binds_name(name) {
            return false;
        }
        if resolved_sym.is_some_and(|sym_id| self.class_symbol_is_actual_lib_iterator(sym_id)) {
            return true;
        }
        if let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) {
            return self.class_symbol_is_actual_lib_iterator(sym_id);
        }
        if resolved_sym.is_some() {
            return false;
        }
        if self
            .ctx
            .binder
            .file_locals
            .get(name)
            .is_some_and(|sym_id| !self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id))
        {
            return false;
        }

        self.ctx.actual_lib_context_has_bare_name(name)
            && !self.ctx.file_local_type_shadow_for_lib_name(name)
            && !self.file_local_import_alias_shadows_lib_name(name)
    }

    pub(crate) fn heritage_symbol_id_for_expression_display(
        &self,
        sym_id: Option<tsz_binder::SymbolId>,
        is_actual_lib_iterator: bool,
    ) -> Option<tsz_binder::SymbolId> {
        let sym_id = sym_id?;
        if self.class_symbol_is_actual_lib_iterator(sym_id) && !is_actual_lib_iterator {
            return None;
        }
        Some(sym_id)
    }

    fn file_local_import_alias_shadows_lib_name(&self, name: &str) -> bool {
        use tsz_binder::symbol_flags;

        if !self.ctx.binder.is_external_module() {
            return false;
        }

        self.ctx.binder.file_locals.get(name).is_some_and(|sym_id| {
            if self.ctx.symbol_is_from_actual_or_cloned_lib(sym_id) {
                return self.current_file_import_binds_name(name);
            }

            self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.import_module.is_some() && symbol.has_any_flags(symbol_flags::ALIAS)
            })
        }) || self.current_file_import_binds_name(name)
    }

    fn current_file_import_binds_name(&self, name: &str) -> bool {
        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        source_file.statements.nodes.iter().any(|&stmt_idx| {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                return false;
            };
            if stmt_node.kind != syntax_kind_ext::IMPORT_DECLARATION {
                return false;
            }
            let Some(import_decl) = self.ctx.arena.get_import_decl(stmt_node) else {
                return false;
            };
            let Some(clause_node) = self.ctx.arena.get(import_decl.import_clause) else {
                return false;
            };
            let Some(clause) = self.ctx.arena.get_import_clause(clause_node) else {
                return false;
            };

            if self.ctx.arena.get_identifier_text(clause.name) == Some(name) {
                return true;
            }

            let Some(bindings_node) = self.ctx.arena.get(clause.named_bindings) else {
                return false;
            };
            if bindings_node.kind == syntax_kind_ext::NAMESPACE_IMPORT {
                return self
                    .ctx
                    .arena
                    .get_named_imports(bindings_node)
                    .is_some_and(|ns| self.ctx.arena.get_identifier_text(ns.name) == Some(name));
            }
            if bindings_node.kind != syntax_kind_ext::NAMED_IMPORTS {
                return false;
            }

            self.ctx
                .arena
                .get_named_imports(bindings_node)
                .is_some_and(|named| {
                    named.elements.nodes.iter().any(|&spec_idx| {
                        let Some(spec_node) = self.ctx.arena.get(spec_idx) else {
                            return false;
                        };
                        let Some(spec) = self.ctx.arena.get_specifier(spec_node) else {
                            return false;
                        };
                        let local_name_idx = if spec.name.is_some() {
                            spec.name
                        } else {
                            spec.property_name
                        };
                        self.ctx.arena.get_identifier_text(local_name_idx) == Some(name)
                    })
                })
        })
    }

    /// Walk the inheritance chain from `class_idx` upward and compose type parameter
    /// substitutions into `substitution`. This ensures that type parameters from
    /// ancestor classes (not just the immediate base) are correctly mapped.
    ///
    /// For example, given `X extends L<X>` where `L<RT> extends T<RT[RT['a']]>`:
    /// - The initial substitution maps `RT -> X_type`
    /// - This method walks L -> T, finding `T<A>` with extends arg `RT[RT['a']]`
    /// - It instantiates the extends arg with the current substitution: `X[X['a']]`
    /// - It adds `A -> X[X['a']]` to the substitution
    fn compose_ancestor_substitutions(
        &mut self,
        class_idx: NodeIndex,
        substitution: &mut TypeSubstitution,
    ) {
        use rustc_hash::FxHashSet;

        let mut current = class_idx;
        let mut visited = FxHashSet::default();

        while visited.insert(current) {
            let Some(class) = self.ctx.arena.get_class_at(current) else {
                break;
            };

            let heritage_clauses = match class.heritage_clauses.as_ref() {
                Some(hc) => hc.clone(),
                None => break,
            };

            let mut next_class = None;

            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                let Some(&type_idx) = heritage.types.nodes.first() else {
                    continue;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_ta) = self.ctx.arena.get_expr_type_args(type_node) {
                        (expr_ta.expression, expr_ta.type_arguments.as_ref().cloned())
                    } else {
                        (type_idx, None)
                    };

                // No type arguments means no intermediate substitution needed
                let Some(ta) = type_arguments else {
                    // Still need to walk up the chain in case there are further ancestors
                    if let Some(parent_idx) = self.get_base_class_idx(current) {
                        next_class = Some(parent_idx);
                    }
                    break;
                };

                // Resolve the parent class
                let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    break;
                };
                let Some(parent_class_idx) = self.get_class_declaration_from_symbol(base_sym_id)
                else {
                    break;
                };
                let Some(parent_class) = self.ctx.arena.get_class_at(parent_class_idx) else {
                    break;
                };

                // Push current class's type params so we can resolve extends type args
                let (_, current_tp_updates) = self.push_type_parameters(&class.type_parameters);

                // Resolve extends clause type arguments
                let mut extends_type_args = Vec::new();
                for &arg_idx in &ta.nodes {
                    extends_type_args.push(self.get_type_from_type_node(arg_idx));
                }

                self.pop_type_parameters(current_tp_updates);

                // Get parent's type parameters
                let (parent_type_params, parent_tp_updates) =
                    self.push_type_parameters(&parent_class.type_parameters);
                self.pop_type_parameters(parent_tp_updates);

                // For each parent type parameter, instantiate the extends type arg
                // with the current (accumulated) substitution and add the mapping
                for (i, param) in parent_type_params.iter().enumerate() {
                    if substitution.get(param.name).is_some() {
                        continue; // Already mapped
                    }
                    let arg_type = if i < extends_type_args.len() {
                        extends_type_args[i]
                    } else {
                        param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN)
                    };
                    let instantiated = crate::query_boundaries::common::instantiate_type(
                        self.ctx.types,
                        arg_type,
                        substitution,
                    );
                    substitution.insert(param.name, instantiated);
                }

                next_class = Some(parent_class_idx);
                break; // Only process first extends clause
            }

            match next_class {
                Some(nc) => current = nc,
                None => break,
            }
        }
    }

    // Index signature compatibility (TS2415), interface extension compatibility (TS2430),
    // member lookup in class chains, and visibility conflict detection are in
    // `class_checker_compat.rs`.
}
