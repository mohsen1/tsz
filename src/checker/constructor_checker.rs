//! Constructor Type Checking Utilities Module
//!
//! This module contains constructor type checking utility methods for CheckerState
//! as part of Phase 2 architecture refactoring.
//!
//! The methods in this module handle:
//! - Constructor accessibility checking (private, protected, public)
//! - Constructor signature utilities
//! - Constructor instantiation validation
//! - Mixin call return type refinement
//! - Instance type extraction from constructors
//! - Abstract constructor assignability
//!
//! This module extends CheckerState with utilities for constructor-related
//! type checking operations.

use crate::binder::symbol_flags;
use crate::checker::state::{CheckerState, MAX_TREE_WALK_ITERATIONS, MemberAccessLevel};
use crate::interner::Atom;
use crate::parser::NodeIndex;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;
use crate::solver::type_queries::get_callable_shape;
use crate::solver::type_queries_extended::classify_for_abstract_constructor;
use rustc_hash::FxHashSet;

// =============================================================================
// Constructor Type Checking Utilities
// =============================================================================

impl<'a> CheckerState<'a> {
    // =========================================================================
    // Constructor Accessibility
    // =========================================================================

    /// Check if a type is an abstract constructor type.
    ///
    /// Abstract constructors cannot be instantiated directly with `new`.
    pub fn is_abstract_ctor(&self, type_id: TypeId) -> bool {
        self.ctx.abstract_constructor_types.contains(&type_id)
    }

    /// Check if a type is a private constructor.
    ///
    /// Private constructors can only be called from within the class.
    pub fn is_private_ctor(&self, type_id: TypeId) -> bool {
        self.ctx.private_constructor_types.contains(&type_id)
    }

    /// Check if a type is a protected constructor.
    ///
    /// Protected constructors can be called from the class and its subclasses.
    pub fn is_protected_ctor(&self, type_id: TypeId) -> bool {
        self.ctx.protected_constructor_types.contains(&type_id)
    }

    /// Check if a type is a public constructor.
    ///
    /// Public constructors have no access restrictions.
    pub fn is_public_ctor(&self, type_id: TypeId) -> bool {
        !self.is_private_ctor(type_id) && !self.is_protected_ctor(type_id)
    }

    // =========================================================================
    // Constructor Signature Utilities
    // =========================================================================

    /// Check if a type has any construct signature.
    ///
    /// Construct signatures allow a type to be called with `new`.
    pub fn has_construct_sig(&self, type_id: TypeId) -> bool {
        if let Some(shape) = get_callable_shape(self.ctx.types, type_id) {
            !shape.construct_signatures.is_empty()
        } else {
            false
        }
    }

    /// Get the number of construct signatures for a type.
    ///
    /// Multiple construct signatures indicate constructor overloading.
    pub fn construct_signature_count(&self, type_id: TypeId) -> usize {
        if let Some(shape) = get_callable_shape(self.ctx.types, type_id) {
            shape.construct_signatures.len()
        } else {
            0
        }
    }

    // =========================================================================
    // Constructor Instantiation
    // =========================================================================

    /// Check if a constructor can be instantiated.
    ///
    /// Returns false for abstract constructors which cannot be instantiated.
    pub fn can_instantiate(&self, constructor_type: TypeId) -> bool {
        !self.is_abstract_ctor(constructor_type)
    }

    /// Check if `new` can be applied to a type.
    ///
    /// This is a convenience check combining constructor type detection
    /// with abstract constructor checking.
    pub fn can_use_new(&self, type_id: TypeId) -> bool {
        self.has_construct_sig(type_id) && self.can_instantiate(type_id)
    }

    /// Check if a type is a class constructor (typeof Class).
    ///
    /// Returns true for Callable types with only construct signatures (no call signatures).
    /// This is used to detect when a class constructor is being called without `new`.
    pub fn is_class_constructor_type(&self, type_id: TypeId) -> bool {
        // A class constructor is a Callable with construct signatures but no call signatures
        self.has_construct_sig(type_id) && !self.has_call_signature(type_id)
    }

    /// Check if two constructor types have compatible accessibility.
    ///
    /// Returns true if source can be assigned to target based on their constructor accessibility.
    /// - Public constructors are compatible with everything
    /// - Private constructors are only compatible with the same private constructor
    /// - Protected constructors are compatible with protected or public targets
    pub fn ctor_access_compatible(&self, source: TypeId, target: TypeId) -> bool {
        // Public constructors are compatible with everything
        if !self.is_private_ctor(source) && !self.is_protected_ctor(source) {
            return true;
        }

        // Private constructors are only compatible with the same private constructor
        if self.is_private_ctor(source) {
            if self.is_private_ctor(target) {
                source == target
            } else {
                false
            }
        } else {
            // Protected constructors are compatible with protected or public targets
            !self.is_private_ctor(target)
        }
    }

    /// Check if a type should be treated as a constructor in `new` expressions.
    ///
    /// This determines if a type can be used with the `new` operator.
    pub fn is_newable(&self, type_id: TypeId) -> bool {
        self.has_construct_sig(type_id)
    }

    // =========================================================================
    // Mixin Call Return Type Refinement
    // =========================================================================

    /// Refine the return type of a mixin call by merging base constructor properties.
    ///
    /// When a mixin function returns a class that extends a base parameter,
    /// this function merges the base type's instance type and static properties
    /// into the return type.
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

    fn mixin_base_param_index(
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

    pub(crate) fn instance_type_from_constructor_type(
        &mut self,
        ctor_type: TypeId,
    ) -> Option<TypeId> {
        let mut visited = FxHashSet::default();
        self.instance_type_from_constructor_type_inner(ctor_type, &mut visited)
    }

    fn instance_type_from_constructor_type_inner(
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
                InstanceTypeKind::SymbolRef(sym_ref) => {
                    // Symbol reference (class name or typeof expression)
                    // Resolve to the class instance type
                    use crate::binder::SymbolId;
                    let sym_id = SymbolId(sym_ref.0);
                    if let Some(instance_type) = self.class_instance_type_from_symbol(sym_id) {
                        return Some(self.resolve_type_for_property_access(instance_type));
                    }
                    // Not a class symbol - might be a variable holding a constructor
                    // Try to get its type and recurse
                    let var_type = self.get_type_of_symbol(sym_id);
                    if var_type != TypeId::ERROR && var_type != current {
                        current = var_type;
                    } else {
                        return None;
                    }
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

    fn merge_base_instance_into_constructor_return(
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

    fn merge_base_constructor_properties_into_constructor_return(
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

    pub(crate) fn abstract_constructor_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<bool> {
        use crate::solver::type_queries::AbstractConstructorKind;

        // Helper to check if a TypeId is abstract
        // This handles both TypeQuery types (before resolution) and resolved Callable types
        let is_abstract_type = |type_id: TypeId| -> bool {
            // First check the cached set (handles resolved types)
            if self.is_abstract_ctor(type_id) {
                return true;
            }

            // Then check TypeQuery types
            match classify_for_abstract_constructor(self.ctx.types, type_id) {
                AbstractConstructorKind::TypeQuery(sym_ref) => {
                    if let Some(symbol) = self
                        .ctx
                        .binder
                        .get_symbol(crate::binder::SymbolId(sym_ref.0))
                    {
                        symbol.flags & symbol_flags::ABSTRACT != 0
                    } else {
                        false
                    }
                }
                _ => false,
            }
        };

        let source_is_abstract = is_abstract_type(source);
        let target_is_abstract = is_abstract_type(target);

        // Case 1: Source is concrete, target is abstract -> Allow (concrete can be assigned to abstract)
        if !source_is_abstract && target_is_abstract {
            // Let the structural subtype checker handle it
            return None;
        }

        // Case 2: Source is abstract, target is also abstract -> Let structural check handle it
        if source_is_abstract && target_is_abstract {
            return None;
        }

        // Case 3: Source is abstract, target is NOT abstract -> Reject
        if source_is_abstract && !target_is_abstract {
            let target_is_constructor = self.has_construct_sig(target);
            if target_is_constructor {
                return Some(false);
            }
        }

        None
    }

    // =========================================================================
    // Constructor Access Level
    // =========================================================================

    fn constructor_access_level(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
        visited: &mut FxHashSet<TypeId>,
    ) -> Option<MemberAccessLevel> {
        use crate::solver::type_queries::{ConstructorAccessKind, classify_for_constructor_access};

        if !visited.insert(type_id) {
            return None;
        }

        if self.is_private_ctor(type_id) {
            return Some(MemberAccessLevel::Private);
        }
        if self.is_protected_ctor(type_id) {
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

    fn constructor_access_level_for_type(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> Option<MemberAccessLevel> {
        let mut visited = FxHashSet::default();
        self.constructor_access_level(type_id, env, &mut visited)
    }

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
    // Helper Methods
    // =========================================================================

    fn resolve_type_env_symbol(
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

    fn is_abstract_constructor_type(
        &self,
        type_id: TypeId,
        env: Option<&crate::solver::TypeEnvironment>,
    ) -> bool {
        use crate::binder::SymbolId;
        use crate::solver::type_queries::{
            AbstractConstructorKind, classify_for_abstract_constructor,
        };

        // First check the cached set
        if self.is_abstract_ctor(type_id) {
            return true;
        }

        match classify_for_abstract_constructor(self.ctx.types, type_id) {
            AbstractConstructorKind::TypeQuery(symbol) => {
                if let Some(symbol) = self.ctx.binder.get_symbol(SymbolId(symbol.0)) {
                    // Check if the symbol is marked as abstract
                    if symbol.flags & symbol_flags::ABSTRACT != 0 {
                        return true;
                    }
                    // Also check if this is an abstract class by examining its declaration
                    // The ABSTRACT flag might not be set on the symbol, so check the class modifiers
                    if symbol.flags & symbol_flags::CLASS != 0 {
                        // Get the class declaration and check if it has the abstract modifier
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
                // For Callable types (constructor types), check if they're in the abstract set
                // This handles `typeof AbstractClass` which returns a Callable type
                if self.is_abstract_ctor(type_id) {
                    return true;
                }
                // Additional check: iterate through symbol_types to find matching class symbols
                // This handles cases where the type wasn't added to abstract_constructor_types
                // or the type is being compared before being cached
                for (&sym_id, &cached_type) in self.ctx.symbol_types.iter() {
                    if cached_type == type_id {
                        // Found a symbol with this type, check if it's an abstract class
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
                // For generic type applications, check the base type
                let app = self.ctx.types.type_application(app_id);
                self.is_abstract_constructor_type(app.base, env)
            }
            AbstractConstructorKind::NotAbstract => false,
        }
    }
}
