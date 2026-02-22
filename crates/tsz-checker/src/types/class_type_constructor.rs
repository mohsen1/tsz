//! Class constructor type resolution (static members, construct signatures, inheritance).

use crate::query_boundaries::class_type::{callable_shape_for_type, construct_signatures_for_type};
use crate::state::{CheckerState, MemberAccessLevel};
use rustc_hash::FxHashMap;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::visitor::is_template_literal_type;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, PropertyInfo, TypeId, TypeParamInfo,
    TypePredicate, TypeSubstitution, Visibility, instantiate_type, types::ParamInfo,
};

// =============================================================================
// Class Constructor Type Resolution
// =============================================================================

impl<'a> CheckerState<'a> {
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
    /// * `class_idx` - The `NodeIndex` of the class declaration
    /// * `class` - The parsed class data
    ///
    /// # Returns
    /// The `TypeId` representing the constructor type of the class
    pub(crate) fn get_class_constructor_type(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        let current_sym = self.ctx.binder.get_node_symbol(class_idx);
        if let Some(&cached) = self.ctx.class_constructor_type_cache.get(&class_idx) {
            return cached;
        }
        let can_use_cache = current_sym
            .map(|sym_id| !self.ctx.class_constructor_resolution_set.contains(&sym_id))
            .unwrap_or(true);

        // Cycle detection: prevent infinite recursion on circular class hierarchies
        // (e.g. class C extends C {}, or A extends B extends A)
        let did_insert = if let Some(sym_id) = current_sym {
            if self.ctx.class_constructor_resolution_set.insert(sym_id) {
                true
            } else {
                // Already resolving this class's constructor type — cycle detected
                return TypeId::ERROR;
            }
        } else {
            false
        };

        // Check fuel to prevent timeout on pathological inheritance hierarchies
        if !self.ctx.consume_fuel() {
            if did_insert && let Some(sym_id) = current_sym {
                self.ctx.class_constructor_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR;
        }

        let result = self.get_class_constructor_type_inner(class_idx, class);

        // Cleanup: remove from resolution set
        if did_insert && let Some(sym_id) = current_sym {
            self.ctx.class_constructor_resolution_set.remove(&sym_id);
        }

        // Cache all terminal outcomes (including ERROR) so repeated constructor
        // type queries can short-circuit pathological inheritance recursion.
        if can_use_cache {
            self.ctx
                .class_constructor_type_cache
                .insert(class_idx, result);
        }

        result
    }

    fn get_class_constructor_type_inner(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);
        let (class_type_params, type_param_updates) =
            self.push_type_parameters(&class.type_parameters);
        let instance_type = self.get_class_instance_type(class_idx, class);

        // Get the class symbol for nominal identity
        let current_sym = self.ctx.binder.get_node_symbol(class_idx);

        struct MethodAggregate {
            overload_signatures: Vec<CallSignature>,
            impl_signatures: Vec<CallSignature>,
            overload_optional: bool,
            impl_optional: bool,
            visibility: Visibility,
        }

        struct AccessorAggregate {
            getter: Option<TypeId>,
            setter: Option<TypeId>,
            visibility: Visibility,
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
                    let type_id = if prop.type_annotation.is_some() {
                        self.get_type_from_type_node(prop.type_annotation)
                    } else if prop.initializer.is_some() {
                        // Set in_static_property_initializer for proper super checking
                        if let Some(ref mut class_info) = self.ctx.enclosing_class {
                            class_info.in_static_property_initializer = true;
                        }
                        let prev = self.ctx.preserve_literal_types;
                        self.ctx.preserve_literal_types = true;
                        let init_type = self.get_type_of_node(prop.initializer);
                        self.ctx.preserve_literal_types = prev;
                        if let Some(ref mut class_info) = self.ctx.enclosing_class {
                            class_info.in_static_property_initializer = false;
                        }

                        let is_readonly = self.has_readonly_modifier(&prop.modifiers);
                        if is_readonly {
                            init_type
                        } else {
                            self.widen_literal_type(init_type)
                        }
                    } else {
                        // Static properties without type annotation or initializer
                        // get implicit 'any' type (same as instance properties).
                        // TS7008 is emitted separately when noImplicitAny is on.
                        TypeId::ANY
                    };

                    let visibility = self.get_visibility_from_modifiers(&prop.modifiers);

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: prop.question_token,
                            readonly: self.has_readonly_modifier(&prop.modifiers),
                            is_method: false,
                            visibility,
                            parent_id: current_sym,
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
                    let visibility = self.get_visibility_from_modifiers(&method.modifiers);
                    // For static methods, `this` refers to the constructor type
                    // Get it from the symbol if available
                    let static_this_type = self
                        .ctx
                        .binder
                        .get_node_symbol(class_idx)
                        .map(|sym_id| self.get_type_of_symbol(sym_id));
                    let signature = self.call_signature_from_method_with_this(
                        method,
                        static_this_type,
                        member_idx,
                    );
                    let entry = methods.entry(name_atom).or_insert(MethodAggregate {
                        overload_signatures: Vec::new(),
                        impl_signatures: Vec::new(),
                        overload_optional: false,
                        impl_optional: false,
                        visibility,
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
                    let visibility = self.get_visibility_from_modifiers(&accessor.modifiers);
                    let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                        getter: None,
                        setter: None,
                        visibility,
                    });

                    if k == syntax_kind_ext::GET_ACCESSOR {
                        let getter_type = if accessor.type_annotation.is_some() {
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
                                (param.type_annotation.is_some())
                                    .then(|| self.get_type_from_type_node(param.type_annotation))
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

                    let param_idx = index_sig
                        .parameters
                        .nodes
                        .first()
                        .copied()
                        .unwrap_or(NodeIndex::NONE);

                    let param_data = index_sig
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&pi| self.ctx.arena.get(pi))
                        .and_then(|pn| self.ctx.arena.get_parameter(pn));

                    let key_type = param_data
                        .and_then(|param| {
                            (param.type_annotation.is_some())
                                .then(|| self.get_type_from_type_node(param.type_annotation))
                        })
                        .unwrap_or(TypeId::STRING);

                    // TS1268: An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type
                    // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                    let has_param_grammar_error =
                        param_data.is_some_and(|p| p.dot_dot_dot_token || p.question_token);
                    let is_valid_index_type = key_type == TypeId::STRING
                        || key_type == TypeId::NUMBER
                        || key_type == TypeId::SYMBOL
                        || is_template_literal_type(self.ctx.types, key_type);

                    if !is_valid_index_type && !has_param_grammar_error {
                        use crate::diagnostics::{diagnostic_codes, diagnostic_messages};
                        self.error_at_node(
                            param_idx,
                            diagnostic_messages::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                            diagnostic_codes::AN_INDEX_SIGNATURE_PARAMETER_TYPE_MUST_BE_STRING_NUMBER_SYMBOL_OR_A_TEMPLATE_LIT,
                        );
                    }

                    let value_type = if index_sig.type_annotation.is_some() {
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
                    visibility: accessor.visibility,
                    parent_id: current_sym,
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
            let type_id = factory.callable(CallableShape {
                call_signatures: signatures,
                construct_signatures: Vec::new(),
                properties: Vec::new(),
                string_index: None,
                number_index: None,
                symbol: None,
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
                    visibility: method.visibility,
                    parent_id: current_sym,
                },
            );
        }

        // Class constructor values always expose an implicit `prototype` property
        // whose type is the class instance type.
        let prototype_name = self.ctx.types.intern_string("prototype");
        properties.insert(
            prototype_name,
            PropertyInfo {
                name: prototype_name,
                type_id: instance_type,
                write_type: instance_type,
                optional: false,
                readonly: false,
                is_method: false,
                visibility: Visibility::Public,
                parent_id: current_sym,
            },
        );

        // Track base class constructor for inheritance
        let mut inherited_construct_signatures: Option<Vec<CallSignature>> = None;
        // Track the base expression's type when it's a type parameter.
        // Used to intersect with the final constructor type so that
        // `class extends base` (where base: T) produces `T & ConstructorType`,
        // making the result assignable to T (mixin pattern).
        let mut base_type_param: Option<TypeId> = None;

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
                            inherited_construct_signatures = self
                                .remap_inherited_construct_signatures(
                                    base_constructor_type,
                                    &class_type_params,
                                    instance_type,
                                    None,
                                );
                        }
                        break;
                    }
                };
                // Check for self-referential class BEFORE processing
                if let Some(sym_id) = current_sym
                    && base_sym_id == sym_id
                {
                    break;
                }
                // Check resolution set to prevent infinite recursion through circular extends
                if self
                    .ctx
                    .class_constructor_resolution_set
                    .contains(&base_sym_id)
                {
                    break;
                }

                let Some(base_class_idx) = self.get_class_declaration_from_symbol(base_sym_id)
                else {
                    // Check if the base expression has a type parameter type (mixin pattern).
                    // e.g., `class extends base` where `base: T extends Constructor<{}>`.
                    let expr_type = self.get_type_of_node(expr_idx);
                    if tsz_solver::visitor::type_param_info(self.ctx.types, expr_type).is_some() {
                        base_type_param = Some(expr_type);
                    }
                    if let Some(base_constructor_type) =
                        self.base_constructor_type_from_expression(expr_idx, type_arguments)
                    {
                        self.merge_constructor_properties_from_type(
                            base_constructor_type,
                            &mut properties,
                        );
                        inherited_construct_signatures = self.remap_inherited_construct_signatures(
                            base_constructor_type,
                            &class_type_params,
                            instance_type,
                            None,
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

                // Prevent infinite recursion when base class node index collides
                // with the current class node index (cross-arena NodeIndex collision)
                if base_class_idx == class_idx {
                    break;
                }

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
                let instantiated_base_constructor_type =
                    instantiate_type(self.ctx.types, base_constructor_type, &substitution);
                self.pop_type_parameters(base_type_param_updates);

                if let Some(base_shape) =
                    callable_shape_for_type(self.ctx.types, instantiated_base_constructor_type)
                {
                    for base_prop in &base_shape.properties {
                        properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                    inherited_construct_signatures = self
                        .remap_inherited_construct_signatures_with_substitution(
                            base_constructor_type,
                            &substitution,
                            &class_type_params,
                            instance_type,
                        );
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
            // If there's a base class with construct signatures, inherit them
            if let Some(inherited) = inherited_construct_signatures {
                construct_signatures = inherited;
            } else {
                // No base class or base class has no explicit constructor - use default
                construct_signatures.push(CallSignature {
                    type_params: class_type_params,
                    params: Vec::new(),
                    this_type: None,
                    return_type: instance_type,
                    type_predicate: None,
                    is_method: false,
                });
            }
        }

        let properties: Vec<PropertyInfo> = properties.into_values().collect();
        self.pop_type_parameters(type_param_updates);

        // Get the class symbol for nominal discrimination - this ensures that distinct
        // classes with identical structures get different TypeIds
        let class_symbol = self.ctx.binder.get_node_symbol(class_idx);

        let constructor_type = factory.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures,
            properties,
            string_index: static_string_index,
            number_index: static_number_index,
            symbol: class_symbol,
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

        // Mixin pattern: when a class extends a type-parameter-typed base
        // (e.g., `class extends base` where `base: T extends Constructor<{}>`),
        // intersect the constructor type with T so that the result is assignable
        // to T. This makes `T & ConstructorType <: T` succeed via the
        // intersection rule in the subtype checker.
        if let Some(base_tp) = base_type_param {
            return factory.intersection(vec![base_tp, constructor_type]);
        }

        constructor_type
    }

    fn remap_inherited_construct_signatures(
        &self,
        constructor_type: TypeId,
        class_type_params: &[TypeParamInfo],
        instance_type: TypeId,
        inherited_substitution: Option<&TypeSubstitution>,
    ) -> Option<Vec<CallSignature>> {
        let signatures = construct_signatures_for_type(self.ctx.types, constructor_type)?;
        if signatures.is_empty() {
            return None;
        }

        Some(
            signatures
                .iter()
                .map(|sig| {
                    let params = if let Some(subst) = inherited_substitution {
                        sig.params
                            .iter()
                            .map(|param| {
                                let mut p = param.clone();
                                p.type_id = instantiate_type(self.ctx.types, p.type_id, subst);
                                p
                            })
                            .collect()
                    } else {
                        sig.params.clone()
                    };
                    let this_type = sig.this_type.map(|t| {
                        inherited_substitution
                            .map_or(t, |subst| instantiate_type(self.ctx.types, t, subst))
                    });
                    CallSignature {
                        type_params: class_type_params.to_vec(),
                        params,
                        this_type,
                        return_type: instance_type,
                        type_predicate: sig.type_predicate.clone(),
                        is_method: sig.is_method,
                    }
                })
                .collect(),
        )
    }

    fn remap_inherited_construct_signatures_with_substitution(
        &self,
        constructor_type: TypeId,
        substitution: &TypeSubstitution,
        class_type_params: &[TypeParamInfo],
        instance_type: TypeId,
    ) -> Option<Vec<CallSignature>> {
        let signatures = construct_signatures_for_type(self.ctx.types, constructor_type)?;
        if signatures.is_empty() {
            return None;
        }

        Some(
            signatures
                .iter()
                .map(|sig| CallSignature {
                    // In inherited constructors, class type params live on the deriving class.
                    // Reusing base signature type_params can incorrectly shadow substitutions.
                    type_params: class_type_params.to_vec(),
                    params: sig
                        .params
                        .iter()
                        .map(|p| ParamInfo {
                            name: p.name,
                            type_id: instantiate_type(self.ctx.types, p.type_id, substitution),
                            optional: p.optional,
                            rest: p.rest,
                        })
                        .collect(),
                    this_type: sig
                        .this_type
                        .map(|t| instantiate_type(self.ctx.types, t, substitution)),
                    return_type: instance_type,
                    type_predicate: sig.type_predicate.as_ref().map(|pred| TypePredicate {
                        asserts: pred.asserts,
                        target: pred.target.clone(),
                        type_id: pred
                            .type_id
                            .map(|t| instantiate_type(self.ctx.types, t, substitution)),
                        parameter_index: pred.parameter_index,
                    }),
                    is_method: sig.is_method,
                })
                .collect(),
        )
    }
}
