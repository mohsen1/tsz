//! Class Type Resolution Module
//!
//! This module contains class and constructor type resolution methods for CheckerState
//! as part of the Phase 2 architecture refactoring (god object decomposition).
//!
//! # Extracted Functions (~1,163 lines from state.rs)
//!
//! - `get_class_instance_type` - Entry point with cycle detection
//! - `get_class_instance_type_inner` - Main implementation (~682 lines)
//! - `get_class_constructor_type` - Constructor/static side type (~469 lines)
//!
//! # Responsibilities
//!
//! - Instance type construction (properties, methods, accessors)
//! - Base class inheritance merging
//! - Interface implementation merging
//! - Index signature handling
//! - Private brand property generation for nominal typing
//! - Constructor type construction (static members, construct signatures)
//! - Constructor accessibility tracking (private/protected)
//! - Abstract class tracking

use crate::binder::SymbolId;
use crate::checker::state::{CheckerState, MemberAccessLevel};
use crate::interner::Atom;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::{
    CallSignature, CallableShape, IndexSignature, ObjectShape, PropertyInfo, TypeId, TypeKey,
    TypeLowering, TypeSubstitution, instantiate_type,
};
use rustc_hash::{FxHashMap, FxHashSet};

// =============================================================================
// Class Type Resolution
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Get the instance type of a class declaration.
    ///
    /// This is the type that instances of the class will have. It includes:
    /// - Instance properties and methods
    /// - Inherited members from base classes
    /// - Members from implemented interfaces
    /// - Index signatures
    /// - Private brand property for nominal typing (if class has private/protected members)
    ///
    /// # Arguments
    /// * `class_idx` - The NodeIndex of the class declaration
    /// * `class` - The parsed class data
    ///
    /// # Returns
    /// The TypeId representing the instance type of the class
    pub(crate) fn get_class_instance_type(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::node::ClassData,
    ) -> TypeId {
        let mut visited = FxHashSet::default();
        let mut visited_nodes = FxHashSet::default();
        self.get_class_instance_type_inner(class_idx, class, &mut visited, &mut visited_nodes)
    }

    /// Inner implementation of class instance type resolution with cycle detection.
    ///
    /// This function builds the complete instance type by:
    /// 1. Collecting all instance members (properties, methods, accessors)
    /// 2. Processing constructor parameter properties
    /// 3. Handling index signatures
    /// 4. Merging base class members
    /// 5. Merging implemented interface members
    /// 6. Adding private brand for nominal typing if needed
    /// 7. Inheriting Object prototype members
    pub(crate) fn get_class_instance_type_inner(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::node::ClassData,
        visited: &mut FxHashSet<SymbolId>,
        visited_nodes: &mut FxHashSet<NodeIndex>,
    ) -> TypeId {
        let current_sym = self.ctx.binder.get_node_symbol(class_idx);

        // Check for cycles using both symbol ID (for same-file cycles)
        // and node index (for cross-file cycles with @Filename annotations)
        if let Some(sym_id) = current_sym {
            if !visited.insert(sym_id) {
                return TypeId::ERROR; // Circular reference detected via symbol
            }
        }
        if !visited_nodes.insert(class_idx) {
            return TypeId::ERROR; // Circular reference detected via node index
        }

        struct MethodAggregate {
            overload_signatures: Vec<CallSignature>,
            impl_signatures: Vec<CallSignature>,
            overload_optional: bool,
            impl_optional: bool,
        }

        struct AccessorAggregate {
            getter: Option<TypeId>,
            setter: Option<TypeId>,
        }

        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let mut methods: FxHashMap<Atom, MethodAggregate> = FxHashMap::default();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;
        let mut has_nominal_members = false;

        // Process all class members
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&prop.modifiers, prop.name) {
                        has_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let type_id = if !prop.type_annotation.is_none() {
                        self.get_type_from_type_node(prop.type_annotation)
                    } else if !prop.initializer.is_none() {
                        self.get_type_of_node(prop.initializer)
                    } else {
                        TypeId::UNKNOWN
                    };

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: prop.question_token,
                            readonly: self.has_readonly_modifier(&prop.modifiers),
                            is_method: false,
                        },
                    );
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&method.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&method.modifiers, method.name) {
                        has_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let signature = self.call_signature_from_method(method);
                    let entry = methods.entry(name_atom).or_insert(MethodAggregate {
                        overload_signatures: Vec::new(),
                        impl_signatures: Vec::new(),
                        overload_optional: false,
                        impl_optional: false,
                    });
                    if method.body.is_none() {
                        entry.overload_signatures.push(signature);
                        entry.overload_optional |= method.question_token;
                    } else {
                        entry.impl_signatures.push(signature);
                        entry.impl_optional |= method.question_token;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&accessor.modifiers, accessor.name) {
                        has_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                        getter: None,
                        setter: None,
                    });

                    if k == syntax_kind_ext::GET_ACCESSOR {
                        let getter_type = if !accessor.type_annotation.is_none() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(accessor.body)
                        };
                        entry.getter = Some(getter_type);
                    } else {
                        let setter_type = accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .and_then(|param| {
                                if !param.type_annotation.is_none() {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(TypeId::UNKNOWN);
                        entry.setter = Some(setter_type);
                    }
                }
                k if k == syntax_kind_ext::CONSTRUCTOR => {
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_none() {
                        continue;
                    }
                    // Process constructor parameter properties
                    for &param_idx in &ctor.parameters.nodes {
                        let Some(param_node) = self.ctx.arena.get(param_idx) else {
                            continue;
                        };
                        let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                            continue;
                        };
                        if !self.has_parameter_property_modifier(&param.modifiers) {
                            continue;
                        }
                        if self.has_private_modifier(&param.modifiers)
                            || self.has_protected_modifier(&param.modifiers)
                        {
                            has_nominal_members = true;
                        }
                        let Some(name) = self.get_property_name(param.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        if properties.contains_key(&name_atom) {
                            continue;
                        }
                        let type_id = if !param.type_annotation.is_none() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else if !param.initializer.is_none() {
                            self.get_type_of_node(param.initializer)
                        } else {
                            TypeId::ANY
                        };
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: param.question_token,
                                readonly: self.has_readonly_modifier(&param.modifiers),
                                is_method: false,
                            },
                        );
                    }
                }
                k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                    let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                        continue;
                    };
                    if self.has_static_modifier(&index_sig.modifiers) {
                        continue;
                    }

                    let param_idx = index_sig
                        .parameters
                        .nodes
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE);
                    let Some(param_node) = self.ctx.arena.get(param_idx) else {
                        continue;
                    };
                    let Some(param) = self.ctx.arena.get_parameter(param_node) else {
                        continue;
                    };

                    let key_type = if param.type_annotation.is_none() {
                        TypeId::ANY
                    } else {
                        self.get_type_from_type_node(param.type_annotation)
                    };
                    let value_type = if index_sig.type_annotation.is_none() {
                        TypeId::ANY
                    } else {
                        self.get_type_from_type_node(index_sig.type_annotation)
                    };
                    let readonly = self.has_readonly_modifier(&index_sig.modifiers);

                    let index = IndexSignature {
                        key_type,
                        value_type,
                        readonly,
                    };

                    if key_type == TypeId::NUMBER {
                        Self::merge_index_signature(&mut number_index, index);
                    } else {
                        Self::merge_index_signature(&mut string_index, index);
                    }
                }
                _ => {}
            }
        }

        // Convert accessors to properties
        for (name, accessor) in accessors {
            if methods.contains_key(&name) {
                continue;
            }
            let read_type = accessor
                .getter
                .or(accessor.setter)
                .unwrap_or(TypeId::UNKNOWN);
            let write_type = accessor.setter.or(accessor.getter).unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id: read_type,
                    write_type,
                    optional: false,
                    readonly,
                    is_method: false,
                },
            );
        }

        // Convert methods to callable properties
        for (name, method) in methods {
            let (signatures, optional) = if !method.overload_signatures.is_empty() {
                (method.overload_signatures, method.overload_optional)
            } else {
                (method.impl_signatures, method.impl_optional)
            };
            if signatures.is_empty() {
                continue;
            }
            let type_id = self.ctx.types.callable(CallableShape {
                call_signatures: signatures,
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
            });
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional,
                    readonly: false,
                    is_method: true,
                },
            );
        }

        // Add private brand property for nominal typing
        if has_nominal_members {
            let brand_name = if let Some(sym_id) = current_sym {
                format!("__private_brand_{}", sym_id.0)
            } else {
                format!("__private_brand_node_{}", class_idx.0)
            };
            let brand_atom = self.ctx.types.intern_string(&brand_name);
            properties.entry(brand_atom).or_insert(PropertyInfo {
                name: brand_atom,
                type_id: TypeId::UNKNOWN,
                write_type: TypeId::UNKNOWN,
                optional: false,
                readonly: true,
                is_method: false,
            });
        }

        // Merge base class instance properties (derived members take precedence)
        if let Some(ref heritage_clauses) = class.heritage_clauses {
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
                    break;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    break;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let base_sym_id = match self.resolve_heritage_symbol(expr_idx) {
                    Some(base_sym_id) => base_sym_id,
                    None => {
                        if let Some(base_instance_type) =
                            self.base_instance_type_from_expression(expr_idx, type_arguments)
                        {
                            self.merge_base_instance_properties(
                                base_instance_type,
                                &mut properties,
                                &mut string_index,
                                &mut number_index,
                            );
                        }
                        break;
                    }
                };

                // Check for circular inheritance using symbol tracking
                if visited.contains(&base_sym_id) {
                    break;
                }

                let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                    break;
                };

                let mut base_class_idx = None;
                for &decl_idx in &base_symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && self.ctx.arena.get_class(node).is_some()
                    {
                        base_class_idx = Some(decl_idx);
                        break;
                    }
                }
                if base_class_idx.is_none() && !base_symbol.value_declaration.is_none() {
                    let decl_idx = base_symbol.value_declaration;
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && self.ctx.arena.get_class(node).is_some()
                    {
                        base_class_idx = Some(decl_idx);
                    }
                }
                let Some(base_class_idx) = base_class_idx else {
                    if let Some(base_instance_type) =
                        self.base_instance_type_from_expression(expr_idx, type_arguments)
                    {
                        self.merge_base_instance_properties(
                            base_instance_type,
                            &mut properties,
                            &mut string_index,
                            &mut number_index,
                        );
                    }
                    break;
                };

                // Check for circular inheritance using node index tracking (for cross-file cycles)
                if visited_nodes.contains(&base_class_idx) {
                    break;
                }
                let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                    break;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    break;
                };

                let mut type_args = Vec::new();
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                let (base_type_params, base_type_param_updates) =
                    self.push_type_parameters(&base_class.type_parameters);

                if type_args.len() < base_type_params.len() {
                    for param in base_type_params.iter().skip(type_args.len()) {
                        let fallback = param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN);
                        type_args.push(fallback);
                    }
                }
                if type_args.len() > base_type_params.len() {
                    type_args.truncate(base_type_params.len());
                }

                let base_instance_type =
                    self.get_class_instance_type_inner(base_class_idx, base_class, visited, visited_nodes);
                let substitution =
                    TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);
                let base_instance_type =
                    instantiate_type(self.ctx.types, base_instance_type, &substitution);
                self.pop_type_parameters(base_type_param_updates);

                if let Some(
                    TypeKey::Object(base_shape_id) | TypeKey::ObjectWithIndex(base_shape_id),
                ) = self.ctx.types.lookup(base_instance_type)
                {
                    let base_shape = self.ctx.types.object_shape(base_shape_id);
                    for base_prop in base_shape.properties.iter() {
                        properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                    if let Some(ref idx) = base_shape.string_index {
                        Self::merge_index_signature(&mut string_index, idx.clone());
                    }
                    if let Some(ref idx) = base_shape.number_index {
                        Self::merge_index_signature(&mut number_index, idx.clone());
                    }
                }

                break;
            }
        }

        // Merge implemented interface properties (class members take precedence)
        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.ctx.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.ctx.arena.get_heritage_clause(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ImplementsKeyword as u16 {
                    continue;
                }

                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };

                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                    let Some(interface_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                        continue;
                    };

                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    let mut interface_type = self.type_reference_symbol_type(interface_sym_id);
                    let interface_type_params = self.get_type_params_for_symbol(interface_sym_id);

                    if type_args.len() < interface_type_params.len() {
                        for param in interface_type_params.iter().skip(type_args.len()) {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            type_args.push(fallback);
                        }
                    }
                    if type_args.len() > interface_type_params.len() {
                        type_args.truncate(interface_type_params.len());
                    }

                    if !interface_type_params.is_empty() {
                        let substitution = TypeSubstitution::from_args(
                            self.ctx.types,
                            &interface_type_params,
                            &type_args,
                        );
                        interface_type =
                            instantiate_type(self.ctx.types, interface_type, &substitution);
                    }

                    match self.ctx.types.lookup(interface_type) {
                        Some(TypeKey::Object(shape_id))
                        | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                            let shape = self.ctx.types.object_shape(shape_id);
                            for prop in shape.properties.iter() {
                                properties.entry(prop.name).or_insert_with(|| prop.clone());
                            }
                            if let Some(ref idx) = shape.string_index {
                                Self::merge_index_signature(&mut string_index, idx.clone());
                            }
                            if let Some(ref idx) = shape.number_index {
                                Self::merge_index_signature(&mut number_index, idx.clone());
                            }
                        }
                        Some(TypeKey::Callable(shape_id)) => {
                            let shape = self.ctx.types.callable_shape(shape_id);
                            for prop in shape.properties.iter() {
                                properties.entry(prop.name).or_insert_with(|| prop.clone());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Merge interface declarations for class/interface merging (class members take precedence)
        if let Some(sym_id) = current_sym
            && let Some(symbol) = self.ctx.binder.get_symbol(sym_id)
        {
            let interface_decls: Vec<NodeIndex> = symbol
                .declarations
                .iter()
                .copied()
                .filter(|&decl_idx| {
                    self.ctx
                        .arena
                        .get(decl_idx)
                        .and_then(|node| self.ctx.arena.get_interface(node))
                        .is_some()
                })
                .collect();

            if !interface_decls.is_empty() {
                let type_param_bindings = self.get_type_param_bindings();
                let type_resolver =
                    |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                let value_resolver =
                    |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                let lowering = TypeLowering::with_resolvers(
                    self.ctx.arena,
                    self.ctx.types,
                    &type_resolver,
                    &value_resolver,
                )
                .with_type_param_bindings(type_param_bindings);
                let interface_type = lowering.lower_interface_declarations(&interface_decls);
                let interface_type =
                    self.merge_interface_heritage_types(&interface_decls, interface_type);

                match self.ctx.types.lookup(interface_type) {
                    Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                        let shape = self.ctx.types.object_shape(shape_id);
                        for prop in shape.properties.iter() {
                            properties.entry(prop.name).or_insert_with(|| prop.clone());
                        }
                        if let Some(ref idx) = shape.string_index {
                            Self::merge_index_signature(&mut string_index, idx.clone());
                        }
                        if let Some(ref idx) = shape.number_index {
                            Self::merge_index_signature(&mut number_index, idx.clone());
                        }
                    }
                    Some(TypeKey::Callable(shape_id)) => {
                        let shape = self.ctx.types.callable_shape(shape_id);
                        for prop in shape.properties.iter() {
                            properties.entry(prop.name).or_insert_with(|| prop.clone());
                        }
                    }
                    _ => {}
                }
            }
        }

        // Classes inherit Object members (toString, hasOwnProperty, etc.)
        if let Some(object_type) = self.resolve_lib_type_by_name("Object") {
            match self.ctx.types.lookup(object_type) {
                Some(TypeKey::Object(shape_id)) | Some(TypeKey::ObjectWithIndex(shape_id)) => {
                    let shape = self.ctx.types.object_shape(shape_id);
                    for prop in shape.properties.iter() {
                        properties.entry(prop.name).or_insert_with(|| prop.clone());
                    }
                    if let Some(ref idx) = shape.string_index {
                        Self::merge_index_signature(&mut string_index, idx.clone());
                    }
                    if let Some(ref idx) = shape.number_index {
                        Self::merge_index_signature(&mut number_index, idx.clone());
                    }
                }
                Some(TypeKey::Callable(shape_id)) => {
                    let shape = self.ctx.types.callable_shape(shape_id);
                    for prop in shape.properties.iter() {
                        properties.entry(prop.name).or_insert_with(|| prop.clone());
                    }
                }
                _ => {}
            }
        }

        // Build the final instance type
        let props: Vec<PropertyInfo> = properties.into_values().collect();
        let mut instance_type = if string_index.is_some() || number_index.is_some() {
            self.ctx.types.object_with_index(ObjectShape {
                properties: props,
                string_index,
                number_index,
            })
        } else {
            self.ctx.types.object(props)
        };

        // Final interface merging pass
        if let Some(sym_id) = current_sym {
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let interface_decls: Vec<NodeIndex> = symbol
                    .declarations
                    .iter()
                    .copied()
                    .filter(|decl_idx| {
                        self.ctx
                            .arena
                            .get(*decl_idx)
                            .and_then(|node| self.ctx.arena.get_interface(node))
                            .is_some()
                    })
                    .collect();

                if !interface_decls.is_empty() {
                    let type_param_bindings = self.get_type_param_bindings();
                    let type_resolver =
                        |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
                    let value_resolver =
                        |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
                    let lowering = TypeLowering::with_resolvers(
                        self.ctx.arena,
                        self.ctx.types,
                        &type_resolver,
                        &value_resolver,
                    )
                    .with_type_param_bindings(type_param_bindings);
                    let interface_type = lowering.lower_interface_declarations(&interface_decls);
                    let interface_type =
                        self.merge_interface_heritage_types(&interface_decls, interface_type);
                    instance_type = self.merge_interface_types(instance_type, interface_type);
                }
            }
            visited.remove(&sym_id);
            visited_nodes.remove(&class_idx);
        }
        instance_type
    }

    /// Get the constructor type of a class declaration.
    ///
    /// This is the type that the class constructor has. It includes:
    /// - Static properties and methods
    /// - Construct signatures (for `new` expressions)
    /// - Inherited static members from base classes
    /// - Constructor accessibility (private/protected)
    /// - Abstract class tracking
    ///
    /// # Arguments
    /// * `class_idx` - The NodeIndex of the class declaration
    /// * `class` - The parsed class data
    ///
    /// # Returns
    /// The TypeId representing the constructor type of the class
    pub(crate) fn get_class_constructor_type(
        &mut self,
        class_idx: NodeIndex,
        class: &crate::parser::node::ClassData,
    ) -> TypeId {
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);
        let (class_type_params, type_param_updates) =
            self.push_type_parameters(&class.type_parameters);
        let instance_type = self.get_class_instance_type(class_idx, class);

        struct MethodAggregate {
            overload_signatures: Vec<CallSignature>,
            impl_signatures: Vec<CallSignature>,
            overload_optional: bool,
            impl_optional: bool,
        }

        struct AccessorAggregate {
            getter: Option<TypeId>,
            setter: Option<TypeId>,
        }

        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let mut methods: FxHashMap<Atom, MethodAggregate> = FxHashMap::default();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut static_string_index: Option<IndexSignature> = None;
        let mut static_number_index: Option<IndexSignature> = None;
        let mut _has_static_nominal_members = false;

        // Process all static class members
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&prop.modifiers, prop.name) {
                        _has_static_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(prop.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let type_id = if !prop.type_annotation.is_none() {
                        self.get_type_from_type_node(prop.type_annotation)
                    } else if !prop.initializer.is_none() {
                        self.get_type_of_node(prop.initializer)
                    } else {
                        TypeId::UNKNOWN
                    };

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: prop.question_token,
                            readonly: self.has_readonly_modifier(&prop.modifiers),
                            is_method: false,
                        },
                    );
                }
                k if k == syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&method.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&method.modifiers, method.name) {
                        _has_static_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(method.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let signature = self.call_signature_from_method(method);
                    let entry = methods.entry(name_atom).or_insert(MethodAggregate {
                        overload_signatures: Vec::new(),
                        impl_signatures: Vec::new(),
                        overload_optional: false,
                        impl_optional: false,
                    });
                    if method.body.is_none() {
                        entry.overload_signatures.push(signature);
                        entry.overload_optional |= method.question_token;
                    } else {
                        entry.impl_signatures.push(signature);
                        entry.impl_optional |= method.question_token;
                    }
                }
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&accessor.modifiers) {
                        continue;
                    }
                    if self.member_requires_nominal(&accessor.modifiers, accessor.name) {
                        _has_static_nominal_members = true;
                    }
                    let Some(name) = self.get_property_name(accessor.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                        getter: None,
                        setter: None,
                    });

                    if k == syntax_kind_ext::GET_ACCESSOR {
                        let getter_type = if !accessor.type_annotation.is_none() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            self.infer_getter_return_type(accessor.body)
                        };
                        entry.getter = Some(getter_type);
                    } else {
                        let setter_type = accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .and_then(|param| {
                                if !param.type_annotation.is_none() {
                                    Some(self.get_type_from_type_node(param.type_annotation))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(TypeId::UNKNOWN);
                        entry.setter = Some(setter_type);
                    }
                }
                k if k == syntax_kind_ext::INDEX_SIGNATURE => {
                    let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) else {
                        continue;
                    };
                    if !self.has_static_modifier(&index_sig.modifiers) {
                        continue;
                    }
                    let key_type = index_sig
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                        .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                        .and_then(|param| {
                            if !param.type_annotation.is_none() {
                                Some(self.get_type_from_type_node(param.type_annotation))
                            } else {
                                None
                            }
                        })
                        .unwrap_or(TypeId::STRING);

                    let value_type = if !index_sig.type_annotation.is_none() {
                        self.get_type_from_type_node(index_sig.type_annotation)
                    } else {
                        TypeId::ANY
                    };

                    let readonly = self.has_readonly_modifier(&index_sig.modifiers);

                    let idx_sig = IndexSignature {
                        key_type,
                        value_type,
                        readonly,
                    };

                    if key_type == TypeId::NUMBER {
                        static_number_index = Some(idx_sig);
                    } else {
                        static_string_index = Some(idx_sig);
                    }
                }
                _ => {}
            }
        }

        // Convert accessors to properties
        for (name, accessor) in accessors {
            if methods.contains_key(&name) {
                continue;
            }
            let read_type = accessor
                .getter
                .or(accessor.setter)
                .unwrap_or(TypeId::UNKNOWN);
            let write_type = accessor.setter.or(accessor.getter).unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id: read_type,
                    write_type,
                    optional: false,
                    readonly,
                    is_method: false,
                },
            );
        }

        // Convert methods to callable properties
        for (name, method) in methods {
            let (signatures, optional) = if !method.overload_signatures.is_empty() {
                (method.overload_signatures, method.overload_optional)
            } else {
                (method.impl_signatures, method.impl_optional)
            };
            if signatures.is_empty() {
                continue;
            }
            let type_id = self.ctx.types.callable(CallableShape {
                call_signatures: signatures,
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
            });
            properties.insert(
                name,
                PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional,
                    readonly: false,
                    is_method: true,
                },
            );
        }

        // Merge base class static properties (derived members take precedence)
        if let Some(ref heritage_clauses) = class.heritage_clauses {
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
                    break;
                };
                let Some(type_node) = self.ctx.arena.get(type_idx) else {
                    break;
                };

                let (expr_idx, type_arguments) =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else {
                        (type_idx, None)
                    };

                let base_sym_id = match self.resolve_heritage_symbol(expr_idx) {
                    Some(base_sym_id) => base_sym_id,
                    None => {
                        if let Some(base_constructor_type) =
                            self.base_constructor_type_from_expression(expr_idx, type_arguments)
                        {
                            self.merge_constructor_properties_from_type(
                                base_constructor_type,
                                &mut properties,
                            );
                        }
                        break;
                    }
                };
                let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                    break;
                };

                let mut base_class_idx = None;
                for &decl_idx in &base_symbol.declarations {
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && self.ctx.arena.get_class(node).is_some()
                    {
                        base_class_idx = Some(decl_idx);
                        break;
                    }
                }
                if base_class_idx.is_none() && !base_symbol.value_declaration.is_none() {
                    let decl_idx = base_symbol.value_declaration;
                    if let Some(node) = self.ctx.arena.get(decl_idx)
                        && self.ctx.arena.get_class(node).is_some()
                    {
                        base_class_idx = Some(decl_idx);
                    }
                }
                let Some(base_class_idx) = base_class_idx else {
                    if let Some(base_constructor_type) =
                        self.base_constructor_type_from_expression(expr_idx, type_arguments)
                    {
                        self.merge_constructor_properties_from_type(
                            base_constructor_type,
                            &mut properties,
                        );
                    }
                    break;
                };
                let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                    break;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    break;
                };

                let mut type_args = Vec::new();
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                let (base_type_params, base_type_param_updates) =
                    self.push_type_parameters(&base_class.type_parameters);

                if type_args.len() < base_type_params.len() {
                    for param in base_type_params.iter().skip(type_args.len()) {
                        let fallback = param
                            .default
                            .or(param.constraint)
                            .unwrap_or(TypeId::UNKNOWN);
                        type_args.push(fallback);
                    }
                }
                if type_args.len() > base_type_params.len() {
                    type_args.truncate(base_type_params.len());
                }

                let base_constructor_type =
                    self.get_class_constructor_type(base_class_idx, base_class);
                let substitution =
                    TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);
                let base_constructor_type =
                    instantiate_type(self.ctx.types, base_constructor_type, &substitution);
                self.pop_type_parameters(base_type_param_updates);

                if let Some(TypeKey::Callable(base_shape_id)) =
                    self.ctx.types.lookup(base_constructor_type)
                {
                    let base_shape = self.ctx.types.callable_shape(base_shape_id);
                    for base_prop in base_shape.properties.iter() {
                        properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                }

                break;
            }
        }

        // Build construct signatures
        let mut has_overloads = false;
        let mut constructor_access: Option<MemberAccessLevel> = None;
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR
                && let Some(ctor) = self.ctx.arena.get_constructor(member_node)
            {
                if self.has_private_modifier(&ctor.modifiers) {
                    constructor_access = Some(MemberAccessLevel::Private);
                } else if self.has_protected_modifier(&ctor.modifiers)
                    && constructor_access != Some(MemberAccessLevel::Private)
                {
                    constructor_access = Some(MemberAccessLevel::Protected);
                }
                if ctor.body.is_none() {
                    has_overloads = true;
                }
            }
        }

        let mut construct_signatures = Vec::new();
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind != syntax_kind_ext::CONSTRUCTOR {
                continue;
            }
            let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                continue;
            };

            if has_overloads {
                if ctor.body.is_none() {
                    construct_signatures.push(self.call_signature_from_constructor(
                        ctor,
                        instance_type,
                        &class_type_params,
                    ));
                }
            } else {
                construct_signatures.push(self.call_signature_from_constructor(
                    ctor,
                    instance_type,
                    &class_type_params,
                ));
                break;
            }
        }

        // Add default constructor if none exists
        if construct_signatures.is_empty() {
            construct_signatures.push(CallSignature {
                type_params: class_type_params,
                params: Vec::new(),
                this_type: None,
                return_type: instance_type,
                type_predicate: None,
            });
        }

        let properties: Vec<PropertyInfo> = properties.into_values().collect();
        self.pop_type_parameters(type_param_updates);

        let constructor_type = self.ctx.types.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures,
            properties,
            string_index: static_string_index,
            number_index: static_number_index,
        });

        // Track constructor accessibility
        if let Some(level) = constructor_access {
            match level {
                MemberAccessLevel::Private => {
                    self.ctx.private_constructor_types.insert(constructor_type);
                }
                MemberAccessLevel::Protected => {
                    self.ctx
                        .protected_constructor_types
                        .insert(constructor_type);
                }
            }
        }

        // Track abstract classes
        if is_abstract_class {
            self.ctx.abstract_constructor_types.insert(constructor_type);
        }

        constructor_type
    }
}
