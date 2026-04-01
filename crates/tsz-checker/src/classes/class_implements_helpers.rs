//! Helper methods for class implements checking.
//! - Interface-extends-class accessibility checks
//! - Private/protected member detection
//! - Inherited public member collection

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
                    if symbol.value_declaration.is_some() {
                        symbol.value_declaration
                    } else if let Some(&d) = symbol.declarations.first() {
                        d
                    } else {
                        continue;
                    }
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

                // Collect public members from the base class
                for &member_idx in &base_class.members.nodes {
                    if let Some(name) = self.get_member_name(member_idx) {
                        if direct_members.contains_key(&name) || result.contains_key(&name) {
                            continue;
                        }
                        let sym_flags = self
                            .ctx
                            .binder
                            .get_node_symbol(member_idx)
                            .and_then(|sid| self.ctx.binder.get_symbol(sid))
                            .map(|s| s.flags)
                            .unwrap_or(0);
                        if (sym_flags & tsz_binder::symbol_flags::PRIVATE) != 0
                            || (sym_flags & tsz_binder::symbol_flags::PROTECTED) != 0
                        {
                            continue;
                        }
                        let member_type = self.get_type_of_class_member(member_idx);
                        result.insert(name, member_type);
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
                                result.insert(name, member_type);
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
                    if symbol.value_declaration.is_some() {
                        symbol.value_declaration
                    } else if let Some(&d) = symbol.declarations.first() {
                        d
                    } else {
                        continue;
                    }
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
}
