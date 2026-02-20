//! Class declaration, heritage clause, and class member checking.
//!
//! Split from `state_property_checking.rs` to keep file sizes manageable.
//! Contains heritage clause validation, class declaration/expression checking,
//! property initialization, and decorator/triple-slash reference checks.

use crate::EnclosingClassInfo;
use crate::error_handler::ErrorHandler;
use crate::flow_analysis::{ComputedKey, PropertyKey};
use crate::query_boundaries::definite_assignment::{
    check_constructor_property_use_before_assignment, constructor_assigned_properties,
};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check heritage clauses (extends/implements) for unresolved names.
    /// Emits TS2304 when a referenced name cannot be resolved.
    /// Emits TS2689 when a class extends an interface.
    ///
    /// Parameters:
    /// - `heritage_clauses`: The heritage clauses to check
    /// - `is_class_declaration`: true if checking a class, false if checking an interface
    ///   (TS2689 should only be emitted for classes extending interfaces, not interfaces extending interfaces)
    pub(crate) fn check_heritage_clauses_for_unresolved_names(
        &mut self,
        heritage_clauses: &Option<tsz_parser::parser::NodeList>,
        is_class_declaration: bool,
        class_type_param_names: &[String],
    ) {
        use tsz_parser::parser::syntax_kind_ext::HERITAGE_CLAUSE;
        use tsz_scanner::SyntaxKind;

        let Some(clauses) = heritage_clauses else {
            return;
        };

        for &clause_idx in &clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };

            if clause_node.kind != HERITAGE_CLAUSE {
                continue;
            }

            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Check if this is an extends clause (for TS2507 errors)
            let is_extends_clause = heritage.token == SyntaxKind::ExtendsKeyword as u16;

            // Check each type in the heritage clause
            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression (identifier or property access) from ExpressionWithTypeArguments
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                // TS2562: Base class expressions cannot reference class type parameters.
                // This applies to `extends` expressions that include type positions
                // (e.g., call type arguments like `extends base<T>()`), but should not
                // flag same-named value symbols.
                if is_class_declaration
                    && is_extends_clause
                    && let Some(type_param_ref) = self.find_class_type_param_ref_in_base_expression(
                        expr_idx,
                        class_type_param_names,
                    )
                {
                    self.error_at_node(
                        type_param_ref,
                        crate::diagnostics::diagnostic_messages::BASE_CLASS_EXPRESSIONS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                        crate::diagnostics::diagnostic_codes::BASE_CLASS_EXPRESSIONS_CANNOT_REFERENCE_CLASS_TYPE_PARAMETERS,
                    );
                }

                // Try to resolve the heritage symbol
                if let Some(heritage_sym) = self.resolve_heritage_symbol(expr_idx) {
                    let type_args = self
                        .ctx
                        .arena
                        .get_expr_type_args(type_node)
                        .and_then(|e| e.type_arguments.as_ref())
                        .or_else(|| {
                            self.ctx
                                .arena
                                .get(expr_idx)
                                .and_then(|expr_node| self.ctx.arena.get_call_expr(expr_node))
                                .and_then(|call| call.type_arguments.as_ref())
                        });

                    let required_count = self.count_required_type_params(heritage_sym);
                    let total_type_params = self.get_type_params_for_symbol(heritage_sym).len();
                    if let Some(type_args) = type_args {
                        if total_type_params == 0 {
                            let symbol_type = self.get_type_of_symbol(heritage_sym);
                            let has_generic_construct_signature =
                                tsz_solver::type_queries::get_construct_signatures(
                                    self.ctx.types,
                                    symbol_type,
                                )
                                .is_some_and(|sigs| {
                                    sigs.iter().any(|sig| !sig.type_params.is_empty())
                                });

                            // Also check declaration directly (catches cross-arena lib types)
                            let has_type_params_in_decl =
                                self.symbol_declaration_has_type_parameters(heritage_sym);

                            if !has_generic_construct_signature
                                && !has_type_params_in_decl
                                && symbol_type != TypeId::ERROR
                                && symbol_type != TypeId::ANY
                                && let Some(&arg_idx) = type_args.nodes.first()
                            {
                                let name = self
                                    .heritage_name_text(expr_idx)
                                    .unwrap_or_else(|| "<expression>".to_string());
                                self.error_at_node_msg(
                                    arg_idx,
                                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                                    &[name.as_str()],
                                );
                            }
                        } else {
                            if type_args.nodes.len() < required_count
                                && let Some(name) = self.heritage_name_text(expr_idx)
                            {
                                self.error_generic_type_requires_type_arguments_at(
                                    &name,
                                    required_count,
                                    type_idx,
                                );
                            }

                            self.validate_type_reference_type_arguments(heritage_sym, type_args);
                        }
                    } else if required_count > 0
                        // In class extends clauses, TypeScript allows omitting type
                        // arguments (e.g. `class C extends Array {}`). The type
                        // defaults to the constructor's instance type with default args.
                        && !(is_class_declaration && is_extends_clause)
                        && let Some(name) = self.heritage_name_text(expr_idx)
                    {
                        self.error_generic_type_requires_type_arguments_at(
                            &name,
                            required_count,
                            type_idx,
                        );
                    }

                    // TS2449/TS2450: Check if class/enum is used before its declaration
                    if is_extends_clause && is_class_declaration {
                        self.check_heritage_class_before_declaration(heritage_sym, expr_idx);
                    }

                    // Symbol was resolved - check if it represents a constructor type for extends clauses
                    if is_extends_clause {
                        use tsz_binder::symbol_flags;

                        // Note: Must resolve type aliases before checking flags and getting type
                        let mut visited_aliases = Vec::new();
                        let resolved_sym =
                            self.resolve_alias_symbol(heritage_sym, &mut visited_aliases);
                        let sym_to_check = resolved_sym.unwrap_or(heritage_sym);

                        // Guard against infinite recursion: if this symbol is already being resolved
                        // as a class instance type, skip the type resolution to prevent stack overflow.
                        // This can happen with circular class inheritance across multiple files.
                        let is_being_resolved = self
                            .ctx
                            .class_instance_resolution_set
                            .contains(&sym_to_check);
                        let symbol_type = if is_being_resolved {
                            // Skip type resolution for symbols already being resolved to prevent infinite recursion
                            TypeId::ERROR
                        } else {
                            self.get_type_of_symbol(sym_to_check)
                        };
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_to_check) {
                            let is_namespace = (symbol.flags & symbol_flags::MODULE) != 0;
                            // Merged declarations like `namespace N {}` + `class N {}`
                            // are valid values in `extends`. Only emit TS2708 for
                            // namespace-only symbols.
                            let has_non_namespace_value = (symbol.flags
                                & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE))
                                != 0;
                            if is_namespace && !has_non_namespace_value {
                                if let Some(name) = self.heritage_name_text(expr_idx) {
                                    if is_class_declaration && is_extends_clause {
                                        self.error_namespace_used_as_value_at(&name, expr_idx);
                                    } else {
                                        self.error_namespace_used_as_type_at(&name, expr_idx);
                                    }
                                }
                                continue;
                            }
                        }

                        // TS2675: Check if base class has a private constructor (only for class declarations)
                        if is_class_declaration {
                            use crate::state::MemberAccessLevel;
                            if let Some(MemberAccessLevel::Private) =
                                self.class_constructor_access_level(sym_to_check)
                            {
                                // Check if we are inside the class that defines the private constructor
                                // Nested classes can extend a class with private constructor
                                let is_accessible = if let Some(ref enclosing) =
                                    self.ctx.enclosing_class
                                {
                                    // Get the symbol of the enclosing class
                                    self.ctx
                                        .binder
                                        .get_node_symbol(enclosing.class_idx)
                                        .is_some_and(|enclosing_sym| enclosing_sym == sym_to_check)
                                } else {
                                    false
                                };

                                if !is_accessible {
                                    if let Some(name) = self.heritage_name_text(expr_idx) {
                                        use crate::diagnostics::{
                                            diagnostic_codes, diagnostic_messages, format_message,
                                        };
                                        let message = format_message(
                                            diagnostic_messages::CANNOT_EXTEND_A_CLASS_CLASS_CONSTRUCTOR_IS_MARKED_AS_PRIVATE,
                                            &[&name],
                                        );
                                        self.error_at_node(
                                            expr_idx,
                                            &message,
                                            diagnostic_codes::CANNOT_EXTEND_A_CLASS_CLASS_CONSTRUCTOR_IS_MARKED_AS_PRIVATE,
                                        );
                                    }
                                    // Continue to next type - no need to check further for this symbol
                                    continue;
                                }
                            }
                        }

                        // Check if this is ONLY an interface (not also a class or variable
                        // from declaration merging) - emit TS2689 instead of TS2507
                        // BUT only for class declarations, not interface declarations
                        // (interfaces can validly extend other interfaces)
                        // When a name is both an interface and a class (merged declaration),
                        // the class part can be validly extended, so don't emit TS2689.
                        // Also skip when the symbol has VARIABLE flag — built-in types
                        // like Array, Object, Promise have both interface and variable
                        // declarations (`interface Array` + `declare var Array: ArrayConstructor`),
                        // and the variable provides the constructor for extends.
                        let is_interface_only =
                            self.ctx.binder.get_symbol(sym_to_check).is_some_and(|s| {
                                (s.flags & symbol_flags::INTERFACE) != 0
                                    && (s.flags & symbol_flags::CLASS) == 0
                                    && (s.flags & symbol_flags::VARIABLE) == 0
                            });

                        if is_interface_only && is_class_declaration {
                            // Emit TS2689: Cannot extend an interface (only for classes)
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::CANNOT_EXTEND_AN_INTERFACE_DID_YOU_MEAN_IMPLEMENTS,
                                    &[&name],
                                );
                                self.error_at_node(
                                    expr_idx,
                                    &message,
                                    diagnostic_codes::CANNOT_EXTEND_AN_INTERFACE_DID_YOU_MEAN_IMPLEMENTS,
                                );
                            }
                        } else if !is_interface_only
                            && is_class_declaration
                            && symbol_type != TypeId::ERROR  // Skip error recovery - don't emit TS2507 for unresolved types
                            && !self.is_constructor_type(symbol_type)
                            && !self.is_class_symbol(sym_to_check)
                            // Skip TS2507 for symbols with both INTERFACE and VARIABLE flags
                            // (built-in types like Array, Object, Promise) — the variable
                            // side provides the constructor even though the interface type
                            // doesn't have construct signatures.
                            && self
                                .ctx
                                .binder
                                .get_symbol(sym_to_check)
                                .is_none_or(|s| {
                                    !((s.flags & symbol_flags::INTERFACE) != 0
                                        && (s.flags & symbol_flags::VARIABLE) != 0)
                                })
                        {
                            // For classes extending non-interfaces: emit TS2507 if not a constructor type
                            // For interfaces: don't check constructor types (interfaces can extend any interface)
                            if let Some(name) = self.heritage_name_text(expr_idx) {
                                use crate::diagnostics::{
                                    diagnostic_codes, diagnostic_messages, format_message,
                                };
                                let message = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    &[&name],
                                );
                                self.error_at_node(
                                    expr_idx,
                                    &message,
                                    diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                );
                            }
                        }
                    }
                } else {
                    // Heritage expression with explicit type arguments over a call expression
                    // (e.g. `class C extends getBase()<T> {}`) should report TS2315 when
                    // the expression resolves but is not generic.
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                        && let Some(type_args) = expr_type_args.type_arguments.as_ref()
                        && !type_args.nodes.is_empty()
                        && let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                    {
                        let expr_type = self.get_type_of_node(expr_idx);
                        let has_generic_construct_sig =
                            tsz_solver::type_queries::get_construct_signatures(
                                self.ctx.types,
                                expr_type,
                            )
                            .is_some_and(|sigs| sigs.iter().any(|sig| !sig.type_params.is_empty()));
                        if !tsz_solver::type_queries::is_generic_type(self.ctx.types, expr_type)
                            && !has_generic_construct_sig
                            && expr_type != TypeId::ERROR
                            && expr_type != TypeId::ANY
                            && let Some(&arg_idx) = type_args.nodes.first()
                        {
                            let name = self
                                .heritage_name_text(expr_idx)
                                .unwrap_or_else(|| "<expression>".to_string());
                            self.error_at_node_msg(
                                arg_idx,
                                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                                &[name.as_str()],
                            );
                        }
                    }

                    // Could not resolve as a heritage symbol - check if it's an identifier
                    // that references a value with a constructor type
                    //
                    // For property access expressions (e.g., `M1.A`, `"".bogus`),
                    // skip TS2304 — normal type checking will emit TS2339 if the property
                    // doesn't exist, matching tsc behavior.
                    if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && expr_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    {
                        continue;
                    }

                    let is_valid_constructor = if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && expr_node.kind == SyntaxKind::Identifier as u16
                    {
                        // Check if this is a primitive type keyword in a class heritage clause.
                        // TypeScript reports dedicated diagnostics:
                        // - TS2863 for `class C extends number {}`
                        // - TS2864 for `class C implements number {}`
                        if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                            let name = ident.escaped_text.as_str();
                            if matches!(
                                name,
                                "number"
                                    | "string"
                                    | "boolean"
                                    | "symbol"
                                    | "bigint"
                                    | "any"
                                    | "unknown"
                                    | "never"
                                    | "object"
                            ) {
                                if is_class_declaration {
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages, format_message,
                                    };

                                    if is_extends_clause {
                                        let message = format_message(
                                            diagnostic_messages::A_CLASS_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_CLASSES_CAN_ONLY_EXTEND_CONSTRUCTABL,
                                            &[name],
                                        );
                                        self.error_at_node(
                                            expr_idx,
                                            &message,
                                            diagnostic_codes::A_CLASS_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_CLASSES_CAN_ONLY_EXTEND_CONSTRUCTABL,
                                        );
                                    } else {
                                        let message = format_message(
                                            diagnostic_messages::A_CLASS_CANNOT_IMPLEMENT_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_IMPLEMENT_OTHER_NAMED,
                                            &[name],
                                        );
                                        self.error_at_node(
                                            expr_idx,
                                            &message,
                                            diagnostic_codes::A_CLASS_CANNOT_IMPLEMENT_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_IMPLEMENT_OTHER_NAMED,
                                        );
                                    }
                                } else if is_extends_clause {
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages, format_message,
                                    };
                                    let message = format_message(
                                        diagnostic_messages::AN_INTERFACE_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_EXTEND_OTHER_NAMED,
                                        &[name],
                                    );
                                    self.error_at_node(
                                        expr_idx,
                                        &message,
                                        diagnostic_codes::AN_INTERFACE_CANNOT_EXTEND_A_PRIMITIVE_TYPE_LIKE_IT_CAN_ONLY_EXTEND_OTHER_NAMED,
                                    );
                                }

                                // Skip further name/type resolution for primitive type keywords.
                                continue;
                            }
                        }
                        // Try to get the type of the expression to check if it's a constructor
                        let expr_type = self.get_type_of_node(expr_idx);
                        self.is_constructor_type(expr_type)
                    } else {
                        false
                    };

                    if !is_valid_constructor {
                        if let Some(expr_node) = self.ctx.arena.get(expr_idx) {
                            // Special case: `extends null` is valid in TypeScript!
                            // It creates a class that doesn't inherit from Object.prototype
                            if expr_node.kind == SyntaxKind::NullKeyword as u16
                                || (expr_node.kind == SyntaxKind::Identifier as u16
                                    && self
                                        .ctx
                                        .arena
                                        .get_identifier(expr_node)
                                        .is_some_and(|id| id.escaped_text == "null"))
                            {
                                continue;
                            }

                            // Check for literals - emit TS2507 for extends clauses
                            // NOTE: TypeScript allows `extends null` as a special case,
                            // so we don't emit TS2507 for null in extends clauses
                            let literal_type_name: Option<&str> = match expr_node.kind {
                                k if k == SyntaxKind::NullKeyword as u16 => {
                                    // Don't error on null - TypeScript allows `extends null`
                                    None
                                }
                                k if k == SyntaxKind::UndefinedKeyword as u16 => Some("undefined"),
                                k if k == SyntaxKind::TrueKeyword as u16 => Some("true"),
                                k if k == SyntaxKind::FalseKeyword as u16 => Some("false"),
                                k if k == SyntaxKind::VoidKeyword as u16 => Some("void"),
                                k if k == SyntaxKind::NumericLiteral as u16 => Some("number"),
                                k if k == SyntaxKind::StringLiteral as u16 => Some("string"),
                                // Also check for identifiers with reserved names (parsed as identifier)
                                k if k == SyntaxKind::Identifier as u16 => {
                                    if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                                        match ident.escaped_text.as_str() {
                                            "undefined" => Some("undefined"),
                                            "void" => Some("void"),
                                            _ => None,
                                        }
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };

                            if let Some(type_name) = literal_type_name {
                                if is_extends_clause {
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages, format_message,
                                    };
                                    let message = format_message(
                                    diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    &[type_name],
                                );
                                    self.error_at_node(
                                        expr_idx,
                                        &message,
                                        diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    );
                                }
                                continue;
                            }
                        }
                        // Get the name for the error message
                        if let Some(name) = self.heritage_name_text(expr_idx) {
                            // Skip certain reserved names that are handled elsewhere or shouldn't trigger errors
                            // Note: "null" is not included because `extends null` is valid and handled above
                            // Primitive type keywords (number, string, boolean, etc.) in extends clauses
                            // are parsed as identifiers but shouldn't emit TS2318/TS2304 errors.
                            // TypeScript silently fails to resolve them without emitting these errors.
                            if matches!(
                                name.as_str(),
                                "undefined"
                                    | "true"
                                    | "false"
                                    | "void"
                                    | "0"
                                    | "number"
                                    | "string"
                                    | "boolean"
                                    | "symbol"
                                    | "bigint"
                                    | "any"
                                    | "unknown"
                                    | "never"
                                    | "object"
                            ) {
                                continue;
                            }
                            if self.is_known_global_type_name(&name) {
                                // Check if the global type is actually available in lib contexts
                                if !self.ctx.has_name_in_lib(&name) {
                                    // TS2318/TS2583: Emit error for missing global type
                                    self.error_cannot_find_global_type(&name, expr_idx);
                                }
                                continue;
                            }
                            // Skip TS2304 for property accesses on imports from unresolved modules
                            // TS2307 is already emitted for the unresolved module
                            if self.is_property_access_on_unresolved_import(expr_idx) {
                                continue;
                            }
                            // TS2422: For implements clauses referencing type parameters,
                            // emit "A class may only implement another class or interface"
                            if !is_extends_clause
                                && is_class_declaration
                                && class_type_param_names.contains(&name)
                            {
                                use crate::diagnostics::diagnostic_codes;
                                self.error_at_node(
                                    expr_idx,
                                    "A class may only implement another class or interface.",
                                    diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                );
                                continue;
                            }
                            self.error_cannot_find_name_at(&name, expr_idx);
                        }
                    }
                }
            }
        }
    }

    /// Find a reference to an enclosing class type parameter inside a base class expression.
    ///
    /// This traverses the runtime expression tree and only inspects embedded type nodes
    /// (e.g., call/new type arguments, type assertions). It intentionally skips nested
    /// function/class expression scopes to avoid shadowing false positives.
    fn find_class_type_param_ref_in_base_expression(
        &self,
        expr_idx: NodeIndex,
        class_type_param_names: &[String],
    ) -> Option<NodeIndex> {
        if expr_idx.is_none() || class_type_param_names.is_empty() {
            return None;
        }

        let mut stack = vec![expr_idx];
        let mut visited: FxHashSet<NodeIndex> = FxHashSet::default();

        while let Some(current) = stack.pop() {
            if current.is_none() || !visited.insert(current) {
                continue;
            }

            let Some(node) = self.ctx.arena.get(current) else {
                continue;
            };

            // Nested function/class expressions introduce their own type parameter
            // scopes and should not be treated as references to the outer class.
            if matches!(
                node.kind,
                syntax_kind_ext::FUNCTION_EXPRESSION
                    | syntax_kind_ext::ARROW_FUNCTION
                    | syntax_kind_ext::CLASS_EXPRESSION
            ) {
                continue;
            }

            if node.is_type_node() {
                if let Some(found) =
                    self.find_class_type_param_ref_in_type_node(current, class_type_param_names)
                {
                    return Some(found);
                }
                continue;
            }

            for child_idx in self.ctx.arena.get_children(current) {
                if child_idx.is_some() {
                    stack.push(child_idx);
                }
            }
        }

        None
    }

    /// Find a reference to one of `class_type_param_names` inside a type node.
    fn find_class_type_param_ref_in_type_node(
        &self,
        type_idx: NodeIndex,
        class_type_param_names: &[String],
    ) -> Option<NodeIndex> {
        if type_idx.is_none() || class_type_param_names.is_empty() {
            return None;
        }

        let node = self.ctx.arena.get(type_idx)?;

        match node.kind {
            k if k == syntax_kind_ext::TYPE_REFERENCE => {
                if let Some(type_ref) = self.ctx.arena.get_type_ref(node) {
                    if let Some(name_node) = self.ctx.arena.get(type_ref.type_name)
                        && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                        && class_type_param_names.contains(&ident.escaped_text)
                    {
                        return Some(type_ref.type_name);
                    }

                    if let Some(type_args) = &type_ref.type_arguments {
                        for &arg_idx in &type_args.nodes {
                            if let Some(found) = self.find_class_type_param_ref_in_type_node(
                                arg_idx,
                                class_type_param_names,
                            ) {
                                return Some(found);
                            }
                        }
                    }
                }
                None
            }
            k if k == syntax_kind_ext::FUNCTION_TYPE || k == syntax_kind_ext::CONSTRUCTOR_TYPE => {
                let func_type = self.ctx.arena.get_function_type(node)?;

                let own_params = self.collect_type_parameter_names(&func_type.type_parameters);
                let filtered: Vec<String> = class_type_param_names
                    .iter()
                    .filter(|name| !own_params.contains(*name))
                    .cloned()
                    .collect();

                let names_to_check: &[String] = if own_params.is_empty() {
                    class_type_param_names
                } else if filtered.is_empty() {
                    return None;
                } else {
                    &filtered
                };

                for &param_idx in &func_type.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && let Some(found) = self.find_class_type_param_ref_in_type_node(
                            param.type_annotation,
                            names_to_check,
                        )
                    {
                        return Some(found);
                    }
                }

                self.find_class_type_param_ref_in_type_node(
                    func_type.type_annotation,
                    names_to_check,
                )
            }
            _ => {
                for child_idx in self.ctx.arena.get_children(type_idx) {
                    if let Some(found) = self
                        .find_class_type_param_ref_in_type_node(child_idx, class_type_param_names)
                    {
                        return Some(found);
                    }
                }
                None
            }
        }
    }

    /// Collect type parameter names from a type parameter list.
    fn collect_type_parameter_names(
        &self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<String> {
        let Some(list) = type_parameters else {
            return Vec::new();
        };

        let mut names = Vec::new();
        for &param_idx in &list.nodes {
            if let Some(node) = self.ctx.arena.get(param_idx)
                && let Some(param) = self.ctx.arena.get_type_parameter(node)
                && let Some(name_node) = self.ctx.arena.get(param.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                names.push(ident.escaped_text.clone());
            }
        }
        names
    }

    /// TS2449/TS2450: Check if a class or enum referenced in a heritage clause
    /// is used before its declaration in the source order.
    fn check_heritage_class_before_declaration(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        usage_idx: NodeIndex,
    ) {
        use tsz_binder::symbol_flags;

        let Some(symbol) = self.ctx.binder.symbols.get(sym_id) else {
            return;
        };

        let is_class = symbol.flags & symbol_flags::CLASS != 0;
        let is_enum = symbol.flags & symbol_flags::REGULAR_ENUM != 0;
        if !is_class && !is_enum {
            return;
        }

        // Skip check for cross-file symbols (imported from another file).
        // Position comparison only makes sense within the same file.
        if symbol.import_module.is_some() {
            return;
        }
        // If decl_file_idx is set and differs from the current file, the declaration
        // is in another file — TDZ position comparison is meaningless across files.
        if symbol.decl_file_idx != u32::MAX
            && symbol.decl_file_idx != self.ctx.current_file_idx as u32
        {
            return;
        }

        // Get the declaration position
        let decl_idx = if symbol.value_declaration.is_some() {
            symbol.value_declaration
        } else if let Some(&first_decl) = symbol.declarations.first() {
            first_decl
        } else {
            return;
        };

        let Some(usage_node) = self.ctx.arena.get(usage_idx) else {
            return;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return;
        };

        // In multi-file mode, decl_idx may be from a different file's arena.
        // Validate that the node at decl_idx actually matches the expected kind.
        // A mismatch means the declaration is in another file — no TDZ applies.
        if self.ctx.all_arenas.is_some() {
            let kind_ok = (is_class
                && (decl_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || decl_node.kind == syntax_kind_ext::CLASS_EXPRESSION))
                || (is_enum && decl_node.kind == syntax_kind_ext::ENUM_DECLARATION);
            if !kind_ok {
                return;
            }
        }

        // Skip check for ambient declarations — `declare class` is hoisted
        // and can be referenced before its source position.
        if self.is_ambient_declaration(decl_idx) {
            return;
        }

        // Skip check for ambient declarations - they don't have runtime initialization order
        // Check if the using class (heritage clause) is in an ambient declaration
        if is_class {
            // Find the class declaration that contains this heritage clause usage
            let mut current = usage_idx;
            while let Some(ext) = self.ctx.arena.get_extended(current) {
                let parent = ext.parent;
                if parent.is_none() {
                    break;
                }
                if let Some(parent_node) = self.ctx.arena.get(parent)
                    && parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                {
                    // Check if this class is ambient
                    if self.is_ambient_class_declaration(parent) {
                        return;
                    }
                    break; // Found the containing class, no need to check further
                }
                current = parent;
            }
        }

        // Only flag if usage is before declaration in source order
        if usage_node.pos >= decl_node.pos {
            return;
        }

        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        // Get the name from the usage site
        let name = self.heritage_name_text(usage_idx).unwrap_or_default();

        let (msg_template, code) = if is_class {
            (
                diagnostic_messages::CLASS_USED_BEFORE_ITS_DECLARATION,
                diagnostic_codes::CLASS_USED_BEFORE_ITS_DECLARATION,
            )
        } else {
            (
                diagnostic_messages::ENUM_USED_BEFORE_ITS_DECLARATION,
                diagnostic_codes::ENUM_USED_BEFORE_ITS_DECLARATION,
            )
        };
        let message = format_message(msg_template, &[&name]);
        self.error_at_node(usage_idx, &message, code);
    }

    /// Check a class declaration.
    pub(crate) fn check_class_declaration(&mut self, stmt_idx: NodeIndex) {
        use crate::class_inheritance::ClassInheritanceChecker;
        use crate::diagnostics::diagnostic_codes;

        // Optimization: Skip if already fully checked
        if self.ctx.checked_classes.contains(&stmt_idx) {
            return;
        }

        // Recursion guard: if we're already checking this class, return early.
        // This handles complex cycles where class checking triggers type resolution
        // (e.g. for method return types) that references the class itself or its base.
        if !self.ctx.checking_classes.insert(stmt_idx) {
            return;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            self.ctx.checking_classes.remove(&stmt_idx);
            self.ctx.checked_classes.insert(stmt_idx);
            return;
        };

        let Some(class) = self.ctx.arena.get_class(node) else {
            self.ctx.checking_classes.remove(&stmt_idx);
            self.ctx.checked_classes.insert(stmt_idx);
            return;
        };

        // TS1042: async modifier cannot be used on class declarations
        self.check_async_modifier_on_declaration(&class.modifiers);

        // CRITICAL: Check for circular inheritance using InheritanceGraph
        // This prevents stack overflow from infinite recursion in get_class_instance_type
        // Must be done BEFORE any type checking to catch cycles early
        let mut checker = ClassInheritanceChecker::new(&mut self.ctx);
        if checker.check_class_inheritance_cycle(stmt_idx, class) {
            self.ctx.checking_classes.remove(&stmt_idx);
            self.ctx.checked_classes.insert(stmt_idx);
            return; // Cycle detected - error already emitted, skip all type checking
        }

        // TS1212: Check class name for strict mode reserved words
        self.check_strict_mode_reserved_name_at(class.name, stmt_idx);

        // Check for reserved class names (error 2414)
        if class.name.is_some()
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text == "any"
        {
            self.error_at_node(
                class.name,
                "Class name cannot be 'any'.",
                diagnostic_codes::CLASS_NAME_CANNOT_BE,
            );
        }

        // TS2725: Class name cannot be 'Object' when targeting ES5 and above with module X
        // Only applies to non-ES module kinds (CommonJS, AMD, UMD, System) and non-ambient classes
        if class.name.is_some()
            && !self.has_declare_modifier(&class.modifiers)
            && let Some(name_node) = self.ctx.arena.get(class.name)
            && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            && ident.escaped_text == "Object"
        {
            use tsz_common::common::ModuleKind;
            let module = self.ctx.compiler_options.module;
            let module_name = match module {
                ModuleKind::CommonJS => Some("CommonJS"),
                ModuleKind::AMD => Some("AMD"),
                ModuleKind::UMD => Some("UMD"),
                ModuleKind::System => Some("System"),
                _ => None, // ES modules and None don't trigger this error
            };
            if let Some(module_name) = module_name {
                self.error_at_node(
                    class.name,
                    &format!(
                        "Class name cannot be 'Object' when targeting ES5 and above with module {module_name}."
                    ),
                    diagnostic_codes::CLASS_NAME_CANNOT_BE_OBJECT_WHEN_TARGETING_ES5_AND_ABOVE_WITH_MODULE,
                );
            }
        }

        // Check if this is a declared class (ambient declaration)
        let is_declared = self.has_declare_modifier(&class.modifiers);

        // Check if this class is abstract
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        // Push type parameters BEFORE checking heritage clauses and abstract members
        // This allows heritage clauses and member checks to reference the class's type parameters
        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        // Collect class type parameter names for TS2302 checking in static members
        let class_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        // Check for unused type parameters (TS6133)
        self.check_unused_type_params(&class.type_parameters, stmt_idx);

        // Check heritage clauses for unresolved names (TS2304)
        // Must be checked AFTER type parameters are pushed so heritage can reference type params
        self.check_heritage_clauses_for_unresolved_names(
            &class.heritage_clauses,
            true,
            &class_type_param_names,
        );

        // Check for abstract members in non-abstract class (error 1253),
        // private identifiers in ambient classes (error 2819),
        // and private identifiers when targeting ES5 or lower (error 18028)
        for &member_idx in &class.members.nodes {
            if let Some(member_node) = self.ctx.arena.get(member_idx) {
                // Get member name for private identifier checks
                let member_name_idx = match member_node.kind {
                    syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .map(|p| p.name),
                    syntax_kind_ext::METHOD_DECLARATION => {
                        self.ctx.arena.get_method_decl(member_node).map(|m| m.name)
                    }
                    syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                        self.ctx.arena.get_accessor(member_node).map(|a| a.name)
                    }
                    _ => None,
                };
                let Some(member_name_idx) = member_name_idx else {
                    continue;
                };

                // Check if member has a private identifier name
                let is_private_identifier =
                    self.ctx.arena.get(member_name_idx).is_some_and(|node| {
                        node.kind == tsz_scanner::SyntaxKind::PrivateIdentifier as u16
                    });

                if is_private_identifier {
                    use crate::context::ScriptTarget;
                    use crate::diagnostics::diagnostic_messages;

                    // TS18028: Check for private identifiers when targeting ES5 or lower
                    let is_es5_or_lower = matches!(
                        self.ctx.compiler_options.target,
                        ScriptTarget::ES3 | ScriptTarget::ES5
                    );
                    if is_es5_or_lower {
                        self.error_at_node(
                            member_name_idx,
                            diagnostic_messages::PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER,
                            diagnostic_codes::PRIVATE_IDENTIFIERS_ARE_ONLY_AVAILABLE_WHEN_TARGETING_ECMASCRIPT_2015_AND_HIGHER,
                        );
                    }

                    // TS18019: Check for private identifiers in ambient classes
                    if is_declared {
                        self.error_at_node(
                            member_name_idx,
                            diagnostic_messages::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            diagnostic_codes::MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                        );
                    }
                }

                // Check for abstract members in non-abstract class
                if !is_abstract_class {
                    let member_has_abstract = match member_node.kind {
                        syntax_kind_ext::PROPERTY_DECLARATION => {
                            if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                                self.has_abstract_modifier(&prop.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::METHOD_DECLARATION => {
                            if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                                self.has_abstract_modifier(&method.modifiers)
                            } else {
                                false
                            }
                        }
                        syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                            if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                                self.has_abstract_modifier(&accessor.modifiers)
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };

                    if member_has_abstract {
                        // TS1244 for methods/accessors, TS1253 for properties
                        let is_method = matches!(
                            member_node.kind,
                            syntax_kind_ext::METHOD_DECLARATION
                                | syntax_kind_ext::GET_ACCESSOR
                                | syntax_kind_ext::SET_ACCESSOR
                        );
                        if is_method {
                            self.error_at_node(
                                member_idx,
                                "Abstract methods can only appear within an abstract class.",
                                diagnostic_codes::ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
                            );
                        } else {
                            self.error_at_node(
                                member_idx,
                                "Abstract properties can only appear within an abstract class.",
                                diagnostic_codes::ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS,
                            );
                        }
                    }
                }
            }
        }

        // Collect class name
        let class_name = self.get_class_name_from_decl(stmt_idx);

        // Save previous enclosing class and set current
        let prev_enclosing_class = self.ctx.enclosing_class.take();
        self.ctx.enclosing_class = Some(EnclosingClassInfo {
            name: class_name,
            class_idx: stmt_idx,
            member_nodes: class.members.nodes.clone(),
            in_constructor: false,
            is_declared,
            in_static_property_initializer: false,
            in_static_method: false,
            has_super_call_in_current_constructor: false,
            cached_instance_this_type: None,
            type_param_names: class_type_param_names,
        });

        // Class bodies reset the async context — field initializers and static blocks
        // don't inherit async from the enclosing function. Methods define their own context.
        let saved_async_depth = self.ctx.async_depth;
        self.ctx.async_depth = 0;

        // Check each class member
        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        self.ctx.async_depth = saved_async_depth;

        // Check for duplicate member names (TS2300, TS2393)
        self.check_duplicate_class_members(&class.members.nodes);

        // Check for missing method/constructor implementations (2389, 2390, 2391)
        // Skip for declared classes (ambient declarations don't need implementations)
        if !is_declared {
            self.check_class_member_implementations(&class.members.nodes);
        }

        // Check static/instance consistency for method overloads (TS2387, TS2388)
        self.check_static_instance_overload_consistency(&class.members.nodes);

        // Check for accessor abstract consistency (error 2676)
        // Getter and setter must both be abstract or both non-abstract
        self.check_accessor_abstract_consistency(&class.members.nodes);

        // Check for accessor type compatibility (TS2322)
        // TS 5.1+ allows divergent types ONLY if both have explicit annotations.
        self.check_accessor_type_compatibility(&class.members.nodes);

        // Check strict property initialization (TS2564)
        self.check_property_initialization(stmt_idx, class, is_declared, is_abstract_class);

        // TS2417 (classExtendsNull2): a class that extends `null` and merges with an
        // interface that has heritage must report static-side incompatibility with `null`.
        if self.class_extends_null(class) && self.class_has_merged_interface_extends(class) {
            let class_name = if let Some(name_node) = self.ctx.arena.get(class.name) {
                self.ctx
                    .arena
                    .get_identifier(name_node)
                    .map_or_else(|| "<anonymous>".to_string(), |id| id.escaped_text.clone())
            } else {
                "<anonymous>".to_string()
            };
            self.error_at_node(
                class.name,
                &format!(
                    "Class static side 'typeof {class_name}' incorrectly extends base class static side 'null'."
                ),
                diagnostic_codes::CLASS_STATIC_SIDE_INCORRECTLY_EXTENDS_BASE_CLASS_STATIC_SIDE,
            );
        }

        // Check for property type compatibility with base class (error 2416)
        // Property type in derived class must be assignable to same property in base class
        self.check_property_inheritance_compatibility(stmt_idx, class);

        // Check that non-abstract class implements all abstract members from base class (error 2654)
        self.check_abstract_member_implementations(stmt_idx, class);

        // Check that class properly implements all interfaces from implements clauses (error 2420)
        self.check_implements_clauses(stmt_idx, class);

        // Check that class properties are compatible with index signatures (TS2411)
        // Get the class instance type (not constructor type) to access instance index signatures
        let class_instance_type = self.get_class_instance_type(stmt_idx, class);
        self.check_index_signature_compatibility(&class.members.nodes, class_instance_type);

        self.check_class_declaration(stmt_idx);

        self.check_index_signature_compatibility(&class.members.nodes, class_instance_type);

        // Check for decorator-related global types (TS2318)
        // When experimentalDecorators is enabled and a method/accessor has decorators,
        // TypedPropertyDescriptor must be available
        self.check_decorator_global_types(&class.members.nodes);

        // Check for decorator-related global types (TS2318)
        // When experimentalDecorators is enabled and a method/accessor has decorators,
        // TypedPropertyDescriptor must be available
        self.check_decorator_global_types(&class.members.nodes);

        // Restore previous enclosing class
        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);

        self.ctx.checked_classes.insert(stmt_idx);
        self.ctx.checking_classes.remove(&stmt_idx);
    }

    pub(crate) fn check_class_expression(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) {
        // TS8004: Type parameters on class expression in JS files
        if self.is_js_file() {
            if let Some(ref type_params) = class.type_parameters
                && !type_params.nodes.is_empty()
            {
                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                self.error_at_node(
                        type_params.nodes[0],
                        diagnostic_messages::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                        diagnostic_codes::TYPE_PARAMETER_DECLARATIONS_CAN_ONLY_BE_USED_IN_TYPESCRIPT_FILES,
                    );
            }

            // Also check members for JS grammar errors
            for &member_idx in &class.members.nodes {
                self.check_js_grammar_class_member(member_idx);
            }
        }

        let (_type_params, type_param_updates) = self.push_type_parameters(&class.type_parameters);

        let class_type_param_names: Vec<String> = type_param_updates
            .iter()
            .map(|(name, _)| name.clone())
            .collect();

        let class_name = self.get_class_name_from_decl(class_idx);
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);

        let prev_enclosing_class = self.ctx.enclosing_class.take();
        self.ctx.enclosing_class = Some(EnclosingClassInfo {
            name: class_name,
            class_idx,
            member_nodes: class.members.nodes.clone(),
            in_constructor: false,
            is_declared: false,
            in_static_property_initializer: false,
            in_static_method: false,
            has_super_call_in_current_constructor: false,
            cached_instance_this_type: None,
            type_param_names: class_type_param_names,
        });

        // Class bodies reset the async context — field initializers don't
        // inherit async from the enclosing function.
        let saved_async_depth = self.ctx.async_depth;
        self.ctx.async_depth = 0;

        for &member_idx in &class.members.nodes {
            self.check_class_member(member_idx);
        }

        self.ctx.async_depth = saved_async_depth;

        // Check strict property initialization (TS2564) for class expressions
        // Class expressions should have the same property initialization checks as class declarations
        self.check_property_initialization(class_idx, class, false, is_abstract_class);

        // Check for decorator-related global types (TS2318)
        self.check_decorator_global_types(&class.members.nodes);

        self.ctx.enclosing_class = prev_enclosing_class;

        self.pop_type_parameters(type_param_updates);
    }

    pub(crate) fn check_property_initialization(
        &mut self,
        _class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        is_declared: bool,
        _is_abstract: bool,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

        // Skip TS2564 for declared classes (ambient declarations) and .d.ts files.
        // In tsc, .d.ts files are inherently ambient even without the `declare` keyword.
        // Note: Abstract classes DO get TS2564 errors - they can have constructors
        // and properties must be initialized either with defaults or in the constructor
        if is_declared || self.ctx.file_name.ends_with(".d.ts") {
            return;
        }

        // Only check property initialization when strictPropertyInitialization is enabled
        // tsc also requires strictNullChecks to be enabled for TS2564
        if !self.ctx.strict_property_initialization() || !self.ctx.strict_null_checks() {
            return;
        }

        // Check if this is a derived class (has base class)
        let is_derived_class = self.class_has_base(class);

        let mut properties = Vec::new();
        let mut tracked = FxHashSet::default();
        let mut parameter_properties = FxHashSet::default();

        // First pass: collect parameter properties from constructor
        // Parameter properties are always definitely assigned
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                continue;
            };

            // Collect parameter properties from constructor parameters
            for &param_idx in &ctor.parameters.nodes {
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };

                // Parameter properties have modifiers (public/private/protected/readonly)
                if param.modifiers.is_some()
                    && let Some(key) = self.property_key_from_name(param.name)
                {
                    parameter_properties.insert(key.clone());
                }
            }
        }

        // Second pass: collect class properties that need initialization
        for &member_idx in &class.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }

            let Some(prop) = self.ctx.arena.get_property_decl(node) else {
                continue;
            };

            if !self.property_requires_initialization(member_idx, prop, is_derived_class) {
                continue;
            }

            let Some(key) = self.property_key_from_name(prop.name) else {
                continue;
            };

            // Get property name for error message. Use fallback for complex computed properties.
            let name = self.get_property_name(prop.name).unwrap_or_else(|| {
                // For complex computed properties (e.g., [getKey()]), use a descriptive fallback
                match &key {
                    PropertyKey::Computed(ComputedKey::Ident(s)) => format!("[{s}]"),
                    PropertyKey::Computed(ComputedKey::String(s)) => format!("[\"{s}\"]"),
                    PropertyKey::Computed(ComputedKey::Number(n)) => format!("[{n}]"),
                    PropertyKey::Private(s) => format!("#{s}"),
                    PropertyKey::Ident(s) => s.clone(),
                }
            });

            tracked.insert(key.clone());
            properties.push((key, name, prop.name));
        }

        if properties.is_empty() {
            return;
        }

        let requires_super = self.class_has_base(class);
        let constructor_body = self.find_constructor_body(&class.members);
        let assigned = if let Some(body_idx) = constructor_body {
            constructor_assigned_properties(self, body_idx, &tracked, requires_super)
        } else {
            FxHashSet::default()
        };

        for (key, name, name_node) in properties {
            // Property is assigned if it's in the assigned set OR it's a parameter property
            if assigned.contains(&key) || parameter_properties.contains(&key) {
                continue;
            }
            use crate::diagnostics::format_message;

            // Use TS2524 if there's a constructor (definite assignment analysis)
            // Use TS2564 if no constructor (just missing initializer)
            let (message, code) = (
                diagnostic_messages::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
                diagnostic_codes::PROPERTY_HAS_NO_INITIALIZER_AND_IS_NOT_DEFINITELY_ASSIGNED_IN_THE_CONSTRUCTOR,
            );

            self.error_at_node(name_node, &format_message(message, &[&name]), code);
        }

        // Check for TS2565 (Property used before being assigned in constructor)
        if let Some(body_idx) = constructor_body {
            check_constructor_property_use_before_assignment(
                self,
                body_idx,
                &tracked,
                requires_super,
            );
        }
    }

    pub(crate) fn property_requires_initialization(
        &mut self,
        member_idx: NodeIndex,
        prop: &tsz_parser::parser::node::PropertyDeclData,
        is_derived_class: bool,
    ) -> bool {
        use tsz_scanner::SyntaxKind;

        if prop.initializer.is_some()
            || prop.question_token
            || prop.exclamation_token
            || self.has_static_modifier(&prop.modifiers)
            || self.has_abstract_modifier(&prop.modifiers)
            || self.has_declare_modifier(&prop.modifiers)
        {
            return false;
        }

        // Properties with string or numeric literal names are not checked for strict property initialization
        // Example: class C { "b": number; 0: number; }  // These are not checked
        let Some(name_node) = self.ctx.arena.get(prop.name) else {
            return false;
        };
        if matches!(
            name_node.kind,
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
        ) {
            return false;
        }

        let prop_type = if prop.type_annotation.is_some() {
            self.get_type_from_type_node(prop.type_annotation)
        } else if let Some(sym_id) = self.ctx.binder.get_node_symbol(member_idx) {
            self.get_type_of_symbol(sym_id)
        } else {
            TypeId::ANY
        };

        // Enhanced property initialization checking:
        // 1. ANY/UNKNOWN types don't need initialization
        // 2. Union types with undefined don't need initialization
        // 3. Optional types don't need initialization
        if prop_type == TypeId::ANY || prop_type == TypeId::UNKNOWN {
            return false;
        }

        // ERROR types also don't need initialization - these indicate parsing/binding errors
        if prop_type == TypeId::ERROR {
            return false;
        }

        // For derived classes, be more strict about definite assignment
        // Properties in derived classes that redeclare base class properties need initialization
        // This catches cases like: class B extends A { property: any; } where A has property
        if is_derived_class {
            // In derived classes, properties without definite assignment assertions
            // need initialization unless they include undefined in their type
            return !tsz_solver::type_queries::type_includes_undefined(self.ctx.types, prop_type);
        }

        !tsz_solver::type_queries::type_includes_undefined(self.ctx.types, prop_type)
    }

    // Note: class_has_base, type_includes_undefined, find_constructor_body are in type_checking.rs

    /// Check for TS2565: Properties used before being assigned in the constructor.
    ///
    /// This function analyzes the constructor body to detect when a property
    /// is accessed (via `this.X`) before it has been assigned a value.
    pub(crate) fn check_properties_used_before_assigned(
        &mut self,
        body_idx: NodeIndex,
        tracked: &FxHashSet<PropertyKey>,
        require_super: bool,
    ) {
        if body_idx.is_none() {
            return;
        }

        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };

        if body_node.kind != syntax_kind_ext::BLOCK {
            return;
        }

        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return;
        };

        let start_idx = if require_super {
            self.find_super_statement_start(&block.statements.nodes)
                .unwrap_or(0)
        } else {
            0
        };

        let mut assigned = FxHashSet::default();

        // Track parameter properties as already assigned
        for _key in tracked {
            // Parameter properties are assigned in the parameter list
            // We'll collect them separately if needed
        }

        // Analyze statements in order, checking for property accesses before assignment
        for &stmt_idx in block.statements.nodes.iter().skip(start_idx) {
            self.check_statement_for_early_property_access(stmt_idx, &mut assigned, tracked);
        }
    }

    /// Check a single statement for property accesses that occur before assignment.
    /// Returns true if the statement definitely assigns to the tracked property.
    pub(crate) fn check_statement_for_early_property_access(
        &mut self,
        stmt_idx: NodeIndex,
        assigned: &mut FxHashSet<PropertyKey>,
        tracked: &FxHashSet<PropertyKey>,
    ) -> bool {
        if stmt_idx.is_none() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(stmt_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::BLOCK => {
                if let Some(block) = self.ctx.arena.get_block(node) {
                    for &stmt_idx in &block.statements.nodes {
                        self.check_statement_for_early_property_access(stmt_idx, assigned, tracked);
                    }
                }
                false
            }
            k if k == syntax_kind_ext::EXPRESSION_STATEMENT => {
                if let Some(expr_stmt) = self.ctx.arena.get_expression_statement(node) {
                    self.check_expression_for_early_property_access(
                        expr_stmt.expression,
                        assigned,
                        tracked,
                    );
                }
                false
            }
            k if k == syntax_kind_ext::IF_STATEMENT => {
                if let Some(if_stmt) = self.ctx.arena.get_if_statement(node) {
                    // Check the condition expression for property accesses
                    self.check_expression_for_early_property_access(
                        if_stmt.expression,
                        assigned,
                        tracked,
                    );
                    // Check both branches
                    let mut then_assigned = assigned.clone();
                    let mut else_assigned = assigned.clone();
                    self.check_statement_for_early_property_access(
                        if_stmt.then_statement,
                        &mut then_assigned,
                        tracked,
                    );
                    if if_stmt.else_statement.is_some() {
                        self.check_statement_for_early_property_access(
                            if_stmt.else_statement,
                            &mut else_assigned,
                            tracked,
                        );
                    }
                    // Properties assigned in both branches are considered assigned
                    *assigned = then_assigned
                        .intersection(&else_assigned)
                        .cloned()
                        .collect();
                }
                false
            }
            k if k == syntax_kind_ext::RETURN_STATEMENT => {
                if let Some(ret_stmt) = self.ctx.arena.get_return_statement(node)
                    && ret_stmt.expression.is_some()
                {
                    self.check_expression_for_early_property_access(
                        ret_stmt.expression,
                        assigned,
                        tracked,
                    );
                }
                false
            }
            k if k == syntax_kind_ext::WHILE_STATEMENT
                || k == syntax_kind_ext::DO_STATEMENT
                || k == syntax_kind_ext::FOR_STATEMENT
                || k == syntax_kind_ext::FOR_IN_STATEMENT
                || k == syntax_kind_ext::FOR_OF_STATEMENT =>
            {
                // For loops, we conservatively don't track assignments across iterations
                // This is a simplified approach - the full TypeScript implementation is more complex
                false
            }
            k if k == syntax_kind_ext::TRY_STATEMENT => {
                if let Some(try_stmt) = self.ctx.arena.get_try(node) {
                    self.check_statement_for_early_property_access(
                        try_stmt.try_block,
                        assigned,
                        tracked,
                    );
                    // Check catch and finally blocks
                    // ...
                }
                false
            }
            k if k == syntax_kind_ext::VARIABLE_STATEMENT => {
                if let Some(var_stmt) = self.ctx.arena.get_variable(node) {
                    for &decl_idx in &var_stmt.declarations.nodes {
                        if let Some(decl_node) = self.ctx.arena.get(decl_idx)
                            && let Some(decl) = self.ctx.arena.get_variable_declaration(decl_node)
                            && decl.initializer.is_some()
                        {
                            self.check_expression_for_early_property_access(
                                decl.initializer,
                                assigned,
                                tracked,
                            );
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }

    // Flow analysis functions moved to checker/flow_analysis.rs

    /// Check for decorator-related global types (TS2318).
    ///
    /// When experimentalDecorators is enabled and a method or accessor has decorators,
    /// TypeScript requires the `TypedPropertyDescriptor` type to be available.
    /// If it's not available (e.g., with noLib), we emit TS2318.
    pub(crate) fn check_decorator_global_types(&mut self, members: &[NodeIndex]) {
        // Only check if experimentalDecorators is enabled
        if !self.ctx.compiler_options.experimental_decorators {
            return;
        }

        // Check if any method or accessor has decorators
        let mut has_method_or_accessor_decorator = false;
        for &member_idx in members {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            let modifiers = match node.kind {
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(node)
                    .and_then(|m| m.modifiers.as_ref()),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(node)
                        .and_then(|a| a.modifiers.as_ref())
                }
                _ => continue,
            };

            if let Some(mods) = modifiers {
                for &mod_idx in &mods.nodes {
                    if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                        && mod_node.kind == syntax_kind_ext::DECORATOR
                    {
                        has_method_or_accessor_decorator = true;
                        break;
                    }
                }
            }
            if has_method_or_accessor_decorator {
                break;
            }
        }

        if !has_method_or_accessor_decorator {
            return;
        }

        // Check if TypedPropertyDescriptor is available
        let type_name = "TypedPropertyDescriptor";
        if self.ctx.has_name_in_lib(type_name) {
            return; // Type is available from lib
        }
        if self.ctx.binder.file_locals.has(type_name) {
            return; // Type is declared locally
        }

        // TypedPropertyDescriptor is not available - emit TS2318
        // TSC emits this error twice for method decorators
        use tsz_binder::lib_loader::emit_error_global_type_missing;
        let diag = emit_error_global_type_missing(type_name, self.ctx.file_name.clone(), 0, 0);
        self.ctx.push_diagnostic(diag.clone());
        self.ctx.push_diagnostic(diag);
    }

    /// Check triple-slash reference directives and emit TS6053 for missing files.
    ///
    /// Validates `/// <reference path="..." />` directives in TypeScript source files.
    /// If a referenced file doesn't exist, emits error 6053.
    pub(crate) fn check_triple_slash_references(&mut self, file_name: &str, source_text: &str) {
        use crate::triple_slash_validator::{extract_reference_paths, validate_reference_path};
        use std::collections::HashSet;
        use std::path::Path;

        let references = extract_reference_paths(source_text);
        if references.is_empty() {
            return;
        }

        let source_path = Path::new(file_name);

        let mut known_files: HashSet<String> = HashSet::new();
        if let Some(arenas) = self.ctx.all_arenas.as_ref() {
            for arena in arenas.iter() {
                for source_file in &arena.source_files {
                    known_files.insert(source_file.file_name.clone());
                }
            }
        } else {
            for source_file in &self.ctx.arena.source_files {
                known_files.insert(source_file.file_name.clone());
            }
        }

        let has_virtual_reference = |reference_path: &str| {
            let base = source_path.parent().unwrap_or_else(|| Path::new(""));
            if validate_reference_path(source_path, reference_path) {
                return true;
            }

            let direct_candidate = base.join(reference_path);
            if known_files.contains(direct_candidate.to_string_lossy().as_ref()) {
                return true;
            }

            if !reference_path.contains('.') {
                for ext in [".ts", ".tsx", ".d.ts"] {
                    let candidate = base.join(format!("{reference_path}{ext}"));
                    if known_files.contains(candidate.to_string_lossy().as_ref()) {
                        return true;
                    }
                }
            }
            false
        };

        for (reference_path, line_num) in references {
            if !has_virtual_reference(&reference_path) {
                // Calculate the position of the error (start of the line)
                let mut pos = 0u32;
                for (idx, _) in source_text.lines().enumerate() {
                    if idx == line_num {
                        break;
                    }
                    pos += source_text.lines().nth(idx).map_or(0, |l| l.len() + 1) as u32;
                }

                // Find the actual directive on the line to get accurate position
                if let Some(line) = source_text.lines().nth(line_num)
                    && let Some(directive_start) = line.find("///")
                {
                    pos += directive_start as u32;
                }

                let length = source_text
                    .lines()
                    .nth(line_num)
                    .map_or(0, |l| l.len() as u32);

                use crate::diagnostics::{diagnostic_codes, format_message};
                let message = format_message("File '{0}' not found.", &[&reference_path]);
                self.emit_error_at(pos, length, &message, diagnostic_codes::FILE_NOT_FOUND);
            }
        }
    }

    /// Check for duplicate AMD module name assignments.
    ///
    /// Validates `/// <amd-module name="..." />` directives in TypeScript source files.
    /// If multiple AMD module name assignments are found, emits error TS2458.
    pub(crate) fn check_amd_module_names(&mut self, source_text: &str) {
        use crate::triple_slash_validator::extract_amd_module_names;

        let amd_modules = extract_amd_module_names(source_text);

        // Only emit error if there are multiple AMD module name assignments
        if amd_modules.len() <= 1 {
            return;
        }

        // Emit TS2458 error at the position of the second (and subsequent) directive(s)
        for (_, line_num) in amd_modules.iter().skip(1) {
            // Calculate the position of the error (start of the line)
            let mut pos = 0u32;
            for (idx, _) in source_text.lines().enumerate() {
                if idx == *line_num {
                    break;
                }
                pos += source_text.lines().nth(idx).map_or(0, |l| l.len() + 1) as u32;
            }

            // Find the actual directive on the line to get accurate position
            if let Some(line) = source_text.lines().nth(*line_num)
                && let Some(directive_start) = line.find("///")
            {
                pos += directive_start as u32;
            }

            let length = source_text
                .lines()
                .nth(*line_num)
                .map_or(0, |l| l.len() as u32);

            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.emit_error_at(
                pos,
                length,
                diagnostic_messages::AN_AMD_MODULE_CANNOT_HAVE_MULTIPLE_NAME_ASSIGNMENTS,
                diagnostic_codes::AN_AMD_MODULE_CANNOT_HAVE_MULTIPLE_NAME_ASSIGNMENTS,
            );
        }
    }
}
