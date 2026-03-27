//! Heritage clause (extends/implements) checking for classes and interfaces.

use crate::query_boundaries::class_type as class_query;
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

                // Evaluate the heritage expression to trigger control flow analysis (TS2454)
                // and compute the actual type of the expression. We only do this for `extends`,
                // because `implements` only takes types, not expressions.
                if is_extends_clause {
                    let _ = self.get_type_of_node(expr_idx);
                }

                // TS2499: An interface can only extend an identifier/qualified-name with optional type arguments.
                if !is_class_declaration && is_extends_clause {
                    let mut is_valid = true;

                    let mut current_idx = expr_idx;
                    use tsz_parser::parser::flags::node_flags;
                    use tsz_parser::parser::syntax_kind_ext::*;

                    loop {
                        let Some(node) = self.ctx.arena.get(current_idx) else {
                            is_valid = false;
                            break;
                        };

                        if node.flags & (node_flags::OPTIONAL_CHAIN as u16) != 0 {
                            is_valid = false;
                            break;
                        }

                        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                            break;
                        } else if node.kind == PROPERTY_ACCESS_EXPRESSION
                            && let Some(p) = self.ctx.arena.get_access_expr(node)
                            && !p.question_dot_token
                        {
                            current_idx = p.expression;
                        } else {
                            is_valid = false;
                            break;
                        }
                    }

                    if !is_valid {
                        self.error_at_node(
                            expr_idx,
                            crate::diagnostics::diagnostic_messages::AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG,
                            crate::diagnostics::diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARG,
                        );
                    }
                }

                // TS2500: A class can only implement an identifier/qualified-name with optional type arguments.
                // Same check as TS2499 but for class `implements` clauses.
                if is_class_declaration && !is_extends_clause {
                    let mut is_valid = true;

                    let mut current_idx = expr_idx;
                    use tsz_parser::parser::flags::node_flags;
                    use tsz_parser::parser::syntax_kind_ext::*;

                    loop {
                        let Some(node) = self.ctx.arena.get(current_idx) else {
                            is_valid = false;
                            break;
                        };

                        if node.flags & (node_flags::OPTIONAL_CHAIN as u16) != 0 {
                            is_valid = false;
                            break;
                        }

                        if node.kind == tsz_scanner::SyntaxKind::Identifier as u16 {
                            break;
                        } else if node.kind == PROPERTY_ACCESS_EXPRESSION
                            && let Some(p) = self.ctx.arena.get_access_expr(node)
                            && !p.question_dot_token
                        {
                            current_idx = p.expression;
                        } else {
                            is_valid = false;
                            break;
                        }
                    }

                    if !is_valid {
                        self.error_at_node(
                            expr_idx,
                            crate::diagnostics::diagnostic_messages::A_CLASS_CAN_ONLY_IMPLEMENT_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARGUM,
                            crate::diagnostics::diagnostic_codes::A_CLASS_CAN_ONLY_IMPLEMENT_AN_IDENTIFIER_QUALIFIED_NAME_WITH_OPTIONAL_TYPE_ARGUM,
                        );
                    }
                }

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
                                class_query::construct_signatures_for_type(
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
                                && !type_args.nodes.is_empty()
                            {
                                let name = self
                                    .heritage_name_text(expr_idx)
                                    .unwrap_or_else(|| "<expression>".to_string());
                                self.error_at_node_msg(
                                    expr_idx,
                                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                                    &[name.as_str()],
                                );
                            }
                            // Still resolve type arguments even when the type is not
                            // generic. This ensures identifiers in type arguments are
                            // marked as referenced for noUnusedLocals (TS6133).
                            for &arg_idx in &type_args.nodes {
                                self.get_type_of_node(arg_idx);
                            }
                        } else {
                            if type_args.nodes.len() < required_count
                                && let Some(name) = self.heritage_name_text(expr_idx)
                            {
                                let type_params = self.get_type_params_for_symbol(heritage_sym);
                                let display_name = Self::format_generic_display_name_with_interner(
                                    &name,
                                    &type_params,
                                    self.ctx.types,
                                );
                                self.error_generic_type_requires_type_arguments_at(
                                    &display_name,
                                    required_count,
                                    type_idx,
                                );
                            }

                            self.validate_type_reference_type_arguments(
                                heritage_sym,
                                type_args,
                                type_idx,
                            );
                        }
                    } else if required_count > 0
                        && let Some(name) = self.heritage_name_text(expr_idx)
                    {
                        // tsc skips TS2314 for heritage clauses when:
                        // 1. JS files — type arguments are never required
                        // 2. Extends clauses where the symbol has a variable
                        //    declaration (e.g. `declare var Set: SetConstructor`)
                        //    — the constructor infers type args
                        use tsz_binder::symbol_flags;
                        let skip_ts2314 = self.is_js_file()
                            || (is_class_declaration
                                && is_extends_clause
                                && self
                                    .get_cross_file_symbol(heritage_sym)
                                    .is_some_and(|s| (s.flags & symbol_flags::VARIABLE) != 0));
                        if skip_ts2314 {
                            continue;
                        }
                        let type_params = self.get_type_params_for_symbol(heritage_sym);
                        let display_name = Self::format_generic_display_name_with_interner(
                            &name,
                            &type_params,
                            self.ctx.types,
                        );
                        self.error_generic_type_requires_type_arguments_at(
                            &display_name,
                            required_count,
                            type_idx,
                        );
                    }

                    // TS2449/TS2450: Check if class/enum is used before its declaration
                    if is_extends_clause && is_class_declaration {
                        self.check_heritage_class_before_declaration(heritage_sym, expr_idx);
                    }

                    // TS2709: Check if namespace-only symbol is used in an implements clause.
                    // For extends clauses, the namespace check happens below inside
                    // the is_extends_clause block.
                    if !is_extends_clause {
                        use tsz_binder::symbol_flags;
                        let mut visited_aliases = Vec::new();
                        let resolved_sym =
                            self.resolve_alias_symbol(heritage_sym, &mut visited_aliases);
                        let sym_to_check = resolved_sym.unwrap_or(heritage_sym);
                        if let Some(symbol) = self.get_cross_file_symbol(sym_to_check) {
                            let is_namespace = (symbol.flags & symbol_flags::MODULE) != 0;
                            let has_non_namespace_value = (symbol.flags
                                & (symbol_flags::VALUE & !symbol_flags::VALUE_MODULE))
                                != 0;
                            if is_namespace && !has_non_namespace_value {
                                if let Some(name) = self.heritage_name_text(expr_idx) {
                                    self.error_namespace_used_as_type_at(&name, expr_idx);
                                }
                                continue;
                            }
                        }
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

                        // Note: `is_being_resolved` is a recursion guard, NOT a cycle
                        // detector. A symbol being in `class_instance_resolution_set`
                        // means its type is currently being computed somewhere in the
                        // call stack — it does NOT prove a circular base expression.
                        // True TS2506 cycle detection is handled by dedicated
                        // inheritance cycle checks, not by this resolution guard.

                        if let Some(symbol) = self.get_cross_file_symbol(sym_to_check) {
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
                                        self.report_wrong_meaning_diagnostic(
                                            &name,
                                            expr_idx,
                                            crate::query_boundaries::name_resolution::NameLookupKind::Namespace,
                                        );
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
                                // Check if the extending class is lexically inside the
                                // base class (e.g., defined inside one of the base class's
                                // methods). Walk AST parents from the current node up to
                                // the root, looking for a class declaration whose symbol
                                // matches the base class. This is robust regardless of
                                // whether enclosing_class state is set (heritage checking
                                // can happen during type environment building before the
                                // statement walker sets enclosing_class).
                                let is_accessible =
                                    self.is_lexically_inside_class(expr_idx, sym_to_check);

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
                            self.get_cross_file_symbol(sym_to_check).is_some_and(|s| {
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
                        } else if !is_interface_only && is_class_declaration {
                            // Fast path: pure class symbols are valid extends targets without
                            // needing full symbol type resolution here. Merged class/value
                            // symbols (like a user class colliding with lib `Symbol`) still need
                            // constructor validation because their value side may be non-newable.
                            let skip_constructor_check = self
                                .get_cross_file_symbol(sym_to_check)
                                .is_some_and(|s| {
                                    (s.flags & symbol_flags::CLASS) != 0
                                        && (s.flags & symbol_flags::VARIABLE) == 0
                                });
                            if !skip_constructor_check {
                                let symbol_type = if is_being_resolved {
                                    // Skip type resolution for symbols already being resolved to prevent infinite recursion
                                    TypeId::ERROR
                                } else {
                                    self.get_type_of_symbol(sym_to_check)
                                };
                                if symbol_type != TypeId::ERROR
                                    && !self.is_constructor_type(symbol_type)
                                    && !self.symbol_has_js_constructor_evidence(sym_to_check)
                                    // Skip TS2507 for symbols with both INTERFACE and VARIABLE flags
                                    // (built-in types like Array, Object, Promise) — the variable
                                    // side provides the constructor even though the interface type
                                    // doesn't have construct signatures.
                                    && self
                                        .get_cross_file_symbol(sym_to_check)
                                        .is_none_or(|s| {
                                            !((s.flags & symbol_flags::INTERFACE) != 0
                                                && (s.flags & symbol_flags::VARIABLE) != 0)
                                        })
                                {
                                    // For classes extending non-interfaces: emit TS2507 if not a constructor type
                                    // Use the resolved value type (not the variable name) to match tsc behavior.
                                    {
                                        use crate::diagnostics::{
                                            diagnostic_codes, diagnostic_messages, format_message,
                                        };
                                        let type_name = self.format_type(symbol_type);
                                        let message = format_message(
                                            diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                            &[&type_name],
                                        );
                                        self.error_at_node(
                                            expr_idx,
                                            &message,
                                            diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                        );
                                    }
                                }
                            }
                        } else if !is_class_declaration {
                            let symbol_type = if is_being_resolved {
                                TypeId::ERROR
                            } else {
                                self.get_type_of_symbol(sym_to_check)
                            };
                            if symbol_type == TypeId::ERROR || symbol_type == TypeId::ANY {
                                continue;
                            }
                            let mut instantiated_type = symbol_type;
                            if let Some(args) = type_args {
                                let mut evaluated_args = Vec::new();
                                for &arg_idx in &args.nodes {
                                    evaluated_args.push(self.get_type_from_type_node(arg_idx));
                                }
                                let base_type_params =
                                    self.get_type_params_for_symbol(sym_to_check);
                                if evaluated_args.len() < base_type_params.len() {
                                    for param in base_type_params.iter().skip(evaluated_args.len())
                                    {
                                        let fallback = param
                                            .default
                                            .or(param.constraint)
                                            .unwrap_or(TypeId::UNKNOWN);
                                        evaluated_args.push(fallback);
                                    }
                                }
                                if evaluated_args.len() > base_type_params.len() {
                                    evaluated_args.truncate(base_type_params.len());
                                }
                                let substitution =
                                    crate::query_boundaries::common::TypeSubstitution::from_args(
                                        self.ctx.types,
                                        &base_type_params,
                                        &evaluated_args,
                                    );
                                instantiated_type =
                                    crate::query_boundaries::common::instantiate_type(
                                        self.ctx.types,
                                        symbol_type,
                                        &substitution,
                                    );
                            }

                            // TS2312: Only reject *generic* mapped types — those whose
                            // key constraint still contains type parameters (e.g.,
                            // `{ [K in keyof T]: ... }` where T is unresolved). Mapped
                            // types with concrete key types (like `Partial<ConcreteType>`)
                            // resolve to object types with statically known members and
                            // are valid base types. This matches tsc's `isValidBaseType`
                            // which checks `isGenericMappedType`.
                            //
                            // Skip evaluate_type: when heritage type args match the
                            // alias's own params (e.g., `interface I<K, V> extends
                            // Alias<K, V>`), the interner deduplicates them to the
                            // same TypeId, making substitution identity. evaluate_type
                            // would then flatten the mapped type into an Object, losing
                            // the structure is_generic_mapped_type needs.
                            if class_query::is_generic_mapped_type(
                                self.ctx.types,
                                instantiated_type,
                            ) {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                self.error_at_node(
                                    expr_idx,
                                    diagnostic_messages::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                    diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                );
                            }
                        }
                    }
                } else {
                    // Heritage expression with explicit type arguments over a call expression
                    // (e.g. `class C extends getBase()<T> {}`) should report TS2315 when
                    // the expression resolves but is not generic.
                    let mut emitted_ts2315 = false;
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                        && let Some(type_args) = expr_type_args.type_arguments.as_ref()
                        && !type_args.nodes.is_empty()
                        && let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                    {
                        let expr_type = self.get_type_of_node(expr_idx);
                        let base_constructor_type =
                            self.base_constructor_type_from_expression(expr_idx, None);
                        let has_generic_construct_sig =
                            base_constructor_type.is_some_and(|ctor_type| {
                                self.has_generic_construct_signatures(ctor_type)
                            });
                        if !class_query::is_generic_type(self.ctx.types, expr_type)
                            && !has_generic_construct_sig
                            && expr_type != TypeId::ERROR
                            && expr_type != TypeId::ANY
                            && !type_args.nodes.is_empty()
                        {
                            // For call expressions (e.g. `getSomething()`), the
                            // expression text can't be used as a type name. Fall
                            // back to the formatted return type (e.g. "D") which
                            // matches tsc's `typeToString(type)` behavior.
                            // Strip `typeof ` prefix since tsc shows the class
                            // name without the constructor qualifier here.
                            let name = self.heritage_name_text(expr_idx).unwrap_or_else(|| {
                                let formatted = self.format_type(expr_type);
                                formatted
                                    .strip_prefix("typeof ")
                                    .map(String::from)
                                    .unwrap_or(formatted)
                            });
                            self.error_at_node_msg(
                                expr_idx,
                                crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                                &[name.as_str()],
                            );
                            emitted_ts2315 = true;
                        }
                    }

                    // Skip TS2508 check when TS2315 was already emitted — the type
                    // is not generic, so constructor arg count is irrelevant.
                    if !emitted_ts2315
                        && is_extends_clause
                        && is_class_declaration
                        && let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && expr_node.kind == syntax_kind_ext::CALL_EXPRESSION
                    {
                        let base_constructor_type =
                            self.base_constructor_type_from_expression(expr_idx, None);
                        let call_type_args = self
                            .ctx
                            .arena
                            .get_expr_type_args(type_node)
                            .and_then(|type_args| type_args.type_arguments.as_ref());
                        if let Some(ctor_type) = base_constructor_type {
                            self.check_heritage_call_expression_constructor_compatibility(
                                expr_idx,
                                ctor_type,
                                call_type_args,
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
                        // If the identifier has no symbol resolution at all, it is truly
                        // unresolved — don't fall through to `is_constructor_type` which
                        // would return true for the `any` fallback type and suppress TS2304.
                        let has_symbol = self.resolve_identifier_symbol(expr_idx).is_some();
                        if !has_symbol {
                            false
                        } else {
                            // Try to get the type of the expression to check if it's a constructor
                            let expr_type = self.get_type_of_node(expr_idx);
                            self.is_constructor_type(expr_type)
                        }
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
                            // so we don't emit TS2507 for null in extends clauses.
                            // For literal values, tsc preserves the literal type (e.g., 42, "hello")
                            // rather than the widened type (number, string).
                            let literal_type_name: Option<String> = match expr_node.kind {
                                k if k == SyntaxKind::NullKeyword as u16 => {
                                    // Don't error on null - TypeScript allows `extends null`
                                    None
                                }
                                k if k == SyntaxKind::UndefinedKeyword as u16 => {
                                    Some("undefined".to_string())
                                }
                                k if k == SyntaxKind::TrueKeyword as u16 => {
                                    Some("true".to_string())
                                }
                                k if k == SyntaxKind::FalseKeyword as u16 => {
                                    Some("false".to_string())
                                }
                                k if k == SyntaxKind::VoidKeyword as u16 => {
                                    Some("void".to_string())
                                }
                                k if k == SyntaxKind::NumericLiteral as u16 => {
                                    // Use the actual literal text (e.g., "42") not "number"
                                    self.ctx
                                        .arena
                                        .get_literal_text(expr_idx)
                                        .map(|t| t.to_string())
                                        .or_else(|| Some("number".to_string()))
                                }
                                k if k == SyntaxKind::StringLiteral as u16 => {
                                    // Use the actual literal text with quotes (e.g., "\"hello\"")
                                    self.ctx
                                        .arena
                                        .get_literal_text(expr_idx)
                                        .map(|t| format!("\"{t}\""))
                                        .or_else(|| Some("string".to_string()))
                                }
                                // Also check for identifiers with reserved names (parsed as identifier)
                                k if k == SyntaxKind::Identifier as u16 => {
                                    if let Some(ident) = self.ctx.arena.get_identifier(expr_node) {
                                        match ident.escaped_text.as_str() {
                                            "undefined" => Some("undefined".to_string()),
                                            "void" => Some("void".to_string()),
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
                                        &[&type_name],
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
                            // TS2422: For implements clauses referencing type parameters
                            if !is_extends_clause
                                && is_class_declaration
                                && class_type_param_names.contains(&name)
                            {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                self.error_at_node(
                                    expr_idx,
                                    diagnostic_messages::A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S,
                                    diagnostic_codes::A_CLASS_CAN_ONLY_IMPLEMENT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH_S,
                                );
                                continue;
                            }

                            // Emit TS2312 for interface extending a type parameter
                            if !is_class_declaration && class_type_param_names.contains(&name) {
                                use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                                self.error_at_node(
                                    expr_idx,
                                    diagnostic_messages::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                    diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                );
                                continue;
                            }
                            // Route through boundary for TS2304/TS2552 with suggestion collection
                            self.report_not_found_at_boundary(
                                &name,
                                expr_idx,
                                crate::query_boundaries::name_resolution::NameLookupKind::Value,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Check heritage clauses for primitive type keywords only (TS2863/TS2864).
    /// This is a lighter-weight check than `check_heritage_clauses_for_unresolved_names` and is
    /// safe to call for class expressions without triggering side effects like constructor
    /// accessibility checking (TS2675) that `get_type_of_node` would cause.
    pub(crate) fn check_heritage_clauses_for_primitive_types(
        &mut self,
        heritage_clauses: &Option<tsz_parser::parser::NodeList>,
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

            let is_extends_clause = heritage.token == SyntaxKind::ExtendsKeyword as u16;

            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                if let Some(expr_node) = self.ctx.arena.get(expr_idx)
                    && expr_node.kind == SyntaxKind::Identifier as u16
                    && let Some(ident) = self.ctx.arena.get_identifier(expr_node)
                {
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
                    }
                }
            }
        }
    }

    /// Check whether `node_idx` is lexically enclosed within a class declaration
    /// whose binder symbol equals `target_class_sym`.
    ///
    /// Walks AST parent pointers from `node_idx` upward. When it encounters a
    /// `CLASS_DECLARATION` or `CLASS_EXPRESSION`, it checks whether that class's
    /// binder symbol matches `target_class_sym`. If so, the node is inside the
    /// target class and has access to its private/protected constructor.
    ///
    /// This is used for TS2675: a class with a private constructor can still be
    /// extended by a class that is defined *within* the declaring class's body
    /// (e.g., inside one of its methods).
    fn is_lexically_inside_class(
        &self,
        node_idx: NodeIndex,
        target_class_sym: tsz_binder::SymbolId,
    ) -> bool {
        let mut current = node_idx;
        loop {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return false;
            };
            if (parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION)
                && self
                    .ctx
                    .binder
                    .get_node_symbol(parent)
                    .is_some_and(|sym| sym == target_class_sym)
            {
                return true;
            }
            current = parent;
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

        // Get the simple name from the symbol, not the full qualified expression text.
        // tsc uses the symbol's simple name (e.g., 'E') not the qualified name ('N.E').
        let name = symbol.escaped_name.clone();

        // For property access expressions like N.E, point the error at the right-hand
        // identifier (E), not the whole expression (N.E). tsc reports the error span
        // on just the class name, not the qualified access path.
        let error_node = if let Some(usage_node_data) = self.ctx.arena.get(usage_idx)
            && usage_node_data.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
        {
            if let Some(access) = self.ctx.arena.get_access_expr(usage_node_data) {
                access.name_or_argument
            } else {
                usage_idx
            }
        } else {
            usage_idx
        };

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
        self.error_at_node(error_node, &message, code);
    }

    /// Check if a type (including intersection members) has generic construct signatures.
    ///
    /// For intersection types like `T & Constructor<MyMixin>`, checks each member
    /// for generic construct signatures. This is needed because `construct_signatures_for_type`
    /// only works on direct `Callable` types, not intersections.
    fn has_generic_construct_signatures(&self, type_id: TypeId) -> bool {
        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id)
            && sigs.iter().any(|sig| !sig.type_params.is_empty())
        {
            return true;
        }
        // For intersection types, check each member
        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            for member in &members {
                if self.has_generic_construct_signatures(*member) {
                    return true;
                }
            }
        }
        false
    }

    fn check_heritage_call_expression_constructor_compatibility(
        &mut self,
        expr_idx: NodeIndex,
        base_constructor_type: TypeId,
        explicit_type_args: Option<&tsz_parser::parser::NodeList>,
    ) {
        let mut signatures = Vec::new();
        self.collect_heritage_call_expression_constructor_signatures(
            base_constructor_type,
            &mut signatures,
            &mut rustc_hash::FxHashSet::default(),
        );
        if signatures.is_empty() {
            return;
        }

        let type_args_nodes: &[NodeIndex] = explicit_type_args
            .map(|args| args.nodes.as_slice())
            .unwrap_or(&[]);
        let provided_count = type_args_nodes.len();

        let provided_types: Vec<TypeId> = type_args_nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();

        let matching: Vec<&tsz_solver::CallSignature> = signatures
            .iter()
            .filter(|sig| {
                let max = sig.type_params.len();
                let min = sig
                    .type_params
                    .iter()
                    .filter(|tp| tp.default.is_none())
                    .count();
                provided_count >= min && provided_count <= max
            })
            .collect();

        if provided_count > 0 && matching.is_empty() {
            let anchor =
                self.find_heritage_call_expression_type_argument_anchor(expr_idx, type_args_nodes);
            let message = crate::diagnostics::format_message(
                crate::diagnostics::diagnostic_messages::NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS,
                &[],
            );
            self.error(
                anchor,
                1,
                message,
                crate::diagnostics::diagnostic_codes::NO_BASE_CONSTRUCTOR_HAS_THE_SPECIFIED_NUMBER_OF_TYPE_ARGUMENTS,
            );
            return;
        }

        if matching.is_empty() {
            return;
        }

        // When the base constructor type is an intersection (e.g., mixin patterns
        // like `T & (new (...args) => Mixin)`), constructor signatures come from
        // different intersection members and naturally have different return types.
        // tsc doesn't compare return types across intersection members — the
        // instance type is the intersection of all individual return types.
        // Only check TS2510 for overloaded signatures on a single type.
        if tsz_solver::is_intersection_type(self.ctx.types, base_constructor_type) {
            return;
        }

        let mut return_types = Vec::with_capacity(matching.len());
        for sig in matching {
            let mut args = provided_types.clone();
            if args.len() < sig.type_params.len() {
                for param in sig.type_params.iter().skip(args.len()) {
                    let fallback = param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN);
                    args.push(fallback);
                }
            }
            if args.len() > sig.type_params.len() {
                args.truncate(sig.type_params.len());
            }
            let instantiated = self.instantiate_signature(sig, &args);
            return_types.push(instantiated.return_type);
        }

        let Some((first_return, rest)) = return_types.split_first() else {
            return;
        };
        for &candidate_return in rest {
            if !self.are_mutually_assignable(*first_return, candidate_return)
                || !self.are_mutually_assignable(candidate_return, *first_return)
            {
                self.error_at_node(
                    expr_idx,
                    crate::diagnostics::diagnostic_messages::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
                    crate::diagnostics::diagnostic_codes::BASE_CONSTRUCTORS_MUST_ALL_HAVE_THE_SAME_RETURN_TYPE,
                );
                return;
            }
        }
    }

    fn collect_heritage_call_expression_constructor_signatures(
        &self,
        type_id: TypeId,
        signatures: &mut Vec<tsz_solver::CallSignature>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        if !visited.insert(type_id) {
            return;
        }

        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id) {
            signatures.extend(sigs);
        }

        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            for member in &members {
                self.collect_heritage_call_expression_constructor_signatures(
                    *member, signatures, visited,
                );
            }
        }
    }

    fn find_heritage_call_expression_type_argument_anchor(
        &self,
        expr_idx: NodeIndex,
        type_arg_nodes: &[NodeIndex],
    ) -> u32 {
        let (call_expr_start, _) = self.get_node_span(expr_idx).unwrap_or((0, 0));
        let explicit_start = type_arg_nodes
            .first()
            .and_then(|&arg| self.get_node_span(arg).map(|(start, _)| start));

        find_heritage_call_expression_type_argument_anchor_impl(
            call_expr_start,
            explicit_start,
            call_expr_start,
        )
    }
}

const fn find_heritage_call_expression_type_argument_anchor_impl(
    call_expr_start: u32,
    explicit_type_arg_start: Option<u32>,
    fallback_start: u32,
) -> u32 {
    if explicit_type_arg_start.is_some() {
        call_expr_start
    } else {
        fallback_start
    }
}

#[cfg(test)]
mod tests {
    use super::find_heritage_call_expression_type_argument_anchor_impl;

    #[test]
    fn test_prefers_explicit_type_argument_node_start() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, Some(23), 5);
        assert_eq!(anchor, 15);
    }

    #[test]
    fn test_falls_back_to_call_start_when_source_text_missing() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(26, Some(2), 5);
        assert_eq!(anchor, 26);
    }

    #[test]
    fn test_falls_back_to_call_start_without_type_arguments() {
        let anchor = find_heritage_call_expression_type_argument_anchor_impl(15, None, 7);
        assert_eq!(anchor, 7);
    }
}
