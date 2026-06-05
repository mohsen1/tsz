//! Helper methods for class implements checking.
//! - Interface-extends-class accessibility checks
//! - Private/protected member detection
//! - Inherited public member collection

use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use tsz_common::Visibility;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Check if an interface extends a class with private/protected members that are
    /// inaccessible to the implementing class.
    ///
    /// When an interface extends a class with private/protected members, those members
    /// become part of the interface's contract. A class implementing such an interface
    /// can only satisfy this contract if it extends the same base class (giving it
    /// access to those private members). Otherwise, TS2420 should be emitted.
    ///
    /// # Arguments
    /// * `interface_idx` - The `NodeIndex` of the interface declaration
    /// * `interface_decl` - The interface data
    /// * `class_idx` - The `NodeIndex` of the implementing class
    /// * `class_data` - The class data
    ///
    /// # Returns
    /// true if the interface extends a class with private/protected members that the
    /// implementing class cannot access
    pub(crate) fn interface_extends_class_with_inaccessible_members(
        &mut self,
        _interface_idx: NodeIndex,
        interface_decl: &tsz_parser::parser::node::InterfaceData,
        _class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        // First, collect the base classes that the implementing class extends
        let mut class_extends_symbols = std::collections::HashSet::new();
        if let Some(ref class_heritage) = class_data.heritage_clauses {
            for &clause_idx in &class_heritage.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };

                // Only look at extends clauses
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };

                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                    if let Some(base_name) = self.heritage_name_text(expr_idx)
                        && let Some(sym_id) = self.ctx.binder.file_locals.get(&base_name)
                    {
                        class_extends_symbols.insert(sym_id);
                    }
                }
            }
        }

        let Some(ref heritage_clauses) = interface_decl.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                continue;
            };
            let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                continue;
            };

            // Only check extends clauses (not implements)
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            for &type_idx in &heritage.types.nodes {
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    continue;
                };

                // Get the expression from ExpressionWithTypeArguments or TypeReference
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                // Resolve the symbol being extended
                if let Some(base_name) = self.heritage_name_text(expr_idx)
                    && let Some(sym_id) = self.ctx.binder.file_locals.get(&base_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                {
                    // If the implementing class extends this same base class, then it has
                    // access to the private members - no error needed
                    if class_extends_symbols.contains(&sym_id) {
                        continue;
                    }

                    // Check if any declaration is a class with private/protected members
                    for &decl_idx in &symbol.declarations {
                        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                            continue;
                        };

                        // Check if it's a class declaration
                        if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                            continue;
                        }

                        let Some(class_data) = self.ctx.arena.get_class(decl_node) else {
                            continue;
                        };

                        // Check if class has any private or protected members
                        for &member_idx in &class_data.members.nodes {
                            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                                continue;
                            };

                            match member_node.kind {
                                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                                    if let Some(prop) =
                                        self.ctx.arena.get_property_decl(member_node)
                                        && (self.has_private_modifier(&prop.modifiers)
                                            || self.has_protected_modifier(&prop.modifiers))
                                    {
                                        return true;
                                    }
                                }
                                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                    if let Some(method) =
                                        self.ctx.arena.get_method_decl(member_node)
                                        && (self.has_private_modifier(&method.modifiers)
                                            || self.has_protected_modifier(&method.modifiers))
                                    {
                                        return true;
                                    }
                                }
                                k if k == syntax_kind_ext::GET_ACCESSOR => {
                                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                                        && (self.has_private_modifier(&accessor.modifiers)
                                            || self.has_protected_modifier(&accessor.modifiers))
                                    {
                                        return true;
                                    }
                                }
                                k if k == syntax_kind_ext::SET_ACCESSOR => {
                                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                                        && (self.has_private_modifier(&accessor.modifiers)
                                            || self.has_protected_modifier(&accessor.modifiers))
                                    {
                                        return true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // Also check value_declaration
                    if symbol.value_declaration.is_some() {
                        let decl_idx = symbol.value_declaration;
                        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                            continue;
                        };

                        if decl_node.kind == syntax_kind_ext::CLASS_DECLARATION {
                            let Some(class_data) = self.ctx.arena.get_class(decl_node) else {
                                continue;
                            };

                            for &member_idx in &class_data.members.nodes {
                                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                                    continue;
                                };

                                match member_node.kind {
                                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                                        if let Some(prop) =
                                            self.ctx.arena.get_property_decl(member_node)
                                            && (self.has_private_modifier(&prop.modifiers)
                                                || self.has_protected_modifier(&prop.modifiers))
                                        {
                                            return true;
                                        }
                                    }
                                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                                        if let Some(method) =
                                            self.ctx.arena.get_method_decl(member_node)
                                            && (self.has_private_modifier(&method.modifiers)
                                                || self.has_protected_modifier(&method.modifiers))
                                        {
                                            return true;
                                        }
                                    }
                                    k if k == syntax_kind_ext::GET_ACCESSOR => {
                                        if let Some(accessor) =
                                            self.ctx.arena.get_accessor(member_node)
                                            && (self.has_private_modifier(&accessor.modifiers)
                                                || self.has_protected_modifier(&accessor.modifiers))
                                        {
                                            return true;
                                        }
                                    }
                                    k if k == syntax_kind_ext::SET_ACCESSOR => {
                                        if let Some(accessor) =
                                            self.ctx.arena.get_accessor(member_node)
                                            && (self.has_private_modifier(&accessor.modifiers)
                                                || self.has_protected_modifier(&accessor.modifiers))
                                        {
                                            return true;
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        false
    }

    /// Check if an interface declaration extends a class with private/protected members
    /// that the implementing class CAN access (because the class extends that base class).
    /// This is used to detect TS2320 conflicts: when one merged interface declaration
    /// extends a class the implementing class extends (accessible) and another extends
    /// a class it doesn't extend (inaccessible), the conflict is a TS2320 issue on the
    /// interface, not a TS2420 issue on the implementing class.
    pub(crate) fn interface_extends_class_with_accessible_private_members(
        &mut self,
        interface_decl: &tsz_parser::parser::node::InterfaceData,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        // Collect the base classes that the implementing class extends
        let mut class_extends_symbols = std::collections::HashSet::new();
        if let Some(ref class_heritage) = class_data.heritage_clauses {
            for &clause_idx in &class_heritage.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    let expr_idx = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };
                    if let Some(base_name) = self.heritage_name_text(expr_idx)
                        && let Some(sym_id) = self.ctx.binder.file_locals.get(&base_name)
                    {
                        class_extends_symbols.insert(sym_id);
                    }
                }
            }
        }

        let Some(ref heritage_clauses) = interface_decl.heritage_clauses else {
            return false;
        };

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
                if let Some(base_name) = self.heritage_name_text(expr_idx)
                    && let Some(sym_id) = self.ctx.binder.file_locals.get(&base_name)
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && class_extends_symbols.contains(&sym_id)
                {
                    // The implementing class extends this base class.
                    // Check if the base class has private/protected members.
                    for &decl_idx in &symbol.declarations {
                        let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                            continue;
                        };
                        if decl_node.kind != syntax_kind_ext::CLASS_DECLARATION {
                            continue;
                        }
                        let Some(base_class_data) = self.ctx.arena.get_class(decl_node) else {
                            continue;
                        };
                        if self.class_has_private_or_protected_members(base_class_data) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    pub(crate) fn class_has_private_or_protected_members(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> bool {
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(member_node)
                        && (self.has_private_modifier(&prop.modifiers)
                            || self.has_protected_modifier(&prop.modifiers))
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(member_node)
                        && (self.has_private_modifier(&method.modifiers)
                            || self.has_protected_modifier(&method.modifiers))
                    {
                        return true;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                        && (self.has_private_modifier(&accessor.modifiers)
                            || self.has_protected_modifier(&accessor.modifiers))
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Collect public inherited members from the base class chain.
    ///
    /// Iteratively walks up the `extends` chain with cycle detection, collecting
    /// member names and their types. Only PUBLIC members are collected -- private/protected
    /// members cannot satisfy interface requirements, matching tsc's behavior.
    pub(crate) fn collect_inherited_public_members(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        direct_members: &rustc_hash::FxHashMap<String, NodeIndex>,
        result: &mut rustc_hash::FxHashMap<String, TypeId>,
    ) {
        let mut visited = rustc_hash::FxHashSet::default();
        let mut current_heritage = class_data.heritage_clauses.clone();
        // Accumulated substitutions, in walk order (outermost step first). When
        // collecting a member from a base, applying these in reverse order
        // (innermost / closest to the base first, outermost last) maps the
        // base's open type parameters through the chain to the implementing
        // class's context. This mirrors `tsc`'s `getTypeOfPropertyOfType` on an
        // instantiated heritage type.
        let mut step_substitutions: Vec<TypeSubstitution> = Vec::new();

        while let Some(ref heritage_clauses) = current_heritage {
            let mut next_heritage = None;

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

                let (expr_idx, heritage_type_arg_nodes) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(expr_node) else {
                    continue;
                };

                let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text) else {
                    continue;
                };
                let base_decl = {
                    let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                        continue;
                    };
                    let Some(d) = symbol.primary_declaration() else {
                        continue;
                    };
                    d
                };

                // Cycle detection
                if !visited.insert(base_decl) {
                    break;
                }

                let Some(base_node) = self.ctx.arena.get(base_decl) else {
                    continue;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    continue;
                };

                // Push the per-step substitution for this `extends BaseClass<...>`
                // line: base's type parameters → resolved type arguments expressed
                // in the CURRENT class's context. The arguments themselves may
                // reference outer-class type parameters; outer steps in
                // `step_substitutions` will substitute those when applied.
                let base_type_params = self.get_type_params_for_symbol(sym_id);
                if !base_type_params.is_empty() {
                    let step_type_args: Vec<TypeId> = heritage_type_arg_nodes
                        .map(|args| {
                            args.nodes
                                .iter()
                                .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                                .collect()
                        })
                        .unwrap_or_default();
                    step_substitutions.push(TypeSubstitution::from_args(
                        self.ctx.types,
                        &base_type_params,
                        &step_type_args,
                    ));
                }

                // Member names with more than one declaration in this base
                // (method overloads or a get/set accessor pair). For those, any
                // single declaration's type is incomplete: collecting only the
                // first overload signature drops the rest, which produces a false
                // TS2416/TS2420 when the implemented interface relies on a later
                // overload. The base class's instance-type shape already
                // aggregates the full overload set (and hides the implementation
                // signature) exactly as the own-class overloaded path does, so
                // pull the merged member type from there for these names.
                let overloaded_member_names: rustc_hash::FxHashSet<String> = {
                    let mut counts: rustc_hash::FxHashMap<String, u32> =
                        rustc_hash::FxHashMap::default();
                    for &member_idx in &base_class.members.nodes {
                        if let Some(name) = self.get_member_name(member_idx) {
                            *counts.entry(name).or_default() += 1;
                        }
                    }
                    counts
                        .into_iter()
                        .filter_map(|(name, count)| (count >= 2).then_some(name))
                        .collect()
                };
                let base_aggregated_member_types: rustc_hash::FxHashMap<String, TypeId> =
                    if overloaded_member_names.is_empty() {
                        rustc_hash::FxHashMap::default()
                    } else {
                        let base_instance_type =
                            self.get_class_instance_type(base_decl, base_class);
                        crate::query_boundaries::class::instance_member_types_by_name(
                            self.ctx.types,
                            base_instance_type,
                        )
                    };

                // Collect public members from the base class
                for &member_idx in &base_class.members.nodes {
                    if let Some(name) = self.get_member_name(member_idx)
                        && !direct_members.contains_key(&name)
                        && !result.contains_key(&name)
                    {
                        let sym_flags = self
                            .ctx
                            .binder
                            .get_node_symbol(member_idx)
                            .and_then(|sid| self.ctx.binder.get_symbol(sid))
                            .map(|s| s.flags)
                            .unwrap_or(0);
                        let visibility_mask =
                            tsz_binder::symbol_flags::PRIVATE | tsz_binder::symbol_flags::PROTECTED;
                        if sym_flags & visibility_mask == 0 {
                            let member_type = if overloaded_member_names.contains(&name) {
                                base_aggregated_member_types
                                    .get(&name)
                                    .copied()
                                    .unwrap_or_else(|| self.get_type_of_class_member(member_idx))
                            } else {
                                self.get_type_of_class_member(member_idx)
                            };
                            result.insert(
                                name,
                                self.apply_inherited_member_substitutions(
                                    member_type,
                                    &step_substitutions,
                                ),
                            );
                        }
                    }

                    // Also handle constructor parameter properties
                    if let Some(node) = self.ctx.arena.get(member_idx)
                        && node.kind == syntax_kind_ext::CONSTRUCTOR
                        && let Some(ctor) = self.ctx.arena.get_constructor(node)
                    {
                        for &param_idx in &ctor.parameters.nodes {
                            if let Some(param_node) = self.ctx.arena.get(param_idx)
                                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                                && self.has_parameter_property_modifier(&param.modifiers)
                                && !self.has_private_modifier(&param.modifiers)
                                && !self.has_protected_modifier(&param.modifiers)
                                && let Some(name) = self.get_property_name(param.name)
                                && !direct_members.contains_key(&name)
                                && !result.contains_key(&name)
                            {
                                let member_type = self.get_type_of_class_member(param_idx);
                                result.insert(
                                    name,
                                    self.apply_inherited_member_substitutions(
                                        member_type,
                                        &step_substitutions,
                                    ),
                                );
                            }
                        }
                    }
                }

                // Continue to the base class's base class
                next_heritage = base_class.heritage_clauses.clone();
                break; // Only one extends clause
            }

            current_heritage = next_heritage;
        }
    }

    /// Apply per-step substitutions accumulated while walking up the extends
    /// chain to a member type from a base class. The substitutions are applied
    /// innermost-first (closest to the base class declaring the member) and
    /// outermost-last, so each level's type-argument bindings resolve into the
    /// next level's context until the implementing class's context is reached.
    fn apply_inherited_member_substitutions(
        &mut self,
        mut member_type: TypeId,
        step_substitutions: &[TypeSubstitution],
    ) -> TypeId {
        for sub in step_substitutions.iter().rev() {
            member_type = instantiate_type(self.ctx.types, member_type, sub);
        }
        member_type
    }

    /// Collect inherited PRIVATE/PROTECTED members from the base class chain.
    ///
    /// These members cannot satisfy interface requirements, but when an interface
    /// extends the same base class as the implementing class, the private members
    /// appear in the interface type shape. We need to know which members are
    /// inherited private/protected so we can skip them in the "missing" check.
    pub(crate) fn collect_inherited_non_public_members(
        &mut self,
        class_data: &tsz_parser::parser::node::ClassData,
        result: &mut rustc_hash::FxHashMap<String, Visibility>,
    ) {
        let mut visited = rustc_hash::FxHashSet::default();
        let mut current_heritage = class_data.heritage_clauses.clone();

        while let Some(ref heritage_clauses) = current_heritage {
            let mut next_heritage = None;

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

                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };

                let Some(expr_node) = self.ctx.arena.get(expr_idx) else {
                    continue;
                };
                let Some(ident) = self.ctx.arena.get_identifier(expr_node) else {
                    continue;
                };

                let Some(sym_id) = self.ctx.binder.file_locals.get(&ident.escaped_text) else {
                    continue;
                };
                let base_decl = {
                    let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
                        continue;
                    };
                    let Some(d) = symbol.primary_declaration() else {
                        continue;
                    };
                    d
                };

                if !visited.insert(base_decl) {
                    break;
                }

                let Some(base_node) = self.ctx.arena.get(base_decl) else {
                    continue;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    continue;
                };

                for &member_idx in &base_class.members.nodes {
                    if let Some(name) = self.get_member_name(member_idx) {
                        let sym_flags = self
                            .ctx
                            .binder
                            .get_node_symbol(member_idx)
                            .and_then(|sid| self.ctx.binder.get_symbol(sid))
                            .map(|s| s.flags)
                            .unwrap_or(0);
                        let visibility = if (sym_flags & tsz_binder::symbol_flags::PRIVATE) != 0 {
                            Some(Visibility::Private)
                        } else if (sym_flags & tsz_binder::symbol_flags::PROTECTED) != 0 {
                            Some(Visibility::Protected)
                        } else {
                            None
                        };
                        if let Some(visibility) = visibility {
                            result.entry(name).or_insert(visibility);
                        }
                    }

                    if let Some(node) = self.ctx.arena.get(member_idx)
                        && node.kind == syntax_kind_ext::CONSTRUCTOR
                        && let Some(ctor) = self.ctx.arena.get_constructor(node)
                    {
                        for &param_idx in &ctor.parameters.nodes {
                            if let Some(param_node) = self.ctx.arena.get(param_idx)
                                && let Some(param) = self.ctx.arena.get_parameter(param_node)
                                && self.has_parameter_property_modifier(&param.modifiers)
                                && let Some(name) = self.get_property_name(param.name)
                            {
                                let visibility = if self.has_private_modifier(&param.modifiers) {
                                    Some(Visibility::Private)
                                } else if self.has_protected_modifier(&param.modifiers) {
                                    Some(Visibility::Protected)
                                } else {
                                    None
                                };
                                if let Some(visibility) = visibility {
                                    result.entry(name).or_insert(visibility);
                                }
                            }
                        }
                    }
                }

                next_heritage = base_class.heritage_clauses.clone();
                break;
            }

            current_heritage = next_heritage;
        }
    }

    /// Collect names of members declared directly by `class_data` that
    /// satisfy an inherited abstract member contract.
    ///
    /// A constructor parameter property (`constructor(public foo: T)`)
    /// declares and initializes the instance property `foo`, so it satisfies
    /// `abstract foo: T` from a base class regardless of visibility modifier.
    /// Visibility mismatches between the abstract member and the parameter
    /// property are reported separately (TS2415/TS2611/TS2612); they are not
    /// the absence-of-implementation diagnostic that TS2515/TS2654 tracks.
    pub(crate) fn collect_concrete_member_names_for_abstract_impl(
        &self,
        class_data: &tsz_parser::parser::node::ClassData,
    ) -> rustc_hash::FxHashSet<String> {
        let mut names = rustc_hash::FxHashSet::default();
        for &member_idx in &class_data.members.nodes {
            let Some(node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor) = self.ctx.arena.get_constructor(node) else {
                    continue;
                };
                for &param_idx in &ctor.parameters.nodes {
                    if let Some(param_node) = self.ctx.arena.get(param_idx)
                        && let Some(param) = self.ctx.arena.get_parameter(param_node)
                        && self.has_parameter_property_modifier(&param.modifiers)
                        && let Some(name) = self.get_property_name(param.name)
                    {
                        names.insert(name);
                    }
                }
            } else if !self.member_is_abstract(member_idx)
                && let Some(name_idx) = self.get_member_name_node(node)
                && let Some(name) = self.get_property_name(name_idx)
            {
                names.insert(name);
            }
        }
        names
    }
}
