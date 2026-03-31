//! Core implementation for class instance type resolution.

use crate::context::{EnclosingClassInfo, is_js_file_name};
use crate::query_boundaries::class_type::{callable_shape_for_type, object_shape_for_type};
use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_binder::SymbolId;
use tsz_common::interner::Atom;
use tsz_lowering::TypeLowering;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::visitor::is_template_literal_type;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, ObjectShape, PropertyInfo, TypeId, TypeParamInfo,
    Visibility,
};

#[inline]
pub(in crate::types_domain) const fn can_skip_base_instantiation(
    base_type_param_count: usize,
    explicit_type_arg_count: usize,
) -> bool {
    base_type_param_count == 0 && explicit_type_arg_count == 0
}

#[inline]
const fn exceeds_class_inheritance_depth_limit(depth: usize) -> bool {
    // Keep well above realistic inheritance chains while bounding pathological recursion.
    depth > 256
}

#[inline]
fn in_progress_class_instance_result(
    in_resolution_set: bool,
    cached: Option<TypeId>,
) -> Option<TypeId> {
    if in_resolution_set {
        Some(cached.unwrap_or(TypeId::ERROR))
    } else {
        None
    }
}

// =============================================================================
// Class Type Resolution
// =============================================================================

impl<'a> CheckerState<'a> {
    /// Get the instance type of a class declaration.
    ///
    /// This is the type that instances of the class will have. It includes:
    /// - Instance properties and methods
    /// - Inherited members from base classes
    /// - Index signatures
    /// - Private brand property for nominal typing (if class has private/protected members)
    ///
    /// # Arguments
    /// * `class_idx` - The `NodeIndex` of the class declaration
    /// * `class` - The parsed class data
    ///
    /// # Returns
    /// The `TypeId` representing the instance type of the class
    pub(crate) fn get_class_instance_type(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        let current_sym = self.ctx.binder.get_node_symbol(class_idx);
        let is_in_resolution_set = current_sym
            .is_some_and(|sym_id| self.ctx.class_instance_resolution_set.contains(&sym_id));

        // Fast path for re-entrant class instance queries: avoid re-entering
        // the full inheritance walk while the class is already being resolved.
        if let Some(result) = in_progress_class_instance_result(
            is_in_resolution_set,
            self.ctx.class_instance_type_cache.get(&class_idx).copied(),
        ) {
            return result;
        }

        if let Some(&cached) = self.ctx.class_instance_type_cache.get(&class_idx) {
            return cached;
        }

        let mut visited = FxHashSet::default();
        let mut visited_nodes = FxHashSet::default();
        let result =
            self.get_class_instance_type_inner(class_idx, class, &mut visited, &mut visited_nodes);

        // Cache all terminal outcomes (including ERROR) so pathological
        // inheritance graphs don't repeatedly recompute the same failing class.
        self.ctx.class_instance_type_cache.insert(class_idx, result);

        result
    }

    /// Inner implementation of class instance type resolution with cycle detection.
    ///
    /// This function builds the complete instance type by:
    /// 1. Collecting all instance members (properties, methods, accessors)
    /// 2. Processing constructor parameter properties
    /// 3. Handling index signatures
    /// 4. Merging base class members
    /// 5. Adding private brand for nominal typing if needed
    /// 6. Inheriting Object prototype members
    pub(crate) fn get_class_instance_type_inner(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        visited: &mut FxHashSet<SymbolId>,
        visited_nodes: &mut FxHashSet<NodeIndex>,
    ) -> TypeId {
        let current_sym = self.ctx.binder.get_node_symbol(class_idx);
        let factory = self.ctx.types.factory();

        // Try to insert into global class_instance_resolution_set for recursion prevention.
        let did_insert_into_global_set = if let Some(sym_id) = current_sym {
            if self.ctx.class_instance_resolution_set.insert(sym_id) {
                true // We inserted it
            } else {
                // Symbol already being resolved — break recursion without diagnostic
                return TypeId::ERROR;
            }
        } else {
            false
        };

        // Check for cycles using both symbol ID (for same-file cycles)
        // and node index (for cross-file cycles with @Filename annotations)
        if let Some(sym_id) = current_sym
            && !visited.insert(sym_id)
        {
            // Cleanup global set before returning (only if we inserted it)
            if did_insert_into_global_set {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR; // Circular reference detected via symbol
        }
        if !visited_nodes.insert(class_idx) {
            // Cleanup global set before returning (only if we inserted it)
            if did_insert_into_global_set && let Some(sym_id) = current_sym {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR; // Circular reference detected via node index
        }
        if exceeds_class_inheritance_depth_limit(visited_nodes.len()) {
            if did_insert_into_global_set && let Some(sym_id) = current_sym {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR;
        }

        // Check fuel to prevent timeout on pathological inheritance hierarchies
        if !self.ctx.consume_fuel() {
            // Cleanup global set before returning (only if we inserted it)
            if did_insert_into_global_set && let Some(sym_id) = current_sym {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
            return TypeId::ERROR; // Fuel exhausted - prevent infinite loop
        }

        // Class member types can reference class type parameters (e.g. `class Box<T> { value: T }`).
        // Keep class type parameters in scope while constructing the instance type.
        let (mut class_type_params, mut class_type_param_updates) =
            self.push_type_parameters(&class.type_parameters);

        // In JS files, classes don't have syntax-level type parameters.
        // JSDoc `@template T` tags serve the same purpose. If no syntax type params
        // were found, check for JSDoc @template tags on the class declaration.
        if class_type_params.is_empty() {
            let (jsdoc_params, jsdoc_updates) =
                self.push_jsdoc_class_template_type_params(class_idx);
            if !jsdoc_params.is_empty() {
                class_type_params = jsdoc_params;
                class_type_param_updates.extend(jsdoc_updates);
            }
        }

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

        struct DeferredAccessor<'b> {
            member_idx: NodeIndex,
            accessor: &'b tsz_parser::parser::node::AccessorData,
            is_getter: bool,
            name_atom: Atom,
            visibility: Visibility,
        }

        // PERF: Pre-size maps based on member count to avoid rehashing
        let member_count = class.members.nodes.len();
        let mut properties: FxHashMap<Atom, PropertyInfo> =
            FxHashMap::with_capacity_and_hasher(member_count, Default::default());
        let mut methods: FxHashMap<Atom, MethodAggregate> =
            FxHashMap::with_capacity_and_hasher(member_count / 2, Default::default());
        let mut accessors: FxHashMap<Atom, AccessorAggregate> =
            FxHashMap::with_capacity_and_hasher(4, Default::default());
        let mut string_index: Option<IndexSignature> = None;
        let mut number_index: Option<IndexSignature> = None;
        let mut has_nominal_members = false;
        let mut has_late_bound_members = false;
        let mut merged_interface_type_for_class: Option<TypeId> = None;

        // Phase 0: Pre-scan annotated properties to build a preliminary partial `this` type.
        // Property initializers like `n = this.s` need `this` to resolve during Phase 1.
        // The type builder is called from `build_type_environment` BEFORE `enclosing_class`
        // is set, so `this` in property initializers would otherwise resolve to `any`.
        // By pushing a partial type onto `this_type_stack`, initializer expressions that
        // reference `this.annotatedProp` can resolve correctly.
        let mut pushed_prescan_this = false;
        let mut prescan_this_type = None;
        {
            // PERF: Single pass over class members for prescan (was 3 separate loops).
            let mut prescan_props: Vec<PropertyInfo> = Vec::with_capacity(member_count);
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
                        let declared_type = if let Some(dt) =
                            self.effective_class_property_declared_type(member_idx, prop)
                        {
                            dt
                        } else if prop.initializer.is_some() {
                            TypeId::ANY
                        } else {
                            continue;
                        };
                        let Some(name) = self.get_property_name_resolved(prop.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let is_readonly = self.has_readonly_modifier(&prop.modifiers)
                            || self.jsdoc_has_readonly_tag(member_idx);
                        let visibility = self.get_member_visibility(&prop.modifiers, prop.name);
                        prescan_props.push(PropertyInfo {
                            name: name_atom,
                            type_id: declared_type,
                            write_type: declared_type,
                            optional: prop.question_token,
                            readonly: is_readonly,
                            is_method: false,
                            is_class_prototype: false,
                            visibility,
                            parent_id: current_sym,
                            declaration_order: 0,
                            is_string_named: false,
                        });
                    }
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                            continue;
                        };
                        if self.has_static_modifier(&method.modifiers) {
                            continue;
                        }
                        let Some(name) = self.get_property_name_resolved(method.name) else {
                            continue;
                        };
                        let name_atom = self.ctx.types.intern_string(&name);
                        let visibility = self.get_member_visibility(&method.modifiers, method.name);
                        // For methods with explicit return type annotations, create a
                        // proper Callable type so that `this.method()` during other
                        // method body inference resolves to the correct return type.
                        // Without this, the prescan type has methods typed as `any`,
                        // causing `{ ...this.annotatedMethod() }` to produce `{}`.
                        let method_type = if method.type_annotation.is_some() {
                            let (type_params, type_param_updates) =
                                self.push_type_parameters(&method.type_parameters);
                            let return_type = self.get_type_from_type_node(method.type_annotation);
                            self.pop_type_parameters(type_param_updates);
                            self.ctx.types.factory().callable(CallableShape {
                                call_signatures: vec![CallSignature {
                                    type_params,
                                    params: vec![tsz_solver::ParamInfo {
                                        name: None,
                                        type_id: TypeId::ANY,
                                        optional: false,
                                        rest: true,
                                    }],
                                    this_type: None,
                                    return_type,
                                    type_predicate: None,
                                    is_method: true,
                                }],
                                construct_signatures: Vec::new(),
                                properties: Vec::new(),
                                string_index: None,
                                number_index: None,
                                symbol: None,
                                is_abstract: false,
                            })
                        } else {
                            TypeId::ANY
                        };
                        prescan_props.push(PropertyInfo {
                            name: name_atom,
                            type_id: method_type,
                            write_type: method_type,
                            optional: false,
                            readonly: false,
                            is_method: true,
                            is_class_prototype: false,
                            visibility,
                            parent_id: current_sym,
                            declaration_order: 0,
                            is_string_named: false,
                        });
                    }
                    k if k == syntax_kind_ext::CONSTRUCTOR => {
                        let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                            continue;
                        };
                        if ctor.body.is_none() {
                            continue;
                        }
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
                            let Some(name) = self.get_property_name(param.name) else {
                                continue;
                            };
                            let name_atom = self.ctx.types.intern_string(&name);
                            let is_readonly = self.has_readonly_modifier(&param.modifiers);
                            let declared_type = if param.type_annotation.is_some() {
                                self.get_type_from_type_node(param.type_annotation)
                            } else {
                                TypeId::ANY
                            };
                            let visibility = self.get_visibility_from_modifiers(&param.modifiers);
                            prescan_props.push(PropertyInfo {
                                name: name_atom,
                                type_id: declared_type,
                                write_type: declared_type,
                                optional: param.question_token,
                                readonly: is_readonly,
                                is_method: false,
                                is_class_prototype: false,
                                visibility,
                                parent_id: current_sym,
                                declaration_order: 0,
                                is_string_named: false,
                            });
                        }
                    }
                    _ => {}
                }
            }

            // Also include the base class's already-cached instance type in the prescan
            // so that `this.inheritedProp` in a derived class initializer doesn't produce
            // a false TS2339. For example: `class B extends A { x = this.a; }` where `a`
            // is declared in A — without this, `this` would only have B's own properties.
            let base_prescan_type = self
                .get_base_class_idx(class_idx)
                .and_then(|base_idx| self.ctx.class_instance_type_cache.get(&base_idx).copied());

            if !prescan_props.is_empty() || base_prescan_type.is_some() {
                let own_prescan_type = if !prescan_props.is_empty() {
                    Some(factory.object(prescan_props))
                } else {
                    None
                };
                let prescan_type = match (own_prescan_type, base_prescan_type) {
                    (Some(own), Some(base)) => {
                        // Intersection: derived-class own props take priority in lookup,
                        // base props are reachable through the second member.
                        self.ctx.types.factory().intersection(vec![own, base])
                    }
                    (Some(own), None) => own,
                    (None, Some(base)) => base,
                    (None, None) => unreachable!(),
                };
                self.ctx
                    .class_instance_type_cache
                    .insert(class_idx, prescan_type);
                self.ctx.this_type_stack.push(prescan_type);
                prescan_this_type = Some(prescan_type);
                pushed_prescan_this = true;
            }
        }

        // Phase 1: Process all non-method members (properties, accessors, constructors, index sigs).
        // Methods are deferred to phase 2 so that a partial instance type (with property types)
        // can be pushed as `this`, allowing method body inference to resolve `this.x` references.
        let mut deferred_methods: Vec<(NodeIndex, &tsz_parser::parser::node::MethodDeclData)> =
            Vec::with_capacity(member_count / 2);
        let mut deferred_accessors: Vec<DeferredAccessor<'_>> = Vec::with_capacity(4);

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
                    let Some(name) = self.get_property_name_resolved(prop.name) else {
                        if self
                            .ctx
                            .arena
                            .get(prop.name)
                            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        {
                            has_late_bound_members = true;
                        }
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let is_readonly = self.has_readonly_modifier(&prop.modifiers)
                        || self.jsdoc_has_readonly_tag(member_idx);
                    let visibility = self.get_member_visibility(&prop.modifiers, prop.name);

                    // In JS/checkJs, arrow-property initializers inherit the class `this`.
                    // Pre-scan `this.prop = value` writes inside the arrow body so the
                    // partial instance type already includes those implicit members while
                    // we type-check the initializer itself.
                    if self.ctx.is_js_file()
                        && !prop.initializer.is_none()
                        && let Some(init_node) = self.ctx.arena.get(prop.initializer)
                        && init_node.kind == syntax_kind_ext::ARROW_FUNCTION
                        && let Some(init_func) = self.ctx.arena.get_function(init_node)
                        && !init_func.body.is_none()
                    {
                        self.collect_js_constructor_this_properties(
                            init_func.body,
                            &mut properties,
                            current_sym,
                            true,
                        );
                    }

                    let declared_type =
                        self.effective_class_property_declared_type(member_idx, prop);

                    let type_id = if let Some(declared_type) = declared_type {
                        declared_type
                    } else if prop.initializer.is_some() {
                        let current_property_placeholder = PropertyInfo {
                            name: name_atom,
                            type_id: TypeId::ANY,
                            write_type: TypeId::ANY,
                            optional: prop.question_token,
                            readonly: is_readonly,
                            is_method: false,
                            is_class_prototype: false,
                            visibility,
                            parent_id: current_sym,
                            declaration_order: 0,
                            is_string_named: false,
                        };
                        let mut partial_props: Vec<PropertyInfo> =
                            properties.values().cloned().collect();
                        if !partial_props.iter().any(|p| p.name == name_atom) {
                            partial_props.push(current_property_placeholder);
                        }
                        let refreshed_this_type = if partial_props.is_empty() {
                            prescan_this_type
                        } else {
                            let own_partial = factory.object_with_index(ObjectShape {
                                properties: partial_props,
                                string_index,
                                number_index,
                                symbol: current_sym,
                                ..ObjectShape::default()
                            });
                            Some(if let Some(prescan) = prescan_this_type {
                                factory.intersection(vec![own_partial, prescan])
                            } else {
                                own_partial
                            })
                        };
                        if let Some(partial_this) = refreshed_this_type {
                            self.ctx.this_type_stack.push(partial_this);
                            // Property initializers may already have been typed earlier
                            // during statement checking with a stale provisional `this`.
                            // Recompute them against the refreshed partial instance type.
                            self.clear_type_cache_recursive(prop.initializer);
                        }
                        // If the initializer is exactly `this`, use the polymorphic
                        // ThisType so that `class C<T> { x = this; }` with `c: C<string>`
                        // makes `c.x` resolve to `C<string>`, not the raw class type.
                        let is_this_init = self
                            .ctx
                            .arena
                            .get(prop.initializer)
                            .is_some_and(|n| n.kind == SyntaxKind::ThisKeyword as u16);
                        let prev = self.ctx.preserve_literal_types;
                        self.ctx.preserve_literal_types = true;
                        let init_type = if is_this_init {
                            self.ctx.types.this_type()
                        } else {
                            self.get_type_of_node(prop.initializer)
                        };
                        if refreshed_this_type.is_some() {
                            self.ctx.this_type_stack.pop();
                        }
                        self.ctx.preserve_literal_types = prev;
                        let init_type = if init_type == TypeId::ANY
                            && self.has_accessor_modifier(&prop.modifiers)
                        {
                            self.this_access_name_node(prop.initializer)
                                .and_then(|name_idx| {
                                    self.infer_property_type_from_class_member_assignments(
                                        &class.members.nodes,
                                        name_idx,
                                        false,
                                    )
                                })
                                .unwrap_or(init_type)
                        } else {
                            init_type
                        };
                        // Widen literal types for mutable class properties.
                        // `class Foo { name = "" }` → `name: string`.
                        // Readonly properties keep literal types:
                        // `class Foo { readonly tag = "x" }` → `tag: "x"`.
                        if is_readonly {
                            init_type
                        } else {
                            self.widen_literal_type(init_type)
                        }
                    } else if self.has_accessor_modifier(&prop.modifiers) {
                        self.infer_property_type_from_class_member_assignments(
                            &class.members.nodes,
                            prop.name,
                            false,
                        )
                        .unwrap_or(TypeId::ANY)
                    } else {
                        // Class properties without type annotation or initializer
                        // get implicit 'any' type (TS7008 when noImplicitAny is on)
                        TypeId::ANY
                    };
                    self.ctx.node_types.insert(member_idx.0, type_id);

                    properties.insert(
                        name_atom,
                        PropertyInfo {
                            name: name_atom,
                            type_id,
                            write_type: type_id,
                            optional: prop.question_token,
                            readonly: is_readonly,
                            is_method: false,
                            is_class_prototype: false,
                            visibility,
                            parent_id: current_sym,
                            declaration_order: 0,
                            is_string_named: false,
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

                    // In JS/checkJs mode, method body `this.prop = value` assignments
                    // serve as property declarations (same as constructor assignments).
                    // Scan before deferring so properties are in the partial `this` type.
                    if self.ctx.is_js_file() && !method.body.is_none() {
                        self.collect_js_constructor_this_properties(
                            method.body,
                            &mut properties,
                            current_sym,
                            true,
                        );
                    }

                    // Defer method processing to phase 2
                    deferred_methods.push((member_idx, method));
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
                    let Some(name) = self.get_property_name_resolved(accessor.name) else {
                        if self
                            .ctx
                            .arena
                            .get(accessor.name)
                            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        {
                            has_late_bound_members = true;
                        }
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let visibility = self.get_member_visibility(&accessor.modifiers, accessor.name);
                    deferred_accessors.push(DeferredAccessor {
                        member_idx,
                        accessor,
                        is_getter: k == syntax_kind_ext::GET_ACCESSOR,
                        name_atom,
                        visibility,
                    });
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
                        let is_readonly = self.has_readonly_modifier(&param.modifiers);
                        let type_id = if param.type_annotation.is_some() {
                            self.get_type_from_type_node(param.type_annotation)
                        } else if param.initializer.is_some() {
                            let init_type = self.get_type_of_node(param.initializer);
                            // Widen for mutable constructor parameter properties
                            if is_readonly {
                                init_type
                            } else {
                                self.widen_literal_type(init_type)
                            }
                        } else {
                            TypeId::ANY
                        };

                        let visibility = self.get_visibility_from_modifiers(&param.modifiers);
                        properties.insert(
                            name_atom,
                            PropertyInfo {
                                name: name_atom,
                                type_id,
                                write_type: type_id,
                                optional: param.question_token,
                                readonly: is_readonly,
                                is_method: false,
                                is_class_prototype: false,
                                visibility,
                                parent_id: current_sym,
                                declaration_order: 0,
                                is_string_named: false,
                            },
                        );
                    }

                    // In JS/checkJs mode, constructor body `this.prop = value`
                    // assignments serve as property declarations.
                    // Scan the constructor body for these patterns and add
                    // them to the class instance type.
                    // Check if the class is defined in a JS file, not just if the
                    // current file being processed is a JS file. This ensures that
                    // when a TS file references a class from a JS file, the JSDoc
                    // annotated properties are still collected.
                    let class_is_in_js_file = self
                        .source_file_data_for_node(class_idx)
                        .map(|sf| is_js_file_name(&sf.file_name))
                        .unwrap_or(false);
                    if class_is_in_js_file {
                        self.collect_js_constructor_this_properties(
                            ctor.body,
                            &mut properties,
                            current_sym,
                            true,
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

                    // TS1268: An index signature parameter type must be 'string', 'number', 'symbol', or a template literal type
                    // Suppress when the parameter already has grammar errors (rest/optional) — matches tsc.
                    let has_param_grammar_error = param.dot_dot_dot_token || param.question_token;
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

                    let value_type = if index_sig.type_annotation.is_none() {
                        TypeId::ANY
                    } else {
                        self.get_type_from_type_node(index_sig.type_annotation)
                    };
                    let readonly = self.has_readonly_modifier(&index_sig.modifiers);
                    let param_name = self
                        .ctx
                        .arena
                        .get(param.name)
                        .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                        .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));

                    let index = IndexSignature {
                        key_type,
                        value_type,
                        readonly,
                        param_name,
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

        // Pop the prescan `this` type — Phase 2 will push its own partial type.
        if pushed_prescan_this {
            self.ctx.this_type_stack.pop();
        }

        let restore_enclosing_class =
            if !deferred_methods.is_empty() || !deferred_accessors.is_empty() {
                let prev_enclosing_class = self.ctx.enclosing_class.take();
                if let Some(ref prev) = prev_enclosing_class {
                    self.ctx.enclosing_class_chain.push(prev.class_idx);
                }
                let class_type_param_names: Vec<String> = class_type_param_updates
                    .iter()
                    .map(|(name, _, _)| name.clone())
                    .collect();
                self.ctx.enclosing_class = Some(EnclosingClassInfo {
                    name: self.get_class_name_from_decl(class_idx),
                    class_idx,
                    member_nodes: class.members.nodes.clone(),
                    in_constructor: false,
                    is_declared: self.is_ambient_class_declaration(class_idx),
                    in_static_property_initializer: false,
                    in_static_member: false,
                    has_super_call_in_current_constructor: false,
                    cached_instance_this_type: Some(
                        self.ctx
                            .class_instance_type_cache
                            .get(&class_idx)
                            .copied()
                            .unwrap_or(TypeId::ERROR),
                    ),
                    type_param_names: class_type_param_names,
                    class_type_parameters: class_type_params.clone(),
                });
                Some(prev_enclosing_class)
            } else {
                None
            };

        // Phase 2: Process deferred methods with a partial `this` type so that
        // method body inference can resolve `this.x` references (e.g. `return this.b`).
        if !deferred_methods.is_empty() {
            // Build a partial instance type from properties collected so far,
            // including placeholder entries for ALL deferred methods so that
            // methods can reference each other via `this` (e.g. `typeof a`
            // in return type where `a` defaults to `this.getNumber()`).
            let mut partial_props: Vec<PropertyInfo> = Vec::with_capacity(
                properties.len() + deferred_methods.len() + deferred_accessors.len(),
            );
            partial_props.extend(properties.values().cloned());
            for (_, method) in &deferred_methods {
                if let Some(name) = self.get_property_name_resolved(method.name) {
                    let name_atom = self.ctx.types.intern_string(&name);
                    if !partial_props.iter().any(|p| p.name == name_atom) {
                        // For methods with explicit return type annotations, use the
                        // declared return type instead of ANY. This allows other methods
                        // that reference `this.method()` during body inference to get the
                        // correct return type. Without this, `{ ...this.annotatedMethod() }`
                        // would see return type ANY and produce an empty spread result.
                        let return_type = if method.type_annotation.is_some() {
                            let (_type_params, type_param_updates) =
                                self.push_type_parameters(&method.type_parameters);
                            let return_type = self.get_type_from_type_node(method.type_annotation);
                            self.pop_type_parameters(type_param_updates);
                            return_type
                        } else {
                            TypeId::ANY
                        };
                        let (type_params, type_param_updates) =
                            self.push_type_parameters(&method.type_parameters);
                        self.pop_type_parameters(type_param_updates);
                        let placeholder = factory.callable(CallableShape {
                            call_signatures: vec![CallSignature {
                                type_params,
                                params: vec![tsz_solver::ParamInfo {
                                    name: None,
                                    type_id: TypeId::ANY,
                                    optional: false,
                                    rest: true,
                                }],
                                this_type: None,
                                return_type,
                                type_predicate: None,
                                is_method: true,
                            }],
                            construct_signatures: Vec::new(),
                            properties: Vec::new(),
                            string_index: None,
                            number_index: None,
                            symbol: None,
                            is_abstract: false,
                        });
                        partial_props.push(PropertyInfo {
                            name: name_atom,
                            type_id: placeholder,
                            write_type: placeholder,
                            optional: false,
                            readonly: false,
                            is_method: true,
                            is_class_prototype: true,
                            visibility: Visibility::Public,
                            parent_id: current_sym,
                            declaration_order: 0,
                            is_string_named: false,
                        });
                    }
                }
            }
            for deferred in &deferred_accessors {
                if !partial_props.iter().any(|p| p.name == deferred.name_atom) {
                    partial_props.push(PropertyInfo {
                        name: deferred.name_atom,
                        type_id: TypeId::ANY,
                        write_type: TypeId::UNKNOWN,
                        optional: false,
                        readonly: false,
                        is_method: false,
                        is_class_prototype: true,
                        visibility: deferred.visibility,
                        parent_id: current_sym,
                        declaration_order: 0,
                        is_string_named: false,
                    });
                }
            }
            let partial_type = factory.object_with_index(ObjectShape {
                properties: partial_props,
                string_index,
                number_index,
                symbol: current_sym,
                ..ObjectShape::default()
            });
            self.ctx.this_type_stack.push(partial_type);

            // Cache the partial instance type in the node-indexed cache only.
            // Method return-type inference can trigger property access on
            // self-referential parameters (e.g. `p.x` where `p: Point` inside
            // class Point).  resolve_type_for_property_access_inner checks
            // class_instance_type_cache as a fallback for in-progress builds,
            // so Lazy(DefId) resolves to this partial type during building and
            // to the final type afterward.
            //
            // We avoid caching in symbol_instance_types here because parameter
            // types cached by get_type_of_node would permanently hold the
            // partial type, causing private-name brand-check failures.
            self.ctx
                .class_instance_type_cache
                .insert(class_idx, partial_type);

            for (member_idx, method) in deferred_methods {
                let mut signature = self.call_signature_from_method(method, member_idx);
                // When a class method without an explicit return type annotation
                // infers its return type from the body and the result is the partial
                // class instance type (i.e. the body does `return this;`), replace
                // with polymorphic `ThisType`.  This enables fluent method chaining
                // on subclass instances:  c.foo().bar().baz()  where each method is
                // defined on a different class in the hierarchy.
                if method.body.is_some()
                    && method.type_annotation.is_none()
                    && signature.return_type == partial_type
                {
                    signature.return_type = self.ctx.types.this_type();
                }
                let callable_type = factory.callable(CallableShape {
                    call_signatures: vec![signature.clone()],
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                });
                let callable_or_undefined = if method.question_token {
                    factory.union2(callable_type, TypeId::UNDEFINED)
                } else {
                    callable_type
                };
                let Some(name) = self.get_property_name_resolved(method.name) else {
                    if self
                        .ctx
                        .arena
                        .get(method.name)
                        .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                    {
                        has_late_bound_members = true;
                        self.merge_index_signature_from_unresolved_computed_name(
                            method.name,
                            callable_or_undefined,
                            &mut string_index,
                            &mut number_index,
                        );
                    }
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);
                let visibility = self.get_member_visibility(&method.modifiers, method.name);
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

            self.ctx.this_type_stack.pop();
        }

        if !deferred_accessors.is_empty() {
            let mut partial_props: Vec<PropertyInfo> =
                Vec::with_capacity(properties.len() + methods.len());
            partial_props.extend(properties.values().cloned());
            for (&name, method) in &methods {
                let (signatures, optional) = if !method.overload_signatures.is_empty() {
                    (&method.overload_signatures, method.overload_optional)
                } else {
                    (&method.impl_signatures, method.impl_optional)
                };
                if signatures.is_empty() {
                    continue;
                }
                let type_id = factory.callable(CallableShape {
                    call_signatures: signatures.clone(),
                    construct_signatures: Vec::new(),
                    properties: Vec::new(),
                    string_index: None,
                    number_index: None,
                    symbol: None,
                    is_abstract: false,
                });
                partial_props.push(PropertyInfo {
                    name,
                    type_id,
                    write_type: type_id,
                    optional,
                    readonly: false,
                    is_method: true,
                    is_class_prototype: true,
                    visibility: method.visibility,
                    parent_id: current_sym,
                    declaration_order: 0,
                    is_string_named: false,
                });
            }
            let partial_type = factory.object_with_index(ObjectShape {
                properties: partial_props,
                string_index,
                number_index,
                symbol: current_sym,
                ..ObjectShape::default()
            });
            self.ctx.this_type_stack.push(partial_type);

            for deferred in &deferred_accessors {
                if deferred.is_getter {
                    let getter_type = if deferred.accessor.type_annotation.is_some() {
                        self.get_type_from_type_node(deferred.accessor.type_annotation)
                    } else if let Some(jsdoc_type) =
                        self.jsdoc_type_annotation_for_node(deferred.member_idx)
                    {
                        jsdoc_type
                    } else {
                        let t = self.infer_getter_return_type(deferred.accessor.body);
                        self.ctx.node_types.insert(deferred.member_idx.0, t);
                        // When a getter without an explicit return type annotation
                        // infers its return type from the body and the result is the
                        // partial class instance type (i.e. the body does `return this;`),
                        // replace with polymorphic `ThisType` — same as for methods.
                        if deferred.accessor.type_annotation.is_none() && t == partial_type {
                            self.ctx.types.this_type()
                        } else {
                            t
                        }
                    };
                    let entry = accessors
                        .entry(deferred.name_atom)
                        .or_insert(AccessorAggregate {
                            getter: None,
                            setter: None,
                            visibility: deferred.visibility,
                        });
                    entry.getter = Some(getter_type);
                } else {
                    let setter_type = deferred
                        .accessor
                        .parameters
                        .nodes
                        .first()
                        .and_then(|&param_idx| {
                            let param_node = self.ctx.arena.get(param_idx)?;
                            let param = self.ctx.arena.get_parameter(param_node)?;
                            // TS type annotation (non-JS files)
                            if !self.ctx.is_js_file() && param.type_annotation.is_some() {
                                return Some(self.get_type_from_type_node(param.type_annotation));
                            }
                            // JSDoc @param annotation (JS files)
                            if self.ctx.is_js_file() {
                                let jsdoc = self.get_jsdoc_for_function(deferred.member_idx)?;
                                let pname = self.parameter_name_for_error(param.name);
                                let comment_start =
                                    self.get_jsdoc_comment_pos_for_function(deferred.member_idx);
                                return self.resolve_jsdoc_param_type_with_pos(
                                    &jsdoc,
                                    &pname,
                                    comment_start,
                                );
                            }
                            None
                        })
                        .unwrap_or(TypeId::UNKNOWN);
                    let entry = accessors
                        .entry(deferred.name_atom)
                        .or_insert(AccessorAggregate {
                            getter: None,
                            setter: None,
                            visibility: deferred.visibility,
                        });
                    entry.setter = Some(setter_type);
                }
            }

            self.ctx.this_type_stack.pop();
        }

        if let Some(prev_enclosing_class) = restore_enclosing_class {
            self.ctx.enclosing_class = prev_enclosing_class;
            if self.ctx.enclosing_class.is_some() {
                self.ctx.enclosing_class_chain.pop();
            }
        }

        // Convert accessors to properties
        for (name, accessor) in accessors {
            if methods.contains_key(&name) {
                continue;
            }
            let read_type = accessor.getter.unwrap_or_else(|| {
                if accessor.setter.is_some() {
                    TypeId::UNDEFINED
                } else {
                    TypeId::UNKNOWN
                }
            });
            // When a setter parameter has no type annotation, its type is UNKNOWN
            // (sentinel). In tsc, the setter type is inferred from the getter's
            // return type. Filter out the UNKNOWN sentinel so we fall back to the
            // getter type, matching tsc behavior for unannotated setters.
            let write_type = accessor
                .setter
                .filter(|&t| t != TypeId::UNKNOWN)
                .or(accessor.getter)
                .unwrap_or(read_type);
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
                    is_class_prototype: true,
                    visibility: accessor.visibility,
                    parent_id: current_sym,
                    declaration_order: 0,
                    is_string_named: false,
                },
            );
        }

        // Convert methods to callable properties
        for (name, method) in methods {
            // Keep existing field/accessor entries for duplicate names.
            // Duplicate member diagnostics are handled separately (TS2300/TS2393),
            // and preserving the non-method member avoids cascading TS2322 errors.
            if properties.contains_key(&name) {
                continue;
            }
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
                is_abstract: false,
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
                    is_class_prototype: true,
                    visibility: method.visibility,
                    parent_id: current_sym,
                    declaration_order: 0,
                    is_string_named: false,
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
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: None,
                declaration_order: 0,
                is_string_named: false,
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
                        // Can't resolve symbol (e.g., anonymous class expression like
                        // `class extends class { a = 1 }`), try expression-based resolution
                        if let Some(base_instance_type) =
                            self.base_instance_type_from_expression(expr_idx, type_arguments)
                        {
                            tracing::debug!(
                                ?base_instance_type,
                                "heritage: resolved base instance type from expression"
                            );
                            self.merge_base_instance_properties(
                                base_instance_type,
                                &mut properties,
                                &mut string_index,
                                &mut number_index,
                            );
                        } else {
                            tracing::debug!(
                                ?expr_idx,
                                "heritage: base_instance_type_from_expression returned None"
                            );
                        }
                        break;
                    }
                };
                let base_class_decl = self.get_class_declaration_from_symbol(base_sym_id);

                // Canonicalize class symbol for cycle guards. Some paths can observe
                // alias/default-export symbols while the active resolution set tracks
                // the declaration symbol; check both to avoid recursion leaks.
                let canonical_base_sym =
                    base_class_decl.and_then(|decl_idx| self.ctx.binder.get_node_symbol(decl_idx));
                let base_in_resolution_set = self
                    .ctx
                    .class_instance_resolution_set
                    .contains(&base_sym_id)
                    || canonical_base_sym
                        .is_some_and(|sym| self.ctx.class_instance_resolution_set.contains(&sym));
                let base_visited = visited.contains(&base_sym_id)
                    || canonical_base_sym.is_some_and(|sym| visited.contains(&sym));

                // CRITICAL: Check for self-referential class BEFORE processing
                // This catches class C extends C, class D<T> extends D<T>, etc.
                if let Some(current_sym) = current_sym {
                    if base_sym_id == current_sym || canonical_base_sym == Some(current_sym) {
                        // Self-referential inheritance - stop processing.
                        // TS2506 is emitted by the dedicated cycle detection in
                        // class_inheritance.rs, which anchors at the class name (matching tsc).
                        break;
                    }

                    // CRITICAL: Check global resolution set to prevent infinite recursion.
                    // If the base class is currently being resolved, use its cached
                    // partial type (if available) instead of recursing. This handles
                    // nested class expressions that extend their enclosing class:
                    //   class F { Inner = class extends F { p2 = this.p1 }; p1 = 0 }
                    // F is in the resolution set, but its partial type (from prescan
                    // or phase-2 caching) may be in class_instance_type_cache.
                    // If no partial type is cached, build one from the base class's
                    // declared members (annotated properties and constructor params).
                    if base_in_resolution_set {
                        if let Some(base_class_idx) = base_class_decl {
                            if let Some(&cached_partial) =
                                self.ctx.class_instance_type_cache.get(&base_class_idx)
                            {
                                self.merge_base_instance_properties(
                                    cached_partial,
                                    &mut properties,
                                    &mut string_index,
                                    &mut number_index,
                                );
                            } else if let Some(base_node) = self.ctx.arena.get(base_class_idx)
                                && let Some(base_class) = self.ctx.arena.get_class(base_node)
                            {
                                // No cached partial type yet — build a quick prescan
                                // from the base class's declared property types and
                                // constructor parameter properties.
                                let partial =
                                    self.quick_prescan_class_members(base_class_idx, base_class);
                                if partial != TypeId::ERROR {
                                    self.merge_base_instance_properties(
                                        partial,
                                        &mut properties,
                                        &mut string_index,
                                        &mut number_index,
                                    );
                                }
                            }
                        }
                        break;
                    }
                }

                // Check for circular inheritance using symbol tracking
                if base_visited {
                    break;
                }

                let Some(base_class_idx) = base_class_decl else {
                    // Base class node not found in current arena (cross-file case).
                    // Try to resolve the base class type through the symbol system.
                    // If base class is being resolved, skip to prevent infinite loop
                    if base_in_resolution_set {
                        break;
                    }

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
                // CRITICAL: Return immediately to prevent infinite recursion, not just break
                if visited_nodes.contains(&base_class_idx) {
                    if did_insert_into_global_set && let Some(sym_id) = current_sym {
                        self.ctx.class_instance_resolution_set.remove(&sym_id);
                    }
                    return TypeId::ANY; // Cycle detected - break recursion
                }
                let Some(base_node) = self.ctx.arena.get(base_class_idx) else {
                    break;
                };
                let Some(base_class) = self.ctx.arena.get_class(base_node) else {
                    break;
                };

                // CRITICAL: Check global resolution set BEFORE recursing into base class
                // This prevents infinite recursion when we have forward references in cycles
                if let Some(base_class_sym) = self.ctx.binder.get_node_symbol(base_class_idx) {
                    if self
                        .ctx
                        .class_instance_resolution_set
                        .contains(&base_class_sym)
                    {
                        // Base class is already being resolved up the call stack
                        // Return ANY to break the cycle and stop recursion
                        if did_insert_into_global_set && let Some(sym_id) = current_sym {
                            self.ctx.class_instance_resolution_set.remove(&sym_id);
                        }
                        return TypeId::ANY;
                    }
                } else {
                    // CRITICAL: Forward reference detected (symbol not bound yet)
                    // If we've seen this node before in the current resolution path, it's a cycle
                    // This handles cases like: class C extends E {} where E doesn't exist yet
                    // but will be declared later with extends D, and D extends C
                    if visited_nodes.contains(&base_class_idx) {
                        if did_insert_into_global_set && let Some(sym_id) = current_sym {
                            self.ctx.class_instance_resolution_set.remove(&sym_id);
                        }
                        return TypeId::ANY; // Forward reference cycle - break recursion
                    }
                    // Otherwise, continue - the forward reference might resolve later
                }

                let mut type_args = Vec::with_capacity(type_arguments.map_or(0, |a| a.nodes.len()));
                if let Some(args) = type_arguments {
                    for &arg_idx in &args.nodes {
                        type_args.push(self.get_type_from_type_node(arg_idx));
                    }
                }

                // Get the base class instance type.
                // We already resolved a concrete class declaration (`base_class_idx`) above, so
                // we can read through the declaration cache directly and avoid an extra symbol
                // resolution round trip on this hot inheritance path.
                let base_instance_type = self
                    .ctx
                    .class_instance_type_cache
                    .get(&base_class_idx)
                    .copied()
                    .unwrap_or_else(|| self.get_class_instance_type(base_class_idx, base_class));
                let base_instance_type = self.resolve_lazy_type(base_instance_type);
                let mut base_type_params = Vec::new();
                let base_instance_type = if can_skip_base_instantiation(
                    base_class
                        .type_parameters
                        .as_ref()
                        .map_or(0, |params| params.nodes.len()),
                    type_args.len(),
                ) {
                    base_instance_type
                } else {
                    let (resolved_base_type_params, base_type_param_updates) =
                        self.push_type_parameters(&base_class.type_parameters);
                    base_type_params = resolved_base_type_params;

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

                    let substitution =
                        TypeSubstitution::from_args(self.ctx.types, &base_type_params, &type_args);
                    let instantiated =
                        instantiate_type(self.ctx.types, base_instance_type, &substitution);
                    self.pop_type_parameters(base_type_param_updates);
                    instantiated
                };

                let has_structural_self_arg = current_sym.is_some_and(|current_sym| {
                    type_args.iter().copied().any(|arg| {
                        self.type_requires_structure_of_symbol_for_base_type(arg, current_sym)
                    })
                });

                if let Some(current_sym) = current_sym
                    && (has_structural_self_arg
                        || self.type_requires_structure_of_symbol_for_base_type(
                            base_instance_type,
                            current_sym,
                        ))
                {
                    self.report_recursive_base_type_for_symbol(current_sym);
                    self.report_instantiated_type_alias_mapped_constraint_cycles(
                        base_sym_id,
                        &base_type_params,
                        &type_args,
                        current_sym,
                    );
                    if let Some(base_shape) =
                        object_shape_for_type(self.ctx.types, base_instance_type)
                    {
                        for base_prop in &base_shape.properties {
                            properties
                                .entry(base_prop.name)
                                .or_insert_with(|| base_prop.clone());
                        }
                        if let Some(ref idx) = base_shape.string_index {
                            Self::merge_index_signature(&mut string_index, *idx);
                        }
                        if let Some(ref idx) = base_shape.number_index {
                            Self::merge_index_signature(&mut number_index, *idx);
                        }
                    }
                    break;
                }

                if let Some(base_shape) = object_shape_for_type(self.ctx.types, base_instance_type)
                {
                    for base_prop in &base_shape.properties {
                        properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                    if let Some(ref idx) = base_shape.string_index {
                        Self::merge_index_signature(&mut string_index, *idx);
                    }
                    if let Some(ref idx) = base_shape.number_index {
                        Self::merge_index_signature(&mut number_index, *idx);
                    }
                }

                break;
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
                merged_interface_type_for_class = Some(interface_type);

                if let Some(shape) = object_shape_for_type(self.ctx.types, interface_type) {
                    for prop in &shape.properties {
                        properties.entry(prop.name).or_insert_with(|| prop.clone());
                    }
                    if let Some(ref idx) = shape.string_index {
                        Self::merge_index_signature(&mut string_index, *idx);
                    }
                    if let Some(ref idx) = shape.number_index {
                        Self::merge_index_signature(&mut number_index, *idx);
                    }
                } else if let Some(shape) = callable_shape_for_type(self.ctx.types, interface_type)
                {
                    for prop in &shape.properties {
                        properties.entry(prop.name).or_insert_with(|| prop.clone());
                    }
                }
            }

            // When the symbol has INTERFACE flags (class merged with interface) but no
            // local interface declarations were found, the interface declarations live
            // in a lib arena (e.g., user `class TemplateStringsArray {}` merged with
            // built-in `interface TemplateStringsArray extends ReadonlyArray<string>`).
            // Check cross-arena declarations and resolve the lib interface type.
            if interface_decls.is_empty()
                && (symbol.flags & tsz_binder::symbol_flags::INTERFACE) != 0
                && !self.ctx.lib_contexts.is_empty()
            {
                // Check for cross-arena interface declarations
                let mut cross_arena_interface_type: Option<TypeId> = None;
                for &decl_idx in &symbol.declarations {
                    if let Some(arenas) =
                        self.ctx.binder.declaration_arenas.get(&(sym_id, decl_idx))
                    {
                        for arena in arenas.iter() {
                            if std::ptr::eq(arena.as_ref(), self.ctx.arena) {
                                continue;
                            }
                            if let Some(node) = arena.get(decl_idx)
                                && arena.get_interface(node).is_some()
                            {
                                let cross_type =
                                    self.lower_cross_file_interface_decl(arena, decl_idx, sym_id);
                                if cross_type != TypeId::ERROR {
                                    cross_arena_interface_type =
                                        Some(if let Some(existing) = cross_arena_interface_type {
                                            self.merge_interface_types(existing, cross_type)
                                        } else {
                                            cross_type
                                        });
                                }
                            }
                        }
                    }
                }

                // Fall back to resolve_lib_type_by_name if no cross-arena decls found
                let lib_interface_type = cross_arena_interface_type
                    .or_else(|| self.resolve_lib_type_by_name(&symbol.escaped_name));

                if let Some(interface_type) = lib_interface_type {
                    // Merge heritage types for the lib interface
                    let interface_type = self.merge_cross_file_heritage(
                        &symbol.declarations,
                        sym_id,
                        interface_type,
                    );
                    merged_interface_type_for_class = Some(interface_type);

                    if let Some(shape) = object_shape_for_type(self.ctx.types, interface_type) {
                        for prop in &shape.properties {
                            properties.entry(prop.name).or_insert_with(|| prop.clone());
                        }
                        if let Some(ref idx) = shape.string_index {
                            Self::merge_index_signature(&mut string_index, *idx);
                        }
                        if let Some(ref idx) = shape.number_index {
                            Self::merge_index_signature(&mut number_index, *idx);
                        }
                    } else if let Some(shape) =
                        callable_shape_for_type(self.ctx.types, interface_type)
                    {
                        for prop in &shape.properties {
                            properties.entry(prop.name).or_insert_with(|| prop.clone());
                        }
                    }
                }
            }
        }

        // NOTE: Object prototype members (toString, hasOwnProperty, etc.) are NOT
        // merged into the class instance type. The solver handles these via its own
        // Object prototype fallback (resolve_object_member) during property access.
        // Including them as explicit properties would cause false TS2322 errors when
        // assigning plain objects to class-typed variables, since the plain objects
        // wouldn't have these as own properties.

        // Build the final instance type
        let props: Vec<PropertyInfo> = properties.into_values().collect();
        let mut shape = ObjectShape {
            properties: props,
            string_index,
            number_index,
            symbol: current_sym,
            ..ObjectShape::default()
        };
        if has_late_bound_members {
            shape.mark_has_late_bound_members();
        }
        let mut instance_type = factory.object_with_index(shape);

        // Final interface merging pass
        if let Some(sym_id) = current_sym {
            if let Some(interface_type) = merged_interface_type_for_class {
                instance_type = self.merge_interface_types(instance_type, interface_type);
            }

            // Apply module augmentations targeting this class's interface name.
            // When another file has `declare module './thisFile' { interface ClassName { ... } }`,
            // those augmented members must be merged into the class instance type so that
            // `ClassName.prototype` and value-position usage see the full merged type.
            if let Some(symbol) = self.ctx.binder.get_symbol(sym_id) {
                let class_name = symbol.escaped_name.clone();
                if let Some(sf) = self.ctx.arena.source_files.first() {
                    let file_name = sf.file_name.clone();
                    instance_type =
                        self.apply_module_augmentations(&file_name, &class_name, instance_type);
                }
            }

            visited.remove(&sym_id);
            visited_nodes.remove(&class_idx);
            // Only remove from global set if we inserted it ourselves
            if did_insert_into_global_set {
                self.ctx.class_instance_resolution_set.remove(&sym_id);
            }
        }
        // Register the mapping from instance type to class declaration.
        // This allows get_class_decl_from_type to correctly identify the class
        // for derived classes that have no private/protected members (and thus no brand).
        self.ctx
            .class_decl_miss_cache
            .borrow_mut()
            .remove(&instance_type);
        self.ctx
            .class_instance_type_to_decl
            .insert(instance_type, class_idx);

        // Register instance type → DefId in the definition store so the TypeFormatter
        // can display the class name (e.g., "A") instead of expanding structurally
        // (e.g., "{ a: string }"), even across file boundaries.
        //
        // Guard: Only register when the symbol is actually a CLASS. In cross-arena
        // scenarios, get_node_symbol(class_idx) can return a wrong symbol when a lib
        // arena's class NodeIndex collides with the user file's node-to-symbol mapping
        // (e.g., a TYPE_ALIAS like SequenceFactory at the same NodeIndex). Registering
        // class type params under a non-class symbol's DefId causes false TS2314.
        if let Some(sym_id) = current_sym {
            let is_class_symbol = self
                .get_symbol_globally(sym_id)
                .is_some_and(|s| s.flags & tsz_binder::symbol_flags::CLASS != 0);
            if is_class_symbol {
                let def_id = self.ctx.get_or_create_def_id(sym_id);
                self.ctx
                    .definition_store
                    .register_type_to_def(instance_type, def_id);
                // Use get_type_params_for_symbol to populate the cache with properly
                // merged params. For merged class+interface declarations (e.g.,
                // `declare class C<P, S>` + `interface C<P = {}, S = {}>`), the
                // class AST alone lacks defaults. get_type_params_for_symbol merges
                // defaults from all declarations and caches the result, preventing
                // false TS2314 when the merged type has fewer required args.
                if !class_type_params.is_empty() {
                    self.get_type_params_for_symbol(sym_id);
                }
            }
        }

        self.pop_type_parameters(class_type_param_updates);

        instance_type
    }

    fn merge_union_index_signature(
        &self,
        target: &mut Option<IndexSignature>,
        incoming: IndexSignature,
    ) {
        if let Some(existing) = target.as_mut() {
            if existing.value_type != incoming.value_type {
                existing.value_type = self
                    .ctx
                    .types
                    .factory()
                    .union2(existing.value_type, incoming.value_type);
            }
            existing.readonly &= incoming.readonly;
        } else {
            *target = Some(incoming);
        }
    }

    fn merge_index_signature_from_unresolved_computed_name(
        &mut self,
        name_idx: NodeIndex,
        value_type: TypeId,
        string_index: &mut Option<IndexSignature>,
        number_index: &mut Option<IndexSignature>,
    ) {
        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return;
        }
        let Some(computed) = self.ctx.arena.get_computed_property(name_node) else {
            return;
        };

        let prev = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = true;
        let key_type = self.get_type_of_node(computed.expression);
        self.ctx.preserve_literal_types = prev;

        if let Some((wants_string, wants_number)) = self.get_index_key_kind(key_type) {
            if wants_string {
                self.merge_union_index_signature(
                    string_index,
                    IndexSignature {
                        key_type: TypeId::STRING,
                        value_type,
                        readonly: false,
                        param_name: None,
                    },
                );
            }
            if wants_number {
                self.merge_union_index_signature(
                    number_index,
                    IndexSignature {
                        key_type: TypeId::NUMBER,
                        value_type,
                        readonly: false,
                        param_name: None,
                    },
                );
            }
        }
    }

    /// For JS classes without syntax-level type parameters, check the leading
    /// JSDoc for `@template` tags and create type parameters from them.
    ///
    /// Returns `(type_params, scope_updates)` — identical shape to `push_type_parameters`.
    /// The caller must pass `scope_updates` to `pop_type_parameters` when done.
    #[allow(clippy::type_complexity)]
    pub(in crate::types_domain) fn push_jsdoc_class_template_type_params(
        &mut self,
        class_idx: NodeIndex,
    ) -> (Vec<TypeParamInfo>, Vec<(String, Option<TypeId>, bool)>) {
        if !self.is_js_file() {
            return (Vec::new(), Vec::new());
        }

        let jsdoc = {
            let sf = match self.ctx.arena.source_files.first() {
                Some(sf) => sf,
                None => return (Vec::new(), Vec::new()),
            };
            let source_text: &str = &sf.text;
            let comments = &sf.comments;
            match self.try_leading_jsdoc(
                comments,
                self.ctx.arena.get(class_idx).map_or(0, |n| n.pos),
                source_text,
            ) {
                Some(j) => j,
                None => return (Vec::new(), Vec::new()),
            }
        };

        self.validate_jsdoc_template_tag_syntax_at_decl(class_idx);

        let template_names = Self::jsdoc_template_type_params(&jsdoc);
        if template_names.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut type_params = Vec::with_capacity(template_names.len());
        let mut scope_updates = Vec::with_capacity(template_names.len());
        let factory = self.ctx.types.factory();
        for name in template_names {
            let atom = self.ctx.types.intern_string(&name);
            let info = TypeParamInfo {
                name: atom,
                constraint: None,
                default: None,
                is_const: false,
            };
            let ty = factory.type_param(info);
            type_params.push(info);
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), ty);
            scope_updates.push((name, previous, false));
        }
        (type_params, scope_updates)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        can_skip_base_instantiation, exceeds_class_inheritance_depth_limit,
        in_progress_class_instance_result,
    };
    use tsz_solver::TypeId;

    #[test]
    fn skip_base_instantiation_only_without_generics() {
        assert!(can_skip_base_instantiation(0, 0));
        assert!(!can_skip_base_instantiation(1, 0));
        assert!(!can_skip_base_instantiation(0, 1));
        assert!(!can_skip_base_instantiation(2, 3));
    }

    #[test]
    fn class_inheritance_depth_guard_is_conservative() {
        assert!(!exceeds_class_inheritance_depth_limit(1));
        assert!(!exceeds_class_inheritance_depth_limit(100));
        assert!(!exceeds_class_inheritance_depth_limit(256));
        assert!(exceeds_class_inheritance_depth_limit(257));
    }

    #[test]
    fn in_progress_class_instance_uses_cached_or_error() {
        assert_eq!(
            in_progress_class_instance_result(true, Some(TypeId(42))),
            Some(TypeId(42))
        );
        assert_eq!(
            in_progress_class_instance_result(true, None),
            Some(TypeId::ERROR)
        );
        assert_eq!(in_progress_class_instance_result(false, None), None);
    }
}
