//! Constructor type operations, type argument application, and base instance
//! type resolution for `CheckerState`.
//!
//! Extracted from `state_type_resolution_module.rs` to keep files focused and
//! under the 2 000-line architectural limit.

use crate::query_boundaries::state_type_resolution as query;
use crate::state::CheckerState;
use tsz_common::interner::Atom;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;

impl<'a> CheckerState<'a> {
    pub(crate) fn apply_type_arguments_to_constructor_type(
        &mut self,
        ctor_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        use tsz_solver::CallableShape;

        let Some(type_arguments) = type_arguments else {
            return ctor_type;
        };

        if type_arguments.nodes.is_empty() {
            return ctor_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return ctor_type;
        }

        let Some(shape) = query::callable_shape_for_type(self.ctx.types, ctor_type) else {
            return ctor_type;
        };
        let mut matching: Vec<&tsz_solver::CallSignature> = shape
            .construct_signatures
            .iter()
            .filter(|sig| sig.type_params.len() == type_args.len())
            .collect();

        if matching.is_empty() {
            matching = shape
                .construct_signatures
                .iter()
                .filter(|sig| !sig.type_params.is_empty())
                .collect();
        }

        if matching.is_empty() {
            return ctor_type;
        }

        let instantiated_constructs: Vec<tsz_solver::CallSignature> = matching
            .iter()
            .map(|sig| {
                {
                    let app_info = query::get_application_info(self.ctx.types, sig.return_type)
                        .map(|(base, args)| format!("base={base:?} args={args:?}"))
                        .unwrap_or_default();
                    tracing::trace!(
                        ?sig.return_type,
                        %app_info,
                        type_params_count = sig.type_params.len(),
                        "apply_type_args_to_ctor: BEFORE instantiation"
                    );
                }
                let mut args = type_args.clone();
                if args.len() < sig.type_params.len() {
                    for param in sig.type_params.iter().skip(args.len()) {
                        let fallback = param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN);
                        args.push(fallback);
                    }
                }
                if args.len() > sig.type_params.len() {
                    args.truncate(sig.type_params.len());
                }
                let result = self.instantiate_constructor_signature(sig, &args);
                {
                    let app_info = query::get_application_info(self.ctx.types, result.return_type)
                        .map(|(base, args)| format!("base={base:?} args={args:?}"))
                        .unwrap_or_default();
                    tracing::trace!(
                        ?result.return_type,
                        %app_info,
                        "apply_type_args_to_ctor: AFTER instantiation"
                    );
                }
                result
            })
            .collect();

        let new_shape = CallableShape {
            call_signatures: shape.call_signatures.clone(),
            construct_signatures: instantiated_constructs,
            properties: shape.properties.clone(),
            string_index: shape.string_index.clone(),
            number_index: shape.number_index.clone(),
            symbol: None,
        };
        let factory = self.ctx.types.factory();
        factory.callable(new_shape)
    }

    /// Apply explicit type arguments to a callable type for function calls.
    ///
    /// When a function is called with explicit type arguments like `fn<T>(x: T)`,
    /// calling it as `fn<number>("hello")` should substitute `T` with `number` and
    /// then check if `"hello"` is assignable to `number`.
    ///
    /// This function creates a new callable type with the type parameters substituted,
    /// so that argument type checking can work correctly.
    pub(crate) fn apply_type_arguments_to_callable_type(
        &mut self,
        callee_type: TypeId,
        type_arguments: Option<&NodeList>,
    ) -> TypeId {
        use tsz_solver::CallableShape;

        let Some(type_arguments) = type_arguments else {
            return callee_type;
        };

        if type_arguments.nodes.is_empty() {
            return callee_type;
        }

        let mut type_args: Vec<TypeId> = Vec::with_capacity(type_arguments.nodes.len());
        for &arg_idx in &type_arguments.nodes {
            type_args.push(self.get_type_from_type_node(arg_idx));
        }

        if type_args.is_empty() {
            return callee_type;
        }

        // Resolve Lazy types before classification.
        let callee_type = {
            let resolved = self.resolve_lazy_type(callee_type);
            if resolved != callee_type {
                resolved
            } else {
                callee_type
            }
        };
        let factory = self.ctx.types.factory();
        match query::classify_for_signatures(self.ctx.types, callee_type) {
            query::SignatureTypeKind::Callable(shape_id) => {
                let shape = self.ctx.types.callable_shape(shape_id);

                // Find call signatures that match the type argument count
                let mut matching: Vec<&tsz_solver::CallSignature> = shape
                    .call_signatures
                    .iter()
                    .filter(|sig| sig.type_params.len() == type_args.len())
                    .collect();

                // If no exact match, try signatures with type params
                if matching.is_empty() {
                    matching = shape
                        .call_signatures
                        .iter()
                        .filter(|sig| !sig.type_params.is_empty())
                        .collect();
                }

                if matching.is_empty() {
                    return callee_type;
                }

                // Instantiate each matching signature with the type arguments
                let instantiated_calls: Vec<tsz_solver::CallSignature> = matching
                    .iter()
                    .map(|sig| {
                        let mut args = type_args.clone();
                        // Fill in default type arguments if needed
                        if args.len() < sig.type_params.len() {
                            for param in sig.type_params.iter().skip(args.len()) {
                                let fallback = param
                                    .default
                                    .or(param.constraint)
                                    .unwrap_or(TypeId::UNKNOWN);
                                args.push(fallback);
                            }
                        }
                        if args.len() > sig.type_params.len() {
                            args.truncate(sig.type_params.len());
                        }
                        self.instantiate_call_signature(sig, &args)
                    })
                    .collect();

                let new_shape = CallableShape {
                    call_signatures: instantiated_calls,
                    construct_signatures: shape.construct_signatures.clone(),
                    properties: shape.properties.clone(),
                    string_index: shape.string_index.clone(),
                    number_index: shape.number_index.clone(),
                    symbol: None,
                };
                factory.callable(new_shape)
            }
            query::SignatureTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if shape.type_params.len() != type_args.len() {
                    return callee_type;
                }

                let instantiated_call = self.instantiate_call_signature(
                    &tsz_solver::CallSignature {
                        type_params: shape.type_params.clone(),
                        params: shape.params.clone(),
                        this_type: None,
                        return_type: shape.return_type,
                        type_predicate: None,
                        is_method: shape.is_method,
                    },
                    &type_args,
                );

                // Convert single signature to callable
                let new_shape = CallableShape {
                    call_signatures: vec![instantiated_call],
                    construct_signatures: vec![],
                    properties: vec![],
                    string_index: None,
                    number_index: None,
                    symbol: None,
                };
                factory.callable(new_shape)
            }
            _ => callee_type,
        }
    }

    pub(crate) fn base_constructor_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        if let Some(name) = self.heritage_name_text(expr_idx) {
            // Filter out primitive types and literals that cannot be used in class extends
            if matches!(
                name.as_str(),
                "undefined"
                    | "true"
                    | "false"
                    | "void"
                    | "0"
                    | "number"
                    | "string"
                    | "boolean"
                    | "never"
                    | "unknown"
                    | "any"
            ) {
                return None;
            }
        }
        let expr_type = self.get_type_of_node(expr_idx);
        tracing::debug!(?expr_type, "base_constructor_type: expr_type");

        // Evaluate application types to get the actual intersection type
        let evaluated_type = self.evaluate_application_type(expr_type);
        tracing::debug!(?evaluated_type, "base_constructor_type: evaluated_type");

        let ctor_types = self.constructor_types_from_type(evaluated_type);
        tracing::debug!(?ctor_types, "base_constructor_type: ctor_types");
        if ctor_types.is_empty() {
            if evaluated_type == TypeId::NULL {
                return Some(TypeId::NULL);
            }
            return None;
        }
        let ctor_type = if ctor_types.len() == 1 {
            ctor_types[0]
        } else {
            let factory = self.ctx.types.factory();
            factory.intersection(ctor_types)
        };
        Some(self.apply_type_arguments_to_constructor_type(ctor_type, type_arguments))
    }

    pub(crate) fn constructor_types_from_type(&mut self, type_id: TypeId) -> Vec<TypeId> {
        use rustc_hash::FxHashSet;

        self.ensure_relation_input_ready(type_id);
        let mut ctor_types = Vec::new();
        let mut visited = FxHashSet::default();
        self.collect_constructor_types_from_type_inner(type_id, &mut ctor_types, &mut visited);
        ctor_types
    }

    pub(crate) fn collect_constructor_types_from_type_inner(
        &mut self,
        type_id: TypeId,
        ctor_types: &mut Vec<TypeId>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        // Resolve Lazy types (e.g., interface references) so the classifier
        // can see the actual type structure (Callable with construct signatures)
        // rather than the opaque Lazy wrapper.
        let evaluated = {
            let resolved = self.resolve_lazy_type(evaluated);
            if resolved != evaluated {
                resolved
            } else {
                evaluated
            }
        };
        if !visited.insert(evaluated) {
            return;
        }

        let classification = query::classify_constructor_type(self.ctx.types, evaluated);
        match classification {
            query::ConstructorTypeKind::Callable => {
                ctor_types.push(evaluated);
            }
            query::ConstructorTypeKind::Function(shape_id) => {
                let shape = self.ctx.types.function_shape(shape_id);
                if shape.is_constructor {
                    ctor_types.push(evaluated);
                }
            }
            query::ConstructorTypeKind::Members(members) => {
                for member in members {
                    self.collect_constructor_types_from_type_inner(member, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::Inner(inner) => {
                self.collect_constructor_types_from_type_inner(inner, ctor_types, visited);
            }
            query::ConstructorTypeKind::Constraint(constraint) => {
                if let Some(constraint) = constraint {
                    self.collect_constructor_types_from_type_inner(constraint, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::NeedsTypeEvaluation => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::NeedsApplicationEvaluation => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_constructor_types_from_type_inner(expanded, ctor_types, visited);
                }
            }
            query::ConstructorTypeKind::TypeQuery(sym_ref) => {
                // typeof X - get the type of the symbol X and collect constructors from it
                use tsz_binder::SymbolId;
                let sym_id = SymbolId(sym_ref.0);
                let sym_type = self.get_type_of_symbol(sym_id);
                self.collect_constructor_types_from_type_inner(sym_type, ctor_types, visited);
            }
            query::ConstructorTypeKind::NotConstructor => {}
        }
    }

    pub(crate) fn static_properties_from_type(
        &mut self,
        type_id: TypeId,
    ) -> rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo> {
        use rustc_hash::{FxHashMap, FxHashSet};

        self.ensure_relation_input_ready(type_id);
        let mut props = FxHashMap::default();
        let mut visited = FxHashSet::default();
        self.collect_static_properties_from_type_inner(type_id, &mut props, &mut visited);
        props
    }

    pub(crate) fn collect_static_properties_from_type_inner(
        &mut self,
        type_id: TypeId,
        props: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        if matches!(type_id, TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN) {
            return;
        }

        let evaluated = self.evaluate_application_type(type_id);
        // Resolve Lazy types so the classifier sees actual type structure.
        let evaluated = {
            let resolved = self.resolve_lazy_type(evaluated);
            if resolved != evaluated {
                resolved
            } else {
                evaluated
            }
        };
        if !visited.insert(evaluated) {
            return;
        }

        match query::static_property_source(self.ctx.types, evaluated) {
            query::StaticPropertySource::Properties(properties) => {
                for prop in properties {
                    props.entry(prop.name).or_insert(prop);
                }
            }
            query::StaticPropertySource::RecurseMembers(members) => {
                for member in members {
                    self.collect_static_properties_from_type_inner(member, props, visited);
                }
            }
            query::StaticPropertySource::RecurseSingle(inner) => {
                self.collect_static_properties_from_type_inner(inner, props, visited);
            }
            query::StaticPropertySource::NeedsEvaluation => {
                let expanded = self.evaluate_type_with_env(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            query::StaticPropertySource::NeedsApplicationEvaluation => {
                let expanded = self.evaluate_application_type(evaluated);
                if expanded != evaluated {
                    self.collect_static_properties_from_type_inner(expanded, props, visited);
                }
            }
            query::StaticPropertySource::None => {}
        }
    }

    pub(crate) fn base_instance_type_from_expression(
        &mut self,
        expr_idx: NodeIndex,
        type_arguments: Option<&NodeList>,
    ) -> Option<TypeId> {
        let ctor_type = self.base_constructor_type_from_expression(expr_idx, type_arguments)?;
        self.instance_type_from_constructor_type(ctor_type)
    }

    pub(crate) fn merge_constructor_properties_from_type(
        &mut self,
        ctor_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
    ) {
        let base_props = self.static_properties_from_type(ctor_type);
        for (name, prop) in base_props {
            properties.entry(name).or_insert(prop);
        }
    }

    pub(crate) fn merge_base_instance_properties(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
        string_index: &mut Option<tsz_solver::IndexSignature>,
        number_index: &mut Option<tsz_solver::IndexSignature>,
    ) {
        use rustc_hash::FxHashSet;

        let mut visited = FxHashSet::default();
        self.merge_base_instance_properties_inner(
            base_instance_type,
            properties,
            string_index,
            number_index,
            &mut visited,
        );
    }

    pub(crate) fn merge_base_instance_properties_inner(
        &mut self,
        base_instance_type: TypeId,
        properties: &mut rustc_hash::FxHashMap<Atom, tsz_solver::PropertyInfo>,
        string_index: &mut Option<tsz_solver::IndexSignature>,
        number_index: &mut Option<tsz_solver::IndexSignature>,
        visited: &mut rustc_hash::FxHashSet<TypeId>,
    ) {
        // Resolve Lazy types so the classifier can see the actual structure.
        let base_instance_type = {
            let resolved = self.resolve_lazy_type(base_instance_type);
            if resolved != base_instance_type {
                resolved
            } else {
                base_instance_type
            }
        };
        if !visited.insert(base_instance_type) {
            return;
        }

        match query::classify_for_base_instance_merge(self.ctx.types, base_instance_type) {
            query::BaseInstanceMergeKind::Object(base_shape_id) => {
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                for base_prop in &base_shape.properties {
                    properties
                        .entry(base_prop.name)
                        .or_insert_with(|| base_prop.clone());
                }
                if let Some(ref idx) = base_shape.string_index {
                    Self::merge_index_signature(string_index, idx.clone());
                }
                if let Some(ref idx) = base_shape.number_index {
                    Self::merge_index_signature(number_index, idx.clone());
                }
            }
            query::BaseInstanceMergeKind::Intersection(members) => {
                for member in members {
                    self.merge_base_instance_properties_inner(
                        member,
                        properties,
                        string_index,
                        number_index,
                        visited,
                    );
                }
            }
            query::BaseInstanceMergeKind::Union(members) => {
                use rustc_hash::FxHashMap;
                let mut common_props: Option<FxHashMap<Atom, tsz_solver::PropertyInfo>> = None;
                let mut common_string_index: Option<tsz_solver::IndexSignature> = None;
                let mut common_number_index: Option<tsz_solver::IndexSignature> = None;

                for member in members {
                    let mut member_props: FxHashMap<Atom, tsz_solver::PropertyInfo> =
                        FxHashMap::default();
                    let mut member_string_index = None;
                    let mut member_number_index = None;
                    let mut member_visited = rustc_hash::FxHashSet::default();
                    member_visited.insert(base_instance_type);

                    self.merge_base_instance_properties_inner(
                        member,
                        &mut member_props,
                        &mut member_string_index,
                        &mut member_number_index,
                        &mut member_visited,
                    );

                    if common_props.is_none() {
                        common_props = Some(member_props);
                        common_string_index = member_string_index;
                        common_number_index = member_number_index;
                        continue;
                    }

                    let mut props = match common_props.take() {
                        Some(props) => props,
                        None => {
                            // This should never happen due to the check above, but handle gracefully
                            common_props = Some(member_props);
                            common_string_index = member_string_index;
                            common_number_index = member_number_index;
                            continue;
                        }
                    };
                    props.retain(|name, prop| {
                        let Some(member_prop) = member_props.get(name) else {
                            return false;
                        };
                        let merged_type = if prop.type_id == member_prop.type_id {
                            prop.type_id
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.type_id, member_prop.type_id])
                        };
                        let merged_write_type = if prop.write_type == member_prop.write_type {
                            prop.write_type
                        } else {
                            self.ctx
                                .types
                                .union(vec![prop.write_type, member_prop.write_type])
                        };
                        prop.type_id = merged_type;
                        prop.write_type = merged_write_type;
                        prop.optional |= member_prop.optional;
                        prop.readonly &= member_prop.readonly;
                        prop.is_method &= member_prop.is_method;
                        true
                    });
                    common_props = Some(props);

                    common_string_index = match (common_string_index.take(), member_string_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };
                    common_number_index = match (common_number_index.take(), member_number_index) {
                        (Some(mut left), Some(right)) => {
                            if left.value_type != right.value_type {
                                left.value_type = self
                                    .ctx
                                    .types
                                    .union(vec![left.value_type, right.value_type]);
                            }
                            left.readonly &= right.readonly;
                            Some(left)
                        }
                        _ => None,
                    };

                    if common_props
                        .as_ref()
                        .is_none_or(std::collections::HashMap::is_empty)
                        && common_string_index.is_none()
                        && common_number_index.is_none()
                    {
                        break;
                    }
                }

                if let Some(props) = common_props {
                    for prop in props.into_values() {
                        properties.entry(prop.name).or_insert(prop);
                    }
                }
                if let Some(idx) = common_string_index {
                    Self::merge_index_signature(string_index, idx);
                }
                if let Some(idx) = common_number_index {
                    Self::merge_index_signature(number_index, idx);
                }
            }
            query::BaseInstanceMergeKind::Other => {}
        }
    }

    /// Check if a node is inside a type parameter declaration (constraint or default).
    /// Used to skip TS2344 validation for type args in type parameter constraints/defaults.
    pub(crate) fn is_inside_type_parameter_declaration(&self, idx: NodeIndex) -> bool {
        use tsz_parser::parser::syntax_kind_ext;

        let mut current = idx;
        for _ in 0..10 {
            let parent = self
                .ctx
                .arena
                .get_extended(current)
                .map_or(NodeIndex::NONE, |e| e.parent);
            if parent.is_none() {
                return false;
            }
            if let Some(parent_node) = self.ctx.arena.get(parent) {
                if parent_node.kind == syntax_kind_ext::TYPE_PARAMETER {
                    return true;
                }
                // Stop at declaration-level nodes
                if parent_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::INTERFACE_DECLARATION
                    || parent_node.kind == syntax_kind_ext::TYPE_ALIAS_DECLARATION
                    || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                    || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
                    || parent_node.kind == syntax_kind_ext::VARIABLE_DECLARATION
                {
                    return false;
                }
            }
            current = parent;
        }
        false
    }

    /// Check if a class extends a type parameter and is "transparent" (adds no new instance members).
    ///
    /// When a class expression extends a generic type parameter but adds no new instance properties
    /// or methods (only has a constructor), it should be typed as that type parameter to maintain
    /// generic compatibility. This is common in simple wrapper patterns.
    ///
    /// # Returns
    /// - `Some(TypeId)` if the class extends a type parameter and has no additional instance members
    /// - `None` otherwise
    pub(crate) fn get_extends_type_parameter_if_transparent(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> Option<TypeId> {
        // Check if class has an extends clause with a type parameter
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        let mut extends_type_param = None;
        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;

            // Only process extends clauses
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }

            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;

            // Handle ExpressionWithTypeArguments
            let expr_idx =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    expr_type_args.expression
                } else {
                    type_idx
                };

            // Get the type of the extends expression
            let base_type = self.get_type_of_node(expr_idx);

            // Check if this is a type parameter
            if query::is_type_parameter(self.ctx.types, base_type) {
                extends_type_param = Some(base_type);
                break;
            }
        }

        let base_type_param = extends_type_param?;

        // Check if class adds any new instance members (excluding constructor)
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            // Skip constructors and static members
            match member_node.kind {
                k if k == syntax_kind_ext::CONSTRUCTOR => continue,
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    if let Some(prop) = self.ctx.arena.get_property_decl(member_node) {
                        // Skip static properties
                        if self.has_static_modifier(&prop.modifiers) {
                            continue;
                        }
                        // Found an instance property - class is not transparent
                        return None;
                    }
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(member_node) {
                        // Skip static methods
                        if self.has_static_modifier(&method.modifiers) {
                            continue;
                        }
                        // Found an instance method - class is not transparent
                        return None;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                        // Skip static accessors
                        if self.has_static_modifier(&accessor.modifiers) {
                            continue;
                        }
                        // Found an instance accessor - class is not transparent
                        return None;
                    }
                }
                _ => {
                    // Other member types - be conservative
                    continue;
                }
            }
        }

        // Class is transparent - return the type parameter
        Some(base_type_param)
    }
}
