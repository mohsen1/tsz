//! Property-readonly and class-name utility methods.
//!
//! Extracted from `enum_utils.rs` to keep both modules under the 2000-LOC
//! arch-guard limit. These methods remain inherent methods on `CheckerState`,
//! so call sites are unchanged.

use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Property Readonly Helper Functions
    // =========================================================================

    /// Check if a class property is readonly.
    ///
    /// Looks up the class by name, finds the property member declaration,
    /// and checks if it has a readonly modifier.
    pub(crate) fn is_class_property_readonly(&self, class_name: &str, prop_name: &str) -> bool {
        let Some(class_sym_id) = self.get_symbol_by_name(class_name) else {
            return false;
        };
        let Some(class_sym) = self.ctx.binder.get_symbol(class_sym_id) else {
            return false;
        };
        if class_sym.value_declaration.is_none() {
            return false;
        }
        let Some(class_node) = self.ctx.arena.get(class_sym.value_declaration) else {
            return false;
        };
        let Some(class_data) = self.ctx.arena.get_class(class_node) else {
            return false;
        };
        for &member_idx in &class_data.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if let Some(prop_decl) = self.ctx.arena.get_property_decl(member_node) {
                let member_name = self.get_identifier_text_from_idx(prop_decl.name);
                if member_name.as_deref() == Some(prop_name) {
                    return self.has_readonly_modifier(&prop_decl.modifiers)
                        || self.jsdoc_has_readonly_tag(member_idx);
                }
            }
        }
        false
    }

    /// Check if an interface property is readonly by looking up the interface declaration in the AST.
    ///
    /// Given a type name (e.g., "I"), finds the interface declaration and checks
    /// if the named property has a readonly modifier.
    pub(crate) fn is_interface_property_readonly(&self, type_name: &str, prop_name: &str) -> bool {
        use tsz_parser::parser::syntax_kind_ext::PROPERTY_SIGNATURE;

        let Some(sym_id) = self.get_symbol_by_name(type_name) else {
            return false;
        };
        let Some(sym) = self.ctx.binder.get_symbol(sym_id) else {
            return false;
        };
        // Check all declarations (interfaces can be merged)
        for &decl_idx in &sym.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(iface_data) = self.ctx.arena.get_interface(decl_node) else {
                continue;
            };
            for &member_idx in &iface_data.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != PROPERTY_SIGNATURE {
                    continue;
                }
                let Some(sig) = self.ctx.arena.get_signature(member_node) else {
                    continue;
                };
                let member_name = self.get_identifier_text_from_idx(sig.name);
                if member_name.as_deref() == Some(prop_name) {
                    return self.has_readonly_modifier(&sig.modifiers);
                }
            }
        }
        false
    }

    /// Get the declared type name from a variable expression.
    ///
    /// For `declare const obj: I`, given the expression node for `obj`,
    /// returns "I" (the type reference name from the variable's type annotation).
    pub(crate) fn get_declared_type_name_from_expression(
        &self,
        expr_idx: NodeIndex,
    ) -> Option<String> {
        let node = self.ctx.arena.get(expr_idx)?;

        // Must be an identifier
        self.ctx.arena.get_identifier(node)?;

        // Resolve the variable's symbol
        let sym_id = self.resolve_identifier_symbol(expr_idx)?;
        let sym = self.ctx.binder.get_symbol(sym_id)?;

        // Get the variable's declaration
        if sym.value_declaration.is_none() {
            return None;
        }
        let decl_node = self.ctx.arena.get(sym.value_declaration)?;
        let var_decl = self.ctx.arena.get_variable_declaration(decl_node)?;

        // Get the type annotation
        if var_decl.type_annotation.is_none() {
            return None;
        }
        let type_node = self.ctx.arena.get(var_decl.type_annotation)?;

        // If it's a type reference, get the name
        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
            return self.get_identifier_text_from_idx(type_ref.type_name);
        }

        None
    }

    /// Check if a property of a type is readonly.
    ///
    /// Delegates to the solver's comprehensive implementation which handles:
    /// - `ReadonlyType` wrappers (readonly arrays/tuples)
    /// - Object types with readonly properties
    /// - `ObjectWithIndex` types (readonly index signatures)
    /// - Union types (readonly if ANY member has readonly property)
    /// - Intersection types (readonly ONLY if ALL members have readonly property)
    pub(crate) fn is_property_readonly(&self, type_id: TypeId, prop_name: &str) -> bool {
        self.ctx.types.is_property_readonly(type_id, prop_name)
    }

    /// Get the class name from a variable declaration.
    ///
    /// Returns the class name if the variable is initialized with a class expression.
    pub(crate) fn get_class_name_from_var_decl(&self, decl_idx: NodeIndex) -> Option<String> {
        let var_decl = self.ctx.arena.get_variable_declaration_at(decl_idx)?;

        if var_decl.initializer.is_none() {
            return None;
        }

        let init_node = self.ctx.arena.get(var_decl.initializer)?;
        if init_node.kind != syntax_kind_ext::CLASS_EXPRESSION {
            return None;
        }

        let class = self.ctx.arena.get_class(init_node)?;
        if class.name.is_none() {
            return None;
        }

        let ident = self.ctx.arena.get_identifier_at(class.name)?;
        Some(ident.escaped_text.to_string())
    }
}
