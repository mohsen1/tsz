//! Constructor Type Checking Module
//!
//! This module contains methods for validating constructor types and their properties.
//! It handles:
//! - Mixin pattern type refinement
//! - Instance type extraction from constructors
//! - Constructor property merging
//! - Abstract constructor assignability
//! - Constructor accessibility (public/private/protected)
//!
//! This module extends CheckerState with constructor-related methods as part of
//! the Phase 2 architecture refactoring (task 2.3 - file splitting).

use crate::binder::symbol_flags;
use crate::checker::state::{CheckerState, MAX_TREE_WALK_ITERATIONS, MemberAccessLevel};
use crate::interner::Atom;
use crate::parser::NodeIndex;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use rustc_hash::FxHashSet;

// =============================================================================
// Constructor Type Checking Methods
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Mixin Pattern Support
    // =========================================================================

    /// Refine the return type of a mixin call expression.
    ///
    /// For mixin patterns like `function Mixin<T>(Base: T)`, this refines
    /// the return type to include base class instance properties.
    pub(crate) fn refine_mixin_call_return_type(
        &mut self,
        callee_idx: NodeIndex,
        arg_types: &[TypeId],
        return_type: TypeId,
    ) -> TypeId {
        if return_type == TypeId::ANY || return_type == TypeId::ERROR {
            return return_type;
        }

        let Some(func_decl_idx) = self.function_decl_from_callee(callee_idx) else {
            return return_type;
        };
        let Some(func_node) = self.ctx.arena.get(func_decl_idx) else {
            return return_type;
        };
        let Some(func) = self.ctx.arena.get_function(func_node) else {
            return return_type;
        };
        let Some(class_expr_idx) = self.returned_class_expression(func.body) else {
            return return_type;
        };
        let Some(base_param_index) = self.mixin_base_param_index(class_expr_idx, func) else {
            return return_type;
        };
        let Some(&base_arg_type) = arg_types.get(base_param_index) else {
            return return_type;
        };
        if matches!(base_arg_type, TypeId::ANY | TypeId::ERROR) {
            return return_type;
        }

        let mut refined_return = self.ctx.types.intersection2(return_type, base_arg_type);

        if let Some(base_instance_type) = self.instance_type_from_constructor_type(base_arg_type) {
            refined_return = self
                .merge_base_instance_into_constructor_return(refined_return, base_instance_type);
        }

        let base_props = self.static_properties_from_type(base_arg_type);
        if !base_props.is_empty() {
            refined_return = self.merge_base_constructor_properties_into_constructor_return(
                refined_return,
                &base_props,
            );
        }

        refined_return
    }

    /// Find the parameter index that represents the base class in a mixin.
    pub(crate) fn mixin_base_param_index(
        &self,
        class_expr_idx: NodeIndex,
        func: &crate::parser::node::FunctionData,
    ) -> Option<usize> {
        let class_node = self.ctx.arena.get(class_expr_idx)?;
        let class_data = self.ctx.arena.get_class(class_node)?;
        let heritage_clauses = class_data.heritage_clauses.as_ref()?;

        let mut base_name = None;
        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };
            let expr_node = self.ctx.arena.get(expr_idx)?;
            if expr_node.kind != SyntaxKind::Identifier as u16 {
                return None;
            }
            let ident = self.ctx.arena.get_identifier(expr_node)?;
            base_name = Some(ident.escaped_text.clone());
            break;
        }

        let base_name = base_name?;
        let mut arg_index = 0usize;
        for &param_idx in &func.parameters.nodes {
            let param_node = self.ctx.arena.get(param_idx)?;
            let param = self.ctx.arena.get_parameter(param_node)?;
            let name_node = self.ctx.arena.get(param.name)?;
            let ident = self.ctx.arena.get_identifier(name_node)?;
            if ident.escaped_text == "this" {
                continue;
            }
            if ident.escaped_text == base_name {
                return Some(arg_index);
            }
            arg_index += 1;
        }

        None
    }

    // =========================================================================
    // Instance Type Extraction
    // =========================================================================

    /// Get the instance type from a constructor type.
    ///
    /// For constructor types like `typeof MyClass`, returns the instance type.
    pub(crate) fn instance_type_from_constructor_type(
        &mut self,
        ctor_type: TypeId,
    ) -> Option<TypeId> {
        let mut visited = FxHashSet::default();
        self.instance_type_from_constructor_type_inner(ctor_type, &mut visited)
    }

    pub(crate) fn instance_type_from_constructor_type_inner(
        &mut self,
        ctor_type: TypeId,
        visited: &mut FxHashSet<TypeId>,
    ) -> Option<TypeId> {
        use crate::solver::type_queries::{InstanceTypeKind, classify_for_instance_type};

        if ctor_type == TypeId::ERROR {
            return None;
        }
        if ctor_type == TypeId::ANY {
            return Some(TypeId::ANY);
        }

        let mut current = ctor_type;
        let mut iterations = 0;
        loop {
            iterations += 1;
            if iterations > MAX_TREE_WALK_ITERATIONS {
                return None;
            }
            if !visited.insert(current) {
                return None;
            }
            current = self.evaluate_application_type(current);
            match classify_for_instance_type(self.ctx.types, current) {
                InstanceTypeKind::Callable(shape_id) => {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    let mut returns = Vec::new();
                    for sig in &shape.construct_signatures {
                        returns.push(sig.return_type);
                    }
                    if returns.is_empty() {
                        return None;
                    }
                    let instance_type = if returns.len() == 1 {
                        returns[0]
                    } else {
                        self.ctx.types.union(returns)
                    };
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                InstanceTypeKind::Function(shape_id) => {
                    let shape = self.ctx.types.function_shape(shape_id);
                    if !shape.is_constructor {
                        return None;
                    }
                    return Some(self.resolve_type_for_property_access(shape.return_type));
                }
                InstanceTypeKind::Intersection(members) => {
                    let mut instance_types = Vec::new();
                    for member in members {
                        if let Some(instance_type) =
                            self.instance_type_from_constructor_type_inner(member, visited)
                        {
                            instance_types.push(instance_type);
                        }
                    }
                    if instance_types.is_empty() {
                        return None;
                    }
                    let instance_type = if instance_types.len() == 1 {
                        instance_types[0]
                    } else {
                        self.ctx.types.intersection(instance_types)
                    };
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                InstanceTypeKind::Union(members) => {
                    let mut instance_types = Vec::new();
                    for member in members {
                        if let Some(instance_type) =
                            self.instance_type_from_constructor_type_inner(member, visited)
                        {
                            instance_types.push(instance_type);
                        }
                    }
                    if instance_types.is_empty() {
                        return None;
                    }
                    let instance_type = if instance_types.len() == 1 {
                        instance_types[0]
                    } else {
                        self.ctx.types.union(instance_types)
                    };
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                InstanceTypeKind::Readonly(inner) => {
                    return self.instance_type_from_constructor_type_inner(inner, visited);
                }
                InstanceTypeKind::TypeParameter { constraint } => {
                    let Some(constraint) = constraint else {
                        return None;
                    };
                    current = constraint;
                }
                InstanceTypeKind::NeedsEvaluation => {
                    let evaluated = self.evaluate_type_with_env(current);
                    if evaluated == current {
                        return None;
                    }
                    current = evaluated;
                }
                InstanceTypeKind::NotConstructor => return None,
            }
        }
    }

    // =========================================================================
    // Constructor Return Type Merging
    // =========================================================================

    /// Merge base instance properties into a constructor's return type.
    pub(crate) fn merge_base_instance_into_constructor_return(
        &mut self,
        ctor_type: TypeId,
        base_instance_type: TypeId,
    ) -> TypeId {
        use crate::solver::type_queries::{
            ConstructorReturnMergeKind, classify_for_constructor_return_merge,
        };

        match classify_for_constructor_return_merge(self.ctx.types, ctor_type) {
            ConstructorReturnMergeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                if shape.construct_signatures.is_empty() {
                    return ctor_type;
                }
                let mut new_shape = (*shape).clone();
                new_shape.construct_signatures = shape
                    .construct_signatures
                    .iter()
                    .map(|sig| {
                        let mut updated = sig.clone();
                        updated.return_type = self
                            .ctx
                            .types
                            .intersection2(updated.return_type, base_instance_type);
                        updated
                    })
                    .collect();
                self.ctx.types.callable(new_shape)
            }
            ConstructorReturnMergeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if !shape.is_constructor {
                    return ctor_type;
                }
                let mut new_shape = (*shape).clone();
                new_shape.return_type = self
                    .ctx
                    .types
                    .intersection2(new_shape.return_type, base_instance_type);
                self.ctx.types.function(new_shape)
            }
            ConstructorReturnMergeKind::Intersection(members) => {
                let mut updated_members = Vec::with_capacity(members.len());
                let mut changed = false;
                for member in members {
                    let updated = self
                        .merge_base_instance_into_constructor_return(member, base_instance_type);
                    if updated != member {
                        changed = true;
                    }
                    updated_members.push(updated);
                }
                if changed {
                    self.ctx.types.intersection(updated_members)
                } else {
                    ctor_type
                }
            }
            ConstructorReturnMergeKind::Other => ctor_type,
        }
    }

    /// Merge base constructor static properties into a constructor's return type.
    pub(crate) fn merge_base_constructor_properties_into_constructor_return(
        &mut self,
        ctor_type: TypeId,
        base_props: &rustc_hash::FxHashMap<Atom, crate::solver::PropertyInfo>,
    ) -> TypeId {
        use crate::solver::type_queries::{
            ConstructorReturnMergeKind, classify_for_constructor_return_merge,
        };
        use rustc_hash::FxHashMap;

        if base_props.is_empty() {
            return ctor_type;
        }

        match classify_for_constructor_return_merge(self.ctx.types, ctor_type) {
            ConstructorReturnMergeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                let mut prop_map: FxHashMap<Atom, crate::solver::PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| (prop.name, prop.clone()))
                    .collect();
                for (name, prop) in base_props.iter() {
                    prop_map.entry(*name).or_insert_with(|| prop.clone());
                }
                let mut new_shape = (*shape).clone();
                new_shape.properties = prop_map.into_values().collect();
                self.ctx.types.callable(new_shape)
            }
            ConstructorReturnMergeKind::Intersection(members) => {
                let mut updated_members = Vec::with_capacity(members.len());
                let mut changed = false;
                for member in members {
                    let updated = self.merge_base_constructor_properties_into_constructor_return(
                        member, base_props,
                    );
                    if updated != member {
                        changed = true;
                    }
                    updated_members.push(updated);
                }
                if changed {
                    self.ctx.types.intersection(updated_members)
                } else {
                    ctor_type
                }
            }
            ConstructorReturnMergeKind::Function(_) => ctor_type,
            ConstructorReturnMergeKind::Other => ctor_type,
        }
    }

    // =========================================================================
    // Abstract Constructor Assignability
    // =========================================================================

    /// Check if abstract constructor assignability should override the default.
    ///
    /// Abstract constructors cannot be assigned to concrete constructor types.
    pub(crate) fn abstract_constructor_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<bool> {
        let source_is_abstract = self.is_abstract_constructor_type(source, env);

        let source_is_abstract_from_symbols = if !source_is_abstract {
            let mut found_abstract = false;
            for (&sym_id, &cached_type) in self.ctx.symbol_types.iter() {
                if cached_type == source
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && symbol.flags & symbol_flags::CLASS != 0
                    && symbol.flags & symbol_flags::ABSTRACT != 0
                {
                    found_abstract = true;
                    break;
                }
            }
            found_abstract
        } else {
            false
        };

        let final_source_is_abstract = source_is_abstract || source_is_abstract_from_symbols;

        if !final_source_is_abstract {
            return None;
        }

        let target_is_abstract = self.is_abstract_constructor_type(target, env);

        let target_is_abstract_from_symbols = {
            let mut found_abstract = false;
            for (&sym_id, &cached_type) in self.ctx.symbol_types.iter() {
                if cached_type == target
                    && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                    && symbol.flags & symbol_flags::CLASS != 0
                    && symbol.flags & symbol_flags::ABSTRACT != 0
                {
                    found_abstract = true;
                    break;
                }
            }
            found_abstract
        };

        let final_target_is_abstract = target_is_abstract || target_is_abstract_from_symbols;

        if final_target_is_abstract {
            return None;
        }

        let target_is_constructor =
            crate::solver::type_queries::has_construct_signatures(self.ctx.types, target);

        if target_is_constructor {
            return Some(false);
        }

        None
    }

    /// Check if a type is an abstract constructor type.
    pub(crate) fn is_abstract_constructor_type(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> bool {
        use crate::binder::SymbolId;
        use crate::solver::type_queries::{
            AbstractConstructorKind, classify_for_abstract_constructor,
        };

        if self.verify_is_abstract_constructor(type_id) {
            return true;
        }

        match classify_for_abstract_constructor(self.ctx.types, type_id) {
            AbstractConstructorKind::TypeQuery(symbol) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(symbol.0)) {
                    if symbol.flags & symbol_flags::ABSTRACT != 0 {
                        return true;
                    }
                    if symbol.flags & symbol_flags::CLASS != 0 {
                        let decl_idx = if !symbol.value_declaration.is_none() {
                            symbol.value_declaration
                        } else {
                            symbol
                                .declarations
                                .first()
                                .copied()
                                .unwrap_or(NodeIndex::NONE)
                        };
                        if !decl_idx.is_none()
                            && let Some(node) = self.ctx.arena.get(decl_idx)
                            && let Some(class) = self.ctx.arena.get_class(node)
                        {
                            return self.has_abstract_modifier(&class.modifiers);
                        }
                    }
                    false
                } else {
                    false
                }
            }
            AbstractConstructorKind::Ref(symbol) => self
                .resolve_type_env_symbol(symbol, env)
                .map(|resolved| {
                    resolved != type_id && self.is_abstract_constructor_type(resolved, env)
                })
                .unwrap_or(false),
            AbstractConstructorKind::Callable(_shape_id) => {
                if self.verify_is_abstract_constructor(type_id) {
                    return true;
                }
                for (&sym_id, &cached_type) in self.ctx.symbol_types.iter() {
                    if cached_type == type_id {
                        if let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
                            && symbol.flags & symbol_flags::CLASS != 0
                            && symbol.flags & symbol_flags::ABSTRACT != 0
                        {
                            return true;
                        }
                    }
                }
                false
            }
            AbstractConstructorKind::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                self.is_abstract_constructor_type(app.base, env)
            }
            AbstractConstructorKind::NotAbstract => false,
        }
    }

    // =========================================================================
    // Constructor Accessibility
    // =========================================================================

    /// Get the access level of a constructor.
    pub(crate) fn constructor_access_level(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
        visited: &mut FxHashSet<TypeId>,
    ) -> Option<MemberAccessLevel> {
        use crate::solver::type_queries::{ConstructorAccessKind, classify_for_constructor_access};

        if !visited.insert(type_id) {
            return None;
        }

        if self.verify_is_private_constructor(type_id) {
            return Some(MemberAccessLevel::Private);
        }
        if self.verify_is_protected_constructor(type_id) {
            return Some(MemberAccessLevel::Protected);
        }

        match classify_for_constructor_access(self.ctx.types, type_id) {
            ConstructorAccessKind::SymbolRef(symbol) => self
                .resolve_type_env_symbol(symbol, env)
                .and_then(|resolved| {
                    if resolved != type_id {
                        self.constructor_access_level(resolved, env, visited)
                    } else {
                        None
                    }
                }),
            ConstructorAccessKind::Application(app_id) => {
                let app = self.ctx.types.type_application(app_id);
                if app.base != type_id {
                    self.constructor_access_level(app.base, env, visited)
                } else {
                    None
                }
            }
            ConstructorAccessKind::Other => None,
        }
    }

    /// Get the access level of a constructor for a type (wrapper with fresh visited set).
    pub(crate) fn constructor_access_level_for_type(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<MemberAccessLevel> {
        let mut visited = FxHashSet::default();
        self.constructor_access_level(type_id, env, &mut visited)
    }

    /// Check for constructor accessibility mismatch between source and target.
    pub(crate) fn constructor_accessibility_mismatch(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<(Option<MemberAccessLevel>, Option<MemberAccessLevel>)> {
        let source_level = self.constructor_access_level_for_type(source, env);
        let target_level = self.constructor_access_level_for_type(target, env);

        if source_level.is_none() && target_level.is_none() {
            return None;
        }

        let source_rank = Self::constructor_access_rank(source_level);
        let target_rank = Self::constructor_access_rank(target_level);
        if source_rank > target_rank {
            return Some((source_level, target_level));
        }
        None
    }

    /// Check if constructor accessibility should override assignability.
    pub(crate) fn constructor_accessibility_override(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<bool> {
        if self
            .constructor_accessibility_mismatch(source, target, env)
            .is_some()
        {
            return Some(false);
        }
        None
    }

    /// Check for constructor accessibility mismatch in an assignment expression.
    pub(crate) fn constructor_accessibility_mismatch_for_assignment(
        &self,
        left_idx: NodeIndex,
        right_idx: NodeIndex,
    ) -> Option<(Option<MemberAccessLevel>, Option<MemberAccessLevel>)> {
        let source_sym = self.class_symbol_from_expression(right_idx)?;
        let target_sym = self.assignment_target_class_symbol(left_idx)?;
        let source_level = self.class_constructor_access_level(source_sym);
        let target_level = self.class_constructor_access_level(target_sym);
        if source_level.is_none() && target_level.is_none() {
            return None;
        }
        if Self::constructor_access_rank(source_level) > Self::constructor_access_rank(target_level)
        {
            return Some((source_level, target_level));
        }
        None
    }

    /// Check for constructor accessibility mismatch in a variable declaration.
    pub(crate) fn constructor_accessibility_mismatch_for_var_decl(
        &self,
        var_decl: &crate::parser::node::VariableDeclarationData,
    ) -> Option<(Option<MemberAccessLevel>, Option<MemberAccessLevel>)> {
        if var_decl.initializer.is_none() {
            return None;
        }
        let source_sym = self.class_symbol_from_expression(var_decl.initializer)?;
        let target_sym = self.class_symbol_from_type_annotation(var_decl.type_annotation)?;
        let source_level = self.class_constructor_access_level(source_sym);
        let target_level = self.class_constructor_access_level(target_sym);
        if source_level.is_none() && target_level.is_none() {
            return None;
        }
        if Self::constructor_access_rank(source_level) > Self::constructor_access_rank(target_level)
        {
            return Some((source_level, target_level));
        }
        None
    }

    // =========================================================================
    // Type Environment Resolution Helper
    // =========================================================================

    /// Resolve a symbol reference from the type environment.
    pub(crate) fn resolve_type_env_symbol(
        &self,
        symbol: crate::solver::SymbolRef,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<TypeId> {
        if let Some(env) = env {
            return env.get(symbol);
        }
        let env_ref = self.ctx.type_env.borrow();
        env_ref.get(symbol)
    }
}
