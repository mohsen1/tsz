//! Constructor type checking (accessibility, signatures, instantiation, mixins).
//! - Instance type extraction from constructors
//! - Abstract constructor assignability
//!
//! This module extends `CheckerState` with utilities for constructor-related
//! type checking operations.

use crate::query_boundaries::checkers::constructor::{
    AbstractConstructorAnchor, ConstructorAccessKind, ConstructorReturnMergeKind, InstanceTypeKind,
    classify_for_constructor_access, classify_for_constructor_return_merge,
    classify_for_instance_type, construct_return_type_for_display, has_construct_signatures,
    resolve_abstract_constructor_anchor,
};
use crate::state::{CheckerState, MAX_TREE_WALK_ITERATIONS, MemberAccessLevel};
use rustc_hash::FxHashSet;
use tsz_binder::{SymbolId, symbol_flags};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

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

    // =========================================================================
    // Constructor Signature Utilities
    // =========================================================================

    /// Check if a type has any construct signature.
    ///
    /// Construct signatures allow a type to be called with `new`.
    pub fn has_construct_sig(&self, type_id: TypeId) -> bool {
        has_construct_signatures(self.ctx.types, type_id)
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

        let factory = self.ctx.types.factory();
        let mut refined_return = factory.intersection2(return_type, base_arg_type);

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
        func: &tsz_parser::parser::node::FunctionData,
    ) -> Option<usize> {
        let class_data = self.ctx.arena.get_class_at(class_expr_idx)?;
        let heritage_clauses = class_data.heritage_clauses.as_ref()?;

        let mut base_name = None;
        for &clause_idx in &heritage_clauses.nodes {
            let heritage = self.ctx.arena.get_heritage_clause_at(clause_idx)?;
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
            let param = self.ctx.arena.get_parameter_at(param_idx)?;
            let ident = self.ctx.arena.get_identifier_at(param.name)?;
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

    /// Compute a display name for an intersection base class that preserves the
    /// original member names (e.g., "I1 & I2" instead of "{ m1: ...; m2: ... }").
    ///
    /// When a constructor type is an intersection (e.g., `C1 & C2` from
    /// `const Foo: C1 & C2`), the instance type gets eagerly merged into a flat
    /// object by the solver's intersection normalization. This loses the original
    /// intersection identity. This method recovers it by:
    /// 1. Getting the constructor type (from cache)
    /// 2. If it's an intersection, extracting each member's raw construct return type
    /// 3. Formatting each return type individually (preserving Lazy → named display)
    /// 4. Joining with " & "
    ///
    /// Returns `None` if the constructor type is not an intersection or if any
    /// member doesn't have construct signatures.
    pub(crate) fn intersection_instance_display_name(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&tsz_parser::NodeList>,
    ) -> Option<String> {
        let ctor_type = self.base_constructor_type_from_expression(expr_idx, type_arguments)?;

        // Only applies to intersection constructor types
        let members = match classify_for_instance_type(self.ctx.types, ctor_type) {
            InstanceTypeKind::Intersection(members) if members.len() >= 2 => members,
            _ => return None,
        };

        let mut names = Vec::with_capacity(members.len());
        for member in &members {
            // Resolve Lazy to see the actual constructor type's shape
            let resolved = self.resolve_lazy_type(*member);
            // Get raw construct return type (without resolve_type_for_property_access)
            if let Some(return_type) = construct_return_type_for_display(self.ctx.types, resolved) {
                // Collect display names from this constructor's return type.
                // If the return type is an intersection (e.g., `AbstractBase & Mixin`),
                // walk each member individually to preserve named type references.
                // Otherwise, handle single types directly.
                self.collect_display_names_from_return_type(return_type, &mut names);
            } else {
                return None;
            }
        }

        // Sort names to approximate source order. The solver's intersection
        // normalization sorts members by TypeId for canonicalization, which may
        // not match the original declaration order. Alphabetical sort produces
        // consistent output matching tsc for common cases (named types come
        // before structural types: "I1 & I2", "A & { ... }").
        names.sort();

        Some(names.join(" & "))
    }

    /// Collect display names from a constructor return type into `names`.
    /// For intersection return types (e.g., `AbstractBase & Mixin`), walks each
    /// member individually to preserve named type references. Deduplicates by name.
    fn collect_display_names_from_return_type(
        &mut self,
        return_type: TypeId,
        names: &mut Vec<String>,
    ) {
        // If the return type is an intersection, walk each member
        if let Some(members_list) =
            tsz_solver::type_queries::get_intersection_members(self.ctx.types, return_type)
        {
            for &member_id in members_list.iter() {
                let name = self.format_single_display_type(member_id);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            return;
        }

        // Single type
        let name = self.format_single_display_type(return_type);
        if !names.contains(&name) {
            names.push(name);
        }
    }

    /// Format a single type for display name purposes.
    /// Lazy (named) types are formatted directly to preserve names.
    /// Structural types are resolved first.
    fn format_single_display_type(&mut self, type_id: TypeId) -> String {
        let is_lazy = matches!(
            crate::query_boundaries::type_computation::complex::classify_for_lazy_resolution(
                self.ctx.types,
                type_id,
            ),
            tsz_solver::type_queries::LazyTypeKind::Lazy(_)
        );
        let display_type = if is_lazy {
            type_id
        } else {
            self.resolve_type_for_property_access(type_id)
        };
        self.format_type(display_type)
    }

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
        if ctor_type == TypeId::NULL {
            return Some(TypeId::NULL);
        }
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
            // Resolve Lazy types so the classifier can see construct signatures.
            let resolved = self.resolve_lazy_type(current);
            if resolved != current {
                current = resolved;
            }
            match classify_for_instance_type(self.ctx.types, current) {
                InstanceTypeKind::Callable(shape_id) => {
                    let instance_type = tsz_solver::type_queries::get_construct_return_type_union(
                        self.ctx.types,
                        shape_id,
                    )?;
                    let resolved = self.resolve_type_for_property_access(instance_type);
                    // Register TypeId→DefId so the TypeFormatter can display the
                    // interface name (e.g., "String", "Date") instead of structural
                    // expansion in diagnostics. The resolve step produces a new TypeId
                    // that loses the Lazy(DefId) wrapper.
                    if resolved != instance_type
                        && let Some(def_id) =
                            tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, instance_type)
                    {
                        self.ctx
                            .definition_store
                            .register_type_to_def(resolved, def_id);
                    }
                    return Some(resolved);
                }
                InstanceTypeKind::Function(_) => {
                    // Delegate to solver query for Function constructor return type
                    let return_type =
                        crate::query_boundaries::common::construct_return_type_for_type(
                            self.ctx.types,
                            current,
                        )?;
                    let resolved = self.resolve_type_for_property_access(return_type);
                    if resolved != return_type
                        && let Some(def_id) =
                            tsz_solver::type_queries::get_lazy_def_id(self.ctx.types, return_type)
                    {
                        self.ctx
                            .definition_store
                            .register_type_to_def(resolved, def_id);
                    }
                    return Some(resolved);
                }
                InstanceTypeKind::Intersection(members) => {
                    let instance_types: Vec<TypeId> = members
                        .into_iter()
                        .filter_map(|m| self.instance_type_from_constructor_type_inner(m, visited))
                        .collect();
                    if instance_types.is_empty() {
                        return None;
                    }
                    let instance_type =
                        tsz_solver::utils::intersection_or_single(self.ctx.types, instance_types);
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                InstanceTypeKind::Union(members) => {
                    let instance_types: Vec<TypeId> = members
                        .into_iter()
                        .filter_map(|m| self.instance_type_from_constructor_type_inner(m, visited))
                        .collect();
                    if instance_types.is_empty() {
                        return None;
                    }
                    let instance_type =
                        tsz_solver::utils::union_or_single(self.ctx.types, instance_types);
                    return Some(self.resolve_type_for_property_access(instance_type));
                }
                InstanceTypeKind::Readonly(inner) => {
                    return self.instance_type_from_constructor_type_inner(inner, visited);
                }
                InstanceTypeKind::TypeParameter { constraint } => {
                    let constraint = constraint?;
                    current = constraint;
                }
                InstanceTypeKind::SymbolRef(sym_ref) => {
                    // Symbol reference (class name or typeof expression)
                    // Resolve to the class instance type
                    use tsz_binder::SymbolId;
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
        // Resolve Lazy types before classification.
        let ctor_type = {
            let resolved = self.resolve_lazy_type(ctor_type);
            if resolved != ctor_type {
                resolved
            } else {
                ctor_type
            }
        };
        match classify_for_constructor_return_merge(self.ctx.types, ctor_type) {
            ConstructorReturnMergeKind::Callable(_) | ConstructorReturnMergeKind::Function(_) => {
                // Delegate to solver: intersect construct/function return types
                // with the base instance type.
                let result = crate::query_boundaries::common::intersect_constructor_returns(
                    self.ctx.types,
                    ctor_type,
                    base_instance_type,
                );
                if result != ctor_type {
                    return result;
                }
                ctor_type
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
                    self.ctx.types.factory().intersection(updated_members)
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
        base_props: &rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
    ) -> TypeId {
        use rustc_hash::FxHashMap;
        if base_props.is_empty() {
            return ctor_type;
        }

        // Resolve Lazy types before classification.
        let ctor_type = {
            let resolved = self.resolve_lazy_type(ctor_type);
            if resolved != ctor_type {
                resolved
            } else {
                ctor_type
            }
        };
        match classify_for_constructor_return_merge(self.ctx.types, ctor_type) {
            ConstructorReturnMergeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);
                let mut prop_map: FxHashMap<Atom, tsz_solver::PropertyInfo> = shape
                    .properties
                    .iter()
                    .map(|prop| (prop.name, prop.clone()))
                    .collect();
                for (name, prop) in base_props {
                    prop_map.entry(*name).or_insert_with(|| prop.clone());
                }
                let mut new_shape = (*shape).clone();
                new_shape.properties = prop_map.into_values().collect();
                self.ctx.types.factory().callable(new_shape)
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
                    self.ctx.types.factory().intersection(updated_members)
                } else {
                    ctor_type
                }
            }
            ConstructorReturnMergeKind::Function(_) | ConstructorReturnMergeKind::Other => {
                ctor_type
            }
        }
    }

    // =========================================================================
    // Abstract Constructor Assignability
    // =========================================================================

    pub(crate) fn abstract_constructor_assignability_override(
        &self,
        source: TypeId,
        target: TypeId,
        _env: Option<&tsz_solver::TypeEnvironment>,
    ) -> Option<bool> {
        // Helper to check if a TypeId is abstract
        // This handles both TypeQuery types (before resolution) and resolved Callable types
        let is_abstract_type = |type_id: TypeId| -> bool {
            // First check the cached set (handles resolved types)
            if self.is_abstract_ctor(type_id) {
                return true;
            }

            // Check if the callable shape itself is marked abstract.
            // This handles anonymous abstract construct signature types like
            // `abstract new (...args: any) => any` from type parameter constraints.
            if let Some(callable_shape) =
                tsz_solver::type_queries::get_callable_shape(self.ctx.types, type_id)
                && callable_shape.is_abstract
            {
                return true;
            }

            // Let solver unwrap application/type-query chains first.
            match resolve_abstract_constructor_anchor(self.ctx.types, type_id) {
                AbstractConstructorAnchor::TypeQuery(sym_ref) => {
                    if let Some(symbol) =
                        self.ctx.binder.get_symbol(tsz_binder::SymbolId(sym_ref.0))
                    {
                        symbol.flags & symbol_flags::ABSTRACT != 0
                    } else {
                        false
                    }
                }
                AbstractConstructorAnchor::CallableType(callable_type) => {
                    self.is_abstract_ctor(callable_type)
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
        env: Option<&tsz_solver::TypeEnvironment>,
        visited: &mut FxHashSet<TypeId>,
    ) -> Option<MemberAccessLevel> {
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
        env: Option<&tsz_solver::TypeEnvironment>,
    ) -> Option<MemberAccessLevel> {
        let mut visited = FxHashSet::default();
        self.constructor_access_level(type_id, env, &mut visited)
    }

    pub(crate) fn constructor_accessibility_mismatch(
        &self,
        source: TypeId,
        target: TypeId,
        env: Option<&tsz_solver::TypeEnvironment>,
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
        env: Option<&tsz_solver::TypeEnvironment>,
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
        var_decl: &tsz_parser::parser::node::VariableDeclarationData,
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
        symbol: tsz_solver::SymbolRef,
        env: Option<&tsz_solver::TypeEnvironment>,
    ) -> Option<TypeId> {
        if let Some(env) = env {
            return env.get(symbol);
        }
        let env_ref = self.ctx.type_env.borrow();
        env_ref.get(symbol)
    }

    /// Check constructor accessibility for a `new` expression.
    ///
    /// Emits TS2673 for private constructors and TS2674 for protected constructors
    /// when called from an invalid scope (outside the class or hierarchy).
    ///
    /// tsc checks ALL enclosing classes in the scope chain, not just the immediately
    /// enclosing one. A nested class inside a method of class A can access A's
    /// private/protected constructor because it's lexically within A's scope.
    pub(crate) fn check_constructor_accessibility_for_new(
        &mut self,
        new_expr_idx: tsz_parser::parser::NodeIndex,
        constructor_type: TypeId,
    ) {
        // Skip check for `any` and `error` types
        if constructor_type == TypeId::ANY || constructor_type == TypeId::ERROR {
            return;
        }

        // Check if constructor is private or protected
        let is_private = self.is_private_ctor(constructor_type);
        let is_protected = self.is_protected_ctor(constructor_type);

        if !is_private && !is_protected {
            return; // Public constructor - no restrictions
        }

        // Find the class symbol being instantiated
        let class_sym = match self.class_symbol_from_new_expr(new_expr_idx) {
            Some(sym) => sym,
            None => return, // Can't determine class - skip check
        };

        // Walk ALL enclosing classes in the scope chain. If ANY enclosing class
        // matches the target class (for private) or is a subclass (for protected),
        // access is allowed. This handles nested classes inside methods:
        //   class A { private constructor() {}
        //     method() { class B { method() { new A(); /* OK */ } } }
        //   }
        let enclosing_classes = self.find_all_enclosing_classes(new_expr_idx);

        if enclosing_classes.is_empty() {
            // No enclosing class - external instantiation
            self.emit_constructor_access_error(new_expr_idx, class_sym, is_private);
            return;
        }

        for &enclosing_sym in &enclosing_classes {
            if enclosing_sym == class_sym {
                // Same class - always allowed (even for private constructors)
                return;
            }
            if is_protected {
                // Check the inheritance graph first (fast path).
                // Fall back to walking heritage clauses directly, because the
                // enclosing class's inheritance may not be registered yet when
                // property initializers are type-checked.
                if self
                    .ctx
                    .inheritance_graph
                    .is_derived_from(enclosing_sym, class_sym)
                    || self.is_heritage_derived_from(enclosing_sym, class_sym)
                {
                    // Protected constructor accessible from subclass
                    return;
                }
            }
        }

        // None of the enclosing classes grant access
        self.emit_constructor_access_error(new_expr_idx, class_sym, is_private);
    }

    /// Find the class symbol from a `new` expression node.
    fn class_symbol_from_new_expr(&self, idx: tsz_parser::parser::NodeIndex) -> Option<SymbolId> {
        use tsz_binder::symbol_flags;

        let call_expr = self.ctx.arena.get_call_expr_at(idx)?;

        // Use scope-aware resolution first (handles namespaces, nested scopes)
        let sym_id = self
            .ctx
            .binder
            .resolve_identifier(self.ctx.arena, call_expr.expression)
            .or_else(|| self.ctx.binder.get_node_symbol(call_expr.expression))
            .or_else(|| {
                let ident = self.ctx.arena.get_identifier_at(call_expr.expression)?;
                self.ctx.binder.file_locals.get(&ident.escaped_text)
            })?;

        let symbol = self.ctx.binder.get_symbol(sym_id)?;

        // Verify it's a class
        (symbol.flags & symbol_flags::CLASS != 0).then_some(sym_id)
    }

    /// Find ALL enclosing class symbols by walking up the AST parent chain.
    ///
    /// Returns all class symbols in the scope chain from innermost to outermost.
    /// This is needed because a nested class inside a method has access to
    /// the outer class's private/protected members (including constructors).
    fn find_all_enclosing_classes(&self, idx: tsz_parser::parser::NodeIndex) -> Vec<SymbolId> {
        use tsz_parser::parser::syntax_kind_ext;

        let mut result = Vec::new();
        let mut current = idx;

        while let Some(ext) = self.ctx.arena.get_extended(current) {
            let parent_idx = ext.parent;
            if parent_idx.is_none() {
                break;
            }
            let Some(parent_node) = self.ctx.arena.get(parent_idx) else {
                break;
            };

            if (parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                || parent_node.kind == syntax_kind_ext::CLASS_EXPRESSION)
                && let Some(class_data) = self.ctx.arena.get_class(parent_node)
            {
                // Try to resolve the class symbol
                let sym_id = self
                    .ctx
                    .binder
                    .get_node_symbol(class_data.name)
                    .or_else(|| self.ctx.binder.get_node_symbol(parent_idx));
                if let Some(sym_id) = sym_id {
                    result.push(sym_id);
                }
            }

            current = parent_idx;
        }

        result
    }

    /// Check if `child_sym` extends `ancestor_sym` by walking heritage clauses.
    ///
    /// This is a fallback for when `InheritanceGraph::is_derived_from` returns
    /// false because the graph hasn't been populated yet (e.g., during property
    /// initializer type-checking before the enclosing class's heritage is registered).
    fn is_heritage_derived_from(&self, child_sym: SymbolId, ancestor_sym: SymbolId) -> bool {
        use tsz_parser::parser::syntax_kind_ext;
        use tsz_scanner::SyntaxKind;

        let child = self.ctx.binder.get_symbol(child_sym);
        let child = match child {
            Some(s) => s,
            None => return false,
        };

        // Walk the class declarations for this symbol
        let decl_idx = if child.value_declaration.is_some() {
            child.value_declaration
        } else {
            match child.declarations.first() {
                Some(&d) => d,
                None => return false,
            }
        };

        let Some(node) = self.ctx.arena.get(decl_idx) else {
            return false;
        };
        if node.kind != syntax_kind_ext::CLASS_DECLARATION
            && node.kind != syntax_kind_ext::CLASS_EXPRESSION
        {
            return false;
        }
        let Some(class_data) = self.ctx.arena.get_class(node) else {
            return false;
        };

        let Some(heritage_clauses) = &class_data.heritage_clauses else {
            return false;
        };

        for &clause_idx in &heritage_clauses.nodes {
            let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                continue;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            for &type_idx in &heritage.types.nodes {
                let expr_idx = self
                    .ctx
                    .arena
                    .get_expr_type_args_at(type_idx)
                    .map_or(type_idx, |e| e.expression);

                // Resolve the heritage expression to a symbol
                let parent_sym = self
                    .ctx
                    .binder
                    .resolve_identifier(self.ctx.arena, expr_idx)
                    .or_else(|| self.ctx.binder.get_node_symbol(expr_idx));

                if let Some(parent_sym) = parent_sym {
                    if parent_sym == ancestor_sym {
                        return true;
                    }
                    // Recurse for transitive inheritance
                    if self.is_heritage_derived_from(parent_sym, ancestor_sym) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Emit the appropriate constructor accessibility error.
    fn emit_constructor_access_error(
        &mut self,
        idx: tsz_parser::parser::NodeIndex,
        class_sym: SymbolId,
        is_private: bool,
    ) {
        use crate::diagnostics::diagnostic_codes;

        let class_name = self.get_symbol_display_name(class_sym);

        if is_private {
            // TS2673: Constructor of class 'X' is private
            let message = format!(
                "Constructor of class '{class_name}' is private and only accessible within the class declaration."
            );
            self.error_at_node(idx, &message, diagnostic_codes::CONSTRUCTOR_OF_CLASS_IS_PRIVATE_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATION);
        } else {
            // TS2674: Constructor of class 'X' is protected
            let message = format!(
                "Constructor of class '{class_name}' is protected and only accessible within the class declaration."
            );
            self.error_at_node(idx, &message, diagnostic_codes::CONSTRUCTOR_OF_CLASS_IS_PROTECTED_AND_ONLY_ACCESSIBLE_WITHIN_THE_CLASS_DECLARATI);
        }
    }

    /// Get the display name of a symbol for error messages.
    ///
    /// For generic classes, includes type parameters: `D<T>` instead of `D`.
    /// This matches tsc's behavior in TS2673/TS2674 diagnostics.
    fn get_symbol_display_name(&self, sym_id: SymbolId) -> String {
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return "<unknown>".to_string();
        };
        let name = symbol.escaped_name.clone();

        // Check if the class declaration has type parameters.
        // Try value_declaration first, then fall back to declarations list.
        let decl_indices: Vec<tsz_parser::parser::NodeIndex> = if symbol.value_declaration.is_some()
        {
            vec![symbol.value_declaration]
        } else {
            symbol.declarations.clone()
        };

        for decl_idx in decl_indices {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(class_data) = self.ctx.arena.get_class(node) else {
                continue;
            };
            if let Some(ref type_params) = class_data.type_parameters {
                let param_names: Vec<&str> = type_params
                    .nodes
                    .iter()
                    .filter_map(|&idx| {
                        let tp = self.ctx.arena.get_type_parameter_at(idx)?;
                        let ident = self.ctx.arena.get_identifier_at(tp.name)?;
                        Some(ident.escaped_text.as_str())
                    })
                    .collect();
                if !param_names.is_empty() {
                    return format!("{}<{}>", name, param_names.join(", "));
                }
            }
        }
        name
    }
}
