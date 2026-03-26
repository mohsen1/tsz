//! Class constructor type resolution (static members, construct signatures, inheritance).

use crate::context::TypingRequest;
use crate::query_boundaries::class_type::{callable_shape_for_type, construct_signatures_for_type};
use crate::query_boundaries::common::instantiate_type;
use crate::state::{CheckerState, MemberAccessLevel};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::visitor::is_template_literal_type;
use tsz_solver::{
    CallSignature, CallableShape, IndexSignature, PropertyInfo, TypeId, TypeParamInfo,
    TypePredicate, TypeSubstitution, Visibility, types::ParamInfo,
};

use super::can_skip_base_instantiation;

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

impl<'a> CheckerState<'a> {
    fn merge_static_late_bound_index_value(
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
                    .union(vec![existing.value_type, incoming.value_type]);
            }
            existing.readonly &= incoming.readonly;
        } else {
            *target = Some(incoming);
        }
    }

    fn merge_static_late_bound_member_from_computed_name(
        &mut self,
        name_idx: NodeIndex,
        value_type: TypeId,
        request: &TypingRequest,
        static_string_index: &mut Option<IndexSignature>,
        static_number_index: &mut Option<IndexSignature>,
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
        let key_request = request.read().contextual_opt(None);
        let key_type = self.get_type_of_node_with_request(computed.expression, &key_request);
        self.ctx.preserve_literal_types = prev;

        let Some((wants_string, wants_number)) = self.get_index_key_kind(key_type) else {
            return;
        };

        if wants_string {
            self.merge_static_late_bound_index_value(
                static_string_index,
                IndexSignature {
                    key_type: TypeId::STRING,
                    value_type,
                    readonly: false,
                    param_name: None,
                },
            );
        }
        if wants_number {
            self.merge_static_late_bound_index_value(
                static_number_index,
                IndexSignature {
                    key_type: TypeId::NUMBER,
                    value_type,
                    readonly: false,
                    param_name: None,
                },
            );
        }
    }

    fn class_constructor_display_name(
        &self,
        class_idx: NodeIndex,
        _class: &tsz_parser::parser::node::ClassData,
    ) -> String {
        self.get_bound_class_name_from_decl(class_idx)
            .unwrap_or_else(|| "(Anonymous class)".to_string())
    }

    /// Get the constructor type of a class declaration (static members,
    /// construct signatures, inherited statics, accessibility, abstractness).
    pub(crate) fn get_class_constructor_type(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        self.get_class_constructor_type_with_request(class_idx, class, &TypingRequest::NONE)
    }

    pub(crate) fn get_class_constructor_type_with_request(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        request: &TypingRequest,
    ) -> TypeId {
        let current_sym = self.ctx.binder.get_node_symbol(class_idx);
        if request.is_empty()
            && let Some(&cached) = self.ctx.class_constructor_type_cache.get(&class_idx)
        {
            return cached;
        }
        let can_use_cache = request.is_empty()
            && current_sym
                .map(|sym_id| !self.ctx.class_constructor_resolution_set.contains(&sym_id))
                .unwrap_or(true);

        // Cycle detection: prevent infinite recursion on circular class hierarchies
        // (e.g. class C extends C {}, or A extends B extends A)
        let did_insert = if let Some(sym_id) = current_sym {
            if self.ctx.class_constructor_resolution_set.insert(sym_id) {
                true
            } else {
                // Already resolving this class's constructor type. If a partial
                // constructor type is cached in symbol_types, prefer that over
                // collapsing the recursive lookup to ERROR.
                return self
                    .ctx
                    .symbol_types
                    .get(&sym_id)
                    .copied()
                    .unwrap_or(TypeId::ERROR);
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

        let result = self.get_class_constructor_type_inner(class_idx, class, request);

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

        // Register constructor type -> DefId(ClassConstructor) so the formatter
        // displays it as "typeof ClassName" instead of expanding the object shape.
        //
        // Prefer pre-populated ClassConstructor companion from binder-owned
        // identity (created during pre-population). If a companion exists,
        // set its body to the computed type rather than creating a new DefId.
        // This moves constructor identity from checker on-demand creation to
        // binder-owned stable identity.
        if result != TypeId::ERROR {
            let class_def_id = current_sym
                .and_then(|sym_id| self.ctx.symbol_to_def.borrow().get(&sym_id).copied());

            let ctor_def_id = if let Some(class_def) = class_def_id
                && let Some(pre_populated_ctor) =
                    self.ctx.definition_store.get_constructor_def(class_def)
            {
                // Reuse the pre-populated companion identity, just set its body.
                self.ctx
                    .definition_store
                    .set_body(pre_populated_ctor, result);
                pre_populated_ctor
            } else {
                // Fallback: create a new DefId (anonymous classes, or classes
                // not covered by pre-population).
                let display_name = self.class_constructor_display_name(class_idx, class);
                let symbol_id = current_sym.map(|sym_id| sym_id.0);
                let name = self.ctx.types.intern_string(&display_name);
                self.ctx
                    .definition_store
                    .register(tsz_solver::def::DefinitionInfo {
                        kind: tsz_solver::def::DefKind::ClassConstructor,
                        name,
                        type_params: Vec::new(),
                        body: Some(result),
                        instance_shape: None,
                        static_shape: None,
                        extends: None,
                        implements: Vec::new(),
                        enum_members: Vec::new(),
                        exports: Vec::new(),
                        file_id: None,
                        span: None,
                        symbol_id,
                        heritage_names: Vec::new(),
                        is_abstract: false,
                        is_const: false,
                        is_exported: false,
                        is_global_augmentation: false,
                        is_declare: false,
                    })
            };
            self.ctx
                .definition_store
                .register_type_to_def(result, ctor_def_id);
        }

        result
    }

    fn get_class_constructor_type_inner(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        request: &TypingRequest,
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        let is_abstract_class = self.has_abstract_modifier(&class.modifiers);
        let (mut class_type_params, mut type_param_updates) =
            self.push_type_parameters(&class.type_parameters);

        // In JS files, classes don't have syntax-level type parameters.
        // JSDoc `@template T` tags serve the same purpose.
        if class_type_params.is_empty() {
            let (jsdoc_params, jsdoc_updates) =
                self.push_jsdoc_class_template_type_params(class_idx);
            if !jsdoc_params.is_empty() {
                class_type_params = jsdoc_params;
                type_param_updates.extend(jsdoc_updates);
            }
        }

        // NOTE: instance type is computed AFTER static member processing (see below).
        // This allows us to temporarily cache a partial constructor type with all static
        // members, so that self-referencing property initializers (e.g., `p = doThing(A)`)
        // can resolve the class type during instance type computation.

        // Get the class symbol for nominal identity.
        // For `export default class Foo`, the class node's symbol is the "default" export
        // symbol. The class NAME symbol (`Foo`) is a separate symbol that references
        // inside the class body resolve to when they write `Foo`. We need to cache the
        // partial constructor under BOTH symbols so that self-referential static
        // initializers like `static x = make(Foo)` resolve correctly.
        let current_sym = self.ctx.binder.get_node_symbol(class_idx);
        let class_name_sym = if class.name.is_some() {
            self.ctx
                .arena
                .get_identifier_at(class.name)
                .and_then(|ident| self.ctx.binder.file_locals.get(&ident.escaped_text))
                .filter(|&name_sym| Some(name_sym) != current_sym)
        } else {
            None
        };

        // Pre-compute inherited static properties from base class so they are available
        // for partial constructor types built during static initializer evaluation.
        // Without this, self-referencing initializers like `static x = P.BaseMethod()`
        // would not see inherited statics in the partial constructor type.
        let inherited_static_props: Vec<PropertyInfo> = self
            .collect_inherited_static_properties(class)
            .into_values()
            .collect();

        let mut properties: FxHashMap<Atom, PropertyInfo> = FxHashMap::default();
        let mut methods: FxHashMap<Atom, MethodAggregate> = FxHashMap::default();
        let mut accessors: FxHashMap<Atom, AccessorAggregate> = FxHashMap::default();
        let mut static_string_index: Option<IndexSignature> = None;
        let mut static_number_index: Option<IndexSignature> = None;
        let mut has_static_late_bound_members = false;

        // Pre-scan all static member names so that partial constructor types
        // built during static initializer evaluation include not-yet-processed
        // members as `any`-typed placeholders. Without this, references like
        // `Class.laterMember` inside an earlier static initializer would get a
        // false TS2339 instead of resolving to `any`.
        let mut all_static_member_names: Vec<Atom> = Vec::new();
        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };
            let name_opt = match member_node.kind {
                k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                    .ctx
                    .arena
                    .get_property_decl(member_node)
                    .filter(|p| self.has_static_modifier(&p.modifiers))
                    .and_then(|p| self.get_property_name_resolved(p.name)),
                k if k == syntax_kind_ext::METHOD_DECLARATION => self
                    .ctx
                    .arena
                    .get_method_decl(member_node)
                    .filter(|m| self.has_static_modifier(&m.modifiers))
                    .and_then(|m| self.get_property_name_resolved(m.name)),
                k if k == syntax_kind_ext::GET_ACCESSOR || k == syntax_kind_ext::SET_ACCESSOR => {
                    self.ctx
                        .arena
                        .get_accessor(member_node)
                        .filter(|a| self.has_static_modifier(&a.modifiers))
                        .and_then(|a| self.get_property_name_resolved(a.name))
                }
                _ => None,
            };
            if let Some(name) = name_opt {
                let atom = self.ctx.types.intern_string(&name);
                all_static_member_names.push(atom);
            }
        }

        // Pre-compute a rough partial instance type from declared (annotated) non-static
        // instance properties. Used as the return type of rough construct signatures so
        // that type inference for generic functions can extract type arguments from
        // construct-signature constraints (e.g., `make<P>(x: { new(): { props: P } })`).
        let rough_instance_return_type = {
            let mut inst_props: Vec<PropertyInfo> = Vec::new();
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    continue;
                }
                let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                    continue;
                };
                if self.has_static_modifier(&prop.modifiers) {
                    continue;
                }
                let Some(type_id) = self.effective_class_property_declared_type(member_idx, prop)
                else {
                    continue;
                };
                let Some(name) = self.get_property_name_resolved(prop.name) else {
                    continue;
                };
                let name_atom = self.ctx.types.intern_string(&name);
                inst_props.push(PropertyInfo {
                    name: name_atom,
                    type_id,
                    write_type: type_id,
                    optional: prop.question_token,
                    readonly: self.has_readonly_modifier(&prop.modifiers),
                    is_method: false,
                    is_class_prototype: false,
                    visibility: self.get_member_visibility(&prop.modifiers, prop.name),
                    parent_id: current_sym,
                    declaration_order: 0,
                });
            }
            if inst_props.is_empty() {
                TypeId::ANY
            } else {
                let factory = self.ctx.types.factory();
                factory.object(inst_props)
            }
        };

        // Pre-compute rough construct signatures for the partial static constructor type.
        // Static methods need `this` to be constructable so that `return this` from a
        // static method makes the return type constructable (prevents false TS2351).
        // The return type uses a rough partial instance type (from declared instance
        // properties) so that type inference can match construct-signature constraints.
        let rough_construct_signatures = {
            let mut has_ctor_overloads = false;
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.ctx.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.ctx.arena.get_constructor(member_node)
                    && ctor.body.is_none()
                {
                    has_ctor_overloads = true;
                    break;
                }
            }
            let mut sigs = Vec::new();
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
                if has_ctor_overloads {
                    if ctor.body.is_none() {
                        sigs.push(self.call_signature_from_constructor(
                            ctor,
                            member_idx,
                            rough_instance_return_type,
                            &class_type_params,
                        ));
                    }
                } else {
                    sigs.push(self.call_signature_from_constructor(
                        ctor,
                        member_idx,
                        rough_instance_return_type,
                        &class_type_params,
                    ));
                    break;
                }
            }
            if sigs.is_empty() {
                // Default construct signature (like the default constructor)
                sigs.push(CallSignature {
                    type_params: class_type_params.clone(),
                    params: Vec::new(),
                    this_type: None,
                    return_type: rough_instance_return_type,
                    type_predicate: None,
                    is_method: false,
                });
            }
            sigs
        };

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
                    let Some(name) = self.get_property_name_resolved(prop.name) else {
                        if self
                            .ctx
                            .arena
                            .get(prop.name)
                            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        {
                            has_static_late_bound_members = true;
                        }
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let visibility = self.get_member_visibility(&prop.modifiers, prop.name);
                    let readonly = self.has_readonly_modifier(&prop.modifiers);
                    let type_id = if let Some(declared_type) =
                        self.effective_class_property_declared_type(member_idx, prop)
                    {
                        declared_type
                    } else if prop.initializer.is_some() {
                        // Set in_static_property_initializer for proper super checking
                        if let Some(ref mut class_info) = self.ctx.enclosing_class {
                            class_info.in_static_property_initializer = true;
                        }
                        // When the class expression has a contextual type (e.g., from a
                        // function return type), set the per-property contextual type so
                        // arrow/function expression initializers get parameter inference.
                        // Without this, `(arg) => {}` initializers would see the whole
                        // interface as contextual type instead of the specific member type.
                        let mut has_contextual_member = false;
                        let member_ctx_type = request.contextual_type.and_then(|ctx_type| {
                            let resolved = self.evaluate_type_for_assignability(ctx_type);
                            let ctx_helper = tsz_solver::ContextualTypeContext::with_expected(
                                self.ctx.types,
                                resolved,
                            );
                            ctx_helper
                                .get_property_type(&name)
                                .filter(|&mt| mt != TypeId::ANY && !self.type_contains_error(mt))
                        });
                        if member_ctx_type.is_some() {
                            has_contextual_member = true;
                            self.invalidate_initializer_for_context_change(prop.initializer);
                        }
                        let prev_sym_cached = current_sym
                            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied());
                        let prev_name_sym_cached = class_name_sym
                            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied());
                        let partial_ctor = self.build_partial_static_constructor_type(
                            current_sym,
                            &properties,
                            &methods,
                            &accessors,
                            &static_string_index,
                            &static_number_index,
                            Some(PropertyInfo {
                                name: name_atom,
                                type_id: TypeId::ANY,
                                write_type: TypeId::ANY,
                                optional: prop.question_token,
                                readonly,
                                is_method: false,
                                is_class_prototype: false,
                                visibility,
                                parent_id: current_sym,
                                declaration_order: 0,
                            }),
                            &inherited_static_props,
                            &all_static_member_names,
                            &rough_construct_signatures,
                        );
                        if let Some(sym_id) = current_sym {
                            self.ctx.symbol_types.insert(sym_id, partial_ctor);
                            // For `export default class Foo`, the class node symbol is the
                            // "default" export symbol. Also cache under the class name symbol
                            // so that self-referential static initializers using `Foo` resolve
                            // to the partial constructor type.
                            if let Some(name_sym) = class_name_sym {
                                self.ctx.symbol_types.insert(name_sym, partial_ctor);
                            }
                        }
                        // Push partial constructor type onto this_type_stack so that
                        // `this` in static property initializers resolves to the
                        // constructor type (typeof ClassName) rather than `any` or
                        // `object`. This is needed when enclosing_class is not yet
                        // set (e.g., during symbol type resolution via
                        // compute_class_symbol_type).
                        self.ctx.this_type_stack.push(partial_ctor);
                        let prev = self.ctx.preserve_literal_types;
                        self.ctx.preserve_literal_types = true;
                        // Clear cached type: check_property_declaration may have
                        // already typed this initializer without preserve_literal_types,
                        // caching a widened type (e.g., "a" → string). We need the
                        // literal type for the constructor type's static properties.
                        self.clear_type_cache_recursive(prop.initializer);
                        let member_request = member_ctx_type
                            .map(|ty| request.read().contextual(ty))
                            .unwrap_or_else(|| request.read().contextual_opt(None));
                        let init_type =
                            self.get_type_of_node_with_request(prop.initializer, &member_request);
                        self.ctx.this_type_stack.pop();
                        self.ctx.preserve_literal_types = prev;
                        let init_type = if init_type == TypeId::ANY
                            && self.has_accessor_modifier(&prop.modifiers)
                        {
                            self.this_access_name_node(prop.initializer)
                                .and_then(|name_idx| {
                                    self.infer_property_type_from_class_member_assignments(
                                        &class.members.nodes,
                                        name_idx,
                                        true,
                                    )
                                })
                                .unwrap_or(init_type)
                        } else {
                            init_type
                        };
                        if let Some(sym_id) = current_sym {
                            if let Some(prev_type) = prev_sym_cached {
                                self.ctx.symbol_types.insert(sym_id, prev_type);
                            } else {
                                self.ctx.symbol_types.remove(&sym_id);
                            }
                        }
                        // Also restore the class name symbol (for `export default class Foo`)
                        if let Some(name_sym) = class_name_sym {
                            if let Some(prev_type) = prev_name_sym_cached {
                                self.ctx.symbol_types.insert(name_sym, prev_type);
                            } else {
                                self.ctx.symbol_types.remove(&name_sym);
                            }
                        }
                        if let Some(ref mut class_info) = self.ctx.enclosing_class {
                            class_info.in_static_property_initializer = false;
                        }

                        // Only widen literal types for mutable properties when
                        // there is no contextual type constraining the property.
                        // When the class expression is contextually typed by an
                        // interface with a literal property type (e.g., `x: "a"`),
                        // tsc preserves the literal type rather than widening.
                        if readonly || has_contextual_member {
                            init_type
                        } else {
                            self.widen_literal_type(init_type)
                        }
                    } else if self.has_accessor_modifier(&prop.modifiers) {
                        // Build and cache a partial constructor type before inferring
                        // the accessor's type from assignments in static blocks.
                        // Without this, evaluating `this.z = this.y` inside a static
                        // block would trigger a cycle: inferring z's type →
                        // get_type_of_node(this.y) → get_class_constructor_type → cycle.
                        // The partial constructor type lets the cycle detection return
                        // a usable type with previously-processed members.
                        // Suppress diagnostics during inference: type resolution for
                        // `this.prop` in static blocks can emit false TS2339 when the
                        // partial constructor type doesn't yet contain all members.
                        // These diagnostics will be re-emitted correctly during the
                        // proper checking phase.
                        let diag_count_before = self.ctx.diagnostics.len();
                        let prev_sym_cached_acc = current_sym
                            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied());
                        let prev_name_sym_cached_acc = class_name_sym
                            .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied());
                        let partial_ctor_acc = self.build_partial_static_constructor_type(
                            current_sym,
                            &properties,
                            &methods,
                            &accessors,
                            &static_string_index,
                            &static_number_index,
                            Some(PropertyInfo {
                                name: name_atom,
                                type_id: TypeId::ANY,
                                write_type: TypeId::ANY,
                                optional: prop.question_token,
                                readonly,
                                is_method: false,
                                is_class_prototype: false,
                                visibility,
                                parent_id: current_sym,
                                declaration_order: 0,
                            }),
                            &inherited_static_props,
                            &all_static_member_names,
                            &rough_construct_signatures,
                        );
                        if let Some(sym_id) = current_sym {
                            self.ctx.symbol_types.insert(sym_id, partial_ctor_acc);
                            if let Some(name_sym) = class_name_sym {
                                self.ctx.symbol_types.insert(name_sym, partial_ctor_acc);
                            }
                        }
                        // Push partial constructor onto this_type_stack so that
                        // `this` in static blocks resolves correctly during inference.
                        // This is needed because build_type_environment triggers
                        // constructor type computation BEFORE enclosing_class is set,
                        // so the `this` dispatch cannot use the enclosing_class path.
                        self.ctx.this_type_stack.push(partial_ctor_acc);
                        let inferred = self
                            .infer_property_type_from_class_member_assignments(
                                &class.members.nodes,
                                prop.name,
                                true,
                            )
                            .unwrap_or(TypeId::ANY);
                        self.ctx.this_type_stack.pop();
                        // Restore symbol_types to previous state
                        if let Some(sym_id) = current_sym {
                            if let Some(prev_type) = prev_sym_cached_acc {
                                self.ctx.symbol_types.insert(sym_id, prev_type);
                            } else {
                                self.ctx.symbol_types.remove(&sym_id);
                            }
                        }
                        if let Some(name_sym) = class_name_sym {
                            if let Some(prev_type) = prev_name_sym_cached_acc {
                                self.ctx.symbol_types.insert(name_sym, prev_type);
                            } else {
                                self.ctx.symbol_types.remove(&name_sym);
                            }
                        }
                        // Roll back diagnostics emitted during inference.
                        // These would be false positives from resolving `this.prop`
                        // against an incomplete partial constructor type.
                        self.ctx.diagnostics.truncate(diag_count_before);
                        // Clear node type cache for nodes inside static blocks so that
                        // the checking phase re-evaluates them with the final constructor type.
                        for &sb_member_idx in &class.members.nodes {
                            if let Some(sb_node) = self.ctx.arena.get(sb_member_idx)
                                && sb_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
                            {
                                self.clear_type_cache_recursive(sb_member_idx);
                            }
                        }
                        inferred
                    } else {
                        // Static properties without type annotation or initializer
                        // get implicit 'any' type (same as instance properties).
                        // TS7008 is emitted separately when noImplicitAny is on.
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
                            readonly,
                            is_method: false,
                            is_class_prototype: false,
                            visibility,
                            parent_id: current_sym,
                            declaration_order: 0,
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
                    let visibility = self.get_member_visibility(&method.modifiers, method.name);
                    // For static methods, `this` refers to the constructor type
                    // Get it from the symbol if available
                    let prev_sym_cached =
                        current_sym.and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied());
                    if let Some(sym_id) = current_sym {
                        let partial_ctor = self.build_partial_static_constructor_type(
                            current_sym,
                            &properties,
                            &methods,
                            &accessors,
                            &static_string_index,
                            &static_number_index,
                            None,
                            &inherited_static_props,
                            &all_static_member_names,
                            &rough_construct_signatures,
                        );
                        self.ctx.symbol_types.insert(sym_id, partial_ctor);
                    }
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
                    if let Some(sym_id) = current_sym {
                        if let Some(prev) = prev_sym_cached {
                            self.ctx.symbol_types.insert(sym_id, prev);
                        } else {
                            self.ctx.symbol_types.remove(&sym_id);
                        }
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
                        factory.union(vec![callable_type, TypeId::UNDEFINED])
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
                            has_static_late_bound_members = true;
                            self.merge_static_late_bound_member_from_computed_name(
                                method.name,
                                callable_or_undefined,
                                request,
                                &mut static_string_index,
                                &mut static_number_index,
                            );
                        }
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
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
                    let Some(name) = self.get_property_name_resolved(accessor.name) else {
                        if self
                            .ctx
                            .arena
                            .get(accessor.name)
                            .is_some_and(|n| n.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        {
                            has_static_late_bound_members = true;
                        }
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    let visibility = self.get_member_visibility(&accessor.modifiers, accessor.name);

                    if k == syntax_kind_ext::GET_ACCESSOR {
                        let getter_type = if accessor.type_annotation.is_some() {
                            self.get_type_from_type_node(accessor.type_annotation)
                        } else {
                            let prev_sym_cached = current_sym
                                .and_then(|sym_id| self.ctx.symbol_types.get(&sym_id).copied());
                            if let Some(sym_id) = current_sym {
                                let partial_ctor = self.build_partial_static_constructor_type(
                                    current_sym,
                                    &properties,
                                    &methods,
                                    &accessors,
                                    &static_string_index,
                                    &static_number_index,
                                    None,
                                    &inherited_static_props,
                                    &all_static_member_names,
                                    &rough_construct_signatures,
                                );
                                self.ctx.symbol_types.insert(sym_id, partial_ctor);
                            }
                            let t = self.infer_getter_return_type(accessor.body);
                            if let Some(sym_id) = current_sym {
                                if let Some(prev) = prev_sym_cached {
                                    self.ctx.symbol_types.insert(sym_id, prev);
                                } else {
                                    self.ctx.symbol_types.remove(&sym_id);
                                }
                            }
                            // Cache so the declaration emitter can look it up
                            self.ctx.node_types.insert(member_idx.0, t);
                            t
                        };
                        let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                            getter: None,
                            setter: None,
                            visibility,
                        });
                        entry.getter = Some(getter_type);
                    } else {
                        let setter_type = accessor
                            .parameters
                            .nodes
                            .first()
                            .and_then(|&param_idx| self.ctx.arena.get(param_idx))
                            .and_then(|param_node| self.ctx.arena.get_parameter(param_node))
                            .and_then(|param| {
                                (!self.ctx.is_js_file() && param.type_annotation.is_some())
                                    .then(|| self.get_type_from_type_node(param.type_annotation))
                            })
                            .unwrap_or(TypeId::UNKNOWN);
                        let entry = accessors.entry(name_atom).or_insert(AccessorAggregate {
                            getter: None,
                            setter: None,
                            visibility,
                        });
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
                    let param_name = param_data
                        .and_then(|p| self.ctx.arena.get(p.name))
                        .and_then(|name_node| self.ctx.arena.get_identifier(name_node))
                        .map(|name_ident| self.ctx.types.intern_string(&name_ident.escaped_text));

                    let idx_sig = IndexSignature {
                        key_type,
                        value_type,
                        readonly,
                        param_name,
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
            let read_type = accessor.getter.unwrap_or_else(|| {
                if accessor.setter.is_some() {
                    TypeId::UNDEFINED
                } else {
                    TypeId::UNKNOWN
                }
            });
            // When a setter parameter has no type annotation, its type is UNKNOWN
            // (sentinel). Filter out so we fall back to getter type, matching tsc.
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
                    is_class_prototype: false,
                    visibility: accessor.visibility,
                    parent_id: current_sym,
                    declaration_order: 0,
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
                    is_class_prototype: false,
                    visibility: method.visibility,
                    parent_id: current_sym,
                    declaration_order: 0,
                },
            );
        }

        // Compute instance type NOW, after all static members are processed.
        //
        // WHY DEFERRED: Instance type construction evaluates property initializers via
        // `get_type_of_node`. When an initializer references the enclosing class
        // (e.g., `p = doThing(A)` inside class A), `get_type_of_symbol(A)` hits the
        // cache. Without the temporary partial constructor type below, it would find
        // only the `Lazy(DefId)` placeholder — an opaque type that fails structural
        // assignability checks, causing false TS2345/TS2322.
        //
        // By building a partial constructor type from the already-processed static
        // members and temporarily caching it for the class symbol, the recursive
        // `get_type_of_symbol(A)` returns a type with static members visible (e.g.,
        // `{ n: string }`). The cache is restored afterward so other code paths
        // (like `resolve_lazy_class_to_constructor` for method return types) continue
        // to see the original `Lazy(DefId)` placeholder.
        let instance_type = {
            let prev_sym_cached = current_sym.and_then(|s| self.ctx.symbol_types.get(&s).copied());
            let prev_inst_cached =
                current_sym.and_then(|s| self.ctx.symbol_instance_types.get(&s).copied());
            if let Some(sym_id) = current_sym {
                // ── Partial CONSTRUCTOR type (for VALUE references) ──
                // Build from already-processed static members + inherited base statics.
                let mut partial_ctor_props: Vec<PropertyInfo> =
                    properties.values().cloned().collect();

                // Include inherited static properties from base class if available
                if let Some(ref heritage_clauses) = class.heritage_clauses {
                    'inherit: for &clause_idx in &heritage_clauses.nodes {
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
                        let expr_idx = if let Some(expr_type_args) =
                            self.ctx.arena.get_expr_type_args(type_node)
                        {
                            expr_type_args.expression
                        } else {
                            type_idx
                        };
                        if let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx)
                            && let Some(&base_type) = self.ctx.symbol_types.get(&base_sym_id)
                        {
                            let base_props = self.static_properties_from_type(base_type);
                            let own_names: std::collections::HashSet<_> =
                                partial_ctor_props.iter().map(|p| p.name).collect();
                            for (name, prop) in base_props {
                                if !own_names.contains(&name) {
                                    partial_ctor_props.push(prop);
                                }
                            }
                        }
                        break 'inherit;
                    }
                }

                let partial_ctor = factory.callable(CallableShape {
                    call_signatures: Vec::new(),
                    construct_signatures: Vec::new(),
                    properties: partial_ctor_props,
                    string_index: static_string_index.clone(),
                    number_index: static_number_index.clone(),
                    symbol: current_sym,
                    is_abstract: false,
                });
                self.ctx.symbol_types.insert(sym_id, partial_ctor);

                // ── Partial INSTANCE type (for TYPE references like `Bar<any>`) ──
                // Build from declared instance properties (those with type annotations).
                // This allows type references to the class being constructed to resolve
                // correctly, preventing false TS2339 on property access (e.g.,
                // `(this as Bar<any>).num` where `num!: number` is declared).
                let mut inst_props: Vec<PropertyInfo> = Vec::new();
                for &member_idx in &class.members.nodes {
                    let Some(member_node) = self.ctx.arena.get(member_idx) else {
                        continue;
                    };
                    if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                        continue;
                    }
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    // Only NON-static properties with semantic declared types.
                    if self.has_static_modifier(&prop.modifiers) {
                        continue;
                    }
                    let Some(type_id) =
                        self.effective_class_property_declared_type(member_idx, prop)
                    else {
                        continue;
                    };
                    let Some(name) = self.get_property_name_resolved(prop.name) else {
                        continue;
                    };
                    let name_atom = self.ctx.types.intern_string(&name);
                    inst_props.push(PropertyInfo {
                        name: name_atom,
                        type_id,
                        write_type: type_id,
                        optional: prop.question_token,
                        readonly: self.has_readonly_modifier(&prop.modifiers),
                        is_method: false,
                        is_class_prototype: false,
                        visibility: self.get_member_visibility(&prop.modifiers, prop.name),
                        parent_id: current_sym,
                        declaration_order: 0,
                    });
                }
                if !inst_props.is_empty() {
                    let partial_instance = factory.object(inst_props);
                    self.ctx
                        .symbol_instance_types
                        .insert(sym_id, partial_instance);
                }
            }
            let result = self.get_class_instance_type(class_idx, class);
            // Restore the previous cached values (Lazy placeholder / no instance type)
            // so other code paths continue to work correctly.
            if let Some(sym_id) = current_sym {
                if let Some(prev) = prev_sym_cached {
                    self.ctx.symbol_types.insert(sym_id, prev);
                }
                if let Some(prev) = prev_inst_cached {
                    self.ctx.symbol_instance_types.insert(sym_id, prev);
                } else {
                    self.ctx.symbol_instance_types.remove(&sym_id);
                }
            }
            result
        };

        // Class constructor values always expose an implicit `prototype` property
        // whose type is the class instance type.
        // For generic classes like `class C<T>`, the prototype is shared across all
        // instantiations, so `C.prototype` must have type `C<any>` (all type params
        // substituted with `any`), not the raw `C<T>`.
        let prototype_type = if !class_type_params.is_empty() {
            let any_args: Vec<TypeId> = class_type_params.iter().map(|_| TypeId::ANY).collect();
            let substitution =
                TypeSubstitution::from_args(self.ctx.types, &class_type_params, &any_args);
            instantiate_type(self.ctx.types, instance_type, &substitution)
        } else {
            instance_type
        };
        let prototype_name = self.ctx.types.intern_string("prototype");
        properties.insert(
            prototype_name,
            PropertyInfo {
                name: prototype_name,
                type_id: prototype_type,
                write_type: prototype_type,
                optional: false,
                readonly: false,
                is_method: false,
                is_class_prototype: false,
                visibility: Visibility::Public,
                parent_id: current_sym,
                declaration_order: 0,
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

                            // When type arguments are provided (e.g., `extends Base<Prop, {}>`),
                            // build a substitution from the base constructor's type params to
                            // the resolved type arguments. Without this, inherited construct
                            // signature params retain uninstantiated type parameter references
                            // (e.g., `props?: P` instead of `props?: Prop`), causing the JSX
                            // checker's G3 guard to bail out on "generic" signatures.
                            let substitution = type_arguments.and_then(|args| {
                                let base_sigs = construct_signatures_for_type(
                                    self.ctx.types,
                                    base_constructor_type,
                                );
                                let base_type_params = base_sigs
                                    .as_ref()
                                    .and_then(|sigs| sigs.first())
                                    .map(|sig| &sig.type_params)?;
                                if base_type_params.is_empty() {
                                    return None;
                                }

                                let mut type_args = Vec::new();
                                for &arg_idx in &args.nodes {
                                    type_args.push(self.get_type_from_type_node(arg_idx));
                                }
                                // Fill missing args with defaults/constraints
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

                                Some(TypeSubstitution::from_args(
                                    self.ctx.types,
                                    base_type_params,
                                    &type_args,
                                ))
                            });

                            inherited_construct_signatures = if let Some(ref subst) = substitution {
                                self.remap_inherited_construct_signatures_with_substitution(
                                    base_constructor_type,
                                    subst,
                                    &class_type_params,
                                    instance_type,
                                )
                            } else {
                                self.remap_inherited_construct_signatures(
                                    base_constructor_type,
                                    &class_type_params,
                                    instance_type,
                                    None,
                                )
                            };
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
                let Some(base_class_idx) = self.get_class_declaration_from_symbol(base_sym_id)
                else {
                    // Mixin pattern detection: check if the base expression is typed
                    // as a type parameter (e.g., `class extends base` where `base: T`).
                    //
                    // The type_parameter_scope may be empty here because this code
                    // runs during symbol type computation (compute_type_of_symbol),
                    // not during the function body walk. We need to temporarily push
                    // the enclosing function's type parameters into scope so that
                    // type annotations can resolve `T` in `superClass: T`.
                    //
                    // IMPORTANT: We cannot use get_type_of_node(expr_idx) because
                    // node_types may already have cached `any` for the identifier
                    // from an earlier resolution when type params weren't in scope.
                    // Instead, resolve the parameter's type annotation directly via
                    // get_type_from_type_node, which has smart caching that
                    // re-resolves TYPE_REFERENCE nodes when type_parameter_scope
                    // is non-empty.
                    let enclosing_type_param_updates =
                        self.push_enclosing_function_type_params(class_idx);

                    // Resolve the base expression's type annotation directly,
                    // bypassing node_types/symbol_types caches.
                    if let Some(annotation_type_id) =
                        self.resolve_param_type_annotation(base_sym_id)
                        && tsz_solver::visitor::type_param_info(self.ctx.types, annotation_type_id)
                            .is_some()
                    {
                        base_type_param = Some(annotation_type_id);
                    }

                    // Pop the temporary type parameters
                    if !enclosing_type_param_updates.is_empty() {
                        self.pop_type_parameters(enclosing_type_param_updates);
                    }

                    if let Some(base_constructor_type) =
                        self.base_constructor_type_from_expression(expr_idx, type_arguments)
                    {
                        self.merge_constructor_properties_from_type(
                            base_constructor_type,
                            &mut properties,
                        );

                        // Instantiate inherited construct signatures with type arguments
                        // (same logic as the resolve_heritage_symbol → None path above).
                        let substitution = type_arguments.and_then(|args| {
                            let base_sigs = construct_signatures_for_type(
                                self.ctx.types,
                                base_constructor_type,
                            );
                            let base_type_params = base_sigs
                                .as_ref()
                                .and_then(|sigs| sigs.first())
                                .map(|sig| &sig.type_params)?;
                            if base_type_params.is_empty() {
                                return None;
                            }

                            let mut type_args_vec = Vec::new();
                            for &arg_idx in &args.nodes {
                                type_args_vec.push(self.get_type_from_type_node(arg_idx));
                            }
                            if type_args_vec.len() < base_type_params.len() {
                                for param in base_type_params.iter().skip(type_args_vec.len()) {
                                    let fallback = param
                                        .default
                                        .or(param.constraint)
                                        .unwrap_or(TypeId::UNKNOWN);
                                    type_args_vec.push(fallback);
                                }
                            }
                            if type_args_vec.len() > base_type_params.len() {
                                type_args_vec.truncate(base_type_params.len());
                            }

                            Some(TypeSubstitution::from_args(
                                self.ctx.types,
                                base_type_params,
                                &type_args_vec,
                            ))
                        });

                        inherited_construct_signatures = if let Some(ref subst) = substitution {
                            self.remap_inherited_construct_signatures_with_substitution(
                                base_constructor_type,
                                subst,
                                &class_type_params,
                                instance_type,
                            )
                        } else {
                            self.remap_inherited_construct_signatures(
                                base_constructor_type,
                                &class_type_params,
                                instance_type,
                                None,
                            )
                        };
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
                let base_constructor_type = if self
                    .ctx
                    .class_constructor_resolution_set
                    .contains(&base_sym_id)
                {
                    self.ctx
                        .symbol_types
                        .get(&base_sym_id)
                        .copied()
                        .unwrap_or_else(|| {
                            self.get_class_constructor_type(base_class_idx, base_class)
                        })
                } else {
                    self.get_class_constructor_type(base_class_idx, base_class)
                };
                let (instantiated_base_constructor_type, inherited_substitution) =
                    if can_skip_base_instantiation(
                        base_class
                            .type_parameters
                            .as_ref()
                            .map_or(0, |params| params.nodes.len()),
                        type_args.len(),
                    ) {
                        (base_constructor_type, None)
                    } else {
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

                        let substitution = TypeSubstitution::from_args(
                            self.ctx.types,
                            &base_type_params,
                            &type_args,
                        );
                        let instantiated =
                            instantiate_type(self.ctx.types, base_constructor_type, &substitution);
                        self.pop_type_parameters(base_type_param_updates);
                        (instantiated, Some(substitution))
                    };

                if let Some(base_shape) =
                    callable_shape_for_type(self.ctx.types, instantiated_base_constructor_type)
                {
                    for base_prop in &base_shape.properties {
                        properties
                            .entry(base_prop.name)
                            .or_insert_with(|| base_prop.clone());
                    }
                    inherited_construct_signatures =
                        if let Some(ref substitution) = inherited_substitution {
                            self.remap_inherited_construct_signatures_with_substitution(
                                base_constructor_type,
                                substitution,
                                &class_type_params,
                                instance_type,
                            )
                        } else {
                            self.remap_inherited_construct_signatures(
                                base_constructor_type,
                                &class_type_params,
                                instance_type,
                                None,
                            )
                        };
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
                        member_idx,
                        instance_type,
                        &class_type_params,
                    ));
                }
            } else {
                construct_signatures.push(self.call_signature_from_constructor(
                    ctor,
                    member_idx,
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

        // When the class has static members with unresolvable computed property names,
        // tsc treats the constructor type as implicitly string-indexable to suppress TS7053.
        let effective_string_index = if let Some(mut static_index) = static_string_index {
            if has_static_late_bound_members {
                static_index.value_type =
                    factory.union(vec![static_index.value_type, instance_type]);
            }
            Some(static_index)
        } else {
            has_static_late_bound_members.then_some(IndexSignature {
                key_type: TypeId::STRING,
                value_type: TypeId::ANY,
                readonly: false,
                param_name: None,
            })
        };

        let constructor_type = factory.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures,
            properties,
            string_index: effective_string_index,
            number_index: static_number_index,
            symbol: class_symbol,
            is_abstract: is_abstract_class,
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
            return factory.intersection2(base_tp, constructor_type);
        }

        constructor_type
    }

    /// Collect inherited static properties from the base class (extends clause).
    /// Returns a map of property name → `PropertyInfo` for each inherited static.
    fn collect_inherited_static_properties(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
    ) -> rustc_hash::FxHashMap<Atom, PropertyInfo> {
        let mut base_props = rustc_hash::FxHashMap::default();
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
                let expr_idx =
                    if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                        expr_type_args.expression
                    } else {
                        type_idx
                    };
                if let Some(base_sym_id) = self.resolve_heritage_symbol(expr_idx)
                    && let Some(&base_type) = self.ctx.symbol_types.get(&base_sym_id)
                {
                    base_props = self.static_properties_from_type(base_type);
                }
                break;
            }
        }
        base_props
    }

    #[allow(clippy::too_many_arguments)]
    fn build_partial_static_constructor_type(
        &self,
        current_sym: Option<tsz_binder::SymbolId>,
        properties: &FxHashMap<Atom, PropertyInfo>,
        methods: &FxHashMap<Atom, MethodAggregate>,
        accessors: &FxHashMap<Atom, AccessorAggregate>,
        static_string_index: &Option<IndexSignature>,
        static_number_index: &Option<IndexSignature>,
        extra_property: Option<PropertyInfo>,
        inherited_static_props: &[PropertyInfo],
        all_static_member_names: &[Atom],
        construct_signatures: &[CallSignature],
    ) -> TypeId {
        let factory = self.ctx.types.factory();
        let mut partial_ctor_props: Vec<PropertyInfo> = properties.values().cloned().collect();

        for (&name, method) in methods {
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
            partial_ctor_props.push(PropertyInfo {
                name,
                type_id,
                write_type: type_id,
                optional,
                readonly: false,
                is_method: true,
                is_class_prototype: false,
                visibility: method.visibility,
                parent_id: current_sym,
                declaration_order: 0,
            });
        }

        for (&name, accessor) in accessors {
            let read_type = accessor.getter.unwrap_or_else(|| {
                if accessor.setter.is_some() {
                    TypeId::UNDEFINED
                } else {
                    TypeId::UNKNOWN
                }
            });
            // When a setter parameter has no type annotation, its type is UNKNOWN
            // (sentinel). Filter out so we fall back to getter type, matching tsc.
            let write_type = accessor
                .setter
                .filter(|&t| t != TypeId::UNKNOWN)
                .or(accessor.getter)
                .unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            partial_ctor_props.push(PropertyInfo {
                name,
                type_id: read_type,
                write_type,
                optional: false,
                readonly,
                is_method: false,
                is_class_prototype: false,
                visibility: accessor.visibility,
                parent_id: current_sym,
                declaration_order: 0,
            });
        }

        if let Some(extra_property) = extra_property {
            partial_ctor_props.push(extra_property);
        }

        // Include inherited static properties from base class
        let own_names: std::collections::HashSet<_> =
            partial_ctor_props.iter().map(|p| p.name).collect();
        for prop in inherited_static_props {
            if !own_names.contains(&prop.name) {
                partial_ctor_props.push(prop.clone());
            }
        }

        // Add `any`-typed placeholders for static members that haven't been
        // processed yet. This prevents false TS2339 when an earlier static
        // initializer references a later-declared member (TSC resolves these
        // to `any` / emits TS2729 instead).
        let final_names: FxHashSet<_> = partial_ctor_props.iter().map(|p| p.name).collect();
        for &name in all_static_member_names {
            if !final_names.contains(&name) {
                partial_ctor_props.push(PropertyInfo {
                    name,
                    type_id: TypeId::ANY,
                    write_type: TypeId::ANY,
                    optional: false,
                    readonly: false,
                    is_method: false,
                    is_class_prototype: false,
                    visibility: Visibility::Public,
                    parent_id: current_sym,
                    declaration_order: 0,
                });
            }
        }

        factory.callable(CallableShape {
            call_signatures: Vec::new(),
            construct_signatures: construct_signatures.to_vec(),
            properties: partial_ctor_props,
            string_index: static_string_index.clone(),
            number_index: static_number_index.clone(),
            symbol: current_sym,
            is_abstract: false,
        })
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

    /// Push enclosing function's type parameters into scope temporarily,
    /// returning the updates needed to pop them later.
    fn push_enclosing_function_type_params(
        &mut self,
        class_idx: NodeIndex,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        // If type_parameter_scope already has entries, no need to push
        if !self.ctx.type_parameter_scope.is_empty() {
            return Vec::new();
        }

        // Walk up the AST to find the enclosing function
        let mut current = class_idx;
        for _ in 0..20 {
            let Some(ext) = self.ctx.arena.get_extended(current) else {
                return Vec::new();
            };
            let parent = ext.parent;
            if !parent.is_some() {
                return Vec::new();
            }
            let Some(parent_node) = self.ctx.arena.get(parent) else {
                return Vec::new();
            };

            if parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
                || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
            {
                if let Some(func) = self.ctx.arena.get_function(parent_node)
                    && func.type_parameters.is_some()
                {
                    let (_, updates) = self.push_type_parameters(&func.type_parameters);
                    return updates;
                }
                return Vec::new();
            }

            current = parent;
        }
        Vec::new()
    }

    /// Resolve a parameter symbol's type annotation directly, bypassing
    /// `node_types/symbol_types` caches.  Used for mixin pattern detection
    /// where the parameter's type may have been cached as `any` before
    /// type parameters were in scope.
    fn resolve_param_type_annotation(&mut self, sym_id: tsz_binder::SymbolId) -> Option<TypeId> {
        let symbol = self.ctx.binder.get_symbol(sym_id)?;
        let node = self.ctx.arena.get(symbol.value_declaration)?;
        if let Some(param) = self.ctx.arena.get_parameter(node)
            && param.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(param.type_annotation));
        }
        if let Some(var_decl) = self.ctx.arena.get_variable_declaration(node)
            && var_decl.type_annotation.is_some()
        {
            return Some(self.get_type_from_type_node(var_decl.type_annotation));
        }
        None
    }
}
