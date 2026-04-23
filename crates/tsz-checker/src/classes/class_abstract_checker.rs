//! Type-level abstract member checking for expression-based heritage (TS2515/TS2654).
//!
//! When a class extends an expression (e.g., `extends MixedBase` where `MixedBase`
//! is a const holding a mixin call result), the base class is not a direct AST class
//! declaration. This module resolves the instance type via the solver and walks
//! contributing class declarations to find abstract members that must be implemented.

use crate::diagnostics::diagnostic_codes;
use crate::state::CheckerState;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::NodeArena;
use tsz_parser::parser::syntax_kind_ext;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    /// Type-level fallback for TS2515/TS2654 checking when the base class is not
    /// a direct class declaration.
    pub(crate) fn check_abstract_members_from_type(
        &mut self,
        class_idx: NodeIndex,
        class_data: &tsz_parser::parser::node::ClassData,
        heritage_expr_idx: Option<NodeIndex>,
        heritage_type_idx: Option<NodeIndex>,
        base_class_name: &str,
    ) {
        let Some(h_expr_idx) = heritage_expr_idx else {
            return;
        };

        let type_arguments = heritage_type_idx.and_then(|tidx| {
            self.ctx
                .arena
                .get(tidx)
                .and_then(|n| self.ctx.arena.get_expr_type_args(n))
                .and_then(|e| e.type_arguments.as_ref())
        });

        let Some(instance_type) =
            self.base_instance_type_from_expression(h_expr_idx, type_arguments)
        else {
            return;
        };
        let heritage_sym_id = self.resolve_heritage_symbol(h_expr_idx);

        // Collect implemented members from the derived class
        let mut implemented_members = rustc_hash::FxHashSet::default();
        for &member_idx in &class_data.members.nodes {
            if let Some(name) = self.get_member_name(member_idx)
                && !self.member_is_abstract(member_idx)
            {
                implemented_members.insert(name);
            }
        }

        // Prefer the resolved heritage symbol for nominal/merged lib classes like
        // `Iterator`, then fall back to the merged instance type walk for mixins and
        // other expression-based heritage.
        let mut missing_members = heritage_sym_id
            .map(|sym_id| self.find_abstract_members_from_symbol(sym_id, &implemented_members))
            .unwrap_or_default();
        if missing_members.is_empty() {
            missing_members =
                self.find_abstract_members_in_type(instance_type, &implemented_members);
        }

        if missing_members.is_empty() {
            return;
        }

        let is_ambient = self.has_declare_modifier(&class_data.modifiers);
        if is_ambient {
            return;
        }

        let derived_class_name = if class_data.name.is_some() {
            if let Some(name_node) = self.ctx.arena.get(class_data.name)
                && let Some(ident) = self.ctx.arena.get_identifier(name_node)
            {
                ident.escaped_text.clone()
            } else {
                String::from("<anonymous>")
            }
        } else {
            String::from("<anonymous>")
        };

        // Determine the base class display name from the instance type.
        let type_base_name = self
            .intersection_instance_display_name(h_expr_idx, type_arguments)
            .or_else(|| {
                heritage_sym_id.and_then(|sym_id| {
                    self.format_symbol_reference_with_type_arguments(sym_id, type_arguments)
                })
            })
            .or_else(|| self.collect_class_names_from_instance_type(instance_type))
            .unwrap_or_else(|| {
                if base_class_name.is_empty() {
                    self.format_type(instance_type)
                } else {
                    base_class_name.to_string()
                }
            });

        let is_class_expression = self
            .ctx
            .arena
            .get(class_idx)
            .is_some_and(|n| n.kind == syntax_kind_ext::CLASS_EXPRESSION);

        let error_node = if is_class_expression {
            class_idx
        } else if class_data.name.is_some() {
            class_data.name
        } else {
            class_idx
        };

        if missing_members.len() == 1 {
            if is_class_expression {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class expression does not implement inherited abstract member '{}' from class '{}'.",
                        missing_members[0], type_base_name
                    ),
                    2653,
                );
            } else {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class '{}' does not implement inherited abstract member {} from class '{}'.",
                        derived_class_name, missing_members[0], type_base_name
                    ),
                    diagnostic_codes::NON_ABSTRACT_CLASS_DOES_NOT_IMPLEMENT_INHERITED_ABSTRACT_MEMBER_FROM_CLASS,
                );
            }
        } else {
            let missing_list = missing_members
                .iter()
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(", ");

            if is_class_expression {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class expression is missing implementations for the following members of '{type_base_name}': {missing_list}."
                    ),
                    2656,
                );
            } else {
                self.error_at_node(
                    error_node,
                    &format!(
                        "Non-abstract class '{derived_class_name}' is missing implementations for the following members of '{type_base_name}': {missing_list}."
                    ),
                    diagnostic_codes::NON_ABSTRACT_CLASS_IS_MISSING_IMPLEMENTATIONS_FOR_THE_FOLLOWING_MEMBERS_OF,
                );
            }
        }
    }

    /// Walk an instance type (possibly an intersection) and find abstract members
    /// from contributing class declarations that are not in `implemented_members`.
    fn find_abstract_members_in_type(
        &self,
        instance_type: TypeId,
        implemented_members: &rustc_hash::FxHashSet<String>,
    ) -> Vec<String> {
        let mut missing: Vec<String> = Vec::new();
        let type_ids_to_check =
            crate::query_boundaries::common::intersection_members(self.ctx.types, instance_type)
                .unwrap_or_else(|| vec![instance_type]);

        for type_id in type_ids_to_check {
            let Some(shape) =
                crate::query_boundaries::common::object_shape_for_type(self.ctx.types, type_id)
            else {
                continue;
            };

            // Strategy 1: class symbol on the shape itself
            if let Some(class_sym_id) = shape.symbol
                && let Some(symbol) = self.get_symbol_globally(class_sym_id)
                && let Some(ref members_table) = symbol.members
            {
                for (name, member_sym_id) in members_table.iter() {
                    if implemented_members.contains(name) || missing.contains(name) {
                        continue;
                    }
                    if self.member_symbol_is_abstract_global(*member_sym_id) {
                        missing.push(name.clone());
                    }
                }
            }

            // Strategy 2: check individual properties via parent_id
            // Handles merged/flattened types where shape.symbol is None
            for prop in &shape.properties {
                let Some(parent_sym_id) = prop.parent_id else {
                    continue;
                };
                let name = self.ctx.types.resolve_atom(prop.name);
                if implemented_members.contains(&name) || missing.contains(&name) {
                    continue;
                }
                if self.is_property_abstract_via_parent(parent_sym_id, &name) {
                    missing.push(name);
                }
            }
        }

        missing
    }

    /// Collect class/interface names from an instance type by inspecting its
    /// structure. For intersections, walks each member; for objects, collects
    /// names from properties' `parent_id` symbols.
    fn collect_class_names_from_instance_type(&self, instance_type: TypeId) -> Option<String> {
        let mut names: Vec<String> = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();

        if let Some(members) =
            crate::query_boundaries::common::intersection_members(self.ctx.types, instance_type)
        {
            for &member_id in members.iter() {
                if let Some(shape) = crate::query_boundaries::common::object_shape_for_type(
                    self.ctx.types,
                    member_id,
                ) && let Some(sym_id) = shape.symbol
                    && seen.insert(sym_id)
                    && let Some(symbol) = self.get_symbol_globally(sym_id)
                    && !symbol.escaped_name.is_empty()
                    && symbol.escaped_name != "__type"
                {
                    names.push(symbol.escaped_name.clone());
                }
            }
        } else if let Some(shape) =
            crate::query_boundaries::common::object_shape_for_type(self.ctx.types, instance_type)
        {
            if let Some(sym_id) = shape.symbol
                && seen.insert(sym_id)
                && let Some(symbol) = self.get_symbol_globally(sym_id)
                && !symbol.escaped_name.is_empty()
                && symbol.escaped_name != "__type"
            {
                names.push(symbol.escaped_name.clone());
            }
            for prop in &shape.properties {
                if let Some(parent_sym_id) = prop.parent_id
                    && seen.insert(parent_sym_id)
                    && let Some(symbol) = self.get_symbol_globally(parent_sym_id)
                    && !symbol.escaped_name.is_empty()
                    && symbol.escaped_name != "__type"
                {
                    names.push(symbol.escaped_name.clone());
                }
            }
        }

        if names.is_empty() {
            None
        } else {
            Some(names.join(" & "))
        }
    }

    /// Check if a property is abstract by tracing through its parent symbol.
    fn is_property_abstract_via_parent(
        &self,
        parent_sym_id: tsz_binder::SymbolId,
        member_name: &str,
    ) -> bool {
        let Some(symbol) = self.get_symbol_globally(parent_sym_id) else {
            return false;
        };

        if let Some(ref members_table) = symbol.members
            && let Some(member_sym_id) = members_table.get(member_name)
        {
            return self.member_symbol_is_abstract_global(member_sym_id);
        }

        // Fallback: check class declarations directly
        for &decl_idx in &symbol.declarations {
            let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(class_decl) = self.ctx.arena.get_class(decl_node) else {
                continue;
            };
            for &member_idx in &class_decl.members.nodes {
                if let Some(name) = self.get_member_name(member_idx)
                    && name == member_name
                    && self.member_is_abstract(member_idx)
                {
                    return true;
                }
            }
        }

        false
    }

    fn find_abstract_members_from_symbol(
        &self,
        sym_id: tsz_binder::SymbolId,
        implemented_members: &rustc_hash::FxHashSet<String>,
    ) -> Vec<String> {
        let mut missing = Vec::new();
        let Some(symbol) = self.get_symbol_globally(sym_id) else {
            return missing;
        };

        if let Some(ref members_table) = symbol.members {
            for (name, member_sym_id) in members_table.iter() {
                if implemented_members.contains(name) || missing.contains(name) {
                    continue;
                }
                if self.member_symbol_is_abstract_global(*member_sym_id) {
                    missing.push(name.clone());
                }
            }
        }

        missing
    }

    pub(crate) fn format_symbol_reference_with_type_arguments(
        &mut self,
        sym_id: tsz_binder::SymbolId,
        type_arguments: Option<&tsz_parser::parser::base::NodeList>,
    ) -> Option<String> {
        let symbol = self.get_symbol_globally(sym_id)?;
        let name = symbol.escaped_name.clone();
        if name.is_empty() || name == "__type" {
            return None;
        }

        let type_params = self
            .class_type_params_for_symbol(sym_id)
            .filter(|params| !params.is_empty())
            .unwrap_or_else(|| self.get_type_params_for_symbol(sym_id));
        if type_params.is_empty() {
            // When the target symbol's type parameters cannot be resolved
            // (e.g. cross-file class like React.Component), still honor
            // explicit type arguments the user wrote — tsc shows them.
            if let Some(type_arguments) = type_arguments
                && !type_arguments.nodes.is_empty()
            {
                let arg_strs: Vec<String> = type_arguments
                    .nodes
                    .iter()
                    .map(|&arg_idx| {
                        let arg_ty = self.get_type_from_type_node(arg_idx);
                        self.format_type(arg_ty)
                    })
                    .collect();
                return Some(format!("{}<{}>", name, arg_strs.join(", ")));
            }
            return Some(name);
        }

        let mut args = Vec::new();
        if let Some(type_arguments) = type_arguments {
            for &arg_idx in &type_arguments.nodes {
                args.push(self.get_type_from_type_node(arg_idx));
            }
        }

        if args.len() < type_params.len() {
            for param in type_params.iter().skip(args.len()) {
                args.push(
                    param
                        .default
                        .or(param.constraint)
                        .unwrap_or(TypeId::UNKNOWN),
                );
            }
        }
        if args.len() > type_params.len() {
            args.truncate(type_params.len());
        }

        Some(format!(
            "{}<{}>",
            name,
            args.iter()
                .map(|&arg| self.format_type(arg))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }

    /// Build the display name for a base class resolved only via its instance
    /// type (no symbol available). When the user supplied explicit type
    /// arguments in the heritage clause, tsc renders them verbatim (e.g.
    /// `Component<U, {}>` rather than `Component`). Without explicit args we
    /// fall back to whatever the type printer produces.
    pub(crate) fn format_heritage_instance_display(
        &mut self,
        instance_type: TypeId,
        _h_expr_idx: tsz_parser::parser::base::NodeIndex,
        type_arguments: Option<&tsz_parser::parser::base::NodeList>,
    ) -> String {
        let base_str = self.format_type(instance_type);
        if let Some(type_arguments) = type_arguments
            && !type_arguments.nodes.is_empty()
        {
            // Strip any trailing type args already present so we can reapply
            // the user-supplied ones for tsc parity.
            let bare_name: String = match base_str.find('<') {
                Some(lt) => base_str[..lt].to_string(),
                None => base_str.clone(),
            };
            if !bare_name.is_empty() {
                let arg_strs: Vec<String> = type_arguments
                    .nodes
                    .iter()
                    .map(|&arg_idx| {
                        let arg_ty = self.get_type_from_type_node(arg_idx);
                        self.format_type(arg_ty)
                    })
                    .collect();
                return format!("{}<{}>", bare_name, arg_strs.join(", "));
            }
        }
        base_str
    }

    fn class_type_params_for_symbol(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let symbol = self.get_symbol_globally(sym_id)?;
        let symbol_name = symbol.escaped_name.clone();

        for decl_idx in symbol.all_declarations() {
            if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
                for arena in arenas {
                    if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                        if let Some(params) = Self::extract_class_type_params_from_current_arena(
                            self,
                            decl_idx,
                            &symbol_name,
                        ) {
                            return Some(params);
                        }
                    } else {
                        if !Self::enter_cross_arena_delegation() {
                            continue;
                        }
                        let mut checker = Box::new(CheckerState::with_parent_cache(
                            arena.as_ref(),
                            self.ctx.binder,
                            self.ctx.types,
                            self.ctx.file_name.clone(),
                            self.ctx.compiler_options.clone(),
                            self,
                        ));
                        let params = Self::extract_class_type_params_from_current_arena(
                            &mut checker,
                            decl_idx,
                            &symbol_name,
                        );
                        Self::leave_cross_arena_delegation();
                        if params.is_some() {
                            return params;
                        }
                    }
                }
            } else if let Some(params) =
                Self::extract_class_type_params_from_current_arena(self, decl_idx, &symbol_name)
            {
                return Some(params);
            }
        }

        None
    }

    fn extract_class_type_params_from_current_arena(
        checker: &mut CheckerState<'_>,
        decl_idx: NodeIndex,
        symbol_name: &str,
    ) -> Option<Vec<tsz_solver::TypeParamInfo>> {
        let node = checker.ctx.arena.get(decl_idx)?;
        let class = checker.ctx.arena.get_class(node)?;

        if class.name.is_some()
            && let Some(name_node) = checker.ctx.arena.get(class.name)
            && let Some(ident) = checker.ctx.arena.get_identifier(name_node)
            && ident.escaped_text != symbol_name
        {
            return None;
        }

        let (params, updates) = checker.push_type_parameters(&class.type_parameters);
        checker.pop_type_parameters(updates);
        Some(params)
    }

    fn member_symbol_is_abstract_global(&self, member_sym_id: tsz_binder::SymbolId) -> bool {
        let Some(member_sym) = self.get_symbol_globally(member_sym_id) else {
            return false;
        };

        member_sym
            .declarations
            .iter()
            .any(|&decl_idx| self.member_declaration_is_abstract_global(member_sym_id, decl_idx))
    }

    fn member_declaration_is_abstract_global(
        &self,
        sym_id: tsz_binder::SymbolId,
        decl_idx: NodeIndex,
    ) -> bool {
        if let Some(arenas) = self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx)) {
            for arena in arenas {
                if Self::member_is_abstract_in_arena(arena, decl_idx) {
                    return true;
                }
            }
        }

        Self::member_is_abstract_in_arena(self.ctx.arena, decl_idx)
    }

    fn member_is_abstract_in_arena(arena: &NodeArena, member_idx: NodeIndex) -> bool {
        let Some(node) = arena.get(member_idx) else {
            return false;
        };

        match node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                arena.get_property_decl(node).is_some_and(|prop| {
                    prop.modifiers.as_ref().is_some_and(|mods| {
                        mods.nodes.iter().any(|&mod_idx| {
                            arena.get(mod_idx).is_some_and(|mod_node| {
                                mod_node.kind == tsz_scanner::SyntaxKind::AbstractKeyword as u16
                            })
                        })
                    })
                })
            }
            k if k == syntax_kind_ext::METHOD_DECLARATION => {
                arena.get_method_decl(node).is_some_and(|method| {
                    method.modifiers.as_ref().is_some_and(|mods| {
                        mods.nodes.iter().any(|&mod_idx| {
                            arena.get(mod_idx).is_some_and(|mod_node| {
                                mod_node.kind == tsz_scanner::SyntaxKind::AbstractKeyword as u16
                            })
                        })
                    })
                })
            }
            k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                arena.get_accessor(node).is_some_and(|accessor| {
                    accessor.modifiers.as_ref().is_some_and(|mods| {
                        mods.nodes.iter().any(|&mod_idx| {
                            arena.get(mod_idx).is_some_and(|mod_node| {
                                mod_node.kind == tsz_scanner::SyntaxKind::AbstractKeyword as u16
                            })
                        })
                    })
                })
            }
            _ => false,
        }
    }
}
