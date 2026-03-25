//! Interface type resolution (heritage merging, structural merge).
//! - `merge_properties` - Merge derived and base interface properties
//!
//! # Responsibilities
//!
//! - Interface type construction (call signatures, construct signatures, properties)
//! - Heritage clause processing (extends)
//! - Base interface/class/alias type merging
//! - Index signature handling
//! - Type parameter instantiation for generic bases

use crate::state::CheckerState;
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::TypeId;
use tsz_solver::Visibility;
use tsz_solver::visitor::is_template_literal_type;

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
    /// * `idx` - The `NodeIndex` of the interface declaration
    ///
    /// # Returns
    /// The `TypeId` representing the interface type
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
        use tsz_parser::parser::syntax_kind_ext::{
            CALL_SIGNATURE, CONSTRUCT_SIGNATURE, METHOD_SIGNATURE, PROPERTY_SIGNATURE,
        };
        use tsz_solver::{
            CallSignature as SolverCallSignature, CallableShape, IndexSignature, ObjectShape,
            PropertyInfo,
        };
        let factory = self.ctx.types.factory();

        let Some(node) = self.ctx.arena.get(idx) else {
            return TypeId::ERROR; // Missing node - propagate error
        };

        let Some(interface) = self.ctx.arena.get_interface(node) else {
            return TypeId::ERROR; // Missing interface data - propagate error
        };
        let interface_symbol = self.ctx.binder.get_node_symbol(idx);

        let (_interface_type_params, interface_type_param_updates) =
            self.push_type_parameters(&interface.type_parameters);

        struct AccessorAggregate {
            getter: Option<TypeId>,
            setter: Option<TypeId>,
            declaration_order: u32,
        }

        let mut call_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut construct_signatures: Vec<SolverCallSignature> = Vec::new();
        let mut properties: Vec<PropertyInfo> = Vec::new();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;
        let mut member_order: u32 = 0;

        // Track method overloads: group call signatures by method name.
        // When an interface has multiple method signatures with the same name
        // (overloads), we need to combine them into a single Callable type
        // so that overload resolution works correctly.
        struct MethodOverloadEntry {
            signatures: Vec<SolverCallSignature>,
            optional: bool,
            readonly: bool,
            declaration_order: u32,
        }
        let mut method_overloads: Vec<(Atom, MethodOverloadEntry)> = Vec::new();

        // Iterate over this interface's own members
        for &member_idx in &interface.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if member_node.kind == CALL_SIGNATURE {
                // Extract call signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    if let Some(ref _params) = sig.parameters {}
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) =
                        self.extract_params_from_signature_in_type_literal(sig);
                    self.push_typeof_param_scope(&params);
                    let (return_type, type_predicate) = if sig.type_annotation.is_some() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE);
                        if is_predicate {
                            self.return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                            )
                        } else {
                            (
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation),
                                None,
                            )
                        }
                    } else {
                        // Return ANY to match TypeScript's implicit 'any' return type
                        (TypeId::ANY, None)
                    };
                    self.pop_typeof_param_scope(&params);

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
                    if let Some(ref _params) = sig.parameters {}
                    let (type_params, type_param_updates) =
                        self.push_type_parameters(&sig.type_parameters);
                    let (params, this_type) =
                        self.extract_params_from_signature_in_type_literal(sig);
                    self.push_typeof_param_scope(&params);
                    let (return_type, type_predicate) = if sig.type_annotation.is_some() {
                        let is_predicate = self
                            .ctx
                            .arena
                            .get(sig.type_annotation)
                            .is_some_and(|node| node.kind == syntax_kind_ext::TYPE_PREDICATE);
                        if is_predicate {
                            self.return_type_and_predicate_in_type_literal(
                                sig.type_annotation,
                                &params,
                            )
                        } else {
                            (
                                self.get_type_from_type_node_in_type_literal(sig.type_annotation),
                                None,
                            )
                        }
                    } else {
                        // Return ANY to match TypeScript's implicit 'any' return type
                        (TypeId::ANY, None)
                    };
                    self.pop_typeof_param_scope(&params);

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
            } else if member_node.kind == PROPERTY_SIGNATURE {
                // Extract property signature
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let name_atom = self.get_member_name_atom(sig.name).or_else(|| {
                        self.get_property_name_resolved(sig.name)
                            .map(|name| self.ctx.types.intern_string(&name))
                    });
                    if let Some(name_atom) = name_atom {
                        let type_id = if sig.type_annotation.is_some() {
                            self.get_type_from_type_node_in_type_literal(sig.type_annotation)
                        } else {
                            TypeId::ANY
                        };

                        member_order += 1;
                        properties.push(PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: sig.question_token,
                            readonly: self.has_readonly_modifier(&sig.modifiers),
                            is_method: false,
                            is_class_prototype: false,
                            visibility: Visibility::Public,
                            parent_id: None,
                            declaration_order: member_order,
                        });
                    }
                }
            } else if member_node.kind == METHOD_SIGNATURE {
                // Extract method signature as a full call signature.
                // Method overloads (multiple signatures with the same name) are
                // collected and later combined into a single Callable type so
                // that overload resolution works correctly (e.g., Object.freeze).
                if let Some(sig) = self.ctx.arena.get_signature(member_node) {
                    let name_atom = self.get_member_name_atom(sig.name).or_else(|| {
                        self.get_property_name_resolved(sig.name)
                            .map(|name| self.ctx.types.intern_string(&name))
                    });
                    if let Some(name_atom) = name_atom {
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&sig.type_parameters);
                        let (params, this_type) =
                            self.extract_params_from_signature_in_type_literal(sig);
                        self.push_typeof_param_scope(&params);
                        let (return_type, type_predicate) = if sig.type_annotation.is_some() {
                            let is_predicate =
                                self.ctx.arena.get(sig.type_annotation).is_some_and(|node| {
                                    node.kind == syntax_kind_ext::TYPE_PREDICATE
                                });
                            if is_predicate {
                                self.return_type_and_predicate_in_type_literal(
                                    sig.type_annotation,
                                    &params,
                                )
                            } else {
                                (
                                    self.get_type_from_type_node_in_type_literal(
                                        sig.type_annotation,
                                    ),
                                    None,
                                )
                            }
                        } else {
                            (TypeId::ANY, None)
                        };
                        self.pop_typeof_param_scope(&params);
                        self.pop_type_parameters(type_param_updates);

                        let call_sig = SolverCallSignature {
                            type_params,
                            params,
                            this_type,
                            return_type,
                            type_predicate,
                            is_method: true,
                        };

                        member_order += 1;
                        let optional = sig.question_token;
                        let readonly = self.has_readonly_modifier(&sig.modifiers);

                        // Add to overload group or create new group
                        if let Some(entry) = method_overloads
                            .iter_mut()
                            .find(|(name, _)| *name == name_atom)
                        {
                            entry.1.signatures.push(call_sig);
                        } else {
                            method_overloads.push((
                                name_atom,
                                MethodOverloadEntry {
                                    signatures: vec![call_sig],
                                    optional,
                                    readonly,
                                    declaration_order: member_order,
                                },
                            ));
                        }
                    }
                }
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                if let Some(accessor) = self.ctx.arena.get_accessor(member_node) {
                    let name_atom = self.get_member_name_atom(accessor.name).or_else(|| {
                        self.get_property_name_resolved(accessor.name)
                            .map(|name| self.ctx.types.intern_string(&name))
                    });
                    if let Some(name_atom) = name_atom {
                        member_order += 1;
                        let current_order = member_order;
                        let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                            getter: None,
                            setter: None,
                            declaration_order: current_order,
                        });

                        if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                            let getter_type = if accessor.type_annotation.is_some() {
                                self.get_type_from_type_node_in_type_literal(
                                    accessor.type_annotation,
                                )
                            } else {
                                TypeId::ANY
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
                                    (param.type_annotation.is_some()).then(|| {
                                        self.get_type_from_type_node_in_type_literal(
                                            param.type_annotation,
                                        )
                                    })
                                })
                                .unwrap_or(TypeId::UNKNOWN);
                            entry.setter = Some(setter_type);
                        }
                    }
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
                let key_type = if param_data.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(param_data.type_annotation)
                } else {
                    TypeId::ANY
                };

                // TS1268/TS1337: Check index signature parameter type validity.
                // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                let has_param_grammar_error =
                    param_data.dot_dot_dot_token || param_data.question_token;
                let is_valid_index_type = key_type == TypeId::STRING
                    || key_type == TypeId::NUMBER
                    || key_type == TypeId::SYMBOL
                    || is_template_literal_type(self.ctx.types, key_type);

                // Also check syntactically for type aliases that resolve to valid types
                let is_valid_via_alias = if let Some(type_node) =
                    self.ctx.arena.get(param_data.type_annotation)
                {
                    self.is_valid_index_sig_param_type(type_node.kind, param_data.type_annotation)
                } else {
                    false
                };

                if !is_valid_index_type && !is_valid_via_alias && !has_param_grammar_error {
                    use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                    // Check if this is a literal type or generic type (TS1337 vs TS1268)
                    let type_node = self.ctx.arena.get(param_data.type_annotation);
                    let type_node_kind = type_node.map(|n| n.kind).unwrap_or(0);
                    let is_generic_or_literal = self.is_type_param_or_literal_in_index_sig(
                        type_node_kind,
                        param_data.type_annotation,
                    );
                    if is_generic_or_literal {
                        self.error_at_node(
                            param_idx,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_CANNOT_BE_A_LITERAL_TYPE_OR_GENERIC_TYPE_CONSI,
                        );
                    } else {
                        self.error_at_node(
                            param_idx,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                        );
                    }
                }

                let value_type = if index_sig.type_annotation.is_some() {
                    self.get_type_from_type_node_in_type_literal(index_sig.type_annotation)
                } else {
                    TypeId::ANY
                };
                let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                let param_name = self
                    .ctx
                    .arena
                    .get(param_data.name)
                    .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                    .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));
                let info = IndexSignature {
                    key_type,
                    value_type,
                    readonly,
                    param_name,
                };
                if key_type == TypeId::NUMBER {
                    Self::merge_index_signature(&mut number_index, info);
                } else {
                    Self::merge_index_signature(&mut string_index, info);
                }
            }
        }

        // Convert method overloads to properly-typed properties.
        // Single-method entries become Function types; multiple-signature entries
        // become Callable types with explicit overloads so the solver can perform
        // overload resolution (e.g., Object.freeze's specific literal-preserving
        // overload is tried before the generic fallback).
        for (name, entry) in method_overloads {
            let type_id = if entry.signatures.len() == 1 {
                // Single method: create a Function type
                let sig = entry
                    .signatures
                    .into_iter()
                    .next()
                    .expect("single signature confirmed by len check");
                factory.function(tsz_solver::FunctionShape {
                    type_params: sig.type_params,
                    params: sig.params,
                    this_type: sig.this_type,
                    return_type: sig.return_type,
                    type_predicate: sig.type_predicate,
                    is_constructor: false,
                    is_method: true,
                })
            } else {
                // Multiple overloads: create a Callable type with all signatures
                let shape = CallableShape {
                    call_signatures: entry.signatures,
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                };
                factory.callable(shape)
            };
            properties.push(PropertyInfo {
                name,
                type_id,
                write_type: type_id,
                optional: entry.optional,
                readonly: entry.readonly,
                is_method: true,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: entry.declaration_order,
            });
        }

        // Convert accessors to properties
        for (name, accessor) in accessors {
            let read_type = accessor
                .getter
                .or(accessor.setter)
                .unwrap_or(TypeId::UNKNOWN);
            // When a setter parameter has no type annotation, its type is UNKNOWN
            // (sentinel). Filter out so we fall back to getter type, matching tsc.
            let write_type = accessor
                .setter
                .filter(|&t| t != TypeId::UNKNOWN)
                .or(accessor.getter)
                .unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            properties.push(PropertyInfo {
                name,
                type_id: read_type,
                write_type,
                optional: false,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: accessor.declaration_order,
            });
        }

        let result = if !call_signatures.is_empty() || !construct_signatures.is_empty() {
            let shape = CallableShape {
                call_signatures,
                construct_signatures,
                properties,
                string_index,
                number_index,
                symbol: interface_symbol,
                is_abstract: false,
            };
            factory.callable(shape)
        } else if string_index.is_some() || number_index.is_some() {
            factory.object_with_index(ObjectShape {
                properties,
                string_index,
                number_index,
                symbol: interface_symbol,
                ..ObjectShape::default()
            })
        } else if !properties.is_empty() {
            factory.object_with_symbol(properties, interface_symbol)
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
    /// The merged `TypeId` including all base interface members
    pub(crate) fn merge_interface_heritage_types(
        &mut self,
        declarations: &[NodeIndex],
        mut derived_type: TypeId,
    ) -> TypeId {
        use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
        use tracing::trace;

        trace!(decls = declarations.len(), derived_type_id = %derived_type.0, "merge_interface_heritage_types called");

        // Depth guard: heritage merging can trigger get_type_of_symbol on base
        // interfaces, which in turn calls compute_type_of_symbol →
        // merge_interface_heritage_types again for cross-referencing interfaces.
        // Use a dedicated counter with a tight limit (10) because each heritage
        // merge cycle is expensive (it resolves full interface types).
        let heritage_depth = self.ctx.heritage_merge_depth.get();
        if heritage_depth >= 5 {
            return derived_type;
        }
        // Bail out early if type resolution fuel is exhausted.
        if !self.ctx.consume_fuel() {
            return derived_type;
        }
        self.ctx.heritage_merge_depth.set(heritage_depth + 1);

        let mut pushed_derived = false;
        let mut derived_param_updates = Vec::new();
        let current_sym = declarations
            .first()
            .and_then(|&decl_idx| self.ctx.binder.get_node_symbol(decl_idx));

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

                    // Resolve the base type and its type params.  We use
                    // get_type_of_symbol (which caches) for the type, and
                    // get_type_params_for_symbol (which also caches) for params.
                    // This ensures the TypeParam TypeIds in base_type_params match
                    // the TypeIds embedded in the base type's member signatures,
                    // which is critical for substitution to work correctly.
                    let mut base_type = None;

                    // Try class instance type first (needs special handling)
                    for &base_decl_idx in &base_symbol.declarations {
                        let Some(base_node) = self.ctx.arena.get(base_decl_idx) else {
                            continue;
                        };
                        if let Some(base_class) = self.ctx.arena.get_class(base_node) {
                            base_type =
                                Some(self.get_class_instance_type(base_decl_idx, base_class));
                            break;
                        }
                    }
                    if base_type.is_none() && base_symbol.value_declaration.is_some() {
                        let base_decl_idx = base_symbol.value_declaration;
                        if let Some(base_node) = self.ctx.arena.get(base_decl_idx)
                            && let Some(base_class) = self.ctx.arena.get_class(base_node)
                        {
                            base_type =
                                Some(self.get_class_instance_type(base_decl_idx, base_class));
                        }
                    }

                    // For interfaces/type aliases, resolve through symbol type
                    if base_type.is_none() {
                        let resolved = self.get_type_of_symbol(base_sym_id);
                        if resolved != TypeId::ERROR && resolved != TypeId::UNKNOWN {
                            base_type = Some(resolved);
                        } else if !self.ctx.lib_contexts.is_empty() {
                            // Fallback: if get_type_of_symbol returned UNKNOWN/ERROR
                            // (e.g., due to circular heritage chains like
                            // IteratorObject <-> Iterator in esnext.iterator.d.ts),
                            // try resolving via lib type resolution which has
                            // dedicated cycle-breaking logic.
                            if let Some(lib_type) =
                                self.resolve_lib_type_by_name(&base_symbol.escaped_name)
                                && lib_type != TypeId::ERROR
                                && lib_type != TypeId::UNKNOWN
                            {
                                base_type = Some(lib_type);
                            }
                        }
                    }

                    let Some(mut base_type) = base_type else {
                        continue;
                    };

                    // Use get_type_params_for_symbol to get the ORIGINAL TypeParam
                    // TypeIds that match the ones in base_type's member signatures.
                    // Previously we used push_type_parameters which creates NEW
                    // TypeIds that don't match, causing substitution to be a no-op.
                    let base_type_params = self.get_type_params_for_symbol(base_sym_id);

                    if type_args.len() < base_type_params.len() {
                        for (param_index, param) in
                            base_type_params.iter().enumerate().skip(type_args.len())
                        {
                            let fallback = param
                                .default
                                .or(param.constraint)
                                .unwrap_or(TypeId::UNKNOWN);
                            let substitution = TypeSubstitution::from_args(
                                self.ctx.types,
                                &base_type_params[..param_index],
                                &type_args,
                            );
                            type_args.push(tsz_solver::instantiate_type_preserving_meta(
                                self.ctx.types,
                                fallback,
                                &substitution,
                            ));
                        }
                    }
                    if type_args.len() > base_type_params.len() {
                        type_args.truncate(base_type_params.len());
                    }

                    let has_structural_self_arg = current_sym.is_some_and(|current_sym| {
                        type_args.iter().copied().any(|arg| {
                            self.type_requires_structure_of_symbol_for_base_type(arg, current_sym)
                        })
                    });

                    let substitution =
                        TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);
                    base_type = instantiate_type(self.ctx.types, base_type, &substitution);
                    let is_builtin_array_heritage =
                        matches!(base_symbol.escaped_name.as_str(), "Array" | "ReadonlyArray");
                    let requires_self = !is_builtin_array_heritage
                        && current_sym.is_some_and(|current_sym| {
                            has_structural_self_arg
                                || self.type_requires_structure_of_symbol_for_base_type(
                                    base_type,
                                    current_sym,
                                )
                        });

                    if let Some(current_sym) = current_sym
                        && requires_self
                    {
                        self.report_recursive_base_type_for_symbol(current_sym);
                        self.report_instantiated_type_alias_mapped_constraint_cycles(
                            base_sym_id,
                            &base_type_params,
                            &type_args,
                            current_sym,
                        );
                        derived_type = self.merge_interface_types(derived_type, base_type);
                        continue;
                    }

                    derived_type = self.merge_interface_types(derived_type, base_type);
                }
            }
        }

        if pushed_derived {
            self.pop_type_parameters(derived_param_updates);
        }

        self.ctx.heritage_merge_depth.set(heritage_depth);
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
    /// The merged `TypeId`
    pub(crate) fn merge_interface_types(&mut self, derived: TypeId, base: TypeId) -> TypeId {
        if derived == base {
            return derived;
        }
        // Depth guard: merge_interface_types can recurse through merge_properties
        // and resolve_type_for_interface_merge, creating an unbounded cycle.
        if !self.ctx.enter_recursion() {
            return derived;
        }
        let result = self.merge_interface_types_impl(derived, base);
        self.ctx.leave_recursion();
        result
    }

    fn merge_interface_types_impl(&mut self, derived: TypeId, base: TypeId) -> TypeId {
        use tracing::trace;
        use tsz_solver::type_queries::{InterfaceMergeKind, classify_for_interface_merge};
        use tsz_solver::{CallableShape, ObjectShape};

        // Bail out if type resolution fuel is exhausted to prevent
        // expensive merges from hanging on augmented module interfaces
        // (e.g., react + create-emotion-styled cross-referencing).
        if !self.ctx.consume_fuel() {
            return derived;
        }

        trace!(derived_id = %derived.0, base_id = %base.0, "merge_interface_types called");
        let factory = self.ctx.types.factory();

        // Resolve Application/Lazy types before classification.
        // When an interface extends a type alias (e.g., `interface TaggedPair<T> extends Pair<T>`
        // where `type Pair<T> = AB<T, T>`), the instantiated base type may be an Application
        // (e.g., `AB<number, number>`) which classify_for_interface_merge cannot structurally
        // merge. Evaluating it first resolves it to an Object type with the actual properties.
        let derived_resolved = self.resolve_type_for_interface_merge(derived);
        let base_resolved = self.resolve_type_for_interface_merge(base);

        let derived_kind = classify_for_interface_merge(self.ctx.types, derived_resolved);
        let base_kind = classify_for_interface_merge(self.ctx.types, base_resolved);
        trace!(derived_kind = ?derived_kind, base_kind = ?base_kind, "Classified types for merge");

        match (derived_kind, base_kind) {
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                trace!(
                    derived_call_sigs = derived_shape.call_signatures.len(),
                    derived_construct_sigs = derived_shape.construct_signatures.len(),
                    base_call_sigs = base_shape.call_signatures.len(),
                    base_construct_sigs = base_shape.construct_signatures.len(),
                    "Callable+Callable merge signature counts"
                );
                let mut call_signatures = derived_shape.call_signatures.clone();
                call_signatures.extend(base_shape.call_signatures.iter().cloned());
                let mut construct_signatures = derived_shape.construct_signatures.clone();
                construct_signatures.extend(base_shape.construct_signatures.iter().cloned());
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.callable(CallableShape {
                    call_signatures,
                    construct_signatures,
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or_else(|| base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or_else(|| base_shape.number_index.clone()),
                    symbol: derived_shape.symbol,
                    is_abstract: false,
                })
            }
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape.string_index.clone(),
                    number_index: derived_shape.number_index.clone(),
                    symbol: derived_shape.symbol,
                    is_abstract: false,
                })
            }
            (
                InterfaceMergeKind::Callable(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.callable_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.callable(CallableShape {
                    call_signatures: derived_shape.call_signatures.clone(),
                    construct_signatures: derived_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or_else(|| base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or_else(|| base_shape.number_index.clone()),
                    symbol: derived_shape.symbol,
                    is_abstract: false,
                })
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                    symbol: derived_shape.symbol,
                    is_abstract: false,
                })
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::Callable(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.callable_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.callable(CallableShape {
                    call_signatures: base_shape.call_signatures.clone(),
                    construct_signatures: base_shape.construct_signatures.clone(),
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or_else(|| base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or_else(|| base_shape.number_index.clone()),
                    symbol: derived_shape.symbol,
                    is_abstract: false,
                })
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.object_with_symbol(properties, derived_shape.symbol)
            }
            (
                InterfaceMergeKind::Object(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                tracing::trace!(
                    ?derived_shape_id,
                    ?base_shape_id,
                    has_base_string_index = base_shape.string_index.is_some(),
                    has_base_number_index = base_shape.number_index.is_some(),
                    "merge_interface_types: Object + ObjectWithIndex"
                );
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                let result = factory.object_with_index(ObjectShape {
                    properties,
                    string_index: base_shape.string_index.clone(),
                    number_index: base_shape.number_index.clone(),
                    symbol: derived_shape.symbol,
                    ..ObjectShape::default()
                });
                tracing::trace!(result_type = %result.0, "merge_interface_types: created merged type");
                result
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::Object(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape.string_index.clone(),
                    number_index: derived_shape.number_index.clone(),
                    symbol: derived_shape.symbol,
                    ..ObjectShape::default()
                })
            }
            (
                InterfaceMergeKind::ObjectWithIndex(derived_shape_id),
                InterfaceMergeKind::ObjectWithIndex(base_shape_id),
            ) => {
                let derived_shape = self.ctx.types.object_shape(derived_shape_id);
                let base_shape = self.ctx.types.object_shape(base_shape_id);
                let properties =
                    self.merge_properties(&derived_shape.properties, &base_shape.properties);
                factory.object_with_index(ObjectShape {
                    properties,
                    string_index: derived_shape
                        .string_index
                        .clone()
                        .or_else(|| base_shape.string_index.clone()),
                    number_index: derived_shape
                        .number_index
                        .clone()
                        .or_else(|| base_shape.number_index.clone()),
                    symbol: derived_shape.symbol,
                    ..ObjectShape::default()
                })
            }
            // When one side is an intersection (e.g., from global augmentation merging
            // an interface with additional properties), decompose it and merge the
            // callable/object parts properly so that construct signatures are preserved.
            // Use resolved types so that Lazy wrappers (e.g., type aliases) are
            // expanded to their structural intersection form before decomposition.
            (_, InterfaceMergeKind::Intersection) | (InterfaceMergeKind::Intersection, _) => self
                .merge_with_intersection(derived_resolved, derived_kind, base_resolved, base_kind),
            // When the derived interface has no own members (TypeId::ANY), just use the base.
            (InterfaceMergeKind::Other, _) if derived == TypeId::ANY => base,
            // When the base is an Array or Tuple type (e.g., `interface MyTuple extends [] { ... }`),
            // create an intersection of derived & base. This preserves the array/tuple nature
            // of the base in the resulting type, which is critical for:
            // - Weak type detection (TS2559): the intersection prevents false weak-type violations
            //   because the target is not a standalone object.
            // - Assignability: array/tuple sources can be checked against the tuple base.
            // Track the result so the checker can also suppress false NoCommonProperties failures.
            (_, InterfaceMergeKind::Other)
                if tsz_solver::type_queries::is_array_or_tuple_type(
                    self.ctx.types,
                    base_resolved,
                ) && derived != TypeId::ANY =>
            {
                let result = factory.intersection(vec![derived, base]);
                self.ctx.types_extending_array.insert(result);
                result
            }
            _ => derived,
        }
    }

    fn resolve_type_for_interface_merge(&mut self, type_id: TypeId) -> TypeId {
        if tsz_solver::type_queries::needs_evaluation_for_merge(self.ctx.types, type_id) {
            // Use the solver evaluator without ensure_relation_input_ready.
            // evaluate_type_with_env triggers lazy ref resolution which can cause
            // explosive type creation on augmented module interfaces (react + emotion).
            //
            // Suppress `this` binding so that ThisType references inside resolved
            // Lazy types are preserved. During heritage merging, `this` must remain
            // unbound until the final derived interface is constructed; binding it
            // here would incorrectly lock it to the base interface identity (e.g.,
            // `A` instead of the derived `D`).
            use crate::query_boundaries::state::type_environment::evaluate_type_suppressing_this;
            let env = self.ctx.type_env.borrow();
            let evaluated = evaluate_type_suppressing_this(self.ctx.types, &*env, type_id);
            if evaluated != type_id {
                return evaluated;
            }
        }
        type_id
    }

    /// Merge an interface type with an intersection base/derived.
    ///
    /// When a lib interface is augmented (e.g., `ErrorConstructor` gets `captureStackTrace`
    /// from user code), the resolved type is an intersection like
    /// `Callable(call_sigs, construct_sigs, props) & Object(captureStackTrace)`.
    ///
    /// When a derived interface (e.g., `RangeErrorConstructor extends ErrorConstructor`)
    /// needs to merge with this intersection base, we must decompose the intersection,
    /// find the callable member, merge it properly with the derived callable (preserving
    /// construct signatures), and then re-wrap with the remaining intersection members.
    fn merge_with_intersection(
        &mut self,
        derived: TypeId,
        _derived_kind: tsz_solver::type_queries::InterfaceMergeKind,
        base: TypeId,
        base_kind: tsz_solver::type_queries::InterfaceMergeKind,
    ) -> TypeId {
        use crate::query_boundaries::common::intersection_members;
        use tsz_solver::type_queries::{InterfaceMergeKind, classify_for_interface_merge};

        let factory = self.ctx.types.factory();

        // Determine which side is the intersection and which is the "other" type
        let (intersection_id, other_id, other_is_derived) =
            if matches!(base_kind, InterfaceMergeKind::Intersection) {
                (base, derived, true)
            } else {
                (derived, base, false)
            };

        // Get the intersection members
        let Some(members) = intersection_members(self.ctx.types, intersection_id) else {
            return factory.intersection2(derived, base);
        };

        // Find a structurally mergeable member in the intersection (Callable, Object,
        // or ObjectWithIndex). Resolve Lazy members first so that interfaces
        // (e.g., `A` in `A & string[]`) are expanded to their structural form.
        let mut mergeable_member = None;
        let mut other_members = Vec::new();

        for &member in &members {
            // Once we have a mergeable member, skip resolution/classification
            // for remaining members — they just pass through as-is.
            if mergeable_member.is_none() {
                let resolved_member = self.resolve_type_for_interface_merge(member);
                let kind = classify_for_interface_merge(self.ctx.types, resolved_member);
                if kind.is_structurally_mergeable() {
                    mergeable_member = Some(resolved_member);
                    continue;
                }
            }
            // Keep original (unresolved) member for re-wrapping into the
            // intersection — these are non-mergeable types like `string[]`
            // that pass through as-is.
            other_members.push(member);
        }

        // If we found a mergeable member, structurally merge it with the other side
        if let Some(mergeable_id) = mergeable_member {
            let (merge_derived, merge_base) = if other_is_derived {
                (other_id, mergeable_id)
            } else {
                (mergeable_id, other_id)
            };

            // Recursively merge the parts (hits Callable+Callable, Object+Object,
            // Callable+Object, etc. paths instead of the Intersection path)
            let merged = self.merge_interface_types(merge_derived, merge_base);

            // Re-wrap with the remaining intersection members (e.g., string[])
            if other_members.is_empty() {
                merged
            } else {
                let mut all = vec![merged];
                all.extend(other_members);
                factory.intersection(all)
            }
        } else {
            // No mergeable member found - fall back to plain intersection
            factory.intersection2(derived, base)
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
    pub(crate) fn merge_properties(
        &mut self,
        derived: &[tsz_solver::PropertyInfo],
        base: &[tsz_solver::PropertyInfo],
    ) -> Vec<tsz_solver::PropertyInfo> {
        use rustc_hash::{FxHashMap, FxHashSet};
        use tsz_common::interner::Atom;

        // Find the max declaration_order from base so derived-only properties
        // can be offset to come after all base properties.
        let base_max_order = base.iter().map(|p| p.declaration_order).max().unwrap_or(0);

        let total_len = derived.len() + base.len();
        if total_len <= 32 {
            let mut merged = Vec::with_capacity(total_len);
            merged.extend_from_slice(base);
            for prop in derived {
                if let Some(pos) = merged.iter().position(|p| p.name == prop.name) {
                    // Declaration merging should preserve callable overload sets from
                    // both sides instead of dropping the earlier property type.
                    let base_prop = &merged[pos];
                    let merged_type = if crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        base_prop.type_id,
                    )
                    .is_some()
                        && crate::query_boundaries::common::callable_shape_for_type(
                            self.ctx.types,
                            prop.type_id,
                        )
                        .is_some()
                    {
                        self.merge_interface_types(prop.type_id, base_prop.type_id)
                    } else {
                        prop.type_id
                    };

                    let mut merged_prop = prop.clone();
                    // When the merge produces a new callable type (from concatenating
                    // derived + base call signatures), update BOTH type_id and write_type.
                    // Leaving write_type pointing to the derived-only callable creates a
                    // false "split accessor" (type_id != write_type) that triggers the
                    // contravariant write-type check in check_property_compatibility,
                    // causing false TS2322 errors for interface-extends assignments.
                    if merged_type != prop.type_id && merged_prop.write_type == prop.type_id {
                        merged_prop.write_type = merged_type;
                    }
                    merged_prop.type_id = merged_type;
                    merged_prop.declaration_order = base_prop.declaration_order;
                    merged[pos] = merged_prop;
                } else {
                    let mut new_prop = prop.clone();
                    new_prop.declaration_order = base_max_order + prop.declaration_order;
                    merged.push(new_prop);
                }
            }
            return merged;
        }

        let mut derived_map: FxHashMap<Atom, &tsz_solver::PropertyInfo> =
            FxHashMap::with_capacity_and_hasher(derived.len(), Default::default());
        for prop in derived {
            derived_map.insert(prop.name, prop);
        }

        let mut merged = Vec::with_capacity(total_len);
        let mut processed: FxHashSet<Atom> =
            FxHashSet::with_capacity_and_hasher(derived.len(), Default::default());

        for base_prop in base {
            if let Some(derived_prop) = derived_map.get(&base_prop.name) {
                let merged_type = if crate::query_boundaries::common::callable_shape_for_type(
                    self.ctx.types,
                    base_prop.type_id,
                )
                .is_some()
                    && crate::query_boundaries::common::callable_shape_for_type(
                        self.ctx.types,
                        derived_prop.type_id,
                    )
                    .is_some()
                {
                    self.merge_interface_types(derived_prop.type_id, base_prop.type_id)
                } else {
                    derived_prop.type_id
                };

                let mut prop = (*derived_prop).clone();
                if merged_type != derived_prop.type_id && prop.write_type == derived_prop.type_id {
                    prop.write_type = merged_type;
                }
                prop.type_id = merged_type;
                prop.declaration_order = base_prop.declaration_order;
                merged.push(prop);
                processed.insert(base_prop.name);
            } else {
                merged.push(base_prop.clone());
            }
        }

        for prop in derived {
            if !processed.contains(&prop.name) {
                let mut new_prop = prop.clone();
                new_prop.declaration_order = base_max_order + prop.declaration_order;
                merged.push(new_prop);
            }
        }

        merged
    }

    /// Get the interned Atom for a member name node, handling identifiers,
    /// string literals, and numeric literals (with canonical normalization).
    fn get_member_name_atom(&self, name_idx: NodeIndex) -> Option<Atom> {
        let name = crate::types_domain::queries::core::get_literal_property_name(
            self.ctx.arena,
            name_idx,
        )?;
        Some(self.ctx.types.intern_string(&name))
    }
}
