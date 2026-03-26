//! Property access checking (accessibility, computed names, const modifiers).

use crate::classes_domain::class_summary::ClassMemberKind;
use crate::query_boundaries::checkers::property as query;
use crate::query_boundaries::type_computation::complex::{
    ClassDeclTypeKind, classify_for_class_decl,
};
use crate::state::CheckerState;
use crate::state::MemberAccessLevel;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

// =============================================================================
// Property Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Property Accessibility
    // =========================================================================

    fn report_computed_this_property_missing(&mut self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(expr_node) else {
            return false;
        };
        if !self.is_this_expression(access.expression)
            || !self.is_this_in_class_member_computed_property_name(access.expression)
        {
            return false;
        }

        let Some(name_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
            return false;
        };
        let property_name = name_ident.escaped_text.as_str();

        let Some(class_idx) = self.nearest_enclosing_class(access.expression) else {
            return false;
        };
        let is_static = self.is_this_in_static_class_member(access.expression);
        if self
            .summarize_class_chain(class_idx)
            .lookup(property_name, is_static, true)
            .is_some()
        {
            return false;
        }

        let Some(class_node) = self.ctx.arena.get(class_idx) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };
        let receiver_type = if is_static {
            self.get_class_constructor_type(class_idx, class_data)
        } else {
            self.get_class_instance_type(class_idx, class_data)
        };
        self.error_property_not_exist_at(property_name, receiver_type, access.name_or_argument);
        true
    }

    /// Check if accessing a property is allowed based on its access modifier.
    ///
    /// ## Access Modifiers:
    /// - **Private**: Accessible only within the declaring class
    /// - **Protected**: Accessible within the declaring class and subclasses
    /// - **Public**: Accessible from anywhere (default)
    ///
    /// ## Returns:
    /// - `true` if access is allowed
    /// - `false` if access is denied (error emitted)
    ///
    /// ## Error Codes:
    /// - TS2341: "Property '{}' is private and only accessible within class '{}'."
    /// - TS2445: "Property '{}' is protected and only accessible within class '{}' and its subclasses."
    pub(crate) fn check_property_accessibility(
        &mut self,
        object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        object_type: tsz_solver::TypeId,
    ) -> bool {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use crate::state::MemberAccessLevel;

        let is_property_identifier = self
            .ctx
            .arena
            .get(error_node)
            .and_then(|node| self.ctx.arena.get_identifier(node))
            .is_some();

        if self.union_restricted_property_is_missing(object_expr, property_name, object_type) {
            self.error_property_not_exist_at(property_name, object_type, error_node);
            return false;
        }

        // TypeScript allows `super["x"]` element-access forms without applying
        // the stricter method-only/private-protected checks used for `super.x`.
        if self.is_super_expression(object_expr) && !is_property_identifier {
            return true;
        }

        let class_result = self.resolve_class_for_access(object_expr, object_type);

        // Fallback: when the object type is an interface extending multiple
        // unrelated classes, `resolve_class_for_access` returns None because
        // `get_class_decl_from_type` can't pick a single most-derived class.
        // Check each candidate class for the property's access restriction.
        if class_result.is_none() {
            return self.check_property_accessibility_via_brands(
                object_expr,
                property_name,
                error_node,
                object_type,
            );
        }

        let (class_idx, is_static) = class_result.expect("early return above handles None case");

        if self.is_super_expression(object_expr)
            && is_static
            && (self.find_enclosing_static_block(object_expr).is_some()
                || self.find_enclosing_static_block(error_node).is_some())
            && self.super_static_block_reads_base_expando(class_idx, property_name)
        {
            self.error_at_node(
                error_node,
                &format!("Property '{property_name}' is used before being assigned."),
                diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
            );
            return false;
        }

        // Mark the class member symbol as referenced for unused-variable tracking.
        // Property accesses like `this.x` go through the solver's property resolution
        // pipeline, which never marks binder symbols. Without this, private members
        // accessed via `this.x` would be falsely reported as unused (TS6133).
        if let Some(&class_sym_id) = self.ctx.binder.node_symbols.get(&class_idx.0)
            && let Some(class_symbol) = self.ctx.binder.get_symbol(class_sym_id)
            && let Some(ref members) = class_symbol.members
            && let Some(member_sym_id) = members.get(property_name)
        {
            self.ctx
                .referenced_symbols
                .borrow_mut()
                .insert(member_sym_id);
            // Also track in the property-specific set so TS6138 can distinguish
            // genuine property reads (this.x, destructuring of this) from
            // parameter variable references that get conflated during dedup.
            self.ctx
                .referenced_as_property
                .borrow_mut()
                .insert(member_sym_id);
        }

        let class_chain_summary = self.summarize_class_chain(class_idx);

        if self.is_super_expression(object_expr)
            && !is_static
            && matches!(
                class_chain_summary.member_kind(property_name, false, true),
                Some(ClassMemberKind::FieldLike)
            )
        {
            // When target < ES2022, useDefineForClassFields defaults to false and
            // super.prop for non-method members just works — tsc emits no error.
            // TS2855 only applies when useDefineForClassFields is effectively true
            // (target >= ES2022 or explicit useDefineForClassFields: true).
            if self.ctx.compiler_options.target.supports_es2022() {
                use crate::diagnostics::format_message;
                let display_name = class_chain_summary
                    .member_display_name(property_name, false, true)
                    .unwrap_or(property_name);
                let message = format_message(
                    diagnostic_messages::CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA,
                    &[display_name],
                );
                self.error_at_node(
                    error_node,
                    &message,
                    diagnostic_codes::CLASS_FIELD_DEFINED_BY_THE_PARENT_CLASS_IS_NOT_ACCESSIBLE_IN_THE_CHILD_CLASS_VIA,
                );
            }
            return false;
        }

        // For accessor properties with divergent visibility (e.g., public get /
        // private set), determine which accessor to check based on whether the
        // property access is in a write context (assignment LHS).
        let access_info = if let Some((getter_level, setter_level, decl_class_idx)) =
            self.find_accessor_levels_in_hierarchy(class_idx, property_name, is_static)
        {
            // Only apply context-aware checking when the accessor levels diverge.
            if getter_level != setter_level {
                let is_write = self.is_property_access_write_context(error_node);
                let level = if is_write { setter_level } else { getter_level };
                level.map(|lvl| crate::state::MemberAccessInfo {
                    level: lvl,
                    declaring_class_idx: decl_class_idx,
                    declaring_class_name: self
                        .get_class_name_with_type_params_from_decl(decl_class_idx),
                })
            } else {
                // Same level on both accessors — use the normal lookup.
                self.find_member_access_info(class_idx, property_name, is_static)
            }
        } else {
            self.find_member_access_info(class_idx, property_name, is_static)
        };

        let Some(access_info) = access_info else {
            return true;
        };

        let current_class_idx = self
            .ctx
            .enclosing_class
            .as_ref()
            .map(|info| info.class_idx)
            .or_else(|| self.nearest_enclosing_class(error_node));
        let protected_candidates =
            self.protected_access_candidate_classes(current_class_idx, object_expr);
        let mut protected_receiver_mismatch: Option<(NodeIndex, NodeIndex)> = None;
        let allowed = match access_info.level {
            MemberAccessLevel::Private => {
                current_class_idx == Some(access_info.declaring_class_idx)
            }
            MemberAccessLevel::Protected => {
                if !protected_candidates.is_empty() {
                    self.check_protected_access_allowed(
                        &protected_candidates,
                        &access_info,
                        is_static,
                        object_expr,
                        object_type,
                        &mut protected_receiver_mismatch,
                    )
                } else {
                    // In free functions with an explicit `this: Class` parameter,
                    // TypeScript allows protected access through contextual `this`.
                    self.is_this_expression(object_expr)
                        && self
                            .resolve_class_for_access(object_expr, object_type)
                            .is_some_and(|(receiver_class_idx, _)| {
                                receiver_class_idx == access_info.declaring_class_idx
                            })
                }
            }
        };

        if allowed {
            return true;
        }

        match access_info.level {
            MemberAccessLevel::Private => {
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
            MemberAccessLevel::Protected => {
                if let Some((current_idx, receiver_idx)) = protected_receiver_mismatch {
                    let current_name = self.get_class_name_with_type_params_from_decl(current_idx);
                    let receiver_name =
                        self.get_class_name_with_type_params_from_decl(receiver_idx);
                    let message = format!(
                        "Property '{property_name}' is protected and only accessible through an instance of class '{current_name}'. This is an instance of class '{receiver_name}'."
                    );
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_THROUGH_AN_INSTANCE_OF_CLASS_THIS_IS_A,
                    );
                } else {
                    let message = format!(
                        "Property '{}' is protected and only accessible within class '{}' and its subclasses.",
                        property_name, access_info.declaring_class_name
                    );
                    self.error_at_node(
                        error_node,
                        &message,
                        diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES,
                    );
                }
            }
        }

        false
    }

    fn super_static_block_reads_base_expando(
        &mut self,
        class_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        if matches!(
            self.summarize_class_chain(class_idx)
                .member_kind(property_name, true, true),
            Some(ClassMemberKind::MethodLike | ClassMemberKind::FieldLike)
        ) {
            return false;
        }

        let base_name = self.get_class_name_from_decl(class_idx);
        if base_name.is_empty() {
            return false;
        }

        let Some(source_file) = self.ctx.arena.source_files.first() else {
            return false;
        };

        source_file
            .statements
            .nodes
            .iter()
            .copied()
            .any(|stmt_idx| {
                let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                    return false;
                };
                let Some(expr_stmt) = self.ctx.arena.get_expression_statement(stmt_node) else {
                    return false;
                };
                let Some(expr_node) = self.ctx.arena.get(expr_stmt.expression) else {
                    return false;
                };
                let Some(binary) = self.ctx.arena.get_binary_expr(expr_node) else {
                    return false;
                };
                if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                    return false;
                }

                let Some(lhs_node) = self.ctx.arena.get(binary.left) else {
                    return false;
                };
                let Some(access) = self.ctx.arena.get_access_expr(lhs_node) else {
                    return false;
                };
                let Some(base_node) = self.ctx.arena.get(access.expression) else {
                    return false;
                };
                let Some(base_ident) = self.ctx.arena.get_identifier(base_node) else {
                    return false;
                };
                if base_ident.escaped_text != base_name {
                    return false;
                }

                self.ctx
                    .arena
                    .get(access.name_or_argument)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .is_some_and(|name_ident| name_ident.escaped_text == property_name)
            })
    }

    /// Check whether protected access is allowed from the given class context.
    ///
    /// This handles nested classes by walking lexical class ancestors.
    /// In TypeScript, when code is inside a nested class (e.g., `class B` inside
    /// `Derived1.method()`), protected access checks consider the outer enclosing
    /// classes, not just the innermost one. If the innermost class doesn't derive
    /// from the declaring class, we walk up to find the first outer class that does.
    ///
    /// This is what distinguishes TS2445 from TS2446:
    /// - TS2445: No enclosing class in the scope chain derives from the declaring class.
    /// - TS2446: An enclosing class derives from the declaring class, but the receiver
    ///   type doesn't match (wrong instance type).
    fn check_protected_access_allowed(
        &mut self,
        candidates: &[NodeIndex],
        access_info: &crate::state::MemberAccessInfo,
        is_static: bool,
        object_expr: NodeIndex,
        object_type: tsz_solver::TypeId,
        protected_receiver_mismatch: &mut Option<(NodeIndex, NodeIndex)>,
    ) -> bool {
        for &candidate_class_idx in candidates {
            if candidate_class_idx == access_info.declaring_class_idx {
                // We're inside the declaring class itself — always allowed.
                return true;
            }
            if !self.is_class_derived_from(candidate_class_idx, access_info.declaring_class_idx) {
                // This enclosing class doesn't derive from the declaring class; skip.
                continue;
            }
            // This class derives from the declaring class. Check the receiver.
            if is_static {
                // Static protected members: the current class extends the
                // declaring class, which is sufficient. No receiver check
                // needed because static access is through the class itself.
                return true;
            }
            let receiver_class_idx =
                self.resolve_receiver_class_for_access(object_expr, object_type);
            if let Some(receiver) = receiver_class_idx {
                if receiver == candidate_class_idx
                    || self.is_class_derived_from(receiver, candidate_class_idx)
                {
                    // Receiver IS the enclosing class or a subclass — allowed.
                    return true;
                } else {
                    // Receiver is a different class (parent, sibling, or unrelated).
                    // This is the TS2446 case: "protected and only accessible through
                    // an instance of class X. This is an instance of class Y."
                    protected_receiver_mismatch.get_or_insert((candidate_class_idx, receiver));
                    continue;
                }
            } else {
                // Can't resolve receiver — deny access.
                return false;
            }
        }

        // No enclosing class derives from the declaring class → TS2445.
        false
    }

    fn protected_access_candidate_classes(
        &self,
        current_class_idx: Option<NodeIndex>,
        access_site_idx: NodeIndex,
    ) -> Vec<NodeIndex> {
        let mut candidates = current_class_idx.into_iter().collect::<Vec<_>>();
        let mut parent = self
            .ctx
            .arena
            .get_extended(access_site_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);

        while parent.is_some() {
            if let Some(node) = self.ctx.arena.get(parent)
                && matches!(
                    node.kind,
                    syntax_kind_ext::CLASS_DECLARATION | syntax_kind_ext::CLASS_EXPRESSION
                )
                && !candidates.contains(&parent)
            {
                candidates.push(parent);
            }
            parent = self
                .ctx
                .arena
                .get_extended(parent)
                .map(|ext| ext.parent)
                .unwrap_or(NodeIndex::NONE);
        }

        for &class_idx in self.ctx.enclosing_class_chain.iter().rev() {
            if !candidates.contains(&class_idx) {
                candidates.push(class_idx);
            }
        }

        candidates
    }

    /// Check property accessibility by examining brand properties on the type.
    ///
    /// Used as a fallback when `resolve_class_for_access` returns `None` because
    /// the object type has multiple unrelated base classes (e.g., an interface
    /// extending two unrelated classes). We check each candidate class for the
    /// property's access restriction.
    fn check_property_accessibility_via_brands(
        &mut self,
        _object_expr: NodeIndex,
        property_name: &str,
        error_node: NodeIndex,
        object_type: tsz_solver::TypeId,
    ) -> bool {
        use crate::diagnostics::diagnostic_codes;

        // Collect all candidate classes from brand properties.
        let candidates = self.collect_brand_class_candidates(object_type);
        if candidates.is_empty() {
            return true;
        }

        let is_static = self.is_constructor_type(object_type);
        let current_class_idx = self.ctx.enclosing_class.as_ref().map(|info| info.class_idx);

        // Check each candidate class for the property. If the property is found
        // as restricted in any candidate, check accessibility against that class.
        for class_idx in &candidates {
            let Some(access_info) =
                self.find_member_access_info(*class_idx, property_name, is_static)
            else {
                continue;
            };

            let allowed = match access_info.level {
                MemberAccessLevel::Private => {
                    current_class_idx == Some(access_info.declaring_class_idx)
                }
                MemberAccessLevel::Protected => match current_class_idx {
                    None => false,
                    Some(cur) => {
                        cur == access_info.declaring_class_idx
                            || self.is_class_derived_from(cur, access_info.declaring_class_idx)
                    }
                },
            };

            if !allowed {
                match access_info.level {
                    MemberAccessLevel::Private => {
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
                    MemberAccessLevel::Protected => {
                        let message = format!(
                            "Property '{}' is protected and only accessible within class '{}' and its subclasses.",
                            property_name, access_info.declaring_class_name
                        );
                        self.error_at_node(
                            error_node,
                            &message,
                            diagnostic_codes::PROPERTY_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_CLASS_AND_ITS_SUBCLASSES,
                        );
                    }
                }
                return false;
            }
        }

        true
    }

    /// Collect all class declaration candidates from brand properties on a type.
    fn collect_brand_class_candidates(&self, object_type: tsz_solver::TypeId) -> Vec<NodeIndex> {
        let mut candidates = Vec::new();

        fn parse_brand_name(name: &str) -> Option<Result<tsz_binder::SymbolId, NodeIndex>> {
            const NODE_PREFIX: &str = "__private_brand_node_";
            const PREFIX: &str = "__private_brand_";

            if let Some(rest) = name.strip_prefix(NODE_PREFIX) {
                let node_id: u32 = rest.parse().ok()?;
                return Some(Err(NodeIndex(node_id)));
            }
            if let Some(rest) = name.strip_prefix(PREFIX) {
                let sym_id: u32 = rest.parse().ok()?;
                return Some(Ok(tsz_binder::SymbolId(sym_id)));
            }
            None
        }

        fn collect<'a>(
            checker: &CheckerState<'a>,
            type_id: tsz_solver::TypeId,
            out: &mut Vec<NodeIndex>,
        ) {
            match classify_for_class_decl(checker.ctx.types, type_id) {
                ClassDeclTypeKind::Object(shape_id) => {
                    let shape = checker.ctx.types.object_shape(shape_id);
                    for prop in &shape.properties {
                        let name = checker.ctx.types.resolve_atom_ref(prop.name);
                        if let Some(parsed) = parse_brand_name(&name) {
                            let class_idx = match parsed {
                                Ok(sym_id) => checker.get_class_declaration_from_symbol(sym_id),
                                Err(node_idx) => Some(node_idx),
                            };
                            if let Some(class_idx) = class_idx {
                                out.push(class_idx);
                            }
                        }
                    }
                }
                ClassDeclTypeKind::Members(members) => {
                    for member in members {
                        collect(checker, member, out);
                    }
                }
                ClassDeclTypeKind::NotObject => {}
            }
        }

        collect(self, object_type, &mut candidates);
        candidates
    }

    /// Find accessor visibility levels in a class hierarchy for divergent get/set.
    /// Returns (`getter_level`, `setter_level`, `declaring_class_idx`) if the property
    /// has accessors with different visibility levels.
    const fn find_accessor_levels_in_hierarchy(
        &mut self,
        _class_idx: NodeIndex,
        _property_name: &str,
        _is_static: bool,
    ) -> Option<(
        Option<MemberAccessLevel>,
        Option<MemberAccessLevel>,
        NodeIndex,
    )> {
        // TODO: Implement accessor level checking across class hierarchy
        None
    }

    /// Determine if a property access is in a write context (LHS of assignment).
    fn is_property_access_write_context(&self, error_node: NodeIndex) -> bool {
        // error_node is the property name (identifier). Walk up to the property
        // access expression, then check if its parent is an assignment.
        let Some(ext) = self.ctx.arena.get_extended(error_node) else {
            return false;
        };
        // Parent should be the property access expression.
        let prop_access_idx = ext.parent;
        let Some(prop_ext) = self.ctx.arena.get_extended(prop_access_idx) else {
            return false;
        };
        // Grandparent should be the binary expression (assignment).
        let grandparent_idx = prop_ext.parent;
        let Some(grandparent_node) = self.ctx.arena.get(grandparent_idx) else {
            return false;
        };
        if grandparent_node.kind != tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION {
            // Also check for prefix/postfix increment/decrement.
            if (grandparent_node.kind
                == tsz_parser::parser::syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                || grandparent_node.kind
                    == tsz_parser::parser::syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
                && let Some(unary) = self.ctx.arena.get_unary_expr(grandparent_node)
            {
                return unary.operator == tsz_scanner::SyntaxKind::PlusPlusToken as u16
                    || unary.operator == tsz_scanner::SyntaxKind::MinusMinusToken as u16;
            }
            return false;
        }
        let Some(binary) = self.ctx.arena.get_binary_expr(grandparent_node) else {
            return false;
        };
        // Check if the property access is on the LHS and the operator is assignment.
        if binary.left != prop_access_idx {
            return false;
        }
        self.is_assignment_operator(binary.operator_token)
    }

    /// Check if a union type has a property that should be treated as "not existing"
    /// because one or more members have it as private/protected while other members
    /// have it publicly or from a different declaring class.
    ///
    /// Matches tsc's `createUnionOrIntersectionProperty` logic: when a property has a
    /// private/protected declaration in one constituent but is missing, public, or has
    /// a different declaration in another constituent, the property doesn't exist on
    /// the union type (TS2339) rather than getting a specific accessibility error.
    fn union_restricted_property_is_missing(
        &mut self,
        _object_expr: NodeIndex,
        property_name: &str,
        object_type: tsz_solver::TypeId,
    ) -> bool {
        use crate::query_boundaries::state::checking;

        if self.ctx.enclosing_class.is_some() {
            return false;
        }

        let Some(members) = checking::union_members(self.ctx.types, object_type) else {
            return false;
        };

        if members.len() < 2 {
            return false;
        }

        let is_static = self.is_constructor_type(object_type);

        let mut has_restricted = false;
        let mut has_other = false;
        let mut first_declaring_class: Option<NodeIndex> = None;

        for member in members {
            let member = self.resolve_type_for_property_access(member);
            let Some(class_idx) = self.get_class_decl_from_type(member) else {
                // Non-class member in the union (e.g., object literal type).
                // Treated as a different declaration from any class member.
                has_other = true;
                continue;
            };

            match self.find_member_access_info(class_idx, property_name, is_static) {
                Some(access_info) => {
                    // Property is restricted (private/protected) in this member
                    has_restricted = true;
                    if let Some(first_decl) = first_declaring_class {
                        if first_decl != access_info.declaring_class_idx {
                            // Different declaring class — counts as "other"
                            has_other = true;
                        }
                    } else {
                        first_declaring_class = Some(access_info.declaring_class_idx);
                    }
                }
                None => {
                    // Property is public or not found in this class member
                    has_other = true;
                }
            }
        }

        // If any member has a restricted property and there's at least one member
        // with a different declaration (public, missing, or different class),
        // the property doesn't exist on the union type.
        has_restricted && has_other
    }
    // =========================================================================
    // Computed Property Name Validation
    // =========================================================================

    /// Check if an expression node is an "entity name expression".
    ///
    /// Entity name expressions are simple identifiers or property access chains
    /// (e.g., `a`, `a.b`, `a.b.c`). These are always allowed as computed property
    /// names in class property declarations, regardless of their type.
    fn is_entity_name_expression(&self, expr_idx: NodeIndex) -> bool {
        let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
            return false;
        };
        if expr_node.kind == SyntaxKind::Identifier as u16 {
            return true;
        }
        if expr_node.kind == tsz_parser::parser::syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && let Some(access) = self.ctx.arena.get_access_expr(expr_node)
        {
            return self.is_entity_name_expression(access.expression);
        }
        false
    }

    /// Check a computed property name requires a simple literal or unique symbol type.
    ///
    /// Used for TS1166 (class properties), TS1169 (interfaces), and TS1170 (type literals).
    /// Entity name expressions (identifiers, property access chains) and literal
    /// expressions are always allowed. Other expressions must have a literal or
    /// unique symbol type.
    fn check_computed_property_requires_literal(
        &mut self,
        name_idx: NodeIndex,
        message: &str,
        code: u32,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        if name_node.kind != tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        let is_entity_name = self.is_entity_name_expression(computed.expression);

        // Literal expressions (string, number, no-substitution template) are always OK
        // since they inherently have literal types
        if let Some(expr_node) = self.ctx.arena.get(computed.expression) {
            let kind = expr_node.kind;
            if kind == SyntaxKind::StringLiteral as u16
                || kind == SyntaxKind::NumericLiteral as u16
                || kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16
            {
                return;
            }
        }

        // Entity name expressions (identifiers, property access chains) are always
        // structurally OK for computed property names — skip the TS1166/TS1169 error.
        if is_entity_name {
            return;
        }

        // Assignment expressions (e.g., `x = ''`, `x = 0`) are never allowed as computed
        // property names in classes/interfaces/type literals. They should emit TS1166/TS1169
        // regardless of whether the type is valid for property names.
        // Check if this is a binary expression with an equals operator (assignment).
        if let Some(expr_node) = self.ctx.arena.get(computed.expression)
            && expr_node.kind == tsz_parser::parser::syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(expr_node)
            && binary.operator_token == SyntaxKind::EqualsToken as u16
        {
            self.error_at_node(name_idx, message, code);
            return;
        }

        // Always evaluate the expression to trigger side-effect diagnostics (e.g., TS2585
        // for `Symbol` at ES5 target). Entity name expressions skip the TS1169/TS1170
        // structural error but still need type evaluation.
        let expr_type = self.get_type_of_node(computed.expression);

        let emitted_computed_this_missing =
            self.report_computed_this_property_missing(computed.expression);

        if expr_type == tsz_solver::TypeId::ERROR {
            if emitted_computed_this_missing {
                self.error_at_node(name_idx, message, code);
            }
            return;
        }

        if emitted_computed_this_missing {
            self.error_at_node(name_idx, message, code);
            return;
        }

        if !query::is_type_usable_as_property_name(self.ctx.types, expr_type) {
            self.error_at_node(name_idx, message, code);
        }
    }

    /// Check a computed property name in a class property declaration (TS1166).
    pub(crate) fn check_class_computed_property_name(&mut self, name_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.check_computed_property_requires_literal(
            name_idx,
            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE,
            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITE,
        );
    }

    /// Check a computed property name in an interface (TS1169).
    pub(crate) fn check_interface_computed_property_name(&mut self, name_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.check_computed_property_requires_literal(
            name_idx,
            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE,
            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_AN_INTERFACE_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYPE,
        );
    }

    /// Check a computed property name in a type literal (TS1170).
    pub(crate) fn check_type_literal_computed_property_name(&mut self, name_idx: NodeIndex) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.check_computed_property_requires_literal(
            name_idx,
            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_IN_A_TYPE_LITERAL_MUST_REFER_TO_AN_EXPRESSION_WHOSE_TYP,
        );
    }

    /// Check a computed property name for type errors (TS2464).
    ///
    /// Validates that the expression used for a computed property name
    /// has a type that is string, number, symbol, or any (including literals).
    /// This check is independent of strictNullChecks.
    pub(crate) fn check_computed_property_name(&mut self, name_idx: NodeIndex) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };

        if name_node.kind != tsz_parser::parser::syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }

        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        // TS1212/TS1213: Check if the computed expression is a strict mode reserved word.
        // E.g., `{ [public]: 0 }` should emit TS1212 in strict mode.
        // Only emit if the parser didn't already handle it (the parser emits TS1213
        // for class member computed property names with contextual keywords).
        if !self.has_parse_errors() {
            self.check_strict_mode_reserved_name_at(computed.expression, name_idx);
        }

        // Contextual keywords (public, private, protected, etc.) are parsed as keyword
        // tokens, not Identifier nodes. The type dispatch table doesn't route them to
        // get_type_of_identifier, so they silently return ERROR without emitting TS2304.
        // Detect this case and explicitly resolve them as identifiers.
        self.ctx.checking_computed_property_name = Some(name_idx);
        let expr_type = if let Some(expr_node) = self.ctx.arena.get(computed.expression)
            && expr_node.kind != tsz_scanner::SyntaxKind::Identifier as u16
            && self.ctx.arena.get_identifier(expr_node).is_some()
        {
            // Keyword token with identifier data — resolve as identifier for TS2304
            self.get_type_of_identifier(computed.expression)
        } else {
            self.get_type_of_node(computed.expression)
        };
        self.ctx.checking_computed_property_name = None;

        // Skip error types to avoid cascading diagnostics
        if expr_type == tsz_solver::TypeId::ERROR {
            return;
        }

        // Enum objects (e.g. `E` or `Ns.E`) are values, not property-name types.
        // Enum members remain valid because their resolved symbols are enum members,
        // not the enum container itself.
        let enum_object_ref = self
            .resolve_identifier_symbol(computed.expression)
            .or_else(|| self.resolve_qualified_symbol(computed.expression))
            .map(|sym_id| {
                self.resolve_alias_symbol(sym_id, &mut Vec::new())
                    .unwrap_or(sym_id)
            })
            .and_then(|sym_id| self.ctx.binder.get_symbol(sym_id))
            .is_some_and(|symbol| {
                (symbol.flags & tsz_binder::symbol_flags::ENUM) != 0
                    && (symbol.flags & tsz_binder::symbol_flags::ENUM_MEMBER) == 0
            });

        // TS2464: type must be string, number, symbol, or any (including literals).
        // This check ignores strictNullChecks: undefined/null always fail.
        // Suppress this diagnostic in files with parse errors to avoid noise (e.g., [await] without operand).
        // Resolve lazy types before validation: Lazy(DefId) types (e.g., Symbol interface
        // from lib.d.ts) can't be evaluated by the solver's interner alone and would be
        // conservatively accepted. Resolving them here ensures boxed wrapper types like
        // Symbol/Number/String are correctly rejected as computed property name types.
        let resolved_type = self.resolve_lazy_type(expr_type);
        let evaluator = tsz_solver::BinaryOpEvaluator::new(self.ctx.types);
        if !self.has_parse_errors()
            && (enum_object_ref || !evaluator.is_valid_computed_property_name_type(resolved_type))
        {
            use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
            self.error_at_node(
                name_idx,
                diagnostic_messages::A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY,
                diagnostic_codes::A_COMPUTED_PROPERTY_NAME_MUST_BE_OF_TYPE_STRING_NUMBER_SYMBOL_OR_ANY,
            );
        }
    }

    // =========================================================================
    // Const Modifier Checking
    // =========================================================================

    /// Get the const modifier node from a list of modifiers, if present.
    ///
    /// Returns the `NodeIndex` of the const modifier for error reporting.
    /// Used to validate that readonly properties cannot have initializers.
    pub(crate) fn get_const_modifier(
        &self,
        modifiers: &Option<tsz_parser::parser::NodeList>,
    ) -> Option<NodeIndex> {
        if let Some(mods) = modifiers {
            for &mod_idx in &mods.nodes {
                if let Some(mod_node) = self.ctx.arena.get(mod_idx)
                    && mod_node.kind == SyntaxKind::ConstKeyword as u16
                {
                    return Some(mod_idx);
                }
            }
        }
        None
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn check_type_parameter_reference_for_computed_property(
        &mut self,
        name: &str,
        type_name_idx: tsz_parser::parser::NodeIndex,
    ) {
        let Some(name_idx) = self.ctx.checking_computed_property_name else {
            return;
        };

        // Only apply this check to names that belong to the currently tracked
        // computed property expression. Recursive type instantiation of the same
        // class can resolve other nodes while this flag is set (e.g. `x: T` inside
        // the class body) and should not trigger TS2467.
        let mut current = Some(type_name_idx);
        let mut is_within_computed_name = false;
        while let Some(idx) = current {
            if idx == name_idx {
                is_within_computed_name = true;
                break;
            }
            let Some(ext) = self.ctx.arena.get_extended(idx) else {
                break;
            };
            current = Some(ext.parent);
        }
        if !is_within_computed_name {
            return;
        }

        let mut enclosing_decl = None;
        let mut current = Some(name_idx);
        while let Some(idx) = current {
            let Some(ext) = self.ctx.arena.get_extended(idx) else {
                break;
            };
            let Some(parent) = self.ctx.arena.get(ext.parent) else {
                break;
            };
            if parent.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION
                || parent.kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION
                || parent.kind == tsz_parser::parser::syntax_kind_ext::CLASS_EXPRESSION
            {
                enclosing_decl = Some(ext.parent);
                break;
            }
            current = Some(ext.parent);
        }

        let Some(decl_idx) = enclosing_decl else {
            return;
        };

        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return;
        };
        let type_params_list =
            if decl_node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_DECLARATION {
                self.ctx
                    .arena
                    .get_class(decl_node)
                    .and_then(|c| c.type_parameters.as_ref())
            } else if decl_node.kind == tsz_parser::parser::syntax_kind_ext::INTERFACE_DECLARATION {
                self.ctx
                    .arena
                    .get_interface(decl_node)
                    .and_then(|i| i.type_parameters.as_ref())
            } else if decl_node.kind == tsz_parser::parser::syntax_kind_ext::CLASS_EXPRESSION {
                self.ctx
                    .arena
                    .get_class(decl_node)
                    .and_then(|c| c.type_parameters.as_ref())
            } else {
                None
            };

        if let Some(list) = type_params_list {
            for &tp_idx in &list.nodes {
                if let Some(tp_node) = self.ctx.arena.get(tp_idx)
                    && let Some(tp) = self.ctx.arena.get_type_parameter(tp_node)
                    && let Some(name_node) = self.ctx.arena.get(tp.name)
                    && let Some(ident) = self.ctx.arena.get_identifier(name_node)
                    && ident.escaped_text == name
                {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    self.error_at_node(
                            type_name_idx,
                            diagnostic_messages::A_COMPUTED_PROPERTY_NAME_CANNOT_REFERENCE_A_TYPE_PARAMETER_FROM_ITS_CONTAINING_T,
                            diagnostic_codes::A_COMPUTED_PROPERTY_NAME_CANNOT_REFERENCE_A_TYPE_PARAMETER_FROM_ITS_CONTAINING_T,
                    );
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    fn check_diagnostics(source: &str) -> Vec<u32> {
        crate::test_utils::check_source_codes(source)
    }

    fn has_code(diagnostics: &[u32], code: u32) -> bool {
        diagnostics.contains(&code)
    }

    #[test]
    fn union_restricted_property_access_missing_member_emits_ts2339() {
        let diagnostics = check_diagnostics(
            r#"
            class A {
                readonly x: number = 0;
            }
            class B {
                y: string = "";
            }
            let value: A | B;
            value.x;
        "#,
        );

        assert!(has_code(&diagnostics, 2339));
    }

    #[test]
    fn union_restricted_property_access_same_declaring_class_is_allowed() {
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                x: number = 0;
            }
            class Derived extends Base {
                y: string = "";
            }
            let value: Base | Derived;
            value.x;
        "#,
        );

        assert!(!has_code(&diagnostics, 2339));
    }

    #[test]
    fn union_restricted_property_access_different_decls_emits_ts2339() {
        let diagnostics = check_diagnostics(
            r#"
            class A {
                private x: number = 0;
            }
            class B {
                private x: number = 1;
            }
            let value: A | B;
            value.x;
        "#,
        );

        assert!(has_code(&diagnostics, 2339));
    }

    /// When a union has one public member and one protected member,
    /// TSC treats the property as "not existing" on the union (TS2339).
    /// Previously this was order-dependent and emitted TS2445 instead.
    #[test]
    fn union_public_and_protected_emits_ts2339_not_ts2445() {
        let diagnostics = check_diagnostics(
            r#"
            class Default {
                member: string = "";
            }
            class Protected {
                protected member: string = "";
            }
            declare var v: Default | Protected;
            v.member;
        "#,
        );

        assert!(
            has_code(&diagnostics, 2339),
            "expected TS2339 for union with public + protected"
        );
        assert!(
            !has_code(&diagnostics, 2445),
            "should NOT emit TS2445 for union type"
        );
    }

    /// When a union has one public member and one private member,
    /// TSC emits TS2339, not TS2341.
    #[test]
    fn union_public_and_private_emits_ts2339_not_ts2341() {
        let diagnostics = check_diagnostics(
            r#"
            class Public {
                public member: string = "";
            }
            class Private {
                private member: number = 0;
            }
            declare var v: Public | Private;
            v.member;
        "#,
        );

        assert!(
            has_code(&diagnostics, 2339),
            "expected TS2339 for union with public + private"
        );
        assert!(
            !has_code(&diagnostics, 2341),
            "should NOT emit TS2341 for union type"
        );
    }

    /// Three-member union with mix of public, protected, private.
    /// All should get TS2339.
    #[test]
    fn union_three_member_mixed_access_emits_ts2339() {
        let diagnostics = check_diagnostics(
            r#"
            class Default { member: string = ""; }
            class Public { public member: string = ""; }
            class Protected { protected member: string = ""; }
            declare var v: Default | Public | Protected;
            v.member;
        "#,
        );

        assert!(has_code(&diagnostics, 2339));
        assert!(!has_code(&diagnostics, 2445));
    }

    /// Union of two public members — no error expected.
    #[test]
    fn union_both_public_no_error() {
        let diagnostics = check_diagnostics(
            r#"
            class A { member: string = ""; }
            class B { public member: string = ""; }
            declare var v: A | B;
            v.member;
        "#,
        );

        assert!(!has_code(&diagnostics, 2339));
        assert!(!has_code(&diagnostics, 2445));
        assert!(!has_code(&diagnostics, 2341));
    }

    // =========================================================================
    // TS2446: Protected access through wrong instance type in nested classes
    // =========================================================================

    #[test]
    fn nested_class_protected_access_through_correct_instance_is_allowed() {
        // Inside Derived1.method, nested class B accesses protected x through
        // a Derived1 instance — should be allowed (no error).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived1 extends Base {
                method() {
                    class B {
                        test() {
                            var d1: Derived1 = undefined as any;
                            d1.x;
                        }
                    }
                }
            }
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2445),
            "Should not emit TS2445 for access through correct instance, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2446),
            "Should not emit TS2446 for access through correct instance, got: {diagnostics:?}"
        );
    }

    #[test]
    fn nested_class_protected_access_through_wrong_instance_emits_ts2446() {
        // Inside Derived1.method, nested class B accesses protected x through
        // a Base instance — should emit TS2446 (wrong instance type).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived1 extends Base {
                method() {
                    class B {
                        test() {
                            var b: Base = undefined as any;
                            b.x;
                        }
                    }
                }
            }
        "#,
        );

        assert!(
            has_code(&diagnostics, 2446),
            "Expected TS2446 for access through Base instance inside nested class, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2445),
            "Should emit TS2446 not TS2445 for wrong-instance access, got: {diagnostics:?}"
        );
    }

    #[test]
    fn nested_class_protected_access_through_sibling_emits_ts2446() {
        // Inside Derived1.method, nested class B accesses protected x through
        // a Derived2 instance — should emit TS2446 (sibling class, wrong instance).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived1 extends Base {
                method() {
                    class B {
                        test() {
                            var d2: Derived2 = undefined as any;
                            d2.x;
                        }
                    }
                }
            }
            class Derived2 extends Base {}
        "#,
        );

        assert!(
            has_code(&diagnostics, 2446),
            "Expected TS2446 for access through sibling instance, got: {diagnostics:?}"
        );
    }

    #[test]
    fn inherited_static_member_property_access_emits_ts2576() {
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                static count = 1;
                static get size() {
                    return 2;
                }
            }
            class Derived extends Base {}
            const value = new Derived();
            value.count;
            value.size;
        "#,
        );

        assert_eq!(
            diagnostics.iter().filter(|&&code| code == 2576).count(),
            2,
            "Expected TS2576 for inherited static field and accessor property access, got: {diagnostics:?}"
        );
    }

    #[test]
    fn nested_class_protected_access_through_subclass_instance_is_allowed() {
        // Inside Derived2.method, nested class C accesses protected x through
        // a Derived4 instance (which extends Derived2) — should be allowed.
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            class Derived2 extends Base {
                method() {
                    class C {
                        test() {
                            var d4: Derived4 = undefined as any;
                            d4.x;
                        }
                    }
                }
            }
            class Derived4 extends Derived2 {}
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2445),
            "Should allow access through subclass instance, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2446),
            "Should allow access through subclass instance, got: {diagnostics:?}"
        );
    }

    #[test]
    fn non_nested_class_protected_access_outside_hierarchy_emits_ts2445() {
        // Outside any derived class, accessing protected member should emit TS2445.
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
            }
            var b: Base = undefined as any;
            b.x;
        "#,
        );

        assert!(
            has_code(&diagnostics, 2445),
            "Expected TS2445 for access outside class hierarchy, got: {diagnostics:?}"
        );
    }

    #[test]
    fn nested_class_declaring_class_allows_access() {
        // Inside Base.method, nested class A accesses protected x through
        // a Base instance — should be allowed (we're in the declaring class).
        let diagnostics = check_diagnostics(
            r#"
            class Base {
                protected x: string = "";
                method() {
                    class A {
                        test() {
                            var b: Base = undefined as any;
                            b.x;
                        }
                    }
                }
            }
        "#,
        );

        assert!(
            !has_code(&diagnostics, 2445),
            "Should allow access from nested class inside declaring class, got: {diagnostics:?}"
        );
        assert!(
            !has_code(&diagnostics, 2446),
            "Should allow access from nested class inside declaring class, got: {diagnostics:?}"
        );
    }

    #[test]
    fn nested_class_full_hierarchy_emits_correct_errors() {
        // Mirrors the conformance test pattern: Base > Derived1 with nested classes.
        // Inside Base.method > class A: access to b.x is OK (declaring class scope).
        // Inside Derived1.method > class B: b.x should be TS2446 (wrong instance),
        // d1.x should be OK (correct instance).
        let diagnostics = crate::test_utils::check_source_diagnostics(
            r#"
class Base {
    protected x!: string;
    method() {
        class A {
            methoda() {
                var b: Base = undefined as any;
                var d1: Derived1 = undefined as any;
                b.x;
                d1.x;
            }
        }
    }
}

class Derived1 extends Base {
    method1() {
        class B {
            method1b() {
                var b: Base = undefined as any;
                var d1: Derived1 = undefined as any;
                b.x;
                d1.x;
            }
        }
    }
}
"#,
        );
        let codes: Vec<u32> = diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&2446),
            "Expected TS2446 for b.x inside nested class in Derived1, got: {codes:?}"
        );
        // Should not have TS2445 for the b.x in Derived1's nested class (it should be TS2446)
        // The only TS2445 errors should be from outside the class hierarchy if any
    }
}
