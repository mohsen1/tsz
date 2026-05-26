//! Heritage-related diagnostics for class checking.

use crate::query_boundaries::class_type as class_query;
use crate::state::CheckerState;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// TS2509: Check that the base constructor return type is an object type or
    /// intersection of object types with statically known members.
    ///
    /// When a class extends another via a heritage clause, the return type of
    /// the base constructor must be valid. For example, if `Mix(Private, Private2)`
    /// returns an intersection that reduces to `never`, this is not a valid base type.
    pub(crate) fn check_base_constructor_return_type(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages, format_message};
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        // A class declaration may only have ONE `extends` clause; subsequent
        // ones are TS1172 parser errors. Skip the duplicates so we don't try
        // to resolve their types and emit cascading TS2304 / TS2509.
        let mut extends_seen = false;

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
            if extends_seen {
                continue;
            }
            extends_seen = true;

            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };

            let (expr_idx, type_arguments) = if let Some(type_node) = self.ctx.arena.get(type_idx)
                && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
            {
                (
                    expr_type_args.expression,
                    expr_type_args.type_arguments.as_ref(),
                )
            } else {
                (type_idx, None)
            };

            // Get the base instance type (constructor return type)
            let Some(base_type) = self.base_instance_type_from_expression(expr_idx, type_arguments)
            else {
                continue;
            };

            // Skip for any/error/null — these are permissive.
            // `class extends null` is valid TS (produces a class with no prototype).
            if base_type == TypeId::ANY || base_type == TypeId::ERROR || base_type == TypeId::NULL {
                continue;
            }

            // Skip for null — `class C extends null` is valid in TypeScript.
            // tsc does not emit TS2509 for null base types; instead, it only
            // checks for TS17005 (super call in null-extending class).
            if base_type == TypeId::NULL {
                continue;
            }

            // Skip union base types. When a constructor has multiple construct
            // signatures (e.g., `Array` with `new(): any[]` and `new<T>(): T[]`),
            // the resolved base instance type can be a union like `any[] | T[]`.
            // tsc resolves these to the correct specific return type; our resolution
            // currently produces the union. Since all constituent types in such
            // unions are valid object types, suppress TS2509 for unions.
            if crate::query_boundaries::common::is_union_type(self.ctx.types, base_type) {
                continue;
            }

            // Check if the base type is a valid base type. Mixin intersections
            // with incompatible private property origins are invalid bases even
            // when the structural intersection has not been fully reduced to
            // `never` yet.
            let private_property_conflict =
                self.intersection_has_private_property_conflict(base_type);
            if !crate::query_boundaries::class::is_valid_base_type(self.ctx.types, base_type)
                || private_property_conflict
            {
                let type_name = if private_property_conflict {
                    self.format_type(TypeId::NEVER)
                } else {
                    self.format_type(base_type)
                };
                let message = format_message(
                    diagnostic_messages::BASE_CONSTRUCTOR_RETURN_TYPE_IS_NOT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYP,
                    &[&type_name],
                );
                self.error_at_node(
                    expr_idx,
                    &message,
                    diagnostic_codes::BASE_CONSTRUCTOR_RETURN_TYPE_IS_NOT_AN_OBJECT_TYPE_OR_INTERSECTION_OF_OBJECT_TYP,
                );
            }

            break; // Only check the first extends clause
        }
    }

    /// TS2797: A mixin class that extends from a type variable containing an
    /// abstract construct signature must also be declared 'abstract'.
    ///
    /// When a non-abstract class extends from a type variable (type parameter)
    /// whose constraint includes `abstract new (...)`, the class must be abstract.
    /// This is the mixin pattern: `class C extends baseClass` where `baseClass: T`
    /// and `T extends abstract new (...args: any) => any`.
    pub(crate) fn check_mixin_abstract_construct_constraint(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        // Skip duplicate `extends` clauses on a class (TS1172 parser error).
        let mut extends_seen = false;

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
            if extends_seen {
                continue;
            }
            extends_seen = true;

            // Get the extends expression
            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(type_idx) else {
                continue;
            };

            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };

            // Try get_type_of_node first; if it returns a usable type, use it.
            // Otherwise, fall back to resolving the parameter's declared type
            // annotation directly (workaround for name-merging issues where
            // get_type_of_node returns ANY).
            let base_type = self.resolve_heritage_expr_declared_type(expr_idx);
            if base_type == TypeId::ERROR {
                return;
            }

            // Check if the base type is a type parameter with a constraint
            let Some(constraint_type) =
                class_query::type_parameter_constraint(self.ctx.types, base_type)
            else {
                return;
            };

            // Check if the constraint has abstract construct signatures
            if self.constraint_has_abstract_construct(constraint_type) {
                let error_node = if class_data.name.is_some() {
                    class_data.name
                } else {
                    class_idx
                };
                self.error_at_node(
                    error_node,
                    diagnostic_messages::A_MIXIN_CLASS_THAT_EXTENDS_FROM_A_TYPE_VARIABLE_CONTAINING_AN_ABSTRACT_CONSTRUCT,
                    diagnostic_codes::A_MIXIN_CLASS_THAT_EXTENDS_FROM_A_TYPE_VARIABLE_CONTAINING_AN_ABSTRACT_CONSTRUCT,
                );
            }

            return;
        }
    }

    /// TS2545: A mixin class must have a constructor with a single rest parameter
    /// of type 'any[]'.
    ///
    /// When a class extends a type variable (type parameter), the construct
    /// signatures of the constraint must each have a single rest parameter whose
    /// type is `any[]` or `readonly any[]`.  If not, emit TS2545.
    pub(crate) fn check_mixin_constructor_rest_parameter(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_scanner::SyntaxKind;

        let Some(ref heritage_clauses) = class_data.heritage_clauses else {
            return;
        };

        // Skip duplicate `extends` clauses on a class (TS1172 parser error).
        let mut extends_seen = false;

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
            if extends_seen {
                continue;
            }
            extends_seen = true;

            let Some(&type_idx) = heritage.types.nodes.first() else {
                continue;
            };
            let Some(type_node) = self.ctx.arena.get(type_idx) else {
                continue;
            };

            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };

            let base_type = self.resolve_heritage_expr_declared_type(expr_idx);
            if base_type == TypeId::ERROR {
                return;
            }

            // Only applies when the base type is a type parameter (mixin pattern)
            let Some(constraint_type) =
                class_query::type_parameter_constraint(self.ctx.types, base_type)
            else {
                return;
            };

            // Match tsc's gate: TS2545 is emitted from inside an
            // `if (baseTypes.length) { addLazyDiagnostic(...) }` block in
            // `checkClassLikeDeclaration`. When the base constructor's return
            // type is not an object-like type (e.g. `new () => never`,
            // `new () => void`), `getBaseTypes` produces an empty list and the
            // whole block — including the mixin-shape check — is skipped, with
            // TS2509 carrying the diagnostic instead. Mirror that here so the
            // two diagnostics don't both fire on the same heritage clause.
            if let Some(base_instance) = self.base_instance_type_from_expression(expr_idx, None)
                && !crate::query_boundaries::class::is_valid_base_type(
                    self.ctx.types,
                    base_instance,
                )
            {
                return;
            }

            // Evaluate the constraint type (may be a Lazy type alias or Application)
            let evaluated = self.evaluate_type_for_assignability(constraint_type);

            // Get construct signatures from the evaluated constraint
            let construct_sigs = self.collect_construct_signatures_from_evaluated(evaluated);
            if construct_sigs.is_empty() {
                return;
            }

            // Surgical fix for #9729: a plain non-generic single signature
            // with zero params fails tsc's mixin constructor contract. Other
            // shapes stay on the pre-existing per-sig path; conditional mixin
            // helper output can currently evaluate to a zero-param signature
            // even though tsc accepts it.
            let single_sig_zero_param = construct_sigs.len() == 1
                && construct_sigs[0].type_params.is_empty()
                && construct_sigs[0].params.is_empty()
                && !class_query::contains_conditional_type(self.ctx.types, constraint_type);
            let has_invalid_sig = single_sig_zero_param
                || construct_sigs.iter().any(|sig| {
                    !sig.params.is_empty() && !self.is_valid_mixin_construct_signature(sig)
                });
            if has_invalid_sig {
                self.error_at_node(
                    class_idx,
                    diagnostic_messages::A_MIXIN_CLASS_MUST_HAVE_A_CONSTRUCTOR_WITH_A_SINGLE_REST_PARAMETER_OF_TYPE_ANY,
                    diagnostic_codes::A_MIXIN_CLASS_MUST_HAVE_A_CONSTRUCTOR_WITH_A_SINGLE_REST_PARAMETER_OF_TYPE_ANY,
                );
            }

            return;
        }
    }

    /// Collect construct signatures from an already-evaluated type.
    fn collect_construct_signatures_from_evaluated(
        &self,
        type_id: TypeId,
    ) -> Vec<tsz_solver::CallSignature> {
        if let Some(sigs) = class_query::construct_signatures_for_type(self.ctx.types, type_id)
            && !sigs.is_empty()
        {
            return sigs;
        }

        // Intersection: collect from all members
        if let Some(members) = class_query::intersection_members(self.ctx.types, type_id) {
            let mut all_sigs = Vec::new();
            for &member in members.iter() {
                if let Some(sigs) =
                    class_query::construct_signatures_for_type(self.ctx.types, member)
                {
                    all_sigs.extend(sigs);
                }
            }
            return all_sigs;
        }

        Vec::new()
    }

    /// Whether a construct signature is shape-compatible with the mixin-base
    /// contract: exactly one rest parameter (not optional) whose type is `any`,
    /// `any[]`, or `readonly any[]`.
    fn is_valid_mixin_construct_signature(&self, sig: &tsz_solver::CallSignature) -> bool {
        sig.params.len() == 1
            && sig.params[0].rest
            && !sig.params[0].optional
            && self.is_valid_mixin_rest_param_type(sig.params[0].type_id)
    }

    /// Check if a rest parameter type is valid for a mixin constructor.
    /// Accepts `any`, `any[]`, or `readonly any[]`.
    fn is_valid_mixin_rest_param_type(&self, type_id: TypeId) -> bool {
        // `any` is valid for mixin rest parameters (e.g., `...args: any`)
        if type_id == TypeId::ANY {
            return true;
        }
        // `any[]` or `readonly any[]`
        matches!(
            class_query::array_element_type(self.ctx.types, type_id),
            Some(elem) if elem == TypeId::ANY
        )
    }

    /// Resolve the declared type for a heritage expression identifier.
    /// First tries `get_type_of_node`. If that returns ANY (which can happen
    /// due to symbol name merging), falls back to resolving the identifier's
    /// symbol, finding its parameter declaration, and evaluating the type
    /// annotation directly.
    fn resolve_heritage_expr_declared_type(&mut self, expr_idx: NodeIndex) -> TypeId {
        let base_type = self.get_type_of_node(expr_idx);
        if base_type != TypeId::ANY {
            return base_type;
        }

        // Fallback: resolve via parameter declaration's type annotation
        let Some(sym_id) = self.resolve_identifier_symbol(expr_idx) else {
            return TypeId::ANY;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return TypeId::ANY;
        };
        let Some(&decl_idx) = symbol.declarations.first() else {
            return TypeId::ANY;
        };
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return TypeId::ANY;
        };
        let Some(param_data) = self.ctx.arena.get_parameter(decl_node) else {
            return TypeId::ANY;
        };
        let type_ann = param_data.type_annotation;
        if type_ann == NodeIndex::NONE {
            return TypeId::ANY;
        }
        self.get_type_from_type_node(type_ann)
    }

    /// Check if a constraint type (or any member of an intersection constraint)
    /// contains abstract construct signatures.
    fn constraint_has_abstract_construct(&self, constraint_type: TypeId) -> bool {
        // Direct callable check
        if let Some(callable) =
            class_query::callable_shape_for_type(self.ctx.types, constraint_type)
            && callable.is_abstract
            && !callable.construct_signatures.is_empty()
        {
            return true;
        }

        // Intersection: check each member
        if let Some(members) = class_query::intersection_members(self.ctx.types, constraint_type) {
            for &member in members.iter() {
                if let Some(callable) = class_query::callable_shape_for_type(self.ctx.types, member)
                    && callable.is_abstract
                    && !callable.construct_signatures.is_empty()
                {
                    return true;
                }
            }
        }

        false
    }

    /// Check if an anonymous class is exported (via export default modifier or parent export node).
    fn is_class_exported_default(
        &self,
        class_idx: NodeIndex,
        modifiers: &Option<NodeList>,
    ) -> bool {
        use tsz_scanner::SyntaxKind;
        // Check for export + default modifiers on the class itself
        let has_export = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword);
        let has_default = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::DefaultKeyword);
        if has_export && has_default {
            return true;
        }
        // Check if parent is an export default (ExportDeclaration with is_default_export)
        if let Some(ext) = self.ctx.arena.get_extended(class_idx)
            && let Some(parent) = self.ctx.arena.get(ext.parent)
        {
            if parent.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_data) = self.ctx.arena.get_export_decl(parent)
                && export_data.is_default_export
            {
                return true;
            }
            // Also check for ExportAssignment (export = class { ... })
            if parent.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                return true;
            }
        }
        false
    }

    /// Return the node index to anchor TS4094 at for an exported anonymous class.
    ///
    /// tsc reports TS4094 at the `export` keyword (col 1), not the `class` keyword.
    /// When the class is an expression inside an export statement, the parent node
    /// starts at `export`. When it's a `ClassDeclaration` with own `export default`
    /// modifiers, the first modifier starts before the class keyword.
    pub(crate) fn get_anonymous_class_export_anchor(
        &self,
        class_idx: NodeIndex,
        modifiers: &Option<NodeList>,
    ) -> Option<NodeIndex> {
        use tsz_scanner::SyntaxKind;
        let has_export = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::ExportKeyword);
        let has_default = self
            .ctx
            .arena
            .has_modifier(modifiers, SyntaxKind::DefaultKeyword);
        if has_export && has_default {
            // ClassDeclaration with `export default` modifiers. Use the first modifier
            // node as the anchor so we report at `export` (col 1), not `class`.
            if let Some(mods) = modifiers
                && let Some(&first_mod_idx) = mods.nodes.first()
            {
                return Some(first_mod_idx);
            }
            return Some(class_idx);
        }
        // ClassExpression in `export default class` or `export = class`.
        // The parent export-statement node starts at the `export` keyword.
        if let Some(ext) = self.ctx.arena.get_extended(class_idx)
            && let Some(parent) = self.ctx.arena.get(ext.parent)
        {
            if parent.kind == syntax_kind_ext::EXPORT_DECLARATION
                && let Some(export_data) = self.ctx.arena.get_export_decl(parent)
                && export_data.is_default_export
            {
                return Some(ext.parent);
            }
            if parent.kind == syntax_kind_ext::EXPORT_ASSIGNMENT {
                return Some(ext.parent);
            }
        }
        None
    }

    /// TS4094: Named exported class whose `extends` heritage resolves to an anonymous
    /// class type.  The anonymous base's private/protected members appear in the .d.ts
    /// type literal and must be reported.  Errors are anchored at the named class's
    /// name identifier (matching tsc's anchor position).
    pub(crate) fn check_ts4094_named_class_anonymous_heritage(
        &mut self,
        _class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) {
        let Some(ref heritage_list) = class.heritage_clauses else {
            return;
        };
        for &clause_idx in &heritage_list.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };
            // Only `extends` clauses carry the anonymous constructor type.
            if heritage.token != tsz_scanner::SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_ref_idx in &heritage.types.nodes {
                let Some(type_ref_node) = self.ctx.arena.get(type_ref_idx) else {
                    continue;
                };
                // Mirror the existing heritage-resolution pattern: if the node is an
                // ExpressionWithTypeArguments, unpack it; otherwise treat the node itself
                // as the expression (handles bare identifier heritage like `extends Foo`).
                let (expr_idx, type_args) =
                    if let Some(eta) = self.ctx.arena.get_expr_type_args(type_ref_node) {
                        (eta.expression, eta.type_arguments.as_ref())
                    } else {
                        (type_ref_idx, None)
                    };
                let Some(base_instance_type) =
                    self.base_instance_type_from_expression(expr_idx, type_args)
                else {
                    continue;
                };
                if self.instance_type_is_from_anonymous_class(base_instance_type) {
                    self.report_instance_type_private_members_as_ts4094(
                        class.name,
                        base_instance_type,
                    );
                }
            }
        }
    }
}
