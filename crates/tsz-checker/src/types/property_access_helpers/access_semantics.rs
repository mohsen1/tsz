//! Property access semantic helpers: prototype reads, write target detection,
//! flow analysis, scope helpers, class/object member checks, union/type-parameter
//! property checks, strict bind/call/apply method synthesis, import.meta CJS
//! checks, and const expando key resolution.

use crate::FlowAnalyzer;
use crate::query_boundaries::common::TypeResolver;
use crate::query_boundaries::property_access as property_access_query;
use crate::state::CheckerState;
use crate::symbols_domain::alias_cycle::AliasCycleTracker;
use crate::symbols_domain::name_text::property_access_chain_text_in_arena;
use tsz_binder::symbol_flags;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(in crate::types_domain::property_access_helpers) fn is_js_prototype_read_root(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }

        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let Some(member_node) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let is_prototype = member_node.kind == SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(member_node)
                .is_some_and(|ident| ident.escaped_text == "prototype");
        if !is_prototype {
            return false;
        }

        let Some(root_name) = self.expression_text(access.expression) else {
            return false;
        };

        if self.class_has_instance_member(&root_name, property_name) {
            return false;
        }

        let Some(sym_id) = self.resolve_identifier_symbol(access.expression) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // An ES `class`'s prototype is the closed instance type: a missing
        // member read through it is an ordinary TS2339, exactly like
        // `new C().x`. Checked-JS prototype-expando opening (which lets a
        // read-before-assignment return `any` instead of reporting) applies
        // only to function-as-constructor receivers, matching the sibling
        // `expando_receiver_is_function_constructor` predicate.
        let is_function = (symbol.flags & symbol_flags::FUNCTION) != 0;
        let is_class = (symbol.flags & symbol_flags::CLASS) != 0;
        is_function && !is_class
    }

    /// Whether `access_expr` (the receiver of a `.prototype.X` access) refers
    /// to a function-as-constructor binding rather than an ES `class`. tsc
    /// treats `function C() {}` as an expando-friendly constructor where
    /// late-attaching a JSDoc-typed prototype property is a declaration; for
    /// `class C {}` the prototype shape is the class instance type and a
    /// late attachment is genuinely "used before assigned".
    /// Whether an expando receiver is a declaration whose property assignments
    /// `tsc` treats as **ordered**.
    ///
    /// `function C() {} C.f(); C.f = a;` reports TS2565 in tsc: the expando is a
    /// declaration on the function, so using it before the assignment is an
    /// error. A plain object (`var o = {}`) or a CommonJS `exports` object is
    /// not ordered — tsc types those from every assignment in the program
    /// regardless of position and reports nothing, so a use that textually
    /// precedes the assignment is fine.
    pub(crate) fn expando_root_has_ordered_declarations(&mut self, access_expr: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(access_expr) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol(access_expr) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let decl_idx = symbol.value_declaration;
        let Some(decl) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        match decl.kind {
            syntax_kind_ext::FUNCTION_DECLARATION | syntax_kind_ext::CLASS_DECLARATION => true,
            syntax_kind_ext::VARIABLE_DECLARATION => self
                .ctx
                .arena
                .get_variable_declaration(decl)
                .and_then(|var_decl| self.ctx.arena.get(var_decl.initializer))
                .is_some_and(|init| init.is_function_expression_or_arrow()),
            _ => false,
        }
    }

    /// Whether `access_expr` is the CommonJS `exports`/`module.exports`
    /// object and `property_name` has a non-aliasable direct assignment
    /// (see `commonjs_export_property_has_non_aliasable_assignment`) — the
    /// per-property counterpart of `expando_root_has_ordered_declarations`
    /// for the one receiver kind (`exports`/`module.exports`) that is never
    /// itself a function/class declaration, so ordering must be decided per
    /// assigned property instead of per receiver.
    pub(crate) fn commonjs_export_property_is_ordered(
        &self,
        access_expr: NodeIndex,
        property_name: &str,
    ) -> bool {
        self.current_file_commonjs_exports_target_is_unshadowed(access_expr)
            && self.commonjs_export_property_has_non_aliasable_assignment(property_name)
    }

    /// Same-file `exports.NAME`/`module.exports.NAME` read fast path.
    ///
    /// An ordered property (`commonjs_export_property_is_ordered`) is typed
    /// from the last assignment textually before `property_access_idx`, not
    /// from the last assignment in the whole file — same flow-sensitivity as
    /// the function/class-declaration-receiver expando case. This fast path
    /// skips the general ordering check in the caller, so an ordered
    /// property needs its own TS2565 check here.
    pub(crate) fn current_file_commonjs_export_property_read_type(
        &mut self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
        name_node: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        let is_ordered = self.commonjs_export_property_is_ordered(object_expr_idx, property_name);
        let prior_type = if is_ordered {
            let read_pos = self
                .ctx
                .arena
                .pos_at(property_access_idx)
                .unwrap_or(u32::MAX);
            self.current_file_commonjs_prior_named_export_type(property_name, read_pos)
        } else {
            self.current_file_commonjs_named_export_type(property_name)
        };
        let prior_type = prior_type?;
        if is_ordered
            && self.expando_property_read_before_assignment(
                property_access_idx,
                object_expr_idx,
                property_name,
            )
        {
            self.report_property_used_before_assigned(name_node, property_name);
        }
        Some(prior_type)
    }

    /// Report TS2565 "Property '{0}' is used before being assigned." at
    /// `name_node`. Shared by the two `exports`/`module.exports` property-read
    /// call sites: the `current_file_commonjs_named_export_type` fast path
    /// (which would otherwise return before ever reaching the general expando
    /// ordering check) and that general check itself.
    pub(crate) fn report_property_used_before_assigned(
        &mut self,
        name_node: NodeIndex,
        property_name: &str,
    ) {
        use crate::diagnostics::format_message;
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        self.error_at_node(
            name_node,
            &format_message(
                diagnostic_messages::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
                &[property_name],
            ),
            diagnostic_codes::PROPERTY_IS_USED_BEFORE_BEING_ASSIGNED,
        );
    }

    pub(crate) fn expando_receiver_is_function_constructor(&self, access_expr: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(access_expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let Some(member) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let is_prototype = member.kind == SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(member)
                .is_some_and(|ident| ident.escaped_text == "prototype");
        if !is_prototype {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol(access.expression) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let is_function = (symbol.flags & symbol_flags::FUNCTION) != 0;
        let is_class = (symbol.flags & symbol_flags::CLASS) != 0;
        is_function && !is_class
    }

    /// Whether `access_expr` is a `C.prototype` receiver where `C` resolves to
    /// an ES `class` declaration. A class's prototype is the closed instance
    /// type, so unlike a function-as-constructor's expando prototype, a bare
    /// JSDoc-commented read of one of its members is not a declaration site —
    /// the member either already exists on the class or the access is an
    /// ordinary missing-member error.
    pub(crate) fn expando_receiver_is_class(&self, access_expr: NodeIndex) -> bool {
        let Some(node) = self.ctx.arena.get(access_expr) else {
            return false;
        };
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            return false;
        }
        let Some(access) = self.ctx.arena.get_access_expr(node) else {
            return false;
        };
        let Some(member) = self.ctx.arena.get(access.name_or_argument) else {
            return false;
        };
        let is_prototype = member.kind == SyntaxKind::Identifier as u16
            && self
                .ctx
                .arena
                .get_identifier(member)
                .is_some_and(|ident| ident.escaped_text == "prototype");
        if !is_prototype {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol(access.expression) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        (symbol.flags & symbol_flags::CLASS) != 0
    }

    pub(crate) fn property_access_is_write_target_or_base(
        &self,
        property_access_idx: NodeIndex,
    ) -> bool {
        let mut current = property_access_idx;

        loop {
            let Some(prop_ext) = self.ctx.arena.get_extended(current) else {
                return false;
            };
            let parent_idx = prop_ext.parent;
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
                && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
                && access.expression == current
            {
                current = parent_idx;
                continue;
            }

            if parent_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
                    || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
                    && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
                {
                    return unary.operator == SyntaxKind::PlusPlusToken as u16
                        || unary.operator == SyntaxKind::MinusMinusToken as u16;
                }
                return false;
            }

            let Some(binary) = self.ctx.arena.get_binary_expr(parent_node) else {
                return false;
            };
            return binary.left == current && self.is_assignment_operator(binary.operator_token);
        }
    }

    pub(crate) fn property_access_is_direct_write_target(
        &self,
        property_access_idx: NodeIndex,
    ) -> bool {
        let Some(prop_ext) = self.ctx.arena.get_extended(property_access_idx) else {
            return false;
        };
        let parent_idx = prop_ext.parent;
        let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
            return false;
        };

        if (parent_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            || parent_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION)
            && let Some(access) = self.ctx.arena.get_access_expr(parent_node)
            && access.expression == property_access_idx
        {
            return false;
        }

        if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
            && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
        {
            return binary.left == property_access_idx
                && self.is_assignment_operator(binary.operator_token);
        }

        if (parent_node.kind == syntax_kind_ext::PREFIX_UNARY_EXPRESSION
            || parent_node.kind == syntax_kind_ext::POSTFIX_UNARY_EXPRESSION)
            && let Some(unary) = self.ctx.arena.get_unary_expr(parent_node)
        {
            return unary.operator == SyntaxKind::PlusPlusToken as u16
                || unary.operator == SyntaxKind::MinusMinusToken as u16;
        }

        false
    }

    pub(in crate::types_domain) fn flow_node_for_reference_usage(
        &self,
        idx: NodeIndex,
    ) -> Option<tsz_binder::FlowNodeId> {
        if let Some(flow) = self.ctx.binder.get_node_flow(idx) {
            return Some(flow);
        }

        let mut current = self.ctx.arena.parent_of(idx);
        while let Some(parent) = current {
            if parent.is_none() {
                break;
            }
            if let Some(flow) = self.ctx.binder.get_node_flow(parent) {
                return Some(flow);
            }
            current = self.ctx.arena.parent_of(parent);
        }

        None
    }

    pub(in crate::types_domain) fn flow_analyzer_for_property_reads(&self) -> FlowAnalyzer<'_> {
        FlowAnalyzer::from_ctx(&self.ctx)
    }

    pub(in crate::types_domain::property_access_helpers) fn expando_read_is_within_initializing_scope(
        &self,
        property_access_idx: NodeIndex,
        object_expr_idx: NodeIndex,
    ) -> bool {
        let use_owner = self.scope_owner_node(property_access_idx);
        let Some(root_ident) = self.root_identifier_index(object_expr_idx) else {
            return use_owner.is_none();
        };
        let Some(root_sym) = self.resolve_identifier_symbol(root_ident) else {
            return use_owner.is_none();
        };
        let Some(symbol) = self.ctx.binder.get_symbol(root_sym) else {
            return use_owner.is_none();
        };
        let decl_idx = symbol.primary_declaration().unwrap_or(NodeIndex::NONE);
        self.declaration_scope_owner_node(decl_idx) == use_owner
    }

    fn root_identifier_index(&self, idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(idx)?;
        if node.kind == SyntaxKind::Identifier as u16 {
            return Some(idx);
        }
        if node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION {
            let access = self.ctx.arena.get_access_expr(node)?;
            return self.root_identifier_index(access.expression);
        }
        None
    }

    fn scope_owner_node(&self, idx: NodeIndex) -> NodeIndex {
        let mut current = Some(idx);
        while let Some(node_idx) = current {
            if node_idx.is_none() {
                return NodeIndex::NONE;
            }
            let Some(node) = self.ctx.arena.get(node_idx) else {
                return NodeIndex::NONE;
            };
            if self.is_scope_owner_kind(node.kind) {
                return node_idx;
            }
            current = self.ctx.arena.parent_of(node_idx);
        }
        NodeIndex::NONE
    }

    fn declaration_scope_owner_node(&self, decl_idx: NodeIndex) -> NodeIndex {
        let current = self
            .ctx
            .arena
            .get_extended(decl_idx)
            .map(|ext| ext.parent)
            .unwrap_or(NodeIndex::NONE);
        self.scope_owner_node(current)
    }

    pub(in crate::types_domain::property_access_helpers) const fn is_scope_owner_kind(
        &self,
        kind: u16,
    ) -> bool {
        kind == syntax_kind_ext::FUNCTION_DECLARATION
            || kind == syntax_kind_ext::FUNCTION_EXPRESSION
            || kind == syntax_kind_ext::ARROW_FUNCTION
            || kind == syntax_kind_ext::METHOD_DECLARATION
            || kind == syntax_kind_ext::CONSTRUCTOR
            || kind == syntax_kind_ext::GET_ACCESSOR
            || kind == syntax_kind_ext::SET_ACCESSOR
    }

    pub(in crate::types_domain::property_access_helpers) fn expando_read_is_self_default_initializer(
        &self,
        property_access_idx: NodeIndex,
    ) -> bool {
        let mut current = property_access_idx;
        loop {
            let Some(parent_idx) = self.ctx.arena.parent_of(current) else {
                return false;
            };
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                return false;
            };

            if parent_node.kind == syntax_kind_ext::BINARY_EXPRESSION
                && let Some(binary) = self.ctx.arena.get_binary_expr(parent_node)
            {
                if matches!(
                    binary.operator_token,
                    op if op == SyntaxKind::BarBarToken as u16
                        || op == SyntaxKind::QuestionQuestionToken as u16
                ) && binary.left == current
                {
                    current = parent_idx;
                    continue;
                }

                return binary.operator_token == SyntaxKind::EqualsToken as u16
                    && binary.right == current
                    && self.same_reference(binary.left, property_access_idx);
            }

            return false;
        }
    }

    fn same_reference(&self, left: NodeIndex, right: NodeIndex) -> bool {
        let analyzer = self.flow_analyzer_for_property_reads();
        analyzer.is_matching_reference(left, right)
    }

    /// Check if a class has an instance member (property, method, or accessor) with the given name.
    /// Used to prevent expando property detection from masking TS2339 errors when accessing
    /// instance members on the class constructor type.
    pub(in crate::types_domain::property_access_helpers) fn class_has_instance_member(
        &self,
        obj_key: &str,
        property_name: &str,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        // Only check simple identifiers (not qualified chains like `a.B`)
        let root_name = obj_key.split('.').next().unwrap_or_default();
        if root_name != obj_key {
            return false;
        }

        let Some(sym_id) = self.ctx.binder.file_locals.get(root_name) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };

        // Only check class declarations
        if !symbol.has_any_flags(symbol_flags::CLASS) {
            return false;
        }

        // Check the class's members table for the property name.
        // Members table stores instance members by name, so a match here
        // means the property is a declared instance member.
        if let Some(ref members) = symbol.members
            && members.get(property_name).is_some()
        {
            return true;
        }

        // Also check the class AST for accessor declarations (get/set),
        // which may not always be in the members table.
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION
                && decl_node.kind != syntax_kind_ext::CLASS_EXPRESSION
            {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(decl_node) else {
                continue;
            };
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                let is_instance_member = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .ctx
                        .arena
                        .get_property_decl(member_node)
                        .is_some_and(|p| {
                            !self.has_static_modifier(&p.modifiers)
                                && self
                                    .get_property_name(p.name)
                                    .is_some_and(|n| n == property_name)
                        }),
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .ctx
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| {
                            !self.has_static_modifier(&m.modifiers)
                                && self
                                    .get_property_name(m.name)
                                    .is_some_and(|n| n == property_name)
                        }),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.ctx.arena.get_accessor(member_node).is_some_and(|a| {
                            !self.has_static_modifier(&a.modifiers)
                                && self
                                    .get_property_name(a.name)
                                    .is_some_and(|n| n == property_name)
                        })
                    }
                    _ => false,
                };
                if is_instance_member {
                    return true;
                }
            }
        }

        false
    }

    pub(in crate::types_domain::property_access_helpers) fn object_literal_root_declares_property(
        &self,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> bool {
        let Some(root_ident) = self.root_identifier_index(object_expr_idx) else {
            return false;
        };
        let Some(sym_id) = self.resolve_identifier_symbol(root_ident) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        if !symbol.has_any_flags(symbol_flags::VARIABLE) {
            return false;
        }

        let decl_idx = symbol.value_declaration;
        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        if init_node.kind != syntax_kind_ext::OBJECT_LITERAL_EXPRESSION {
            return false;
        }
        let Some(obj_lit) = self.ctx.arena.get_literal_expr(init_node) else {
            return false;
        };

        obj_lit.elements.nodes.iter().copied().any(|elem_idx| {
            let Some(elem_node) = self.ctx.arena.get(elem_idx) else {
                return false;
            };

            // A spread element (`{ ...src }`) contributes every property carried by
            // the spread source's type to the variable's semantic shape. The
            // initializer therefore already declares `property_name` when the spread
            // source structurally has it; a later `obj.prop = ...` write is a
            // re-assignment of an existing property, not an expando forward-read.
            if elem_node.kind == syntax_kind_ext::SPREAD_ASSIGNMENT
                || elem_node.kind == syntax_kind_ext::SPREAD_ELEMENT
            {
                return self.spread_source_type_declares_property(elem_node, property_name);
            }

            let elem_prop_name = match elem_node.kind {
                syntax_kind_ext::PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_property_assignment(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::SHORTHAND_PROPERTY_ASSIGNMENT => self
                    .ctx
                    .arena
                    .get_shorthand_property(elem_node)
                    .and_then(|prop| self.get_property_name(prop.name)),
                syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(elem_node)
                    .and_then(|method| self.get_property_name(method.name)),
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => self
                    .ctx
                    .arena
                    .get_accessor(elem_node)
                    .and_then(|accessor| self.get_property_name(accessor.name)),
                _ => None,
            };

            elem_prop_name.is_some_and(|name| name == property_name)
        })
    }

    /// Whether a spread element's source type structurally declares `property_name`.
    ///
    /// The spread source expression is typed when the enclosing object literal is
    /// computed (node-type cache); for a plain identifier source the resolved
    /// symbol type is the fallback. The structural `type_has_property` query covers
    /// named members, index signatures, and members introduced through nested
    /// spreads.
    fn spread_source_type_declares_property(
        &self,
        spread_elem_node: &tsz_parser::parser::node::Node,
        property_name: &str,
    ) -> bool {
        let Some(spread) = self.ctx.arena.get_spread(spread_elem_node) else {
            return false;
        };
        let spread_type = self
            .ctx
            .node_types
            .get(&spread.expression.0)
            .copied()
            .or_else(|| {
                self.resolve_identifier_symbol(spread.expression)
                    .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id))
            });
        let Some(spread_type) = spread_type else {
            return false;
        };
        let prop_atom = self.ctx.types.intern_string(property_name);
        // A spread of `T | undefined` / `T | null` contributes `T`'s properties:
        // nullish constituents spread to `{}` and add nothing, and the remaining
        // constituents contribute their members (as optional). This mirrors tsc's
        // `getSpreadType` union/nullish handling. Strip the nullish constituents
        // first, then accept the property when ANY surviving constituent declares
        // it — so a later `obj.prop = ...` write against an all-nullish-union
        // spread source is a re-assignment of an existing member, not an expando
        // forward-read (issue: tanstack TS2565 FP on spread-built object literals).
        let stripped = self.ctx.types.remove_nullish(spread_type);
        if let Some(members) =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, stripped)
        {
            members
                .iter()
                .copied()
                .any(|member| self.resolved_type_has_property(member, prop_atom))
        } else {
            self.resolved_type_has_property(stripped, prop_atom)
        }
    }

    /// Structural `type_has_property` that first resolves a `Lazy(DefId)` source
    /// reference (type alias / interface) through the checker-owned `DefId`
    /// resolver. The solver's environment-free finite-name query cannot enumerate
    /// members of an unresolved lazy reference, so a spread of an interface-typed
    /// value (`{ ...state }` where `state: QueryState`) would otherwise report no
    /// properties.
    fn resolved_type_has_property(
        &self,
        type_id: tsz_solver::TypeId,
        prop_atom: tsz_common::Atom,
    ) -> bool {
        use crate::query_boundaries::definition_identity::lazy_def_id;
        use crate::query_boundaries::property_access::type_has_property;
        use crate::query_boundaries::type_checking_utilities::application_base;
        if type_has_property(self.ctx.types, type_id, prop_atom) {
            return true;
        }
        // The property *name* set of an interface/alias is independent of its type
        // arguments, so peel a generic application (`QueryState<T, E>`) to its base
        // before resolving the lazy reference. Resolving the `DefId` body yields the
        // interface's structural members, which the environment-free finite-name
        // query cannot enumerate from an unresolved `Lazy` reference alone.
        let base = application_base(self.ctx.types, type_id).unwrap_or(type_id);
        if let Some(def_id) = lazy_def_id(self.ctx.types, base)
            && let Some(body) = self.ctx.resolve_lazy(def_id, self.ctx.types)
            && body != type_id
            && body != base
        {
            return type_has_property(self.ctx.types, body, prop_atom);
        }
        false
    }

    pub(in crate::types_domain) fn union_has_explicit_property_member(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        let members =
            crate::query_boundaries::state::checking::union_members(self.ctx.types, object_type)
                .or_else(|| {
                    crate::query_boundaries::state::checking::intersection_members(
                        self.ctx.types,
                        object_type,
                    )
                });
        let Some(members) = members else {
            return false;
        };

        members.iter().copied().any(|member| {
            let resolved_member = self.resolve_type_for_property_access(member);
            matches!(
                self.resolve_property_access_with_env(resolved_member, prop_name),
                PropertyAccessResult::Success {
                    from_index_signature: false,
                    ..
                }
            )
        })
    }

    pub(in crate::types_domain) fn type_parameter_constraint_has_explicit_property(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common::PropertyAccessResult;

        let Some(constraint) = crate::query_boundaries::state::checking::type_parameter_constraint(
            self.ctx.types,
            object_type,
        ) else {
            return false;
        };

        let resolved_constraint = self.resolve_type_for_property_access(constraint);
        matches!(
            self.resolve_property_access_with_env(resolved_constraint, prop_name),
            PropertyAccessResult::Success {
                from_index_signature: false,
                ..
            }
        )
    }

    fn mapped_type_has_explicit_property(
        &self,
        mapped_id: tsz_solver::MappedTypeId,
        prop_name: &str,
    ) -> bool {
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let preserves_source_names = mapped.name_type.is_none()
            || crate::query_boundaries::state::checking::is_identity_name_mapping(
                self.ctx.types,
                &mapped,
            );
        crate::query_boundaries::state::checking::get_finite_mapped_property_type(
            self.ctx.types,
            mapped_id,
            prop_name,
        )
        .is_some()
            || crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                self.ctx.types,
                mapped_id,
            )
            .is_some_and(|names| names.contains(&self.ctx.types.intern_string(prop_name)))
            || (preserves_source_names
                && crate::query_boundaries::state::checking::extract_string_literal_keys(
                    self.ctx.types,
                    mapped.constraint,
                )
                .iter()
                .any(|name| self.ctx.types.resolve_atom(*name) == prop_name))
    }

    fn mapped_explicit_property_names(&self, mapped_id: tsz_solver::MappedTypeId) -> Vec<String> {
        let mapped = self.ctx.types.mapped_type(mapped_id);
        let mut names: Vec<String> =
            crate::query_boundaries::state::checking::collect_finite_mapped_property_names(
                self.ctx.types,
                mapped_id,
            )
            .into_iter()
            .flatten()
            .map(|name| self.ctx.types.resolve_atom(name))
            .collect();

        let preserves_source_names = mapped.name_type.is_none()
            || crate::query_boundaries::state::checking::is_identity_name_mapping(
                self.ctx.types,
                &mapped,
            );
        if preserves_source_names {
            for name in crate::query_boundaries::state::checking::extract_string_literal_keys(
                self.ctx.types,
                mapped.constraint,
            ) {
                let name = self.ctx.types.resolve_atom(name);
                if !names.iter().any(|existing| existing == &name) {
                    names.push(name);
                }
            }
        }

        names
    }

    /// Resolve a (possibly mapped) instantiation through the type environment and,
    /// when it reduces to a concrete object, report whether `prop_name` is present.
    ///
    /// The solver's environment-free finite-name queries cannot resolve
    /// `Lazy(DefId)` source references (type aliases / interfaces), so a mapped
    /// type whose source is such a reference — combined with a non-identity `as`
    /// clause — cannot be enumerated syntactically. The checker owns the
    /// `DefId -> TypeId` resolver, so evaluating here yields the real, fully
    /// modifier-preserving object shape. Empty evaluated shapes stay uncertain:
    /// they may be artifacts of unresolved generic/keyof constraints rather
    /// than proof that every property is absent.
    fn concrete_mapped_application_has_property(
        &mut self,
        instantiated: TypeId,
        prop_name: &str,
    ) -> Option<bool> {
        use crate::query_boundaries::common as common_query;

        let evaluated = self.evaluate_type_with_env(instantiated);
        // A type that still carries free type parameters is not concrete; defer.
        if common_query::contains_type_parameters(self.ctx.types, evaluated) {
            return None;
        }
        let shape = common_query::object_shape_for_type(self.ctx.types, evaluated)?;

        let has_concrete_property_surface = shape.string_index.is_some()
            || shape.number_index.is_some()
            || !shape.properties.is_empty();
        if !has_concrete_property_surface {
            return None;
        }

        // The property is "known" when a string index signature accepts any
        // string-named property, a named property matches, or a numeric index
        // signature covers a numeric-looking name. Mirrors the intersection
        // excess-property logic so the verdict stays consistent.
        let target_atom = self.ctx.types.intern_string(prop_name);
        let is_known = shape.string_index.is_some()
            || shape.properties.iter().any(|prop| prop.name == target_atom)
            || (shape.number_index.is_some() && prop_name.parse::<f64>().is_ok());
        Some(is_known)
    }

    fn generic_mapped_application_lacks_explicit_property(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
        use_known_finite_names: bool,
        use_concrete_fallback: bool,
    ) -> Option<bool> {
        use crate::query_boundaries::common::{
            TypeSubstitution, application_info, instantiate_type,
        };

        let (base, args) = application_info(self.ctx.types, object_type)?;
        let sym_id = self.ctx.resolve_type_to_symbol_id(base)?;
        if !self
            .ctx
            .binder
            .get_symbol(sym_id)
            .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS))
        {
            return None;
        }
        let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
        let mapped_id = crate::query_boundaries::common::mapped_type_id(self.ctx.types, body_type)?;
        let mapped = self.ctx.types.mapped_type(mapped_id);
        if !crate::query_boundaries::common::contains_type_parameters(
            self.ctx.types,
            mapped.constraint,
        ) {
            return None;
        }

        let substitution = TypeSubstitution::from_args(self.ctx.types, &type_params, &args);
        let instantiated = instantiate_type(self.ctx.types, body_type, &substitution);

        let instantiated_mapped_id =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, instantiated)?;
        let instantiated_mapped = self.ctx.types.mapped_type(instantiated_mapped_id);
        let names = self.mapped_explicit_property_names(instantiated_mapped_id);
        let has_explicit_name = names.iter().any(|name| name == prop_name)
            || self.mapped_type_has_explicit_property(instantiated_mapped_id, prop_name);
        if has_explicit_name {
            return Some(false);
        }
        let preserves_source_names = instantiated_mapped.name_type.is_none()
            || crate::query_boundaries::state::checking::is_identity_name_mapping(
                self.ctx.types,
                &instantiated_mapped,
            );
        if preserves_source_names {
            if use_known_finite_names && !names.is_empty() {
                return Some(true);
            }
            return None;
        }
        if use_concrete_fallback {
            // For non-identity key-remapping over `Lazy(DefId)` sources, the
            // environment-free finite-name path above may find no names even
            // when the concrete instantiated shape has known remapped
            // properties. Only opt-in callers that are checking concrete
            // object-literal/intersection excess properties should ask the
            // checker's environment-backed evaluator for that verdict; broader
            // relation/keyof/constraint paths need the conservative syntactic
            // fallback below.
            if let Some(has_property) =
                self.concrete_mapped_application_has_property(instantiated, prop_name)
            {
                return Some(!has_property);
            }
        }
        Some(true)
    }

    pub(crate) fn generic_mapped_receiver_explicit_property_names(
        &mut self,
        object_type: TypeId,
    ) -> Vec<String> {
        use crate::query_boundaries::common::{
            TypeSubstitution, application_info, instantiate_type,
        };

        if let Some((base, args)) = application_info(self.ctx.types, object_type)
            && let Some(sym_id) = self.ctx.resolve_type_to_symbol_id(base)
        {
            if !self
                .ctx
                .binder
                .get_symbol(sym_id)
                .is_some_and(|symbol| symbol.has_any_flags(symbol_flags::TYPE_ALIAS))
            {
                return Vec::new();
            }
            let (body_type, type_params) = self.type_reference_symbol_type_with_params(sym_id);
            if let Some(mapped_id) =
                crate::query_boundaries::common::mapped_type_id(self.ctx.types, body_type)
            {
                let mapped = self.ctx.types.mapped_type(mapped_id);
                if crate::query_boundaries::common::contains_type_parameters(
                    self.ctx.types,
                    mapped.constraint,
                ) {
                    let substitution =
                        TypeSubstitution::from_args(self.ctx.types, &type_params, &args);
                    let instantiated = instantiate_type(self.ctx.types, body_type, &substitution);
                    if let Some(instantiated_mapped_id) =
                        crate::query_boundaries::common::mapped_type_id(
                            self.ctx.types,
                            instantiated,
                        )
                    {
                        return self.mapped_explicit_property_names(instantiated_mapped_id);
                    }
                }
            }
        }

        if let Some(mapped_id) =
            crate::query_boundaries::common::mapped_type_id(self.ctx.types, object_type)
        {
            return self.mapped_explicit_property_names(mapped_id);
        }

        Vec::new()
    }

    pub(crate) fn generic_mapped_receiver_lacks_explicit_property(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common as common_query;

        if let Some(lacks_explicit_property) = self
            .generic_mapped_application_lacks_explicit_property(
                object_type,
                prop_name,
                false,
                false,
            )
        {
            return lacks_explicit_property;
        }

        let resolved = self.resolve_type_for_property_access(object_type);
        let evaluated = self.evaluate_type_with_env(resolved);

        for candidate in [resolved, evaluated] {
            if !common_query::contains_type_parameters(self.ctx.types, candidate) {
                continue;
            }

            let Some(mapped_id) = common_query::mapped_type_id(self.ctx.types, candidate) else {
                continue;
            };

            return !self.mapped_type_has_explicit_property(mapped_id, prop_name);
        }

        false
    }

    pub(crate) fn generic_mapped_receiver_lacks_explicit_property_with_concrete_fallback(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common as common_query;

        if let Some(lacks_explicit_property) = self
            .generic_mapped_application_lacks_explicit_property(object_type, prop_name, false, true)
        {
            return lacks_explicit_property;
        }

        let resolved = self.resolve_type_for_property_access(object_type);
        let evaluated = self.evaluate_type_with_env(resolved);

        for candidate in [resolved, evaluated] {
            if !common_query::contains_type_parameters(self.ctx.types, candidate) {
                continue;
            }

            let Some(mapped_id) = common_query::mapped_type_id(self.ctx.types, candidate) else {
                continue;
            };

            return !self.mapped_type_has_explicit_property(mapped_id, prop_name);
        }

        false
    }

    pub(crate) fn generic_mapped_receiver_lacks_property_access_name(
        &mut self,
        object_type: TypeId,
        prop_name: &str,
    ) -> bool {
        use crate::query_boundaries::common as common_query;

        if let Some(lacks_explicit_property) = self
            .generic_mapped_application_lacks_explicit_property(object_type, prop_name, true, true)
        {
            return lacks_explicit_property;
        }

        let resolved = self.resolve_type_for_property_access(object_type);
        let evaluated = self.evaluate_type_with_env(resolved);

        for candidate in [resolved, evaluated] {
            if !common_query::contains_type_parameters(self.ctx.types, candidate) {
                continue;
            }

            let Some(mapped_id) = common_query::mapped_type_id(self.ctx.types, candidate) else {
                continue;
            };

            return !self.mapped_type_has_explicit_property(mapped_id, prop_name);
        }

        false
    }

    /// Collapse a generic call target's type parameters that depend on its
    /// `this`-type parameter before synthesizing the `.call`/`.apply` method
    /// signature.
    ///
    /// `tsc` models `CallableFunction.call` as
    /// `call<T, A extends any[], R>(this: (this: T, ...args: A) => R, thisArg: T, ...args: A): R`.
    /// The rest-arg tuple `A` is fixed from the target's ORIGINAL signature
    /// before `T` is pinned from `thisArg`, so a target type parameter whose
    /// constraint references the `this`-type parameter resolves with that
    /// parameter still unknown — e.g. `K extends keyof T` collapses to
    /// `keyof unknown` = `never`. Synthesizing the method with the target's
    /// own type parameters threaded through instead infers them in natural
    /// call order (`T` from `thisArg`, `K` from the argument), which loses
    /// `tsc`'s `never` collapse and accepts the call (false negative). This
    /// rewrites the `this`-dependent parameters to their collapsed constraint
    /// so the synthesized signature reproduces the collapse, while leaving the
    /// `this`-type parameter itself inferable from `thisArg`.
    ///
    /// Returns `None` when nothing collapses (no `this`-type, no type
    /// parameters, or no constraint that depends on the `this`-type parameter)
    /// so the caller can keep the original signature without cloning.
    fn collapse_this_dependent_type_params(
        &mut self,
        sig: &tsz_solver::CallSignature,
    ) -> Option<tsz_solver::CallSignature> {
        use crate::query_boundaries::common::{
            TypeSubstitution, contains_type_parameter_named, instantiate_type,
        };

        let this_type = sig.this_type?;
        if sig.type_params.is_empty() {
            return None;
        }

        // Type parameters referenced by the `this`-type itself (e.g. `T` in
        // `this: T`) stay inferable from `thisArg`, so they must not be
        // collapsed. Fix each of them to `unknown`, mirroring `tsc` fixing the
        // rest-arg tuple before `T` is pinned: a dependent constraint such as
        // `keyof T` then reduces to `keyof unknown` = `never`.
        let mut this_to_unknown = TypeSubstitution::new();
        let mut this_param_names = Vec::new();
        for tp in &sig.type_params {
            if contains_type_parameter_named(self.ctx.types, this_type, tp.name) {
                this_to_unknown.insert(tp.name, TypeId::UNKNOWN);
                this_param_names.push(tp.name);
            }
        }
        if this_param_names.is_empty() {
            return None;
        }

        let mut collapse_subst = TypeSubstitution::new();
        let mut collapsed_names = Vec::new();
        for tp in &sig.type_params {
            if this_param_names.contains(&tp.name) {
                continue;
            }
            let Some(constraint) = tp.constraint else {
                continue;
            };
            let depends_on_this = this_param_names
                .iter()
                .any(|&name| contains_type_parameter_named(self.ctx.types, constraint, name));
            if !depends_on_this {
                continue;
            }
            let collapsed_constraint =
                instantiate_type(self.ctx.types, constraint, &this_to_unknown);
            let collapsed_value = self.evaluate_type_with_env(collapsed_constraint);
            collapse_subst.insert(tp.name, collapsed_value);
            collapsed_names.push(tp.name);
        }
        if collapsed_names.is_empty() {
            return None;
        }

        let params = sig
            .params
            .iter()
            .map(|param| {
                property_access_query::strict_bind_call_apply_param_with_type(
                    *param,
                    instantiate_type(self.ctx.types, param.type_id, &collapse_subst),
                )
            })
            .collect();
        let return_type = instantiate_type(self.ctx.types, sig.return_type, &collapse_subst);
        let this_type = Some(instantiate_type(self.ctx.types, this_type, &collapse_subst));
        let type_params = sig
            .type_params
            .iter()
            .filter(|tp| !collapsed_names.contains(&tp.name))
            .map(|tp| {
                property_access_query::strict_bind_call_apply_type_param_with_constraint(
                    *tp,
                    tp.constraint
                        .map(|c| instantiate_type(self.ctx.types, c, &collapse_subst)),
                )
            })
            .collect();

        Some(
            property_access_query::strict_bind_call_apply_call_signature(
                type_params,
                params,
                this_type,
                return_type,
                sig.type_predicate,
                sig.is_method,
            ),
        )
    }

    pub(in crate::types_domain) fn strict_bind_call_apply_method_type(
        &mut self,
        object_type: TypeId,
        object_expr_idx: NodeIndex,
        property_name: &str,
    ) -> Option<TypeId> {
        if !matches!(property_name, "apply" | "bind" | "call") {
            return None;
        }

        fn method_this_arg_type(
            sig: &tsz_solver::CallSignature,
            is_constructor: bool,
            _receiver_this_type: Option<TypeId>,
        ) -> TypeId {
            if is_constructor {
                sig.return_type
            } else if sig.this_type.is_some() {
                sig.this_type.unwrap_or(TypeId::ANY)
            } else {
                TypeId::ANY
            }
        }

        fn bind_this_arg_type(
            sig: &tsz_solver::CallSignature,
            is_constructor: bool,
            _receiver_this_type: Option<TypeId>,
        ) -> TypeId {
            if is_constructor {
                TypeId::ANY
            } else if sig.this_type.is_some() {
                sig.this_type.unwrap_or(TypeId::ANY)
            } else {
                TypeId::ANY
            }
        }

        let mut candidates = vec![object_type];
        if let Some(sym_id) = self.resolve_identifier_symbol(object_expr_idx) {
            let sym_type = self.get_type_of_symbol(sym_id);
            if sym_type != TypeId::ERROR && !candidates.contains(&sym_type) {
                candidates.push(sym_type);
            }
        }

        let receiver_this_type = self
            .ctx
            .arena
            .get(object_expr_idx)
            .and_then(|node| self.ctx.arena.get_access_expr(node))
            .map(|access| self.get_type_of_node(access.expression))
            .filter(|ty| *ty != TypeId::ERROR);

        let mut call_targets = Vec::new();
        let mut construct_targets = Vec::new();
        for candidate in candidates {
            if let Some(shape) =
                crate::query_boundaries::property_access::function_shape(self.ctx.types, candidate)
            {
                let sig =
                    property_access_query::strict_bind_call_apply_signature_from_function_shape(
                        &shape,
                    );
                if !call_targets.contains(&sig) {
                    call_targets.push(sig);
                }
            }

            if let Some(shape) =
                crate::query_boundaries::property_access::callable_shape(self.ctx.types, candidate)
            {
                for sig in &shape.call_signatures {
                    if !call_targets.contains(sig) {
                        call_targets.push(sig.clone());
                    }
                }
                for sig in &shape.construct_signatures {
                    if !construct_targets.contains(sig) {
                        construct_targets.push(sig.clone());
                    }
                }
            }
        }

        // `tsc` resolves `.call`/`.apply`/`.bind` on a value with call
        // signatures through the lib `CallableFunction` members;
        // `NewableFunction` only applies when the receiver has construct
        // signatures and no call signatures, so construct signatures never
        // contribute method candidates alongside call signatures.
        if !call_targets.is_empty() {
            construct_targets.clear();
        }

        let mut method_signatures = Vec::new();

        // `tsc` parity: `.bind(thisArg)` on an overloaded function whose call
        // signatures declare no `this` parameter returns
        // `OmitThisParameter<T> = T` — the FULL overload set — because
        // `ThisParameterType<T>` is `unknown`, so the first
        // `CallableFunction.bind` overload wins and preserves every signature.
        // The per-signature synthesis below instead emits one bound function
        // per receiver call signature, which collapses an overloaded receiver
        // to its first signature during overload resolution (e.g. immer's
        // `produceWithPatches.bind(immer)`). Emit a single identity `.bind`
        // method first, carrying every call signature of the receiver, so
        // resolving `.bind(thisArg)` yields the whole overload set. It takes
        // exactly one parameter, so partial-application `.bind(thisArg, arg0,
        // ...)` calls still fall through to the per-signature overloads below.
        if property_name == "bind"
            && call_targets.len() > 1
            && call_targets.iter().all(|sig| sig.this_type.is_none())
        {
            let full_receiver =
                property_access_query::strict_bind_call_apply_call_only_callable_type(
                    self.ctx.types,
                    call_targets.clone(),
                );
            method_signatures.push(
                property_access_query::strict_bind_call_apply_call_signature(
                    Vec::new(),
                    vec![
                        property_access_query::strict_bind_call_apply_this_arg_param(
                            self.ctx.types,
                            TypeId::ANY,
                        ),
                    ],
                    None,
                    full_receiver,
                    None,
                    false,
                ),
            );
        }

        // `CallableFunction`/`NewableFunction` expose ONE generic method
        // signature per operation, and `tsc`'s signature-list inference
        // aligns source and target signatures from the end, so an overloaded
        // receiver is modeled by its LAST overload only (the documented
        // `strictBindCallApply` caveat). Synthesizing one candidate per
        // receiver overload instead reports TS2769 where `tsc` reports a
        // single TS2345 against the last overload's parameters, and silently
        // accepts arguments that only match earlier overloads. The
        // `.bind(thisArg)` identity method above is the one exception
        // (`OmitThisParameter<T> = T`), which is why it is synthesized from
        // the full set before this truncation. The alignment rule itself is
        // owned by the solver (`constrain_matching_signatures` in
        // `tsz_solver::operations::constraints::signatures`); this synthesis
        // emulates its single-target outcome.
        call_targets.drain(..call_targets.len().saturating_sub(1));
        construct_targets.drain(..construct_targets.len().saturating_sub(1));

        // For `.call`/`.apply`, `tsc` fixes the rest-arg tuple from the
        // target's original signature, collapsing type parameters whose
        // constraint references the `this`-type parameter (e.g. `K extends
        // keyof T` -> `never`). `.bind` defers the rest-arg check to the bound
        // function's later invocation, so it keeps the un-collapsed target.
        // (Runs after the truncation so only the surviving signature pays for
        // the collapse.)
        if matches!(property_name, "call" | "apply") {
            for sig in &mut call_targets {
                if let Some(collapsed) = self.collapse_this_dependent_type_params(sig) {
                    *sig = collapsed;
                }
            }
        }

        for (sig, is_constructor) in call_targets
            .iter()
            .map(|sig| (sig, false))
            .chain(construct_targets.iter().map(|sig| (sig, true)))
        {
            match property_name {
                "apply" => {
                    let method_sig = property_access_query::strict_bind_call_apply_call_signature(
                        sig.type_params.clone(),
                        vec![
                            property_access_query::strict_bind_call_apply_this_arg_param(
                                self.ctx.types,
                                method_this_arg_type(sig, is_constructor, receiver_this_type),
                            ),
                            property_access_query::strict_bind_call_apply_args_param(
                                self.ctx.types,
                                property_access_query::strict_bind_call_apply_params_tuple_type(
                                    self.ctx.types,
                                    &sig.params,
                                ),
                            ),
                        ],
                        None,
                        if is_constructor {
                            TypeId::VOID
                        } else {
                            sig.return_type
                        },
                        None,
                        false,
                    );
                    if !method_signatures.contains(&method_sig) {
                        method_signatures.push(method_sig);
                    }
                }
                "call" => {
                    let mut params = Vec::with_capacity(1 + sig.params.len());
                    params.push(
                        property_access_query::strict_bind_call_apply_this_arg_param(
                            self.ctx.types,
                            method_this_arg_type(sig, is_constructor, receiver_this_type),
                        ),
                    );
                    params.extend(sig.params.clone());

                    let method_sig = property_access_query::strict_bind_call_apply_call_signature(
                        sig.type_params.clone(),
                        params,
                        None,
                        if is_constructor {
                            TypeId::VOID
                        } else {
                            sig.return_type
                        },
                        None,
                        false,
                    );
                    if !method_signatures.contains(&method_sig) {
                        method_signatures.push(method_sig);
                    }
                }
                "bind" => {
                    let fixed_prefix_count =
                        sig.params.iter().take_while(|param| !param.rest).count();
                    for prefix_len in 0..=fixed_prefix_count {
                        let this_arg_type =
                            bind_this_arg_type(sig, is_constructor, receiver_this_type);
                        let mut params = Vec::with_capacity(1 + prefix_len);
                        params.push(
                            property_access_query::strict_bind_call_apply_this_arg_param(
                                self.ctx.types,
                                this_arg_type,
                            ),
                        );
                        params.extend(sig.params.iter().take(prefix_len).cloned());

                        let remaining_params =
                            sig.params.iter().skip(prefix_len).cloned().collect();
                        let method_sig =
                            property_access_query::strict_bind_call_apply_call_signature(
                                sig.type_params.clone(),
                                params,
                                None,
                                property_access_query::strict_bind_call_apply_bound_return_type(
                                    self.ctx.types,
                                    sig,
                                    remaining_params,
                                    is_constructor,
                                ),
                                None,
                                false,
                            );
                        if !method_signatures.contains(&method_sig) {
                            method_signatures.push(method_sig);
                        }

                        if prefix_len == 0 && sig.this_type.is_some() && !is_constructor {
                            let (generic_this_param, generic_this_type) =
                                property_access_query::strict_bind_call_apply_generic_this_param(
                                    self.ctx.types,
                                    this_arg_type,
                                    sig,
                                );
                            let generic_receiver_type =
                                property_access_query::strict_bind_call_apply_generic_bind_receiver_type(
                                    self.ctx.types,
                                    &self.ctx,
                                    sig,
                                    generic_this_type,
                                );
                            let generic_bind_sig =
                                property_access_query::strict_bind_call_apply_call_signature(
                                    std::iter::once(generic_this_param)
                                    .chain(sig.type_params.clone())
                                    .collect(),
                                    vec![
                                        property_access_query::strict_bind_call_apply_this_arg_param(
                                            self.ctx.types,
                                            generic_this_type,
                                        ),
                                    ],
                                    Some(generic_receiver_type),
                                    property_access_query::strict_bind_call_apply_bound_return_type(
                                        self.ctx.types,
                                    sig,
                                    sig.params.clone(),
                                    is_constructor,
                                    ),
                                    None,
                                    false,
                                );
                            if !method_signatures.contains(&generic_bind_sig) {
                                method_signatures.push(generic_bind_sig);
                            }
                        }
                    }
                }
                _ => return None,
            }
        }

        property_access_query::strict_bind_call_apply_method_type(self.ctx.types, method_signatures)
    }

    /// Report the module-compatibility error for `import.meta` when the
    /// effective module kind does not support the meta-property, matching
    /// `tsc`'s two distinct diagnostics:
    ///
    /// * Node16/Node18/Node20/NodeNext: `import.meta` is fine in ES-module
    ///   files but not in files that resolve to CommonJS output, so the
    ///   per-file format decides whether to emit TS1470 ("not allowed in
    ///   files which will build into CommonJS output").
    /// * CommonJS, AMD, UMD, and ES2015 (every module kind below ES2020 that
    ///   is not System and not a Node mode): the meta-property is unavailable
    ///   regardless of the file, so `tsc` emits TS1343 ("only allowed when the
    ///   '--module' option is 'es2020', ..."). Earlier tsz always emitted
    ///   TS1470 here, which diverged from `tsc`.
    /// * System and ES2020+ support `import.meta` natively, so no error.
    ///
    /// `ModuleKind::None` is the unspecified/unresolved sentinel: the driver
    /// resolves it to a concrete module kind (per `--target`) before checking,
    /// and `tsc`'s own check runs on the *resolved* kind. So a bare `None`
    /// emits no module diagnostic here — the resolved kind (e.g. CommonJS for a
    /// low target, ES2020 otherwise) drives the decision once it is set.
    pub(in crate::types_domain) fn check_import_meta_module_support(
        &mut self,
        node_idx: NodeIndex,
    ) {
        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
        use tsz_common::common::ModuleKind;

        let module_kind = self.ctx.compiler_options.module;
        if module_kind.is_node_module() {
            // Node16/Node18/Node20/NodeNext: per-file CJS/ESM determination.
            // Only files that build into CommonJS output are rejected (TS1470).
            // `CheckerContext::current_file_builds_to_commonjs` owns the
            // extension/`package.json` precedence; the top-level-`await`
            // TS1309 family asks it the same question.
            if self.ctx.current_file_builds_to_commonjs() {
                self.error_at_node(
                    node_idx,
                    diagnostic_messages::THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM,
                    diagnostic_codes::THE_IMPORT_META_META_PROPERTY_IS_NOT_ALLOWED_IN_FILES_WHICH_WILL_BUILD_INTO_COMM,
                );
            }
        } else if module_kind != ModuleKind::None
            && module_kind != ModuleKind::System
            && (module_kind as u32) < (ModuleKind::ES2020 as u32)
        {
            // CommonJS, AMD, UMD, ES2015: import.meta is unavailable for the
            // whole module mode, not a per-file CJS-output decision (TS1343).
            // `None` is excluded — it is the unresolved default, not an
            // explicit sub-ES2020 module choice.
            self.error_at_node(
                node_idx,
                diagnostic_messages::THE_IMPORT_META_META_PROPERTY_IS_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_ES2020_E,
                diagnostic_codes::THE_IMPORT_META_META_PROPERTY_IS_ONLY_ALLOWED_WHEN_THE_MODULE_OPTION_IS_ES2020_E,
            );
        }
        // System and ES2020+ support import.meta natively: no diagnostic.
    }

    /// Mirror the binder's `resolved_const_expando_key` logic so that the checker
    /// resolves element-access keys using the same approach the binder used when
    /// it stored the expando property.
    pub(crate) fn resolved_const_expando_key_from_binder(
        &self,
        sym_id: tsz_binder::SymbolId,
        depth: u8,
    ) -> Option<String> {
        if depth > 8 {
            return None;
        }

        let symbol = self.get_cross_file_symbol(sym_id)?;
        if symbol.has_any_flags(symbol_flags::ALIAS)
            && let Some(target_sym_id) =
                self.resolve_alias_symbol(sym_id, &mut AliasCycleTracker::new())
            && target_sym_id != sym_id
        {
            return self.resolved_const_expando_key_from_binder(target_sym_id, depth + 1);
        }

        let decl_idx = symbol.primary_declaration()?;
        let arena = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .map(|file_idx| self.ctx.get_arena_for_file(file_idx as u32))
            .unwrap_or(self.ctx.arena);
        let binder = self
            .ctx
            .resolve_symbol_file_index(sym_id)
            .and_then(|file_idx| self.ctx.get_binder_for_file(file_idx))
            .unwrap_or(self.ctx.binder);

        if !arena.is_const_variable_declaration(decl_idx) {
            return None;
        }

        let decl_node = arena.get(decl_idx)?;
        let var_decl = arena.get_variable_declaration(decl_node)?;
        let init_idx = var_decl.initializer;
        if init_idx.is_none() {
            return None;
        }
        let init_node = arena.get(init_idx)?;

        match init_node.kind {
            k if k == SyntaxKind::StringLiteral as u16
                || k == SyntaxKind::NumericLiteral as u16
                || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16 =>
            {
                arena.get_literal(init_node).map(|lit| lit.text.clone())
            }
            k if k == tsz_parser::parser::syntax_kind_ext::PREFIX_UNARY_EXPRESSION => {
                let unary = arena.get_unary_expr(init_node)?;
                let operand = arena.get(unary.operand)?;
                if operand.kind != SyntaxKind::NumericLiteral as u16 {
                    return None;
                }
                let lit = arena.get_literal(operand)?;
                match unary.operator {
                    k if k == SyntaxKind::MinusToken as u16 => Some(format!("-{}", lit.text)),
                    k if k == SyntaxKind::PlusToken as u16 => Some(lit.text.clone()),
                    _ => None,
                }
            }
            k if k == SyntaxKind::Identifier as u16 => {
                let name = arena.get_identifier(init_node)?.escaped_text.clone();
                let next_sym = binder.file_locals.get(&name)?;
                self.resolved_const_expando_key_from_binder(next_sym, depth + 1)
            }
            k if k == tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION => {
                Self::is_symbol_call_in_arena(arena, init_idx)
                    .then(|| format!("__unique_{}", sym_id.0))
            }
            _ => None,
        }
    }

    pub(crate) fn canonical_expando_property_name(&self, property_name: &str) -> String {
        self.ctx
            .binder
            .file_locals
            .get(property_name)
            .and_then(|sym_id| self.resolved_const_expando_key_from_binder(sym_id, 0))
            .unwrap_or_else(|| property_name.to_string())
    }

    /// Check if a node is a `Symbol()` or `Symbol("desc")` call expression (pure AST check).
    pub(crate) fn is_symbol_call_in_arena(
        arena: &tsz_parser::parser::node::NodeArena,
        idx: NodeIndex,
    ) -> bool {
        let Some(node) = arena.get(idx) else {
            return false;
        };
        if node.kind != tsz_parser::parser::syntax_kind_ext::CALL_EXPRESSION {
            return false;
        }
        let Some(call) = arena.get_call_expr(node) else {
            return false;
        };
        let Some(expr_node) = arena.get(call.expression) else {
            return false;
        };
        arena
            .get_identifier(expr_node)
            .is_some_and(|ident| ident.escaped_text == "Symbol")
    }

    /// Check if the object expression has any unique-symbol-keyed expando properties
    /// recorded by the binder (i.e., any `__unique_*` entry in `expando_properties`).
    pub(crate) fn object_has_unique_symbol_expandos(&self, object_expr_idx: NodeIndex) -> bool {
        let Some(obj_key) = property_access_chain_text_in_arena(self.ctx.arena, object_expr_idx)
        else {
            return false;
        };
        let mut candidate_keys = vec![obj_key];
        if let Some(node) = self.ctx.arena.get(object_expr_idx)
            && node.kind == SyntaxKind::Identifier as u16
            && let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(object_expr_idx)
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
            && let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration)
            && decl_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
            && let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node)
            && let Some(init_node) = self.ctx.arena.get(var_decl.initializer)
            && init_node.kind == syntax_kind_ext::NEW_EXPRESSION
            && let Some(new_expr) = self.ctx.arena.get_call_expr(init_node)
            && let Some(ctor_key) =
                property_access_chain_text_in_arena(self.ctx.arena, new_expr.expression)
        {
            candidate_keys.push(format!("{ctor_key}.prototype"));
            if let Some(ctor_node) = self.ctx.arena.get(new_expr.expression)
                && ctor_node.kind == syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && let Some(ctor_access) = self.ctx.arena.get_access_expr(ctor_node)
                && let Some(name_node) = self.ctx.arena.get(ctor_access.name_or_argument)
                && let Some(name_ident) = self.ctx.arena.get_identifier(name_node)
            {
                candidate_keys.push(format!("{}.prototype", name_ident.escaped_text));
            }
        }

        let has_unique =
            |expandos: &rustc_hash::FxHashMap<String, rustc_hash::FxHashSet<String>>, key: &str| {
                expandos.get(key).is_some_and(|props| {
                    props.iter().any(|p| {
                        p.starts_with("__unique_")
                            || self
                                .canonical_expando_property_name(p)
                                .starts_with("__unique_")
                    })
                })
            };

        for key in &candidate_keys {
            if has_unique(&self.ctx.binder.expando_properties, key) {
                return true;
            }
        }
        // Use global expando index for O(1) lookup instead of O(N) binder scan
        if let Some(expando_idx) = &self.ctx.global_expando_index {
            for key in &candidate_keys {
                if has_unique(expando_idx, key) {
                    return true;
                }
            }
        } else if let Some(all_binders) = &self.ctx.all_binders {
            for binder in all_binders.iter() {
                for key in &candidate_keys {
                    if has_unique(&binder.expando_properties, key) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn object_expr_is_new_constructor_instance(
        &self,
        object_expr_idx: NodeIndex,
    ) -> bool {
        let Some(node) = self.ctx.arena.get(object_expr_idx) else {
            return false;
        };
        if node.kind != SyntaxKind::Identifier as u16 {
            return false;
        }
        let Some(sym_id) = self.resolve_identifier_symbol_without_tracking(object_expr_idx) else {
            return false;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        let Some(decl_node) = self.ctx.arena.get(symbol.value_declaration) else {
            return false;
        };
        if decl_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return false;
        }
        let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
            return false;
        };
        let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
            return false;
        };
        init_node.kind == syntax_kind_ext::NEW_EXPRESSION
    }
}
