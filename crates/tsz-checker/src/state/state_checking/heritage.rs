//! Heritage clause (extends/implements) checking for classes and interfaces.

use crate::query_boundaries::class_type as class_query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;
impl<'a> CheckerState<'a> {
    /// Heritage base-expression kinds that already own a dedicated diagnostic
    /// path in `check_heritage_clauses_for_unresolved_names`, and so must not be
    /// re-typed by the generic value-expression constructor check:
    /// - named identifiers and property accesses resolve through the symbol path;
    /// - a call expression keeps its `TS2508`/`TS2315` mixin-aware handling;
    /// - literal keywords (`null`, `undefined`, `true`, `false`, `void`, numeric,
    ///   and string) are reported — or, for `null`, accepted — by the literal
    ///   block below.
    const fn heritage_base_has_dedicated_diagnostic_path(kind: u16) -> bool {
        use tsz_scanner::SyntaxKind;
        kind == SyntaxKind::Identifier as u16
            || kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || kind == syntax_kind_ext::CALL_EXPRESSION
            || kind == SyntaxKind::NullKeyword as u16
            || kind == SyntaxKind::UndefinedKeyword as u16
            || kind == SyntaxKind::TrueKeyword as u16
            || kind == SyntaxKind::FalseKeyword as u16
            || kind == SyntaxKind::VoidKeyword as u16
            || kind == SyntaxKind::NumericLiteral as u16
            || kind == SyntaxKind::StringLiteral as u16
    }

    fn symbol_is_import_equals_alias(&self, symbol: &tsz_binder::Symbol) -> bool {
        symbol.has_any_flags(symbol_flags::ALIAS)
            && symbol.all_declarations().iter().any(|&decl_idx| {
                self.ctx
                    .arena
                    .get(decl_idx)
                    .is_some_and(|node| node.kind == syntax_kind_ext::IMPORT_EQUALS_DECLARATION)
            })
    }

    /// Follow a heritage base symbol through a named import / re-export alias
    /// chain to the underlying type declaration.
    ///
    /// A base referenced through a barrel — `import { I } from './barrel'` where
    /// `./barrel` does `export type { I } from './impl'` — resolves to the
    /// barrel's re-export *alias* symbol, which carries no type parameters of its
    /// own. Computing arity off that alias makes a generic interface look
    /// non-generic and falsely emits TS2315 ("Type 'I' is not generic"). The
    /// general type-reference path already chases this chain via
    /// `reference_import_alias_export_target`; the heritage path did not. Reuse
    /// the same multi-hop follower so `extends I<...>` and `var x: I<...>` agree.
    ///
    /// Returns the original `sym_id` unchanged when it is not an import alias or
    /// when the chain cannot be resolved, so non-aliased and `import =` heritage
    /// bases keep their existing behavior.
    fn heritage_symbol_resolved_through_reexport(
        &self,
        sym_id: SymbolId,
        base_name: &str,
    ) -> SymbolId {
        let Some(symbol) = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))
        else {
            return sym_id;
        };
        if !self.reference_symbol_is_import_alias(symbol) {
            return sym_id;
        }
        self.reference_import_alias_export_target(symbol, base_name)
            .map_or(sym_id, |(target_sym_id, _)| target_sym_id)
    }

    fn import_equals_module_base_without_export_equals(&self, sym_id: SymbolId) -> Option<String> {
        let alias = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .or_else(|| self.get_cross_file_symbol(sym_id))?;
        if !self.symbol_is_import_equals_alias(alias) {
            return None;
        }

        let module_specifier = alias.import_module()?;
        let exports = self.resolve_effective_module_exports_from_file(
            module_specifier,
            Some(self.ctx.current_file_idx),
        )?;
        let has_require_target = exports.has("export=")
            || (exports.has("module.exports")
                && self.current_file_uses_module_exports_require_interop(module_specifier));
        (!has_require_target).then(|| module_specifier.to_string())
    }

    fn report_import_equals_module_base_not_constructor(
        &mut self,
        expr_idx: NodeIndex,
        module_specifier: &str,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};

        let display_module = self.imported_namespace_display_module_name(module_specifier);
        let type_name = format!("typeof import(\"{display_module}\")");
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

    /// Whether the *flow-narrowed* type of a heritage base expression (a value
    /// reference) is a constructor at this location.
    ///
    /// tsc types a class `extends <expr>` base via `checkExpression`, which
    /// applies control-flow narrowing. This mirrors that narrowing for the
    /// constructor-validity check so a binding narrowed from `Ctor | undefined`
    /// to `Ctor` (e.g. inside `klass ? class extends klass {} : null`) is
    /// recognized as a valid base. Used only as a fallback *after* the declared
    /// symbol type fails the constructor check, so it can only accept a base —
    /// never introduce a new `TS2507`.
    fn flow_narrowed_base_is_constructor(&mut self, expr_idx: NodeIndex) -> bool {
        let node_type = self.get_type_of_node(expr_idx);
        if node_type == TypeId::ERROR {
            return false;
        }
        let evaluated = self.evaluate_type_for_assignability(node_type);
        self.is_constructor_type(evaluated)
    }

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

        // Class declarations may only have ONE `extends` clause. Subsequent
        // extends clauses are parser errors (TS1172 'extends' clause already
        // seen). tsc does not resolve type names within the duplicate clause,
        // so we skip them here too — otherwise the names would surface as
        // spurious TS2304 ("Cannot find name") cascades on top of the parser
        // error.
        let mut class_extends_seen = false;

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

            if is_class_declaration && is_extends_clause {
                if class_extends_seen {
                    continue;
                }
                class_extends_seen = true;
            }

            // Check each type in the heritage clause.
            // For class `extends`, only check the first type -- additional types
            // after a comma are parser errors (TS1174) and should not be resolved
            // by the checker (matching tsc behavior which only resolves the first
            // base class expression).
            for (heritage_type_index, &type_idx) in heritage.types.nodes.iter().enumerate() {
                if is_class_declaration && is_extends_clause && heritage_type_index > 0 {
                    continue;
                }
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
                if is_class_declaration {
                    self.check_class_heritage_reserved_leftmost_name(expr_idx);
                    self.check_class_heritage_type_only_namespace_left(expr_idx);
                }

                // Evaluate the heritage expression to trigger control flow analysis (TS2454)
                // and compute the actual type of the expression. We only do this for class
                // `extends` because classes can have expression-based heritage (mixins).
                // For interface `extends`, the expression is always a type reference, not a value.
                if is_extends_clause && is_class_declaration {
                    // A class heritage expression is evaluated in the enclosing
                    // container, not inside the class, so it carries the
                    // `await`-grammar walk (TS1308/TS1375/TS1378). The statement
                    // dispatcher's walk stops at `CLASS_DECLARATION` /
                    // `CLASS_EXPRESSION`, so this is the only root that reaches it.
                    self.check_await_expression(expr_idx);
                    let _ = self.get_type_of_node(expr_idx);
                }

                // TS2499: An interface can only extend an identifier/qualified-name with optional type arguments.
                if !is_class_declaration && is_extends_clause {
                    let mut is_valid = true;

                    let mut current_idx = expr_idx;
                    use tsz_parser::parser::syntax_kind_ext::*;

                    loop {
                        let Some(node) = self.ctx.arena.get(current_idx) else {
                            is_valid = false;
                            break;
                        };

                        if node.is_optional_chain() {
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
                    use tsz_parser::parser::syntax_kind_ext::*;

                    loop {
                        let Some(node) = self.ctx.arena.get(current_idx) else {
                            is_valid = false;
                            break;
                        };

                        if node.is_optional_chain() {
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
                let heritage_sym = self.resolve_heritage_symbol(expr_idx).or_else(|| {
                    if !is_class_declaration
                        && is_extends_clause
                        && let crate::symbol_resolver::TypeSymbolResolution::Type(type_sym) =
                            self.resolve_qualified_symbol_in_type_position(expr_idx)
                    {
                        Some(type_sym)
                    } else {
                        None
                    }
                });
                if let Some(heritage_sym) = heritage_sym {
                    // When the base is named through a chain of named re-exports
                    // (`export type { X } from './x'`), the local heritage symbol
                    // is an import alias whose own declaration carries no type
                    // parameters. Chase the alias chain to the original
                    // declaration so the arity / "is generic" checks below read
                    // the real type-parameter list instead of falsely emitting
                    // TS2315.
                    let heritage_sym = self
                        .resolve_heritage_alias_to_declaration_symbol(heritage_sym, expr_idx)
                        .unwrap_or(heritage_sym);
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

                    if is_extends_clause
                        && is_class_declaration
                        && let Some(module_specifier) =
                            self.import_equals_module_base_without_export_equals(heritage_sym)
                    {
                        self.report_import_equals_module_base_not_constructor(
                            expr_idx,
                            &module_specifier,
                        );
                        if let Some(type_args) = type_args {
                            for &arg_idx in &type_args.nodes {
                                self.get_type_of_node(arg_idx);
                            }
                        }
                        continue;
                    }

                    // For a plain identifier (`extends Base<T>`), read arity
                    // through the same reference-aware path as type references.
                    // The raw `SymbolId` can be an import/re-export alias with
                    // no params, falsely making imported generic interfaces look
                    // non-generic. Qualified names and call expressions keep the
                    // resolved-symbol path because it also handles constructor
                    // signatures.
                    let heritage_ref_name = self
                        .ctx
                        .arena
                        .get(expr_idx)
                        .filter(|node| node.kind == tsz_scanner::SyntaxKind::Identifier as u16)
                        .and_then(|_| self.heritage_name_text(expr_idx));
                    let params_sym = heritage_ref_name.as_deref().map_or_else(
                        || {
                            self.heritage_name_text(expr_idx)
                                .map_or(heritage_sym, |base_name| {
                                    self.heritage_symbol_resolved_through_reexport(
                                        heritage_sym,
                                        &base_name,
                                    )
                                })
                        },
                        |base_name| {
                            self.heritage_symbol_resolved_through_reexport(heritage_sym, base_name)
                        },
                    );
                    let type_params = if let Some(base_name) = heritage_ref_name.as_deref() {
                        self.get_reference_type_params_for_symbol(heritage_sym, base_name)
                    } else {
                        self.get_type_params_for_symbol(params_sym)
                    };
                    let required_count = if let Some(base_name) = heritage_ref_name.as_deref() {
                        self.count_required_reference_type_params(heritage_sym, base_name)
                    } else {
                        type_params
                            .iter()
                            .filter(|param| param.default.is_none())
                            .count()
                    };
                    let total_type_params = type_params.len();
                    let heritage_display_name = self
                        .heritage_name_text(expr_idx)
                        .unwrap_or_else(|| "<expression>".to_string());
                    let resolved_display_name =
                        self.heritage_ts2314_display_name(params_sym, &heritage_display_name);
                    let generic_display_name = Self::format_generic_display_name_with_interner(
                        &resolved_display_name,
                        &type_params,
                        self.ctx.types,
                    );
                    if let Some(type_args) = type_args {
                        if total_type_params == 0 {
                            let symbol_type = self.get_type_of_symbol(params_sym);
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
                                self.symbol_declaration_has_type_parameters(params_sym);

                            if !has_generic_construct_signature
                                && !has_type_params_in_decl
                                && symbol_type != TypeId::ERROR
                                && symbol_type != TypeId::ANY
                                && !type_args.nodes.is_empty()
                            {
                                self.error_at_node_msg(
                                    expr_idx,
                                    crate::diagnostics::diagnostic_codes::TYPE_IS_NOT_GENERIC,
                                    &[heritage_display_name.as_str()],
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
                                && !self.skip_ts2314_for_heritage_symbol(
                                    heritage_sym,
                                    is_class_declaration,
                                    is_extends_clause,
                                )
                                && let Some(_name) = self.heritage_name_text(expr_idx)
                            {
                                self.error_generic_type_requires_type_arguments_at(
                                    &generic_display_name,
                                    required_count,
                                    type_idx,
                                );
                            }

                            self.validate_type_reference_type_arguments_against_params(
                                &type_params,
                                required_count,
                                type_args,
                                type_idx,
                                &generic_display_name,
                            );
                        }
                    } else if required_count > 0
                        && let Some(_name) = self.heritage_name_text(expr_idx)
                    {
                        // tsc skips TS2314 for heritage clauses when:
                        // 1. JS files — type arguments are never required
                        // 2. Extends clauses where the symbol has a variable
                        //    declaration (e.g. `declare var Set: SetConstructor`)
                        //    — the constructor infers type args
                        let skip_ts2314 = self.skip_ts2314_for_heritage_symbol(
                            heritage_sym,
                            is_class_declaration,
                            is_extends_clause,
                        );
                        if !skip_ts2314 {
                            self.error_generic_type_requires_type_arguments_at(
                                &generic_display_name,
                                required_count,
                                type_idx,
                            );
                        }
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
                        let mut visited_aliases = AliasCycleTracker::new();
                        let resolved_sym =
                            self.resolve_alias_symbol(heritage_sym, &mut visited_aliases);
                        let sym_to_check = resolved_sym.unwrap_or(heritage_sym);
                        if let Some(symbol) = self.get_cross_file_symbol(sym_to_check) {
                            let is_namespace = symbol.has_any_flags(symbol_flags::MODULE);
                            let has_non_namespace_value = symbol
                                .has_any_flags(symbol_flags::VALUE & !symbol_flags::VALUE_MODULE);
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
                        let mut visited_aliases = AliasCycleTracker::new();
                        let resolved_sym =
                            self.resolve_alias_symbol(heritage_sym, &mut visited_aliases);
                        let sym_to_check = resolved_sym.unwrap_or(heritage_sym);

                        // Guard against infinite recursion: if this symbol is already being resolved
                        // as a class instance type, skip the type resolution to prevent stack overflow.
                        let is_being_resolved = self
                            .ctx
                            .class_instance_resolution_set
                            .contains(&sym_to_check);

                        if let Some(symbol) = self.get_cross_file_symbol(sym_to_check) {
                            let is_namespace = symbol.has_any_flags(symbol_flags::MODULE);
                            // Merged declarations like `namespace N {}` + `class N {}`
                            // are valid values in `extends`. Only emit TS2708 for
                            // namespace-only symbols.
                            let has_non_namespace_value = symbol
                                .has_any_flags(symbol_flags::VALUE & !symbol_flags::VALUE_MODULE);
                            if is_namespace && !has_non_namespace_value {
                                // SUPPRESSION: For import aliases like `import * as A from "mod"`,
                                // suppress TS2708 when the module resolution has failed (TS2307).
                                // The namespace object from an import is always usable as a value
                                // reference, even if the module has no value exports or failed to resolve.
                                let has_alias = symbol.has_any_flags(symbol_flags::ALIAS);
                                if has_alias && symbol.import_module().is_some() {
                                    // Skip TS2708 for import aliases - this handles cases like
                                    // `import * as A from ""` where the module fails to resolve.
                                    continue;
                                }
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
                                s.has_any_flags(symbol_flags::INTERFACE)
                                    && !s.has_any_flags(symbol_flags::CLASS)
                                    && !s.has_any_flags(symbol_flags::VARIABLE)
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
                            //
                            // `use_flow_narrowed_base` is decided from the same symbol read:
                            // a narrowable value binding (a `var`/`let`/`const`/parameter, not a
                            // class/interface type declaration) is typed via its flow-narrowed
                            // node type below, matching tsc's `checkExpression`.
                            let (skip_constructor_check, use_flow_narrowed_base) = self
                                .get_cross_file_symbol(sym_to_check)
                                .map_or((false, false), |s| {
                                    let skip = s.has_any_flags(symbol_flags::CLASS)
                                        && !s.has_any_flags(symbol_flags::VARIABLE);
                                    let flow_narrowed = s.has_any_flags(symbol_flags::VARIABLE)
                                        && !s.has_any_flags(
                                            symbol_flags::CLASS | symbol_flags::INTERFACE,
                                        );
                                    (skip, flow_narrowed)
                                });

                            // When a user class shadows a lib variable of the same name
                            // (e.g., `class Symbol` shadowing `declare var Symbol: SymbolConstructor`),
                            // the class itself is constructable but tsc uses the lib variable's
                            // annotated type. Check if the shadowed lib type is non-constructable.
                            if skip_constructor_check
                                && let Some((lib_type, lib_type_name)) =
                                    self.shadowed_lib_variable_type(sym_to_check)
                            {
                                // Use strict constructor check matching tsc's
                                // isConstructorType: only construct signatures
                                // count, NOT prototype property presence.
                                // SymbolConstructor has `readonly prototype: Symbol`
                                // but no construct signatures — tsc emits TS2507.
                                let lib_has_construct_sigs =
                                    class_query::construct_signatures_for_type(
                                        self.ctx.types,
                                        lib_type,
                                    )
                                    .is_some_and(|sigs| !sigs.is_empty());
                                if lib_type != TypeId::ERROR && !lib_has_construct_sigs {
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages, format_message,
                                    };
                                    let message = format_message(
                                            diagnostic_messages::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                            &[&lib_type_name],
                                        );
                                    self.error_at_node(
                                        expr_idx,
                                        &message,
                                        diagnostic_codes::TYPE_IS_NOT_A_CONSTRUCTOR_FUNCTION_TYPE,
                                    );
                                }
                            }

                            if !skip_constructor_check {
                                let symbol_type = if is_being_resolved {
                                    TypeId::ERROR
                                } else {
                                    self.get_type_of_symbol(sym_to_check)
                                };

                                // For merged CLASS+VARIABLE symbols, get the lib variable's
                                // annotated type. Used both as a type override (when class
                                // constructor is constructable but lib type isn't) and for
                                // the error message (when symbol_type is UNKNOWN/unhelpful).
                                let lib_var_info = self.shadowed_lib_variable_type(sym_to_check);

                                let lib_var_override = if symbol_type != TypeId::ERROR
                                    && self.is_constructor_type(symbol_type)
                                {
                                    lib_var_info
                                        .as_ref()
                                        .filter(|(t, _)| {
                                            // Use strict constructor check matching tsc's
                                            // isConstructorType: only construct signatures,
                                            // not prototype property presence.
                                            let has_construct_sigs =
                                                class_query::construct_signatures_for_type(
                                                    self.ctx.types,
                                                    *t,
                                                )
                                                .is_some_and(|sigs| !sigs.is_empty());
                                            *t != TypeId::ERROR && !has_construct_sigs
                                        })
                                        .map(|(t, _)| *t)
                                } else {
                                    None
                                };

                                // When lib_var_override is set, the lib variable's type
                                // overrides the user class for extends checking. Since it
                                // is set only when the lib type has NO construct signatures,
                                // emit TS2507 with the lib type name (matching tsc).
                                if lib_var_override.is_some() {
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages, format_message,
                                    };
                                    let type_name = lib_var_info
                                        .as_ref()
                                        .map(|(_, name)| name.as_str())
                                        .unwrap_or("unknown");
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

                                // Route heritage constructor validation through the canonical
                                // relation boundary for unified error code routing.
                                // Instead of directly checking is_constructor_type and emitting TS2507,
                                // we evaluate the type through the boundary first, then check.
                                // This ensures proper type resolution and consistent error handling.
                                let should_check_constructor = lib_var_override.is_none()
                                    && symbol_type != TypeId::ERROR
                                    // TypeScript 7 dropped JS constructor-function inference: a
                                    // plain JS function used as an `extends` base no longer gains
                                    // a synthesized construct signature, so it is not a valid
                                    // constructor function type and must report TS2507 (matching
                                    // the TS7009 classification at `new` sites and the existing
                                    // ESM-import extends behavior). The former
                                    // `!symbol_has_js_constructor_evidence` exemption is gone.
                                    // Skip for symbols with INTERFACE+VARIABLE but NOT CLASS
                                    // (built-in types like Array, Object, Promise) — the variable
                                    // side provides the constructor even though the interface type
                                    // doesn't have construct signatures.
                                    && self
                                        .get_cross_file_symbol(sym_to_check)
                                        .is_none_or(|s| {
                                            !(s.has_any_flags(symbol_flags::INTERFACE)
                                                && s.has_any_flags(symbol_flags::VARIABLE)
                                                && !s.has_any_flags(symbol_flags::CLASS))
                                        });

                                if should_check_constructor {
                                    // Evaluate type through the assignability boundary for proper
                                    // resolution. This routes through the canonical boundary and
                                    // ensures the type is fully resolved before checking.
                                    let evaluated_type =
                                        self.evaluate_type_for_assignability(symbol_type);

                                    // Use the assignability boundary to check if this is a valid
                                    // constructor type by checking through the solver's relation logic.
                                    let is_valid_base = if self.is_constructor_type(evaluated_type)
                                    {
                                        true
                                    } else if use_flow_narrowed_base
                                        && self.flow_narrowed_base_is_constructor(expr_idx)
                                    {
                                        // The declared type of a value reference is not (yet) a
                                        // constructor, but tsc types the heritage base via
                                        // `checkExpression`, applying control-flow narrowing at
                                        // this location. A binding narrowed from `Ctor | undefined`
                                        // to `Ctor` (e.g. `klass ? class extends klass {} : null`)
                                        // is a valid base. This only ever *accepts* a base the
                                        // declared-type check rejected — it never introduces a new
                                        // TS2507 — so an `any`/already-constructor declared type is
                                        // unaffected.
                                        true
                                    } else {
                                        // For types that don't directly report as constructors,
                                        // let the general type system handle validation rather
                                        // than emitting TS2507 directly. The heritage type
                                        // relationship will be validated through standard paths.
                                        false
                                    };

                                    if !is_valid_base {
                                        // Emit TS2507 through the standard error boundary
                                        use crate::diagnostics::{
                                            diagnostic_codes, diagnostic_messages, format_message,
                                        };
                                        let type_name = lib_var_info
                                            .map(|(_, name)| name)
                                            .unwrap_or_else(|| self.format_type(evaluated_type));
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
                            let instantiated_type = if is_being_resolved {
                                TypeId::ERROR
                            } else {
                                self.get_type_from_type_node(type_idx)
                            };
                            if instantiated_type == TypeId::ERROR
                                || instantiated_type == TypeId::ANY
                            {
                                if let crate::symbol_resolver::TypeSymbolResolution::Type(
                                    type_sym,
                                ) = self.resolve_qualified_symbol_in_type_position(expr_idx)
                                {
                                    let mut visited_aliases = AliasCycleTracker::new();
                                    let type_sym = self
                                        .resolve_alias_symbol(type_sym, &mut visited_aliases)
                                        .unwrap_or(type_sym);
                                    let is_type_alias = self
                                        .get_cross_file_symbol(type_sym)
                                        .is_some_and(|symbol| {
                                            symbol
                                                .has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
                                        });
                                    if is_type_alias {
                                        let symbol_type = self.get_type_of_symbol(type_sym);
                                        let enclosing_type_params =
                                            self.enclosing_interface_type_param_names(expr_idx);
                                        let args_reference_enclosing_type_param = type_args
                                            .is_some_and(|args| {
                                                self.type_args_reference_type_params(
                                                    args,
                                                    &enclosing_type_params,
                                                )
                                            });
                                        // tsc's `isValidBaseType` rejects any generic
                                        // deferred alias body that lacks the `Object`
                                        // flag — a generic mapped type, a conditional
                                        // (`T extends X ? A : B`), an indexed access
                                        // (`T[keyof T]`), `keyof`, a union, etc. When
                                        // such an alias is applied to the enclosing
                                        // interface's own type parameters the base stays
                                        // deferred and `instantiated_type` erases to
                                        // `Error`/`Any`, so classify it from the alias
                                        // body — but from the body with the alias's own
                                        // type arguments SUBSTITUTED, not the raw body.
                                        // A passthrough/identity alias (`type Id<T> = T`,
                                        // or `T & { ... }`) has a bare type-parameter body
                                        // that is not itself a valid base, yet substituting
                                        // the argument (a valid object base) makes it one —
                                        // `interface I<V> extends Id<Base<V>>` is legal in
                                        // tsc. A genuinely generic mapped/conditional body
                                        // stays generic after substitution (its constraint
                                        // still references a type parameter) and remains an
                                        // invalid base, so this keeps the real TS2312.
                                        let substituted_body = self
                                            .substitute_alias_body_with_heritage_args(
                                                type_sym,
                                                symbol_type,
                                                type_args,
                                            );
                                        let body_is_invalid_base =
                                            !crate::query_boundaries::class::is_valid_interface_base_type(
                                                self.ctx.types,
                                                substituted_body,
                                            );
                                        if body_is_invalid_base
                                            && args_reference_enclosing_type_param
                                        {
                                            use crate::diagnostics::{
                                                diagnostic_codes, diagnostic_messages,
                                            };
                                            self.error_at_node(
                                                expr_idx,
                                                diagnostic_messages::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                                diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                            );
                                        }
                                    }
                                }
                                continue;
                            }
                            let mut generic_mapped_check_type = instantiated_type;
                            let validation_source_type =
                                if let crate::symbol_resolver::TypeSymbolResolution::Type(
                                    type_sym,
                                ) = self.resolve_qualified_symbol_in_type_position(expr_idx)
                                {
                                    let mut visited_aliases = AliasCycleTracker::new();
                                    let type_sym = self
                                        .resolve_alias_symbol(type_sym, &mut visited_aliases)
                                        .unwrap_or(type_sym);
                                    let has_type_params =
                                        !self.get_type_params_for_symbol(type_sym).is_empty();
                                    let is_type_alias = self
                                        .get_cross_file_symbol(type_sym)
                                        .is_some_and(|symbol| {
                                            symbol
                                                .has_any_flags(tsz_binder::symbol_flags::TYPE_ALIAS)
                                        });
                                    let is_non_generic_alias = is_type_alias && !has_type_params;
                                    let symbol_type = if is_type_alias {
                                        Some(self.get_type_of_symbol(type_sym))
                                    } else {
                                        None
                                    };
                                    if let Some(symbol_type) = symbol_type {
                                        generic_mapped_check_type = symbol_type;
                                        if let Some(args) = type_args {
                                            let mut evaluated_args = Vec::new();
                                            for &arg_idx in &args.nodes {
                                                evaluated_args
                                                    .push(self.get_type_from_type_node(arg_idx));
                                            }
                                            let base_type_params =
                                                self.get_type_params_for_symbol(type_sym);
                                            if evaluated_args.len() < base_type_params.len() {
                                                for param in base_type_params
                                                    .iter()
                                                    .skip(evaluated_args.len())
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
                                            let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
                                                self.ctx.types,
                                                &base_type_params,
                                                &evaluated_args,
                                            );
                                            generic_mapped_check_type =
                                                crate::query_boundaries::common::instantiate_type(
                                                    self.ctx.types,
                                                    symbol_type,
                                                    &substitution,
                                                );
                                        }
                                    }
                                    if is_non_generic_alias {
                                        symbol_type.unwrap_or(instantiated_type)
                                    } else {
                                        instantiated_type
                                    }
                                } else {
                                    instantiated_type
                                };
                            let validation_type =
                                self.evaluate_type_for_assignability(validation_source_type);
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
                            if !crate::query_boundaries::class::is_valid_interface_base_type(
                                self.ctx.types,
                                validation_type,
                            ) || class_query::is_generic_mapped_type(
                                self.ctx.types,
                                generic_mapped_check_type,
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
                    // Even when the heritage base name fails to resolve, tsc still
                    // visits the type arguments so identifiers inside them surface
                    // diagnostics (e.g., TS2304 for `T` in `extends A<T>`). Walk
                    // them eagerly — this is a no-op for resolvable args and emits
                    // the expected unresolved-name errors otherwise.
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                        && let Some(type_args) = expr_type_args.type_arguments.as_ref()
                    {
                        for &arg_idx in &type_args.nodes {
                            let _ = self.get_type_from_type_node(arg_idx);
                        }
                    }

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

                    // Qualified/member heritage references need namespace-aware diagnostics
                    // before falling back to generic unresolved-name handling.
                    if self.report_unresolved_qualified_heritage_member(
                        expr_idx,
                        is_class_declaration,
                        is_extends_clause,
                    ) {
                        continue;
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

                    // A class `extends <expr>` base whose expression is a value
                    // expression other than a named identifier, property access, or
                    // literal keyword (`this`, `new X()`, `(expr)`, an array/object
                    // literal, a class/function expression, …) is typed by tsc via
                    // `checkExpression` and reported TS2507 when the resulting type is
                    // concrete (non-`any`, non-`error`) but not a constructor function
                    // type — regardless of the expression's syntactic shape. tsz's
                    // symbol/identifier/literal paths only cover named and bare-keyword
                    // bases; every other value-expression base is handled here. A
                    // non-constructor *call* base (`extends f()`) keeps its dedicated
                    // TS2508/TS2315 handling above to avoid mixin-return false
                    // positives, so calls are excluded.
                    if is_extends_clause
                        && is_class_declaration
                        && let Some(expr_node) = self.ctx.arena.get(expr_idx)
                        && !Self::heritage_base_has_dedicated_diagnostic_path(expr_node.kind)
                    {
                        let base_type = self.get_type_of_node(expr_idx);
                        let evaluated = self.evaluate_type_for_assignability(base_type);
                        // `extends null` is valid (it builds a class with a null
                        // prototype) and the null-ness is a property of the base
                        // *type*, not the `null` keyword: `extends (null)` is
                        // equally accepted by tsc (`baseConstructorType !==
                        // nullWideningType`). Exclude the null type here so a
                        // parenthesized/aliased null base is not flagged.
                        if evaluated != TypeId::ERROR
                            && evaluated != TypeId::ANY
                            && evaluated != TypeId::NULL
                            && !self.is_constructor_type(evaluated)
                        {
                            use crate::diagnostics::{
                                diagnostic_codes, diagnostic_messages, format_message,
                            };
                            let type_name = self.format_type(evaluated);
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
                            let evaluated_type = self.evaluate_type_for_assignability(expr_type);
                            if self.is_constructor_type(evaluated_type) {
                                true
                            } else {
                                if is_extends_clause
                                    && is_class_declaration
                                    && evaluated_type != TypeId::ERROR
                                    && evaluated_type != TypeId::ANY
                                {
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages, format_message,
                                    };
                                    let type_name = self.format_type(evaluated_type);
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
                                false
                            }
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
                            if self.has_special_missing_lib_type_diagnostic(&name) {
                                // Check if the global type is actually available in lib contexts
                                if !self.ctx.has_name_in_lib(&name) {
                                    // TS2318/TS2583: Emit error for missing global type
                                    self.report_missing_lib_type_name(&name, expr_idx);
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

                            // Interface extending one of its own type parameters.
                            // tsc's `isValidBaseType` accepts a type parameter whose
                            // base constraint is a valid object base — members are
                            // inherited from the constraint's statically-known shape
                            // (`interface I<T extends { k: string }> extends T {}`).
                            // Emit TS2312 only when the constraint is not such a base
                            // (unconstrained, or constrained to a non-object). The
                            // type-parameter scope is active here (heritage is checked
                            // after the params are pushed), so the node resolves to the
                            // constrained `TypeParameter` type.
                            if !is_class_declaration && class_type_param_names.contains(&name) {
                                let base_type = self.get_type_from_type_node(type_idx);
                                if !crate::query_boundaries::class::is_valid_interface_base_type(
                                    self.ctx.types,
                                    base_type,
                                ) {
                                    use crate::diagnostics::{
                                        diagnostic_codes, diagnostic_messages,
                                    };
                                    self.error_at_node(
                                        expr_idx,
                                        diagnostic_messages::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                        diagnostic_codes::AN_INTERFACE_CAN_ONLY_EXTEND_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYPES_WITH,
                                    );
                                }
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

    /// Substitute a generic type alias's own heritage type arguments into its
    /// body, for the erased-`instantiated_type` interface-base validity check.
    ///
    /// When `interface I<V> extends Alias<Base<V>>` is checked, `Alias`'s
    /// argument references `I`'s own (unbound) parameter `V`, so the full
    /// instantiation erases to `Error`/`Any` and the base must be classified from
    /// the alias body instead. Classifying the RAW body wrongly rejects a
    /// passthrough alias (`type Alias<T> = T` / `T & { … }`) whose bare
    /// type-parameter body is not itself a valid base though the substituted body
    /// (`Base<V>`, a real object base) is. Returns the body with the alias's
    /// declared parameters replaced by its heritage arguments (missing trailing
    /// args filled from defaults/constraints); returns the body unchanged when
    /// the alias has no parameters or no arguments were supplied.
    fn substitute_alias_body_with_heritage_args(
        &mut self,
        type_sym: SymbolId,
        symbol_type: TypeId,
        type_args: Option<&tsz_parser::parser::base::NodeList>,
    ) -> TypeId {
        let Some(args) = type_args else {
            return symbol_type;
        };
        let base_type_params = self.get_type_params_for_symbol(type_sym);
        if base_type_params.is_empty() {
            return symbol_type;
        }
        let mut evaluated_args: Vec<TypeId> = args
            .nodes
            .iter()
            .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
            .collect();
        if evaluated_args.len() < base_type_params.len() {
            for param in base_type_params.iter().skip(evaluated_args.len()) {
                let fallback = param
                    .default
                    .or(param.constraint)
                    .unwrap_or(TypeId::UNKNOWN);
                evaluated_args.push(fallback);
            }
        }
        evaluated_args.truncate(base_type_params.len());
        let substitution = crate::query_boundaries::common::TypeSubstitution::from_args(
            self.ctx.types,
            &base_type_params,
            &evaluated_args,
        );
        crate::query_boundaries::common::instantiate_type(
            self.ctx.types,
            symbol_type,
            &substitution,
        )
    }

    /// When a user class merges with or shadows a lib variable of the same name
    /// (e.g., user `class Symbol` + lib `declare var Symbol: SymbolConstructor`),
    /// resolve the lib variable's annotated type and its name.
    ///
    /// Returns `Some((type_id, type_name))` if found, `None` otherwise.
    fn shadowed_lib_variable_type(
        &mut self,
        heritage_sym: tsz_binder::SymbolId,
    ) -> Option<(TypeId, String)> {
        use tsz_binder::symbol_flags;

        // Get the heritage symbol's name
        let symbol = self.get_symbol_globally(heritage_sym)?;
        let name = symbol.escaped_name.clone();

        // The heritage symbol must have a CLASS flag (from user code)
        if !symbol.has_any_flags(symbol_flags::CLASS) {
            return None;
        }

        // Only classes at global/module scope can shadow lib variables.
        // A class inside a namespace (e.g., `namespace ts { class Symbol {} }`)
        // does NOT shadow the global `declare var Symbol: SymbolConstructor`.
        let is_at_global_scope = symbol.declarations.iter().any(|&decl_idx| {
            if let Some(ext) = self.ctx.arena.get_extended(decl_idx) {
                let parent = ext.parent;
                if let Some(parent_node) = self.ctx.arena.get(parent) {
                    // Parent is SOURCE_FILE → global scope
                    parent_node.kind == syntax_kind_ext::SOURCE_FILE
                } else {
                    false
                }
            } else {
                false
            }
        });
        if !is_at_global_scope {
            return None;
        }

        // Case A: Heritage symbol itself has VARIABLE (all merged into one symbol:
        // CLASS|INTERFACE|VARIABLE). Use itself as the lib variable symbol.
        // Case B: Heritage symbol lacks VARIABLE. Search lib_symbol_ids for a
        // DIFFERENT lib symbol with the same name and VARIABLE flag.
        let shadowed_lib_id = if symbol.has_any_flags(symbol_flags::VARIABLE)
            && self.ctx.binder.lib_symbol_ids.contains(&heritage_sym)
        {
            Some(heritage_sym)
        } else {
            self.ctx.binder.lib_symbol_ids.iter().find_map(|&lib_id| {
                if lib_id == heritage_sym {
                    return None;
                }
                self.ctx.binder.get_symbol(lib_id).and_then(|s| {
                    if s.escaped_name == name && s.has_any_flags(symbol_flags::VARIABLE) {
                        Some(lib_id)
                    } else {
                        None
                    }
                })
            })
        };

        let shadowed_lib_id = shadowed_lib_id?;
        let lib_sym = self.ctx.binder.get_symbol(shadowed_lib_id)?;
        let declarations = lib_sym.declarations.clone();

        // Iterate ALL declarations to find a variable declaration in a lib arena.
        for decl_idx in declarations {
            let lib_arena = self
                .ctx
                .binder
                .declaration_arenas
                .get(&(shadowed_lib_id, decl_idx))
                .and_then(|v| v.first())
                .filter(|da| !std::ptr::eq(da.as_ref(), self.ctx.arena))
                .map(std::sync::Arc::clone)
                .or_else(|| {
                    // Fallback: check symbol_arenas for any lib arena
                    if self.ctx.arena.get(decl_idx).is_none() {
                        self.ctx.binder.symbol_arenas.get(&shadowed_lib_id).cloned()
                    } else {
                        None
                    }
                });
            let Some(lib_arena) = lib_arena else {
                continue;
            };

            let Some(node) = lib_arena.get(decl_idx) else {
                continue;
            };
            let Some(var_decl) = lib_arena.get_variable_declaration(node) else {
                continue;
            };
            if var_decl.type_annotation.is_none() {
                continue;
            }

            // Resolve the type annotation name from a simple type reference
            let Some(type_annotation_node) = lib_arena.get(var_decl.type_annotation) else {
                continue;
            };
            let Some(type_ref) = lib_arena.get_type_ref(type_annotation_node) else {
                continue;
            };
            let Some(type_name_node) = lib_arena.get(type_ref.type_name) else {
                continue;
            };
            let Some(ident) = lib_arena.get_identifier(type_name_node) else {
                continue;
            };
            let type_name = ident.escaped_text.as_str().to_string();

            if let Some(lib_type) = self.resolve_lib_type_by_name(&type_name)
                && lib_type != TypeId::UNKNOWN
                && lib_type != TypeId::ERROR
            {
                return Some((self.resolve_ref_type(lib_type), type_name));
            }
        }

        None
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
}
