//! Property access type resolution helpers: CommonJS detection, JSDoc annotation,
//! finalization, interface recovery, and enum/namespace utilities.

use crate::context::TypingRequest;
use crate::query_boundaries::common::PropertyAccessResult;
use crate::query_boundaries::property_access as access_query;
use crate::state::{CheckerState, MAX_INSTANTIATION_DEPTH};
use tsz_binder::symbol_flags;
use tsz_common::common::Visibility;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::AccessExprData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Handles import.meta property access.
    /// Returns Some(type) if this is an import.meta access, None otherwise.
    pub(crate) fn try_resolve_import_meta_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        name_or_argument: NodeIndex,
    ) -> Option<TypeId> {
        let expr_node = self.ctx.arena.get(expression)?;
        if expr_node.kind != SyntaxKind::ImportKeyword as u16 {
            return None;
        }

        let is_meta = self
            .ctx
            .arena
            .get(name_or_argument)
            .and_then(|n| self.ctx.arena.get_identifier(n))
            .is_some_and(|ident| ident.escaped_text == "meta");

        if is_meta {
            self.check_import_meta_in_cjs(idx);
            // import.meta resolves to the global `ImportMeta` interface
            // (declared in lib.es2020.full.d.ts). Returning that type
            // enables TS2339 on unknown properties (`import.meta.blah`)
            // and merges `declare global { interface ImportMeta { ... } }`
            // augmentations through lib-heritage merging.
            if let Some(import_meta_ty) = self.resolve_lib_type_by_name("ImportMeta") {
                return Some(import_meta_ty);
            }
        }
        // Fallback (ImportMeta not in lib scope, or non-`meta` meta-property
        // like `import.metal`): return ANY so downstream access doesn't
        // cascade misleading TS2339s. A separate grammar check is expected
        // to emit TS17012 for the invalid meta-property name.
        Some(TypeId::ANY)
    }

    pub(crate) fn symbol_has_nonambient_current_file_declaration(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
            let current_file_idx = self.ctx.current_file_idx as u32;
            symbol
                .declarations
                .iter()
                .enumerate()
                .any(|(idx, &decl_idx)| {
                    let declaration_is_in_current_file = if let Some(stable) =
                        symbol.stable_declarations.get(idx)
                        && stable.is_known()
                        && stable.has_file_idx()
                    {
                        stable.file_idx == current_file_idx
                    } else if symbol.decl_file_idx != u32::MAX {
                        symbol.decl_file_idx == current_file_idx
                    } else {
                        true
                    };

                    declaration_is_in_current_file
                        && self.ctx.arena.get(decl_idx).is_some()
                        && !self.ctx.arena.is_in_ambient_context(decl_idx)
                })
        })
    }

    pub(crate) fn identifier_has_nonambient_current_file_binding(
        &self,
        ident_idx: NodeIndex,
        expected_name: &str,
    ) -> bool {
        let Some(ident) = self.ctx.arena.get_identifier_at(ident_idx) else {
            return false;
        };
        if ident.escaped_text != expected_name {
            return false;
        }

        self.ctx
            .binder
            .node_symbols
            .get(&ident_idx.0)
            .copied()
            .or_else(|| self.resolve_identifier_symbol_without_tracking(ident_idx))
            .is_some_and(|sym_id| self.symbol_has_nonambient_current_file_declaration(sym_id))
    }

    pub(crate) fn is_unshadowed_commonjs_exports_identifier(&self, ident_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get_identifier_at(ident_idx)
            .is_some_and(|ident| ident.escaped_text == "exports")
            && !self.identifier_has_nonambient_current_file_binding(ident_idx, "exports")
    }

    pub(crate) fn is_unshadowed_commonjs_module_identifier(&self, ident_idx: NodeIndex) -> bool {
        self.ctx
            .arena
            .get_identifier_at(ident_idx)
            .is_some_and(|ident| ident.escaped_text == "module")
            && !self.identifier_has_nonambient_current_file_binding(ident_idx, "module")
    }

    /// Choose the type to display in a TS2339 "property does not exist on type X"
    /// message after a `PropertyNotFound` lookup.
    ///
    /// Structural rule: when control-flow narrowing has refined the receiver
    /// (e.g. `if ("a" in x)` over `A | B` narrows `x` to `A`), the message
    /// must name the *narrowed* receiver, not the declared union — that is
    /// the type the property lookup actually ran against. Type parameters
    /// keep their existing apparent-type display (the constraint), so a
    /// `T extends A | B` receiver still reports the miss against `A | B`.
    pub(crate) fn diagnostic_display_type_for_missing_property(
        &self,
        narrowed: TypeId,
        apparent: TypeId,
    ) -> TypeId {
        if narrowed == apparent {
            return apparent;
        }
        // Only swap to the narrowed receiver type when the apparent type is a
        // union — that is the discriminated-narrowing case where the message
        // should name the picked-out member ("Property 'b' does not exist on
        // type 'A'." after `if ("a" in x)` over `A | B`). For non-union
        // apparent types, keep the existing display: it preserves literal
        // shape (e.g. `'""'` rather than the widened `'string'` you get from
        // primitive-narrowing on a literal receiver) and the constraint-based
        // display for type parameters.
        if crate::query_boundaries::state::checking::is_type_parameter_like(
            self.ctx.types,
            narrowed,
        ) {
            return apparent;
        }
        // Index-access narrowed types (e.g. `E[K]` where E and K are generic
        // and the resolved value type is a closed union like `A | B`) should
        // also defer to the apparent display. Returning the IndexAccess form
        // would otherwise route through the suppress-on-IndexAccess branch
        // in `error_property_not_exist_at`, silencing TS2339 entirely on a
        // genuine missing-property situation.
        if crate::query_boundaries::common::is_index_access_type(self.ctx.types, narrowed) {
            return apparent;
        }
        if crate::query_boundaries::common::union_list_id(self.ctx.types, apparent).is_some() {
            return narrowed;
        }
        apparent
    }

    pub(crate) fn is_array_constructor_is_array_recovery(
        &self,
        expression: NodeIndex,
        property_name: &str,
    ) -> bool {
        property_name == "isArray"
            && self
                .ctx
                .arena
                .get_identifier_at(expression)
                .is_some_and(|ident| ident.escaped_text == "Array")
    }

    pub(crate) fn declared_receiver_property_type(
        &mut self,
        expression: NodeIndex,
        _display_object_type: TypeId,
        property_name: &str,
    ) -> Option<TypeId> {
        let sym_id = self.resolve_identifier_symbol_without_tracking(expression)?;
        let declarations = self
            .ctx
            .binder
            .get_symbol(sym_id)
            .map(|symbol| symbol.declarations.clone())?;
        if declarations.len() != 1 {
            return None;
        }

        for decl_idx in declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let type_annotation = match decl_node.kind {
                syntax_kind_ext::PARAMETER => {
                    self.ctx.arena.get_parameter(decl_node).and_then(|param| {
                        param
                            .type_annotation
                            .is_some()
                            .then_some(param.type_annotation)
                    })
                }
                syntax_kind_ext::VARIABLE_DECLARATION => self
                    .ctx
                    .arena
                    .get_variable_declaration(decl_node)
                    .and_then(|var_decl| {
                        var_decl
                            .type_annotation
                            .is_some()
                            .then_some(var_decl.type_annotation)
                    }),
                _ => None,
            };
            let Some(type_annotation) = type_annotation else {
                continue;
            };
            let declared_type = self.get_type_from_type_node(type_annotation);
            let declared_type = self.evaluate_application_type(declared_type);
            if self.is_in_indexed_access_annotation_context(expression)
                && crate::query_boundaries::state::checking::is_type_parameter_like(
                    self.ctx.types,
                    declared_type,
                )
                && self
                    .type_parameter_constraint_has_explicit_property(declared_type, property_name)
            {
                if let Some(constraint) =
                    crate::query_boundaries::state::checking::type_parameter_constraint(
                        self.ctx.types,
                        declared_type,
                    )
                {
                    let constraint = self.evaluate_type_with_env(constraint);
                    if let PropertyAccessResult::Success { type_id, .. } =
                        self.resolve_property_access_with_env(constraint, property_name)
                    {
                        return Some(type_id);
                    }
                }
                continue;
            }
            if matches!(
                declared_type,
                TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
            ) || crate::query_boundaries::common::is_union_type(self.ctx.types, declared_type)
                || crate::query_boundaries::common::is_intersection_type(
                    self.ctx.types,
                    declared_type,
                )
            {
                if self.type_reference_class_declares_public_instance_member(
                    type_annotation,
                    property_name,
                ) {
                    return Some(TypeId::ANY);
                }
                continue;
            }
            let declared_type = self.resolve_type_for_property_access(declared_type);
            if declared_type != TypeId::NEVER
                && let PropertyAccessResult::Success { type_id, .. } =
                    self.resolve_property_access_with_env(declared_type, property_name)
            {
                return Some(type_id);
            }

            if self.type_reference_class_declares_public_instance_member(
                type_annotation,
                property_name,
            ) {
                return Some(TypeId::ANY);
            }
        }

        None
    }

    fn is_in_indexed_access_annotation_context(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
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
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION {
                if let Some(var_decl) = self.ctx.arena.get_variable_declaration(parent_node)
                    && var_decl.type_annotation.is_some()
                    && self
                        .node_text(var_decl.type_annotation)
                        .is_some_and(|text| text.contains("['"))
                {
                    return true;
                }
                return false;
            }
            if parent_node.kind == syntax_kind_ext::RETURN_STATEMENT
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
            {
                return false;
            }
            current = parent;
        }
        false
    }

    fn type_reference_class_declares_public_instance_member(
        &mut self,
        type_annotation: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(type_node) = self.ctx.arena.get(type_annotation) else {
            return false;
        };
        let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) else {
            return false;
        };
        let Some(type_name_node) = self.ctx.arena.get(type_ref.type_name) else {
            return false;
        };
        if self.ctx.arena.get_identifier(type_name_node).is_none() {
            return false;
        }
        let Some(type_sym_id) = self.resolve_identifier_symbol_without_tracking(type_ref.type_name)
        else {
            return false;
        };
        let Some(type_declarations) = self
            .ctx
            .binder
            .get_symbol(type_sym_id)
            .map(|symbol| symbol.declarations.clone())
        else {
            return false;
        };

        type_declarations.iter().any(|&decl_idx| {
            let Some(class_node) = self.ctx.arena.get(decl_idx) else {
                return false;
            };
            if class_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                return false;
            }
            let Some(class_data) = self.ctx.arena.get_class(class_node) else {
                return false;
            };

            class_data.members.nodes.iter().copied().any(|member_idx| {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    return false;
                };
                match member_node.kind {
                    syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .is_some_and(|prop| {
                            self.is_public_instance_member(prop.modifiers.as_ref())
                                && self.get_property_name(prop.name).as_deref()
                                    == Some(property_name)
                        }),
                    syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|method| {
                            self.is_public_instance_member(method.modifiers.as_ref())
                                && self.get_property_name(method.name).as_deref()
                                    == Some(property_name)
                        }),
                    syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                        .ctx
                        .arena
                        .get_accessor(member_node)
                        .is_some_and(|accessor| {
                            self.is_public_instance_member(accessor.modifiers.as_ref())
                                && self.get_property_name(accessor.name).as_deref()
                                    == Some(property_name)
                        }),
                    _ => false,
                }
            })
        })
    }

    fn is_public_instance_member(&self, modifiers: Option<&tsz_parser::parser::NodeList>) -> bool {
        !self
            .ctx
            .arena
            .has_modifier_ref(modifiers, SyntaxKind::StaticKeyword)
            && !self
                .ctx
                .arena
                .has_modifier_ref(modifiers, SyntaxKind::PrivateKeyword)
            && !self
                .ctx
                .arena
                .has_modifier_ref(modifiers, SyntaxKind::ProtectedKeyword)
    }

    pub(crate) fn declared_intersection_receiver_property_type(
        &mut self,
        expression: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let sym_id = self.resolve_identifier_symbol_without_tracking(expression)?;

        let declared_type = self.get_type_of_symbol(sym_id);
        if declared_type == TypeId::ANY
            || declared_type == TypeId::ERROR
            || declared_type == TypeId::UNKNOWN
            || declared_type == TypeId::NEVER
        {
            return None;
        }

        let declared_from_intersection_annotation =
            self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.declarations.iter().any(|&decl_idx| {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        return false;
                    };
                    let type_annotation =
                        if let Some(param) = self.ctx.arena.get_parameter(decl_node) {
                            param.type_annotation
                        } else if let Some(var_decl) =
                            self.ctx.arena.get_variable_declaration(decl_node)
                        {
                            var_decl.type_annotation
                        } else {
                            NodeIndex::NONE
                        };

                    self.type_annotation_is_intersection(type_annotation)
                })
            });

        if !declared_from_intersection_annotation {
            return None;
        }

        let evaluated_declared = self.evaluate_application_type(declared_type);
        if self.intersection_has_private_property_conflict(evaluated_declared) {
            return None;
        }
        let receiver = self.resolve_type_for_property_access(evaluated_declared);
        if receiver == TypeId::NEVER {
            return None;
        }
        matches!(
            self.resolve_property_access_with_env(receiver, property_name),
            PropertyAccessResult::Success { .. }
        )
        .then_some(receiver)
    }

    pub(crate) fn report_declared_intersection_access_if_reduced(
        &mut self,
        expression: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
    ) -> bool {
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(expression) else {
            return false;
        };
        let declared_type = self.get_type_of_symbol(sym_id);
        if matches!(
            declared_type,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) {
            return false;
        }

        let declared_from_intersection_annotation =
            self.ctx.binder.get_symbol(sym_id).is_some_and(|symbol| {
                symbol.declarations.iter().any(|&decl_idx| {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        return false;
                    };
                    let type_annotation =
                        if let Some(param) = self.ctx.arena.get_parameter(decl_node) {
                            param.type_annotation
                        } else if let Some(var_decl) =
                            self.ctx.arena.get_variable_declaration(decl_node)
                        {
                            var_decl.type_annotation
                        } else {
                            NodeIndex::NONE
                        };

                    self.type_annotation_is_intersection(type_annotation)
                })
            });
        if !declared_from_intersection_annotation {
            return false;
        }

        let evaluated_declared = self.evaluate_application_type(declared_type);
        if self.intersection_has_private_property_conflict(evaluated_declared) {
            self.error_property_not_exist_at(property_name, TypeId::NEVER, error_node);
            return true;
        }

        let source_type = self
            .ctx
            .types
            .get_display_alias(evaluated_declared)
            .unwrap_or(evaluated_declared);
        let Some(members) = crate::query_boundaries::property_access::intersection_members(
            self.ctx.types,
            source_type,
        ) else {
            return false;
        };

        let mut restricted = None;
        for member in members {
            let member = self.evaluate_application_type(member);
            let member = self.resolve_type_for_property_access(member);
            let Some(class_idx) = self.get_class_decl_from_type(member) else {
                continue;
            };
            if crate::query_boundaries::property_access::receiver_property_visibility(
                self.ctx.types,
                member,
                property_name,
            )
            .is_some_and(|visibility| visibility == tsz_solver::Visibility::Public)
            {
                return false;
            }
            if let Some(access_info) = self.find_member_access_info(class_idx, property_name, false)
            {
                restricted.get_or_insert(access_info);
            }
        }

        let Some(access_info) = restricted else {
            return false;
        };

        use crate::diagnostics::diagnostic_codes;
        match access_info.level {
            crate::state::MemberAccessLevel::Private => {
                let message = format!(
                    "Property '{}' is private and only accessible within class '{}'.",
                    property_name, access_info.declaring_class_name
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::PROPERTY_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_CLASS,
                );
            }
            crate::state::MemberAccessLevel::Protected => {
                let message = format!(
                    "Property '{property_name}' is protected and only accessible within class '{}' and its subclasses.",
                    access_info.declaring_class_name
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES,
                );
            }
        }
        true
    }

    pub(crate) fn report_declared_intersection_access_on_invalid_receiver(
        &mut self,
        object_type: TypeId,
        expression: NodeIndex,
        name_node: NodeIndex,
        error_node: NodeIndex,
    ) -> bool {
        if !matches!(object_type, TypeId::ANY | TypeId::ERROR) {
            return false;
        }
        let Some(ident) = self.ctx.arena.get_identifier_at(name_node) else {
            return false;
        };
        let property_name = ident.escaped_text.clone();
        self.report_declared_intersection_access_if_reduced(expression, &property_name, error_node)
    }

    pub(crate) fn declared_intersection_receiver_for_never_access(
        &mut self,
        expression: NodeIndex,
        name_node: NodeIndex,
        error_node: NodeIndex,
    ) -> Option<TypeId> {
        let ident = self.ctx.arena.get_identifier_at(name_node)?;
        let property_name = &ident.escaped_text;
        // tsc emits TS2339 on property access against `never` even when the
        // property exists on the un-narrowed declared receiver. Recover only
        // when the declared intersection annotation still exposes the property
        // without a private-property conflict.
        if let Some(receiver) =
            self.declared_intersection_receiver_property_type(expression, property_name)
        {
            return Some(receiver);
        }
        if !property_name.starts_with('#') {
            self.error_property_not_exist_at(property_name, TypeId::NEVER, error_node);
        }
        None
    }

    pub(crate) fn resolve_mixin_static_member_property_access(
        &mut self,
        idx: NodeIndex,
        expression: NodeIndex,
        object_type_for_access: TypeId,
        property_name: &str,
        skip_flow_narrowing: bool,
    ) -> Option<TypeId> {
        let (class_idx, true) =
            self.resolve_class_for_access(expression, object_type_for_access)?
        else {
            return None;
        };
        let member_type = self.find_mixin_static_member_type(class_idx, property_name)?;
        Some(self.finalize_property_access_result(idx, member_type, skip_flow_narrowing, false))
    }

    fn type_annotation_is_intersection(&self, mut type_annotation: NodeIndex) -> bool {
        while type_annotation.is_some() {
            let Some(node) = self.ctx.arena.get(type_annotation) else {
                return false;
            };
            if node.kind == syntax_kind_ext::INTERSECTION_TYPE {
                return true;
            }
            if node.kind == syntax_kind_ext::PARENTHESIZED_TYPE
                && let Some(paren) = self.ctx.arena.get_wrapped_type(node)
            {
                type_annotation = paren.type_node;
                continue;
            }
            break;
        }

        false
    }

    pub(crate) fn current_file_commonjs_module_identifier_is_unshadowed(
        &self,
        idx: NodeIndex,
    ) -> bool {
        self.is_unshadowed_commonjs_module_identifier(idx)
    }

    pub(crate) fn current_file_commonjs_exports_target_is_unshadowed(
        &self,
        idx: NodeIndex,
    ) -> bool {
        if self.current_source_file_has_esm_syntax() {
            return false;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self.is_unshadowed_commonjs_exports_identifier(idx);
        }

        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        self.is_unshadowed_commonjs_module_identifier(access.expression)
            && self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)
                .is_some_and(|ident| ident.escaped_text == "exports")
    }

    pub(crate) fn property_access_direct_write_rhs(
        &self,
        property_access_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        let prop_ext = self.ctx.arena.get_extended(property_access_idx)?;
        let parent_idx = prop_ext.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
            return None;
        }
        let binary = self.ctx.arena.get_binary_expr(parent_node)?;
        (binary.left == property_access_idx && self.is_assignment_operator(binary.operator_token))
            .then_some(binary.right)
    }

    pub(crate) fn current_file_commonjs_direct_write_rhs(
        &self,
        property_access_idx: NodeIndex,
    ) -> Option<NodeIndex> {
        self.property_access_direct_write_rhs(property_access_idx)
    }

    pub(crate) fn current_file_commonjs_write_rhs_is_undefined_like(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(idx) else {
            return false;
        };

        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .is_some_and(|ident| ident.escaped_text == "undefined");
        }

        if node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(node)
            && self.is_assignment_operator(binary.operator_token)
        {
            return self.current_file_commonjs_write_rhs_is_undefined_like(binary.right);
        }

        if node.kind != syntax_kind_ext::VOID_EXPRESSION
            && node.kind != syntax_kind_ext::PREFIX_UNARY_EXPRESSION
        {
            return false;
        }

        let Some(unary) = self.ctx.arena.get_unary_expr(node) else {
            return false;
        };
        if unary.operator != SyntaxKind::VoidKeyword as u16 {
            return false;
        }
        let Some(expr) = self.ctx.arena.get(unary.operand) else {
            return false;
        };

        matches!(expr.kind, k if k == SyntaxKind::NumericLiteral as u16)
            && self
                .ctx
                .arena
                .get_literal(expr)
                .is_some_and(|lit| lit.text == "0")
    }

    pub(crate) fn is_jsdoc_annotated_this_member_declaration(&mut self, idx: NodeIndex) -> bool {
        if !self.is_js_file() {
            return false;
        }

        let mut current = idx;
        for _ in 0..4 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(ext.parent) else {
                return false;
            };
            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT {
                if self.jsdoc_type_annotation_for_node(ext.parent).is_none() {
                    return false;
                }
                let Some(stmt) = self.ctx.arena.get_expression_statement(parent_node) else {
                    return false;
                };
                let Some(expr_node) = self.ctx.arena.get(stmt.expression) else {
                    return false;
                };
                if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                    && expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                {
                    return false;
                }
                let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
                    return false;
                };
                let Some(base_node) = self.ctx.arena.get(access.expression) else {
                    return false;
                };
                return base_node.kind == SyntaxKind::ThisKeyword as u16
                    && self.this_has_contextual_owner(access.expression).is_some();
            }
            current = ext.parent;
        }

        false
    }

    pub(crate) fn finalize_property_access_result(
        &mut self,
        idx: NodeIndex,
        result_type: TypeId,
        skip_flow_narrowing: bool,
        skip_result_flow_for_result: bool,
    ) -> TypeId {
        if self.ctx.types.take_union_too_complex()
            || self.property_access_result_exceeds_complexity_limit(result_type)
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                idx,
                diagnostic_messages::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
                diagnostic_codes::EXPRESSION_PRODUCES_A_UNION_TYPE_THAT_IS_TOO_COMPLEX_TO_REPRESENT,
            );
        }

        if skip_flow_narrowing || skip_result_flow_for_result {
            result_type
        } else {
            self.apply_flow_narrowing(idx, result_type)
        }
    }

    fn property_access_result_exceeds_complexity_limit(&self, type_id: TypeId) -> bool {
        const UNION_COMPLEXITY_LIMIT: usize = 100_000;

        let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, type_id)
        else {
            return false;
        };

        let mut cross_product_size = 1usize;
        for member in members {
            if let Some(union_members) =
                crate::query_boundaries::common::union_members(self.ctx.types, member)
            {
                cross_product_size = cross_product_size.saturating_mul(union_members.len());
                if cross_product_size >= UNION_COMPLEXITY_LIMIT {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn flow_narrowed_write_receiver_type(
        &mut self,
        property_access_idx: NodeIndex,
        receiver_idx: NodeIndex,
        declared_type: TypeId,
    ) -> TypeId {
        let read_type = self.get_type_of_node_with_request(receiver_idx, &TypingRequest::NONE);
        if read_type != declared_type {
            return read_type;
        }
        if self.ctx.daa_error_nodes.contains(&receiver_idx.0)
            || self.ctx.daa_error_nodes.contains(&property_access_idx.0)
        {
            return read_type;
        }
        if !self.write_receiver_can_flow_narrow(property_access_idx, receiver_idx) {
            return read_type;
        }
        let Some(flow_node) = self.flow_node_for_reference_usage(property_access_idx) else {
            return read_type;
        };

        self.flow_analyzer_for_property_reads()
            .get_flow_type(receiver_idx, read_type, flow_node)
    }

    pub(crate) fn write_receiver_type_for_property_access(
        &mut self,
        property_access_idx: NodeIndex,
        receiver_idx: NodeIndex,
        property_name: Option<&str>,
        object_type_no_flow: TypeId,
        preserve_non_js_write_base: bool,
    ) -> (TypeId, bool) {
        let can_use_no_flow = if let Some(property_name) = property_name {
            let evaluated_no_flow = self.evaluate_application_type(object_type_no_flow);
            let resolved_no_flow = self.resolve_type_for_property_access(evaluated_no_flow);
            !matches!(
                self.resolve_property_access_with_env(resolved_no_flow, property_name),
                PropertyAccessResult::PropertyNotFound { .. } | PropertyAccessResult::IsUnknown
            )
        } else {
            false
        };

        if !can_use_no_flow && !preserve_non_js_write_base {
            return (
                self.flow_narrowed_write_receiver_type(
                    property_access_idx,
                    receiver_idx,
                    object_type_no_flow,
                ),
                false,
            );
        }

        let Some(property_name) = property_name else {
            return (object_type_no_flow, false);
        };
        let read_object_type = self.flow_narrowed_write_receiver_type(
            property_access_idx,
            receiver_idx,
            object_type_no_flow,
        );
        let evaluated_read = self.evaluate_application_type(read_object_type);
        let resolved_read = self.resolve_type_for_property_access(evaluated_read);
        if self.union_write_requires_existing_named_member(resolved_read, property_name) {
            return (read_object_type, false);
        }

        let read_has_property = !matches!(
            self.resolve_property_access_with_env(resolved_read, property_name),
            PropertyAccessResult::PropertyNotFound { .. } | PropertyAccessResult::IsUnknown
        );
        if read_has_property {
            (object_type_no_flow, false)
        } else if self.write_receiver_can_flow_narrow(property_access_idx, receiver_idx) {
            (read_object_type, true)
        } else {
            (object_type_no_flow, true)
        }
    }

    fn write_receiver_can_flow_narrow(
        &self,
        property_access_idx: NodeIndex,
        receiver_idx: NodeIndex,
    ) -> bool {
        self.flow_node_for_reference_usage(property_access_idx)
            .is_some()
            && self.ctx.arena.get(receiver_idx).is_some_and(|expr| {
                matches!(
                    expr.kind,
                    k if k == SyntaxKind::Identifier as u16
                        || k == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                        || k == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
                )
            })
    }

    pub(crate) fn report_loop_widened_receiver_property_error(
        &mut self,
        property_access_idx: NodeIndex,
        access: &AccessExprData,
        property_name: &str,
        receiver_type: TypeId,
        property_type: TypeId,
        skip_flow_narrowing: bool,
    ) -> bool {
        if skip_flow_narrowing
            || self.ctx.iteration_depth == 0
            || access.question_dot_token
            || matches!(property_type, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN)
        {
            return false;
        }

        // A self-recursive loop assignment can make the next iteration's
        // receiver wider than the first-pass receiver, e.g. `x = x.length`
        // turns `x` from `string` into `string | number`.
        let mut current = property_access_idx;
        while let Some(parent_idx) = self.ctx.arena.parent_of(current) {
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };
            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
            {
                let assignment_is_statement = self
                    .ctx
                    .arena
                    .parent_of(parent_idx)
                    .and_then(|statement_idx| self.ctx.arena.get(statement_idx))
                    .is_some_and(|node| node.kind == syntax_kind_ext::EXPRESSION_STATEMENT);
                if binary.operator_token == SyntaxKind::EqualsToken as u16
                    && binary.right == property_access_idx
                    && assignment_is_statement
                {
                    let analyzer = self.flow_analyzer_for_property_reads();
                    if analyzer.is_matching_reference(binary.left, access.expression) {
                        let loop_receiver_type = self
                            .ctx
                            .types
                            .factory()
                            .union2(receiver_type, property_type);
                        let loop_receiver_for_access =
                            self.resolve_type_for_property_access(loop_receiver_type);
                        if matches!(
                            self.resolve_property_access_with_env(
                                loop_receiver_for_access,
                                property_name,
                            ),
                            PropertyAccessResult::PropertyNotFound { .. }
                        ) {
                            self.error_property_not_exist_at(
                                property_name,
                                loop_receiver_type,
                                access.name_or_argument,
                            );
                            return true;
                        }
                    }
                }
                break;
            }
            if parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                || parent_node.kind == syntax_kind_ext::BLOCK
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                break;
            }
            current = parent_idx;
        }

        false
    }

    pub(crate) fn recover_self_recursive_property_access_type(
        &self,
        receiver_type: TypeId,
        property_name: &str,
        result_type: TypeId,
    ) -> TypeId {
        let Some((indexed_object, indexed_key)) =
            crate::query_boundaries::common::index_access_types(self.ctx.types, result_type)
        else {
            return result_type;
        };

        if indexed_object != receiver_type {
            return result_type;
        }

        let Some(indexed_key_name) =
            crate::query_boundaries::common::string_literal_value(self.ctx.types, indexed_key)
        else {
            return result_type;
        };

        if self.ctx.types.resolve_atom(indexed_key_name) == property_name {
            TypeId::ANY
        } else {
            result_type
        }
    }

    pub(crate) fn recover_self_recursive_property_access_result_at(
        &mut self,
        idx: NodeIndex,
        result_type: TypeId,
    ) -> TypeId {
        if crate::query_boundaries::common::index_access_types(self.ctx.types, result_type)
            .is_none()
        {
            return result_type;
        }

        let Some(node) = self.ctx.arena.get(idx) else {
            return result_type;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return result_type;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return result_type;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier_at(access.name_or_argument) else {
            return result_type;
        };

        let receiver_type = self
            .ctx
            .node_types
            .get(&access.expression.0)
            .copied()
            .unwrap_or_else(|| self.get_type_of_node(access.expression));

        let recovered = self.recover_self_recursive_property_access_type(
            receiver_type,
            &name_ident.escaped_text,
            result_type,
        );
        if recovered == TypeId::ANY && recovered != result_type {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};

            self.error_at_node(
                idx,
                diagnostic_messages::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
                diagnostic_codes::TYPE_INSTANTIATION_IS_EXCESSIVELY_DEEP_AND_POSSIBLY_INFINITE,
            );
        }
        recovered
    }

    pub(crate) fn is_stale_unconstrained_type_parameter(&self, type_id: TypeId) -> bool {
        if !crate::query_boundaries::state::checking::is_type_parameter_like(
            self.ctx.types,
            type_id,
        ) || access_query::type_parameter_constraint(self.ctx.types, type_id).is_some()
        {
            return false;
        }

        access_query::type_parameter_name(self.ctx.types, type_id).is_some_and(|name_atom| {
            let name = self.ctx.types.resolve_atom(name_atom);
            self.ctx
                .type_parameter_scope
                .get(&name)
                .is_some_and(|&scope_type_id| {
                    scope_type_id != type_id
                        && access_query::type_parameter_constraint(self.ctx.types, scope_type_id)
                            .is_some()
                })
        })
    }

    pub(crate) fn union_write_requires_existing_named_member(
        &mut self,
        object_type: TypeId,
        property_name: &str,
    ) -> bool {
        let Some(members) =
            crate::query_boundaries::common::union_members(self.ctx.types, object_type)
        else {
            return false;
        };

        let mut saw_present_member = false;
        let mut saw_fresh_empty_missing_member = false;

        for member in members {
            if member.is_nullable() {
                continue;
            }

            let evaluated_member = self.evaluate_application_type(member);
            let resolved_member = self.resolve_type_for_property_access(evaluated_member);
            match self.resolve_property_access_with_env(resolved_member, property_name) {
                PropertyAccessResult::Success { .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(_),
                    ..
                } => {
                    saw_present_member = true;
                }
                PropertyAccessResult::PropertyNotFound { .. } => {
                    if crate::query_boundaries::common::is_empty_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) && crate::query_boundaries::common::is_fresh_object_type(
                        self.ctx.types,
                        resolved_member,
                    ) {
                        saw_fresh_empty_missing_member = true;
                    } else {
                        return false;
                    }
                }
                PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: None,
                    ..
                }
                | PropertyAccessResult::IsUnknown => {}
            }
        }

        saw_present_member && saw_fresh_empty_missing_member
    }

    pub(crate) fn recover_property_from_implemented_interfaces(
        &mut self,
        class_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let class_node = self.ctx.arena.get(class_idx)?;
        let class = self.ctx.arena.get_class(class_node)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                continue;
            }

            for &type_idx in &heritage.types.nodes {
                // Heritage clause types are ExpressionWithTypeArguments nodes.
                // Resolve the symbol from the expression, then get its instance type
                // via type_reference_symbol_type (which returns the instance type for
                // classes, not the constructor type).
                let expr_idx = if let Some(type_node) = self.ctx.arena.get(type_idx)
                    && let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node)
                {
                    expr_type_args.expression
                } else {
                    type_idx
                };

                let Some(sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                    continue;
                };
                let interface_type = self.type_reference_symbol_type(sym_id);
                if interface_type == TypeId::ERROR {
                    continue;
                }
                let interface_type_eval = self.evaluate_application_type(interface_type);
                // Resolve Lazy(DefId) types through the checker's TypeEnvironment so the
                // solver can inspect the interface's actual members. Without this step the
                // solver falls back to TypeId::ANY (its "couldn't resolve" sentinel) which
                // would incorrectly suppress TS2339 for properties that don't exist at all.
                let interface_type_resolved =
                    self.resolve_type_for_property_access(interface_type_eval);
                match self.resolve_property_access_with_env(interface_type_resolved, property_name)
                {
                    PropertyAccessResult::Success { type_id, .. }
                    | PropertyAccessResult::PossiblyNullOrUndefined {
                        property_type: Some(type_id),
                        ..
                    } => {
                        // Don't recover private or protected members from implemented
                        // interfaces. When an interface extends a class with private
                        // members, those members should only be accessible on classes
                        // that actually extend that base class, not on any class that
                        // merely implements the interface.
                        if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                            self.ctx.types,
                            interface_type_resolved,
                        ) {
                            let prop_atom = self.ctx.types.intern_string(property_name);
                            if let Some(prop_info) =
                                shape.properties.iter().find(|p| p.name == prop_atom)
                                && prop_info.visibility != Visibility::Public
                            {
                                continue;
                            }
                        }
                        return Some(type_id);
                    }
                    _ => {}
                }
            }
        }

        None
    }

    /// Check if a const enum symbol is "ambient" — declared with `declare` keyword
    /// or originating from a `.d.ts` file. Ambient const enums have no runtime
    /// representation and cannot be accessed under `isolatedModules`.
    pub(crate) fn is_const_enum_ambient(&self, sym: &tsz_binder::Symbol) -> bool {
        // If the file itself is a .d.ts, everything in it is ambient.
        if self.ctx.is_declaration_file() {
            return true;
        }
        // Check if all declarations are in ambient context (e.g., `declare const enum`).
        if sym.declarations.is_empty() {
            return false;
        }
        for &decl_idx in &sym.declarations {
            if !self.ctx.arena.is_in_ambient_context(decl_idx) {
                return false;
            }
        }
        true
    }

    /// Check if a node is in a type-only position (e.g., computed property name
    /// inside a type literal, interface, or type alias). In such positions,
    /// const enum values are resolved at compile time and don't need runtime access.
    pub(crate) fn is_in_type_only_position(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        for _ in 0..20 {
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
            // If we hit a type node or type alias/interface declaration, we're in type context
            if parent_node.is_type_node()
                || parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
            {
                return true;
            }
            // If we hit a statement, class member, or function-like, we're in value context
            if parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                || parent_node.kind == syntax_kind_ext::EXPRESSION_STATEMENT
                || parent_node.kind == syntax_kind_ext::RETURN_STATEMENT
                || parent_node.kind == syntax_kind_ext::CALL_EXPRESSION
                || parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            {
                return false;
            }
            current = parent;
        }
        false
    }

    pub(crate) fn resolve_shadowed_global_value_member(
        &mut self,
        expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let value_type = if let Some(ident) = self.ctx.arena.get_identifier_at(expr_idx) {
            let sym_id = self.resolve_identifier_symbol_without_tracking(expr_idx)?;
            let symbol = self
                .ctx
                .binder
                .get_symbol(sym_id)
                .or_else(|| self.get_cross_file_symbol(sym_id))?;

            let is_namespace = symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE);
            let value_flags_except_module = symbol_flags::VALUE & !symbol_flags::VALUE_MODULE;
            let has_other_value = symbol.has_any_flags(value_flags_except_module);
            if !is_namespace || has_other_value {
                return None;
            }

            let is_instantiated = symbol
                .declarations
                .iter()
                .any(|&decl_idx| self.is_namespace_declaration_instantiated(decl_idx));
            if is_instantiated {
                return None;
            }

            self.type_of_value_symbol_by_name(&ident.escaped_text)
        } else if self
            .ctx
            .arena
            .get(expr_idx)
            .is_some_and(|node| node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION)
        {
            let access = self
                .ctx
                .arena
                .get_access_expr(self.ctx.arena.get(expr_idx)?)?;
            let ns_member_sym_id = self.resolve_qualified_symbol(expr_idx)?;
            let ns_member_symbol = self
                .ctx
                .binder
                .get_symbol(ns_member_sym_id)
                .or_else(|| self.get_cross_file_symbol(ns_member_sym_id))?;
            if !ns_member_symbol.has_any_flags(symbol_flags::ENUM)
                || ns_member_symbol.has_any_flags(symbol_flags::ENUM_MEMBER)
            {
                return None;
            }

            let parent_symbol = self
                .ctx
                .binder
                .get_symbol(ns_member_symbol.parent)
                .or_else(|| self.get_cross_file_symbol(ns_member_symbol.parent))?;
            if !parent_symbol.has_any_flags(symbol_flags::NAMESPACE_MODULE) {
                return None;
            }

            let root_name = self.property_access_chain_key(access.expression)?;
            let value_type = self.type_of_value_symbol_by_name(&root_name);
            if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
                return None;
            }
            let member_name = self
                .ctx
                .arena
                .get_identifier_at(access.name_or_argument)?
                .escaped_text
                .as_str();
            match self.resolve_property_access_with_env(value_type, member_name) {
                PropertyAccessResult::Success { type_id, .. }
                | PropertyAccessResult::PossiblyNullOrUndefined {
                    property_type: Some(type_id),
                    ..
                } => type_id,
                _ => return None,
            }
        } else {
            return None;
        };

        if value_type == TypeId::UNKNOWN || value_type == TypeId::ERROR {
            return None;
        }

        match self.resolve_property_access_with_env(value_type, property_name) {
            PropertyAccessResult::Success { type_id, .. }
            | PropertyAccessResult::PossiblyNullOrUndefined {
                property_type: Some(type_id),
                ..
            } => Some(type_id),
            _ => None,
        }
    }

    fn property_access_chain_key(&self, expr_idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return self
                .ctx
                .arena
                .get_identifier(node)
                .map(|ident| ident.escaped_text.to_string());
        }
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return None;
        }
        let access = self.ctx.arena.get_access_expr(node)?;
        let left = self.property_access_chain_key(access.expression)?;
        let right = self.ctx.arena.get_identifier_at(access.name_or_argument)?;
        Some(format!("{left}.{}", right.escaped_text))
    }

    /// Get type of property access expression.
    #[allow(dead_code)]
    pub(crate) fn get_type_of_property_access(&mut self, idx: NodeIndex) -> TypeId {
        self.get_type_of_property_access_with_request(idx, &TypingRequest::NONE)
    }

    pub(crate) fn get_type_of_property_access_with_request(
        &mut self,
        idx: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        if self.ctx.instantiation_depth.get() >= MAX_INSTANTIATION_DEPTH {
            self.ctx.depth_exceeded.set(true);
            return TypeId::ERROR; // Max instantiation depth exceeded - propagate error
        }

        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() + 1);
        let inner_result = self.get_type_of_property_access_inner(idx, request);
        let result = self.recover_self_recursive_property_access_result_at(idx, inner_result);
        self.ctx
            .instantiation_depth
            .set(self.ctx.instantiation_depth.get() - 1);
        self.instantiate_callable_result_from_request(idx, result, request)
    }

    pub(crate) fn missing_typescript_lib_dom_global_alias(&self, idx: NodeIndex) -> Option<String> {
        let node = self.ctx.arena.get(idx)?;
        let ident = self.ctx.arena.get_identifier(node)?;
        let name = ident.escaped_text.as_str();
        if !matches!(name, "window" | "self") {
            return None;
        }
        if !self.ctx.typescript_dom_replacement_loaded {
            return None;
        }
        match name {
            "window" if !self.ctx.typescript_dom_replacement_has_window => Some(name.to_string()),
            "self" if !self.ctx.typescript_dom_replacement_has_self => Some(name.to_string()),
            _ => None,
        }
    }

    pub(crate) fn enum_member_initializer_display_type(
        &mut self,
        expr_idx: NodeIndex,
    ) -> Option<TypeId> {
        let expr_idx = self.ctx.arena.skip_parenthesized_and_assertions(expr_idx);
        let node = self.ctx.arena.get(expr_idx)?;
        if node.kind != SyntaxKind::Identifier as u16 {
            return None;
        }

        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let decl_idx = symbol.primary_declaration()?;

        let var_decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;
        if var_decl.initializer.is_none() {
            return None;
        }

        let init_type = self.get_type_of_node(var_decl.initializer);
        self.is_enum_member_type_for_widening(init_type)
            .then_some(init_type)
    }

    /// Resolve the base constraint of an `IndexAccess` type for display purposes.
    ///
    /// For `T[K]` where `T extends C` and `K extends D`, resolves through the
    /// constraint chain to produce the concrete type (e.g., `C[D]` evaluated).
    /// This matches tsc's behavior of showing the apparent type in error messages.
    pub(crate) fn resolve_index_access_base_constraint(&mut self, type_id: TypeId) -> TypeId {
        // First try standard evaluation (resolves T to its constraint)
        let evaluated = self.evaluate_type_with_env(type_id);

        // If fully resolved (no longer an IndexAccess), use it
        if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, evaluated) {
            return evaluated;
        }

        // Still an IndexAccess — try resolving the index type parameter's constraint.
        // E.g., {[s:string]:V}[K] where K extends keyof T => evaluate {[s:string]:V}[keyof T] => V
        if let Some((ia_obj, ia_idx)) =
            crate::query_boundaries::common::index_access_parts(self.ctx.types, evaluated)
            && let Some(constraint) =
                access_query::type_parameter_constraint(self.ctx.types, ia_idx)
        {
            let resolved = self
                .ctx
                .types
                .evaluate_index_access_with_options(ia_obj, constraint, false);
            if !crate::query_boundaries::common::is_index_access_type(self.ctx.types, resolved) {
                return resolved;
            }
        }

        type_id
    }

    /// Check if a symbol has any exported value declarations.
    ///
    /// For merged symbols (e.g., namespace + interface with same name), only the
    /// interface part may be exported while the namespace is not. This helper
    /// checks whether any VALUE-contributing declaration (namespace, function,
    /// class, etc.) has an export modifier.
    ///
    /// Returns `true` if:
    /// - The symbol has no TYPE flags (pure value symbol - trust `is_exported`)
    /// - The symbol has at least one value declaration with export modifier
    pub(crate) fn symbol_has_exported_value_declaration(
        &self,
        sym_id: tsz_binder::SymbolId,
    ) -> bool {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // If the symbol only has VALUE flags (no TYPE flags), we can trust is_exported
        let has_type_flags = symbol.has_any_flags(symbol_flags::TYPE);
        if !has_type_flags {
            return symbol.is_exported;
        }

        // For symbols that are both VALUE and TYPE by design (CLASS, ENUM, ENUM_MEMBER),
        // not due to merging with an interface/type-alias, we can trust is_exported.
        // Enum members are considered exported if they're in the enum's exports table.
        // We only need special handling for namespace + interface/type-alias merges.
        let is_merged_with_type_only =
            symbol.has_any_flags(symbol_flags::INTERFACE | symbol_flags::TYPE_ALIAS);
        if !is_merged_with_type_only {
            // Enum members may not have is_exported set, but they're accessible
            // if they're in the enum's exports table (which they must be to get here)
            if symbol.has_any_flags(symbol_flags::ENUM_MEMBER) {
                return true;
            }
            return symbol.is_exported;
        }

        // For lib symbols (decl_file_idx == u32::MAX), trust is_exported since
        // lib declarations have proper export semantics by construction.
        if symbol.decl_file_idx == u32::MAX {
            return symbol.is_exported;
        }

        // For cross-file merged symbols, trust is_exported since declarations
        // may be in different arenas. The cross-file merge logic in the binder
        // correctly tracks export status.
        if self.ctx.all_arenas.is_some() {
            // Check if this looks like a cross-file merged symbol by seeing if
            // any declarations can't be found in the current arena
            let has_cross_file_decl = symbol
                .declarations
                .iter()
                .any(|&decl_idx| self.ctx.arena.get(decl_idx).is_none());
            if has_cross_file_decl {
                return symbol.is_exported;
            }
        }

        // Single-file merged symbol - check declarations individually
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            // Check if this is a value declaration with export modifier
            if let Some(true) =
                self.check_value_decl_has_export_in_arena(self.ctx.arena, decl_idx, decl_node)
            {
                return true;
            }
        }

        tracing::debug!(
            "symbol_has_exported_value_declaration: returning false for {:?}",
            symbol.escaped_name
        );
        false
    }

    /// Check if a declaration node has an export modifier using a specific arena.
    /// Also checks if the declaration is wrapped in an `EXPORT_DECLARATION` node,
    /// since `export namespace B` creates an `EXPORT_DECLARATION` wrapping `MODULE_DECLARATION`.
    fn check_value_decl_has_export_in_arena(
        &self,
        arena: &tsz_parser::parser::node::NodeArena,
        decl_idx: tsz_parser::NodeIndex,
        decl_node: &tsz_parser::parser::node::Node,
    ) -> Option<bool> {
        // Helper to check if a node is wrapped in an EXPORT_DECLARATION
        let is_inside_export_decl = || -> bool {
            // Get parent node from extended info
            if let Some(ext) = arena.get_extended(decl_idx)
                && let Some(parent_node) = arena.get(ext.parent)
                && parent_node.kind == syntax_kind_ext::EXPORT_DECLARATION
            {
                return true;
            }
            false
        };

        // Helper to check if the declaration is inside a `declare` context (ambient).
        // In ambient contexts, members are implicitly exported.
        let is_inside_declare_context = || -> bool {
            let mut current = decl_idx;
            for _ in 0..10 {
                let Some(ext) = arena.get_extended(current) else {
                    break;
                };
                let Some(parent_node) = arena.get(ext.parent) else {
                    break;
                };
                // Check if parent is a module with `declare` modifier
                if parent_node.kind == syntax_kind_ext::MODULE_DECLARATION
                    && let Some(m) = arena.get_module(parent_node)
                    && m.modifiers
                        .as_ref()
                        .is_some_and(|mods| arena.is_declare_ref(Some(mods)))
                {
                    return true;
                }
                current = ext.parent;
            }
            false
        };

        match decl_node.kind {
            syntax_kind_ext::MODULE_DECLARATION => {
                let module = arena.get_module(decl_node);
                if let Some(m) = module {
                    // Check direct modifiers, parent EXPORT_DECLARATION, or ambient context
                    let has_direct_export = m.modifiers.as_ref().is_some_and(|mods| {
                        arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                    });
                    let has_declare = m
                        .modifiers
                        .as_ref()
                        .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                    Some(
                        has_direct_export
                            || has_declare
                            || is_inside_export_decl()
                            || is_inside_declare_context(),
                    )
                } else {
                    None
                }
            }
            syntax_kind_ext::FUNCTION_DECLARATION => arena.get_function(decl_node).map(|f| {
                let has_direct_export = f.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = f
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::CLASS_DECLARATION => arena.get_class(decl_node).map(|c| {
                let has_direct_export = c.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = c
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::ENUM_DECLARATION => arena.get_enum(decl_node).map(|e| {
                let has_direct_export = e.modifiers.as_ref().is_some_and(|mods| {
                    arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                });
                let has_declare = e
                    .modifiers
                    .as_ref()
                    .is_some_and(|mods| arena.is_declare_ref(Some(mods)));
                has_direct_export
                    || has_declare
                    || is_inside_export_decl()
                    || is_inside_declare_context()
            }),
            syntax_kind_ext::VARIABLE_DECLARATION => {
                // For variable declarations, check if inside a declare context
                // (e.g., `declare namespace Foo { var x: number; }`)
                // The export modifier is on the parent VARIABLE_STATEMENT, not the declaration itself.
                // Walk up: VARIABLE_DECLARATION -> VARIABLE_DECLARATION_LIST -> VARIABLE_STATEMENT
                // and check if the VARIABLE_STATEMENT has an `export` modifier.
                let has_export_on_var_stmt = || -> bool {
                    // Walk from VariableDeclaration up to VariableStatement
                    let Some(ext1) = arena.get_extended(decl_idx) else {
                        return false;
                    };
                    // ext1.parent = VariableDeclarationList
                    let Some(ext2) = arena.get_extended(ext1.parent) else {
                        return false;
                    };
                    // ext2.parent = VariableStatement
                    let Some(var_stmt_node) = arena.get(ext2.parent) else {
                        return false;
                    };
                    if var_stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                        return false;
                    }
                    arena
                        .get_variable(var_stmt_node)
                        .and_then(|v| v.modifiers.as_ref())
                        .is_some_and(|mods| {
                            arena.has_modifier_ref(Some(mods), SyntaxKind::ExportKeyword)
                        })
                };
                Some(
                    has_export_on_var_stmt()
                        || is_inside_export_decl()
                        || is_inside_declare_context(),
                )
            }
            _ => Some(false), // Skip non-value declarations (interface, type alias)
        }
    }

    pub(crate) fn check_jsdoc_prototype_type_decl_constructor_assignment(
        &mut self,
        prototype_expr_idx: NodeIndex,
        property_name: &str,
        declared_type: TypeId,
    ) {
        let Some(prototype_node) = self.ctx.arena.get(prototype_expr_idx) else {
            return;
        };
        if prototype_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return;
        }
        let Some(prototype_access) = self.ctx.arena.get_access_expr(prototype_node) else {
            return;
        };
        if self
            .ctx
            .arena
            .get_identifier_at(prototype_access.name_or_argument)
            .is_none_or(|ident| ident.escaped_text != "prototype")
        {
            return;
        }
        let Some(func_idx) = self.js_prototype_owner_function_target(prototype_access.expression)
        else {
            return;
        };
        let Some(body_idx) = self
            .ctx
            .arena
            .get(func_idx)
            .and_then(|node| self.ctx.arena.get_function(node))
            .and_then(|func| func.body.is_some().then_some(func.body))
        else {
            return;
        };
        let Some((source_idx, diag_idx)) =
            self.constructor_this_assignment_for_property(body_idx, property_name)
        else {
            return;
        };

        let source_type = self.get_type_of_node(source_idx);
        let target_type =
            crate::query_boundaries::common::remove_undefined(self.ctx.types, declared_type);
        if !self.is_assignable_to(source_type, target_type) {
            let _ = self.check_assignable_or_report_at_exact_anchor(
                source_type,
                target_type,
                source_idx,
                diag_idx,
            );
        }
    }

    fn constructor_this_assignment_for_property(
        &mut self,
        body_idx: NodeIndex,
        property_name: &str,
    ) -> Option<(NodeIndex, NodeIndex)> {
        let body_node = self.ctx.arena.get(body_idx)?;
        let block = self.ctx.arena.get_block(body_node)?;
        let mut stmts = Vec::new();
        for &stmt_idx in &block.statements.nodes {
            self.collect_nested_js_this_assignment_statements(stmt_idx, &mut stmts);
        }
        let this_aliases = self.collect_this_aliases(&stmts);

        for stmt_idx in stmts {
            let Some((found_name, rhs_idx, is_private, _)) =
                self.extract_this_property_assignment(stmt_idx, &this_aliases)
            else {
                continue;
            };
            if is_private || found_name != property_name {
                continue;
            }
            let stmt_node = self.ctx.arena.get(stmt_idx)?;
            let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
            let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
            let binary = self.ctx.arena.get_binary_expr(expr_node)?;
            return Some((rhs_idx, binary.left));
        }

        None
    }
}
