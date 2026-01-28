//! Interface Type Resolution Module
//!
//! This module contains interface type resolution methods for CheckerState
//! as part of the Phase 2 architecture refactoring (god object decomposition).
//!
//! # Extracted Functions (~565 lines from state.rs)
//!
//! - `get_type_of_interface` - Entry point for interface type resolution
//! - `merge_interface_heritage_types` - Merge interface heritage (extends) types
//! - `merge_interface_types` - Structural merge of derived and base interface types
//! - `merge_properties` - Merge derived and base interface properties
//!
//! # Responsibilities
//!
//! - Interface type construction (call signatures, construct signatures, properties)
//! - Heritage clause processing (extends)
//! - Base interface/class/alias type merging
//! - Index signature handling
//! - Type parameter instantiation for generic bases

use crate::checker::state::CheckerState;
use crate::parser::NodeIndex;
use crate::parser::syntax_kind_ext;
use crate::scanner::SyntaxKind;
use crate::solver::TypeId;

// =============================================================================
// Interface Type Resolution
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Get the type of an interface declaration.
    ///
    /// This function builds the interface type by:
    /// 1. Collecting all interface members (call signatures, construct signatures, properties, index signatures)
    /// 2. Processing heritage clauses (extends)
    /// 3. Merging base interface types
    ///
    /// # Arguments
    /// * `idx` - The NodeIndex of the interface declaration
    ///
    /// # Returns
    /// The TypeId representing the interface type
    ///
    /// # Example
    /// ```typescript
    /// interface Window {
    ///     title: string;
    /// }
    ///
    /// interface Window {
    ///     alert(message: string): void;
    /// }
    ///
    /// // Window type has both title and alert
    /// ```
    pub(crate) fn get_type_of_interface(&mut self, idx: NodeIndex) -> TypeId {
        use crate::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use crate::solver::{
            CallSignature as SolverCallSignature, CallableShape, IndexSignature, ObjectShape,
            PropertyInfo,
        };

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(interface) = self.ctx.arena.get_interface(node) else {
            return TypeId::ERROR; // Missing interface data - propagate error
        };

        let (_interface_type_params, interface_type_param_updates) =
            self.push_type_parameters(&interface.type_parameters);

        let mut call_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut construct_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut properties: Vec<PropertyInfo> = Vec::new();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;

        // Iterate over this interface's own members
        for &member_idx in &interface.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == CALL_SIGNATURE {
                // Extract call signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) = self.extract_params_from_signature(sig);
                    let (return_type, type_predicate) = if !sig.type_annotation.is_none() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .map(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE)
                            .unwrap_or(false);
                        if is_predicate {
                            self.return_type_and_predicate(sig.type_annotation)
                        } else {
                            (self.get_type_of_node(sig.type_annotation), None)
                        }
                    } else {
                        // Return UNKNOWN instead of ANY for missing return type annotation
                        (TypeId::UNKNOWN, None)
                    };

                    call_signatures.push(SolverCallSignature {
                        type_params,
                        params,
                        this_type,
                        return_type,
                        type_predicate,
                        is_method: false,
                    });
                    self.pop_type_parameters(type_param_updates);
                }
            } else if member_node.kind == CONSTRUCT_SIGNATURE {
                // Extract construct signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) = self.extract_params_from_signature(sig);
                    let (return_type, type_predicate) = if !sig.type_annotation.is_none() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .map(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE)
                            .unwrap_or(false);
                        if is_predicate {
                            self.return_type_and_predicate(sig.type_annotation)
                        } else {
                            (self.get_type_of_node(sig.type_annotation), None)
                        }
                    } else {
                        // Return UNKNOWN instead of ANY for missing return type annotation
                        (TypeId::UNKNOWN, None)
                    };

                    construct_signatures.push(SolverCallSignature {
                        type_params,
                        params,
                        this_type,
                        return_type,
                        type_predicate,
                        is_method: false,
                    });
                    self.pop_type_parameters(type_param_updates);
                }
            } else if member_node.kind == PROPERTY_SIGNATURE || member_node.kind == METHOD_SIGNATURE
            {
                // Extract property
                if let Some(sig) = self.ctx.arena.get_signature(member_node)
                    && let Some(name_node) = self.ctx.arena.get(sig.name)
                    && let Some(id_data) = self.ctx.arena.get_identifier(name_node)
                {
                    let type_id = if !sig.type_annotation.is_none() {
                        self.get_type_of_node(sig.type_annotation)
                    } else {
                        TypeId::ANY
                    };

                    properties.push(PropertyInfo {
                        name: self.ctx.types.intern_string(&id_data.escaped_text),
                        type_id,
                        write_type: type_id,
                        optional: sig.question_token,
                        readonly: self.has_readonly_modifier(&sig.modifiers),
                        is_method: member_node.kind == METHOD_SIGNATURE,
                    });
                }
            } else if let Some(index_sig) = self.ctx.arena.get_index_signature(member_node) {
                let param_idx = index_sig
                    .parameters
                    .nodes
                    .first()
                    .copied()
                    .unwrap_or(NodeIndex::NONE);
                let Some(param_node) = self.ctx.arena.get(param_idx) else {
                    continue;
                };
                let Some(param_data) = self.ctx.arena.get_parameter(param_node) else {
                    continue;
                };
                let key_type = if !param_data.type_annotation.is_none() {
                    self.get_type_of_node(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };
                let value_type = if !index_sig.type_annotation.is_none() {
                    self.get_type_of_node(index_sig.type_annotation)
                } else {
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                };
                if key_type == TypeId::NUMBER {
                    Self::merge_index_signature(&mut number_index, info);
                } else {
                    Self::merge_index_signature(&mut string_index, info);
                }
            }
        }

        let result = if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            let shape = CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
            };
            self.ctx.types.callable(shape)
        } else if string_index.is_some() || number_index.is_some() {
            self.ctx.types.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
            })
        } else if !properties.is_empty() {
            self.ctx.types.object(properties)
        } else {
            TypeId::ANY
        };

        self.pop_type_parameters(interface_type_param_updates);
        self.merge_interface_heritage_types(std::slice::from_ref(&idx), result)
    }

    /// Merge interface heritage types (extends clauses).
    ///
    /// This function processes the heritage clauses of interface declarations
    /// and merges the base interface/class/alias types into the derived type.
    ///
    /// # Arguments
    /// * `declarations` - The interface declarations to process
    /// * `derived_type` - The initial derived type
    ///
    /// # Returns
    /// The merged TypeId including all base interface members
    pub(crate) fn merge_interface_heritage_types(
        &mut self,
        declarations: &[NodeIndex],
        mut derived_type: TypeId,
    ) -> TypeId {
        use crate::solver::{TypeSubstitution, instantiate_type};

        let mut pushed_derived = false;
        let mut derived_param_updates = Vec::new();

        for &decl_idx in declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };

            if !pushed_derived {
                let (_params, updates) = self.push_type_parameters(&interface.type_parameters);
                derived_param_updates = updates;
                pushed_derived = true;
            }

            let Some(ref heritage_clauses) = interface.heritage_clauses else {
                continue;
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

                    let (expr_idx, type_arguments) = if let Some(expr_type_args) =
                        self.ctx.arena.get_expr_type_args(type_node)
                    {
                        (
                            expr_type_args.expression,
                            expr_type_args.type_arguments.as_ref(),
                        )
                    } else if type_node.kind == syntax_kind_ext::TYPE_REFERENCE {
                        if let Some(type_ref) = self.ctx.arena.get_type_ref(type_node) {
                            (type_ref.type_name, type_ref.type_arguments.as_ref())
                        } else {
                            (type_idx, None)
                        }
                    } else {
                        (type_idx, None)
                    };

                    let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx) else {
                        continue;
                    };
                    let Some(base_symbol) = self.ctx.binder.get_symbol(base_sym_id) else {
                        continue;
                    };

                    let mut type_args = Vec::new();
                    if let Some(args) = type_arguments {
                        for &arg_idx in &args.nodes {
                            type_args.push(self.get_type_from_type_node(arg_idx));
                        }
                    }

                    let mut base_type_params = Vec::new();
                    let mut base_param_updates = Vec::new();
                    let mut base_type = None;

                    for &base_decl_idx in &base_symbol.declarations {
                        let Some(base_node) = self.ctx.arena.get(base_decl_idx) else {
                            continue;
                        };
                        if let Some(base_iface) = self.ctx.arena.get_interface(base_node) {
                            let (params, updates) =
                                self.push_type_parameters(&base_iface.type_parameters);
                            base_type_params = params;
                            base_param_updates = updates;
                            base_type = Some(self.get_type_of_symbol(base_sym_id));
                            break;
                        }
                        if let Some(base_alias) = self.ctx.arena.get_type_alias(base_node) {
                            let (params, updates) =
                                self.push_type_parameters(&base_alias.type_parameters);
                            base_type_params = params;
                            base_param_updates = updates;
                            base_type = Some(self.get_type_of_symbol(base_sym_id));
                            break;
                        }
                        if let Some(base_class) = self.ctx.arena.get_class(base_node) {
                            let (params, updates) =
                                self.push_type_parameters(&base_class.type_parameters);
                            base_type_params = params;
                            base_param_updates = updates;

                            // Guard against recursion when interface extends class
                            if !self.ctx.class_instance_resolution_set.insert(base_sym_id) {
                                // Recursion detected; use a type reference fallback
                                use crate::solver::SymbolRef;
                                base_type =
                                    Some(self.ctx.types.reference(SymbolRef(base_sym_id.0)));
                            } else {
                                base_type =
                                    Some(self.get_class_instance_type(base_decl_idx, base_class));
                                self.ctx.class_instance_resolution_set.remove(&base_sym_id);
                            }
                            break;
                        }
                    }

                    if base_type.is_none() && !base_symbol.value_declaration.is_none() {
                        let base_decl_idx = base_symbol.value_declaration;
                        if let Some(base_node) = self.ctx.arena.get(base_decl_idx) {
                            if let Some(base_iface) = self.ctx.arena.get_interface(base_node) {
                                let (params, updates) =
                                    self.push_type_parameters(&base_iface.type_parameters);
                                base_type_params = params;
                                base_param_updates = updates;
                                base_type = Some(self.get_type_of_symbol(base_sym_id));
                            } else if let Some(base_alias) =
                                self.ctx.arena.get_type_alias(base_node)
                            {
                                let (params, updates) =
                                    self.push_type_parameters(&base_alias.type_parameters);
                                base_type_params = params;
                                base_param_updates = updates;
                                base_type = Some(self.get_type_of_symbol(base_sym_id));
                            } else if let Some(base_class) = self.ctx.arena.get_class(base_node) {
                                let (params, updates) =
                                    self.push_type_parameters(&base_class.type_parameters);
                                base_type_params = params;
                                base_param_updates = updates;

                                // Guard against recursion when interface extends class
                                if !self.ctx.class_instance_resolution_set.insert(base_sym_id) {
                                    // Recursion detected; use a type reference fallback
                                    use crate::solver::SymbolRef;
                                    base_type =
                                        Some(self.ctx.types.reference(SymbolRef(base_sym_id.0)));
                                } else {
                                    base_type = Some(
                                        self.get_class_instance_type(base_decl_idx, base_class),
                                    );
                                    self.ctx.class_instance_resolution_set.remove(&base_sym_id);
                                }
                            }
                        }
                    }

                    let Some(mut base_type) = base_type else {
                        continue;
                    };

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

                    let substitution =
                        TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);
                    base_type = instantiate_type(self.ctx.types, base_type, &substitution);

                    self.pop_type_parameters(base_param_updates);

                    derived_type = self.merge_interface_types(derived_type, base_type);
                }
            }
        }

        if pushed_derived {
            self.pop_type_parameters(derived_param_updates);
        }

        derived_type
    }

    /// Merge two interface types structurally.
    ///
    /// This function merges a derived interface type with a base interface type,
    /// combining their call signatures, construct signatures, properties, and index signatures.
    /// Derived members take precedence over base members.
    ///
    /// # Arguments
    /// * `derived` - The derived interface type
    /// * `base` - The base interface type
    ///
    /// # Returns
    /// The merged TypeId
    pub(crate) fn merge_interface_types(&mut self, derived: TypeId, base: TypeId) -> TypeId {
        use crate::solver::type_queries::{InterfaceMergeKind, classify_for_interface_merge};
        use crate::solver::{CallableShape, ObjectShape};

        if derived == base {
            return derived;
        }

        let derived_kind = classify_for_interface_merge(self.ctx.types, derived);
        let base_kind = classify_for_interface_merge(self.ctx.types, base);

        match (derived_kind, base_kind) {
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let mut call_signatures = derived_shape.call_signatures.clone();
                call_signatures.extend(base_shape.call_signatures.iter().cloned());
                let mut construct_signatures = derived_shape.construct_signatures.clone();
                construct_signatures.extend(base_shape.construct_signatures.iter().cloned());
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or(base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or(base_shape.number_index.clone()),
                })
            }
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape.string_index.clone(),
                    number_index: derived_shape.number_index.clone(),
                })
            }
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or(base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or(base_shape.number_index.clone()),
                })
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                })
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or(base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or(base_shape.number_index.clone()),
                })
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object(properties)
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object_with_index(ObjectShape {
                    properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                })
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape.string_index.clone(),
                    number_index: derived_shape.number_index.clone(),
                })
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    Self::merge_properties(&derived_shape.properties, &base_shape.properties);
                self.ctx.types.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or_else(|| base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or_else(|| base_shape.number_index.clone()),
                })
            }
            (_, InterfaceMergeKind::Intersection) | (InterfaceMergeKind::Intersection, _) => {
                self.ctx.types.intersection2(derived, base)
            }
            _ => derived,
        }
    }

    /// Merge derived and base interface properties.
    ///
    /// Derived properties override base properties when names match.
    ///
    /// # Arguments
    /// * `derived` - Properties from the derived interface
    /// * `base` - Properties from the base interface
    ///
    /// # Returns
    /// The merged properties vector
    fn merge_properties(
        derived: &[crate::solver::PropertyInfo],
        base: &[crate::solver::PropertyInfo],
    ) -> Vec<crate::solver::PropertyInfo> {
        use crate::interner::Atom;
        use rustc_hash::FxHashMap;

        let mut merged: FxHashMap<Atom, crate::solver::PropertyInfo> = FxHashMap::default();
        for prop in base {
            merged.insert(prop.name, prop.clone());
        }
        for prop in derived {
            merged.insert(prop.name, prop.clone());
        }
        merged.into_values().collect()
    }

    // =============================================================================
    // Module Augmentation Merging (Rule #44)
    // =============================================================================

    /// Get module augmentation declarations for a given module specifier and interface name.
    ///
    /// This function looks up interface/type declarations inside `declare module 'x'` blocks
    /// that should be merged with the target module's interface.
    ///
    /// # Arguments
    /// * `module_spec` - The module specifier (e.g., "express", "lodash")
    /// * `interface_name` - The name of the interface to find augmentations for
    ///
    /// # Returns
    /// A vector of NodeIndex pointing to augmentation declarations
    ///
    /// # Example
    /// ```typescript
    /// // In user code:
    /// declare module 'express' {
    ///     interface Request {
    ///         user: User;  // This augments the original Request interface
    ///     }
    /// }
    /// ```
    pub(crate) fn get_module_augmentation_declarations(
        &self,
        module_spec: &str,
        interface_name: &str,
    ) -> Vec<NodeIndex> {
        self.ctx
            .binder
            .module_augmentations
            .get(module_spec)
            .map(|augmentations| {
                augmentations
                    .iter()
                    .filter(|(name, _)| name == interface_name)
                    .map(|(_, idx)| *idx)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all module augmentation members for a given module specifier and interface name.
    ///
    /// This function retrieves the properties from augmentation declarations and returns them
    /// as PropertyInfo objects ready for merging with the original interface.
    ///
    /// # Arguments
    /// * `module_spec` - The module specifier (e.g., "express", "lodash")
    /// * `interface_name` - The name of the interface to find augmentation members for
    ///
    /// # Returns
    /// A vector of PropertyInfo representing the augmented members
    pub(crate) fn get_module_augmentation_members(
        &mut self,
        module_spec: &str,
        interface_name: &str,
    ) -> Vec<crate::solver::PropertyInfo> {
        use crate::parser::syntax_kind_ext::{METHOD_SIGNATURE, PROPERTY_SIGNATURE};
        use crate::solver::PropertyInfo;

        let augmentation_decls =
            self.get_module_augmentation_declarations(module_spec, interface_name);
        let mut members = Vec::new();

        for decl_idx in augmentation_decls {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };

            let Some(interface) = self.ctx.arena.get_interface(node) else {
                continue;
            };

            // Extract members from the augmentation interface
            for &member_idx in &interface.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };

                if member_node.kind == PROPERTY_SIGNATURE || member_node.kind == METHOD_SIGNATURE {
                    if let Some(sig) = self.ctx.arena.get_signature(member_node)
                        && let Some(name_node) = self.ctx.arena.get(sig.name)
                        && let Some(id_data) = self.ctx.arena.get_identifier(name_node)
                    {
                        let type_id = if !sig.type_annotation.is_none() {
                            self.get_type_of_node(sig.type_annotation)
                        } else {
                            crate::solver::TypeId::ANY
                        };

                        members.push(PropertyInfo {
                            name: self.ctx.types.intern_string(&id_data.escaped_text),
                            type_id,
                            write_type: type_id,
                            optional: sig.question_token,
                            readonly: self.has_readonly_modifier(&sig.modifiers),
                            is_method: member_node.kind == METHOD_SIGNATURE,
                        });
                    }
                }
            }
        }

        members
    }

    /// Apply module augmentations to an interface type.
    ///
    /// This function merges augmentation members into an existing interface type,
    /// implementing Rule #44: Module Augmentation Merging.
    ///
    /// # Arguments
    /// * `module_spec` - The module specifier being augmented
    /// * `interface_name` - The name of the interface being augmented
    /// * `base_type` - The original interface type
    ///
    /// # Returns
    /// The merged TypeId including augmented members
    ///
    /// # Example
    /// ```typescript
    /// // Original express types:
    /// declare module 'express' {
    ///     interface Request { body: any; }
    /// }
    ///
    /// // User augmentation:
    /// declare module 'express' {
    ///     interface Request { user: User; }
    /// }
    ///
    /// // Result: Request has both body and user properties
    /// ```
    pub(crate) fn apply_module_augmentations(
        &mut self,
        module_spec: &str,
        interface_name: &str,
        base_type: crate::solver::TypeId,
    ) -> crate::solver::TypeId {
        use crate::solver::type_queries::{AugmentationTargetKind, classify_for_augmentation};
        use crate::solver::{CallableShape, ObjectShape};

        let augmentation_members =
            self.get_module_augmentation_members(module_spec, interface_name);

        if augmentation_members.is_empty() {
            return base_type;
        }

        // Get the base type's properties and merge with augmentation members
        match classify_for_augmentation(self.ctx.types, base_type) {
            AugmentationTargetKind::Object(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties =
                    Self::merge_properties(&augmentation_members, &base_shape.properties);
                self.ctx.types.object(merged_properties)
            }
            AugmentationTargetKind::ObjectWithIndex(shape_id) => {
                let base_shape = self.ctx.types.object_shape(shape_id);
                let merged_properties =
                    Self::merge_properties(&augmentation_members, &base_shape.properties);
                self.ctx.types.object_with_index(ObjectShape {
                    properties: merged_properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                })
            }
            AugmentationTargetKind::Callable(shape_id) => {
                let base_shape = self.ctx.types.callable_shape(shape_id);
                let merged_properties =
                    Self::merge_properties(&augmentation_members, &base_shape.properties);
                self.ctx.types.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties: merged_properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                })
            }
            AugmentationTargetKind::Other => {
                // For other types (like ANY), create a new object type with the augmentation members
                if !augmentation_members.is_empty() {
                    self.ctx.types.object(augmentation_members)
                } else {
                    base_type
                }
            }
        }
    }
}
