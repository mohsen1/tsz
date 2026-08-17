use crate::context::TypingRequest;
use crate::context::speculation::DiagnosticSpeculationSnapshot;
use crate::query_boundaries::class_type::{
    self as class_type_boundary, construct_signatures_for_type,
};
use crate::query_boundaries::common::{TypeSubstitution, instantiate_type};
use crate::state::CheckerState;
use rustc_hash::FxHashSet;
use tsz_common::interner::Atom;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{CallSignature, IndexSignature, PropertyInfo, TypeId, TypeParamInfo};

use super::build_data::StaticMemberBuildData;

impl<'a> CheckerState<'a> {
    /// Type a static/instance property's function-expression initializer
    /// during constructor-shape building without letting a premature
    /// self-referential constraint diagnostic survive.
    ///
    /// This runs before the class's own statement check, so a
    /// self-referential type-parameter constraint on the initializer (e.g.
    /// `<R extends Schema>` used inside the initializer's own signature) can
    /// resolve against Schema's not-yet-published instance type and
    /// spuriously fail a TS2344/TS2536 constraint check here. The
    /// authoritative check runs later, in the normal member walk
    /// (`check_property_declaration_with_request`), once the class's real
    /// instance type is published, so only THOSE two codes are discarded and
    /// the node-type cache cleared to force a re-validation there (mirrors
    /// `class_instance_phase0_prescan_this`, #17585/#17589). Every other
    /// diagnostic produced while typing the initializer — e.g. a `this`
    /// binding-boundary error inside a nested function/class expression — is
    /// kept: this speculative pass is sometimes the only pass that visits
    /// those nested nodes, so a blanket rollback would silently drop them.
    pub(super) fn speculative_static_property_initializer_type(
        &mut self,
        initializer: NodeIndex,
        request: &TypingRequest,
    ) -> TypeId {
        let diag_snap = DiagnosticSpeculationSnapshot::new(&self.ctx);
        let diag_checkpoint = diag_snap.checkpoint();
        let init_type = self.get_type_of_node_with_request(initializer, request);
        let leaked_self_ref_constraint_failure = self.ctx.diagnostics[diag_checkpoint..]
            .iter()
            .any(|d| matches!(d.code, 2536 | 2344));
        if leaked_self_ref_constraint_failure {
            diag_snap.rollback(&mut self.ctx.diagnostic_state());
            self.clear_type_cache_recursive(initializer);
        } else {
            diag_snap.commit(&mut self.ctx.diagnostic_state());
        }
        init_type
    }

    /// Build the fields-only provisional instance for a class whose constructor
    /// type is being resolved and install it into `symbol_instance_types`.
    ///
    /// The provisional carries only the class's declared instance properties
    /// (no methods, no body inference), so an in-flight TYPE reference to the
    /// class under construction resolves against a concrete structural shape
    /// rather than an opaque `Lazy`. It is a stand-in for the window before the
    /// real instance exists; the caller only installs it when no complete
    /// instance is registered yet and restores/removes it afterwards
    /// (issue #17456).
    ///
    /// Returns the installed provisional, or `None` when the class declares no
    /// annotated instance properties (nothing to install).
    pub(super) fn install_ctor_provisional_instance(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
        member_count: usize,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
        let mut inst_props: Vec<PropertyInfo> = Vec::with_capacity(member_count);
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
            let Some(type_id) = self.effective_class_property_declared_type(member_idx, prop)
            else {
                continue;
            };
            let Some(name) = self.get_property_name_resolved(prop.name) else {
                continue;
            };
            let name_atom = self.ctx.types.intern_string(&name);
            inst_props.push(class_type_boundary::class_member_property(
                class_type_boundary::ClassMemberProperty::new(name_atom, type_id)
                    .optional(prop.question_token)
                    .readonly(self.has_readonly_modifier(&prop.modifiers))
                    .visibility(self.get_member_visibility(&prop.modifiers, prop.name))
                    .parent(Some(sym_id)),
            ));
        }
        if inst_props.is_empty() {
            return None;
        }
        let partial_instance =
            class_type_boundary::class_member_object_type(self.ctx.types, inst_props);
        self.ctx
            .symbol_instance_types
            .insert(sym_id, partial_instance);
        Some(partial_instance)
    }

    /// Deferred fallback for a re-entrant constructor query: a `Lazy` reference
    /// to the class's `ClassConstructor` companion `DefId`, get-or-created so
    /// the same stable identity the completed computation sets the body on
    /// (the `get_class_constructor_type_inner` registration path) is the one the
    /// cycle returns. The companion resolves to the real constructor type once
    /// the outer, non-cyclic computation publishes its body; until then it
    /// behaves like any other unresolved `Lazy` (no eager `any` to cache or
    /// propagate cross-arena). Mirrors the instance side's `Lazy(classDef)`
    /// deferral (issue #13947).
    pub(super) fn deferred_constructor_companion_lazy(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        sym_id: tsz_binder::SymbolId,
    ) -> TypeId {
        let class_def = self.ctx.get_or_create_def_id(sym_id);
        let ctor_def = match self.ctx.definition_store.get_constructor_def(class_def) {
            Some(existing) => existing,
            None => {
                // No pre-populated companion (anonymous classes, or classes not
                // covered by pre-population): create one with no body yet and
                // pin the mapping, so the completing computation reuses this
                // identity via `get_constructor_def` and `set_body`s onto it.
                let display_name = self.class_constructor_display_name(class_idx, class);
                let name = self.ctx.types.intern_string(&display_name);
                let created = self.ctx.definition_store.register(
                    tsz_solver::def::DefinitionInfo::class_constructor_companion(
                        name,
                        Some(sym_id.0),
                    ),
                );
                self.ctx
                    .definition_store
                    .register_constructor_companion(class_def, created);
                created
            }
        };
        class_type_boundary::class_constructor_companion_lazy_type(self.ctx.types, ctor_def)
    }

    /// Get the constructor type of a class declaration (static members,
    /// construct signatures, inherited statics, accessibility, abstractness).
    pub(crate) fn get_class_constructor_type(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        self.get_class_constructor_type_with_request_and_mode(
            class_idx,
            class,
            &TypingRequest::NONE,
            true,
        )
    }

    pub(crate) fn get_class_constructor_type_without_module_augmentations(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> TypeId {
        self.get_class_constructor_type_with_request_and_mode(
            class_idx,
            class,
            &TypingRequest::NONE,
            false,
        )
    }

    pub(crate) fn get_class_constructor_type_with_request(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
        request: &TypingRequest,
    ) -> TypeId {
        self.get_class_constructor_type_with_request_and_mode(class_idx, class, request, true)
    }

    pub(super) fn merge_static_late_bound_index_value(
        &self,
        target: &mut Option<IndexSignature>,
        incoming: IndexSignature,
    ) {
        if let Some(existing) = target.as_mut() {
            if existing.value_type != incoming.value_type {
                existing.value_type =
                    class_type_boundary::merged_static_late_bound_index_value_type(
                        self.ctx.types,
                        existing.value_type,
                        incoming.value_type,
                    );
            }
            existing.readonly &= incoming.readonly;
        } else {
            *target = Some(incoming);
        }
    }

    pub(super) fn merge_static_late_bound_member_from_computed_name(
        &mut self,
        name_idx: NodeIndex,
        value_type: TypeId,
        request: &TypingRequest,
        static_string_index: &mut Option<IndexSignature>,
        static_number_index: &mut Option<IndexSignature>,
        static_symbol_index: &mut Option<IndexSignature>,
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

        // A key whose type is the wide `symbol` (not `unique symbol`, not a
        // well-known `Symbol.xxx` that resolved to a literal name -- callers
        // only reach this function when the name failed to resolve to one)
        // contributes a `[k: symbol]: V` index signature to the constructor
        // type, mirroring the instance-side routing this same structural rule
        // gets in `merge_index_signature_from_unresolved_computed_name`.
        // `get_index_key_kind` below has no symbol case, so this must be
        // checked first or a symbol-typed key is silently dropped.
        if self.computed_member_key_is_wide_symbol(name_idx) {
            self.merge_static_late_bound_index_value(
                static_symbol_index,
                class_type_boundary::static_late_bound_index_signature(TypeId::SYMBOL, value_type),
            );
            return;
        }

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
                class_type_boundary::static_late_bound_index_signature(TypeId::STRING, value_type),
            );
        }
        if wants_number {
            self.merge_static_late_bound_index_value(
                static_number_index,
                class_type_boundary::static_late_bound_index_signature(TypeId::NUMBER, value_type),
            );
        }
    }

    pub(super) fn class_constructor_display_name(
        &self,
        class_idx: NodeIndex,
        _class: &tsz_parser::parser::node::ClassData,
    ) -> String {
        self.get_bound_class_name_from_decl(class_idx)
            .unwrap_or_else(|| "(Anonymous class)".to_string())
    }

    /// Collect inherited static properties from the base class (extends clause).
    /// Returns a map of property name → `PropertyInfo` for each inherited static.
    pub(super) fn collect_inherited_static_properties(
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
                    && let Some(base_type) = self.ctx.symbol_types.get(&base_sym_id)
                {
                    base_props = self.static_properties_from_type(base_type);
                }
                break;
            }
        }
        base_props
    }

    pub(super) fn build_partial_static_constructor_type(
        &self,
        data: StaticMemberBuildData<'_>,
    ) -> TypeId {
        let StaticMemberBuildData {
            current_sym,
            properties,
            methods,
            accessors,
            static_string_index,
            static_number_index,
            static_symbol_index,
            extra_property,
            inherited_static_props,
            all_static_member_names,
            construct_signatures,
        } = data;
        let estimated_cap = properties.len()
            + methods.len()
            + accessors.len()
            + inherited_static_props.len()
            + all_static_member_names.len()
            + 1;
        let mut partial_ctor_props: Vec<PropertyInfo> = Vec::with_capacity(estimated_cap);
        partial_ctor_props.extend(properties.values().cloned());

        for (&name, method) in methods {
            let (signatures, optional) = if !method.overload_signatures.is_empty() {
                (&method.overload_signatures, method.overload_optional)
            } else {
                (&method.impl_signatures, method.impl_optional)
            };
            if signatures.is_empty() {
                continue;
            }

            let type_id =
                class_type_boundary::partial_static_method_type(self.ctx.types, signatures);
            partial_ctor_props.push(class_type_boundary::partial_static_method_property(
                name,
                type_id,
                optional,
                method.visibility,
                current_sym,
            ));
        }

        for (&name, accessor) in accessors {
            // When a setter parameter has no type annotation, its type is UNKNOWN
            // (sentinel). Filter out so we fall back to getter type, matching tsc.
            let setter_type = accessor.setter.filter(|&t| t != TypeId::UNKNOWN);
            let read_type = accessor.getter.or(setter_type).unwrap_or(TypeId::UNKNOWN);
            let write_type = setter_type.or(accessor.getter).unwrap_or(read_type);
            let readonly = accessor.getter.is_some() && accessor.setter.is_none();
            partial_ctor_props.push(class_type_boundary::partial_static_accessor_property(
                name,
                read_type,
                write_type,
                readonly,
                accessor.visibility,
                current_sym,
            ));
        }

        if let Some(extra_property) = extra_property {
            partial_ctor_props.push(extra_property);
        }

        // Include inherited static properties from base class
        let own_names: FxHashSet<_> = partial_ctor_props.iter().map(|p| p.name).collect();
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
                partial_ctor_props.push(class_type_boundary::partial_static_placeholder_property(
                    name,
                    current_sym,
                ));
            }
        }

        // A `CallableShape` carries only one non-numeric index slot (see its
        // doc comment): a symbol index signature rides in `string_index` when
        // no explicit string index is present, exactly as
        // `type_literal_callable_type` folds the two for an inline callable
        // type literal.
        let string_index = static_string_index.or(*static_symbol_index);

        class_type_boundary::partial_static_constructor_callable_type(
            self.ctx.types,
            current_sym,
            partial_ctor_props,
            construct_signatures,
            string_index,
            *static_number_index,
        )
    }

    /// Does this static class member's name node key off a plain
    /// (non-unique) `symbol` binding — `class C { static [s]() {} }` with
    /// `declare const s: symbol`?
    ///
    /// Such a member contributes a `[key: symbol]: V` index signature to the
    /// constructor type instead of a named static member, the static-side
    /// twin of `class_member_computed_key_is_wide_symbol`'s instance-side
    /// routing. `Symbol.NAME` written as property-access syntax is excluded
    /// by `computed_member_key_is_wide_symbol` itself, even when `NAME` is
    /// declared plain `symbol`: tsc derives a NAMED member from the syntactic
    /// well-known shape before it consults the key's type at all (#16307).
    pub(super) fn static_member_computed_key_is_wide_symbol(
        &mut self,
        name_idx: NodeIndex,
    ) -> bool {
        self.computed_member_key_is_wide_symbol(name_idx)
    }

    pub(super) fn remap_inherited_construct_signatures(
        &self,
        constructor_type: TypeId,
        class_type_params: &[TypeParamInfo],
        instance_type: TypeId,
        inherited_substitution: Option<&TypeSubstitution>,
        force_derived_instance: bool,
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
                                let mut p = *param;
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
                    // Preserve ordinary base signature type parameters so generic
                    // call resolution can infer them from constructor arguments.
                    //
                    // For mixin bases typed as a type parameter, though, inherited
                    // constraint signature parameters (for example `Constructor<T>`)
                    // are not owned by the returned class. Keeping them here can
                    // shadow enclosing factory type parameters during later
                    // instantiation, leaving returned class methods with stale `T`s.
                    let type_params = if force_derived_instance || sig.type_params.is_empty() {
                        class_type_params.to_vec()
                    } else {
                        sig.type_params.clone()
                    };
                    // For the return type: when the base signature has type params
                    // and no substitution is provided yet, preserve the base's
                    // return type so generic resolution can instantiate it properly.
                    // Otherwise use the derived instance type.
                    let return_type = if !force_derived_instance
                        && inherited_substitution.is_none()
                        && !sig.type_params.is_empty()
                    {
                        sig.return_type
                    } else {
                        inherited_substitution.map_or(instance_type, |subst| {
                            instantiate_type(self.ctx.types, sig.return_type, subst)
                        })
                    };
                    class_type_boundary::class_construct_signature(
                        type_params,
                        params,
                        this_type,
                        return_type,
                        sig.type_predicate,
                        sig.is_method,
                    )
                })
                .collect(),
        )
    }

    pub(super) fn remap_inherited_construct_signatures_with_substitution(
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
                .map(|sig| {
                    // In inherited constructors, class type params live on the deriving class.
                    // Reusing base signature type_params can incorrectly shadow substitutions.
                    let params = sig
                        .params
                        .iter()
                        .map(|p| {
                            class_type_boundary::class_construct_param(
                                p.name,
                                instantiate_type(self.ctx.types, p.type_id, substitution),
                                p.optional,
                                p.rest,
                            )
                        })
                        .collect();
                    let this_type = sig
                        .this_type
                        .map(|t| instantiate_type(self.ctx.types, t, substitution));
                    let type_predicate = sig.type_predicate.as_ref().map(|pred| {
                        class_type_boundary::class_type_predicate(
                            pred.asserts,
                            pred.target,
                            pred.type_id
                                .map(|t| instantiate_type(self.ctx.types, t, substitution)),
                            pred.parameter_index,
                        )
                    });
                    class_type_boundary::class_construct_signature(
                        class_type_params.to_vec(),
                        params,
                        this_type,
                        instance_type,
                        type_predicate,
                        sig.is_method,
                    )
                })
                .collect(),
        )
    }

    /// Push enclosing function's type parameters into scope temporarily,
    /// returning the updates needed to pop them later.
    pub(super) fn push_enclosing_function_type_params(
        &mut self,
        class_idx: NodeIndex,
    ) -> Vec<(String, Option<TypeId>, bool)> {
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
                || parent_node.is_function_expression_or_arrow()
                || parent_node.kind == syntax_kind_ext::METHOD_DECLARATION
            {
                if let Some(func) = self.ctx.arena.get_function(parent_node)
                    && func.type_parameters.is_some()
                {
                    if self.enclosing_function_type_params_already_active(&func.type_parameters) {
                        return Vec::new();
                    }
                    return self.push_enclosing_function_type_param_names(&func.type_parameters);
                }
                return Vec::new();
            }

            current = parent;
        }
        Vec::new()
    }

    fn push_enclosing_function_type_param_names(
        &mut self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> Vec<(String, Option<TypeId>, bool)> {
        let Some(type_parameters) = type_parameters else {
            return Vec::new();
        };
        let mut updates = Vec::new();
        for &param_idx in &type_parameters.nodes {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                continue;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                continue;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                continue;
            };
            let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
                continue;
            };

            let name = name_ident.escaped_text.to_string();
            let type_id = class_type_boundary::enclosing_function_type_param_type(
                self.ctx.types,
                self.ctx.types.intern_string(&name),
            );
            if let Some(&sym_id) = self.ctx.binder.node_symbols.get(&param.name.0)
                && let Some(def_id) = self.ctx.definition_store.find_def_by_symbol(sym_id.0)
            {
                self.ctx
                    .definition_store
                    .register_type_to_def(type_id, def_id);
            }
            let previous = self.ctx.type_parameter_scope.insert(name.clone(), type_id);
            updates.push((name, previous, false));
        }

        updates
    }

    fn enclosing_function_type_params_already_active(
        &self,
        type_parameters: &Option<tsz_parser::parser::NodeList>,
    ) -> bool {
        let Some(type_parameters) = type_parameters else {
            return true;
        };
        if type_parameters.nodes.is_empty() {
            return true;
        }

        type_parameters.nodes.iter().all(|&param_idx| {
            let Some(param_node) = self.ctx.arena.get(param_idx) else {
                return false;
            };
            let Some(param) = self.ctx.arena.get_type_parameter(param_node) else {
                return false;
            };
            let Some(name_node) = self.ctx.arena.get(param.name) else {
                return false;
            };
            let Some(name_ident) = self.ctx.arena.get_identifier(name_node) else {
                return false;
            };
            let Some(&scoped_type) = self
                .ctx
                .type_parameter_scope
                .get(name_ident.escaped_text.as_str())
            else {
                return false;
            };
            let Some(&sym_id) = self.ctx.binder.node_symbols.get(&param.name.0) else {
                return false;
            };
            let Some(param_def_id) = self.ctx.definition_store.find_def_by_symbol(sym_id.0) else {
                return false;
            };
            self.ctx
                .definition_store
                .find_def_for_type(scoped_type)
                .is_some_and(|scoped_def_id| scoped_def_id == param_def_id)
        })
    }

    /// Resolve a parameter symbol's type annotation directly, bypassing
    /// `node_types/symbol_types` caches.  Used for mixin pattern detection
    /// where the parameter's type may have been cached as `any` before
    /// type parameters were in scope.
    pub(super) fn resolve_param_type_annotation(
        &mut self,
        sym_id: tsz_binder::SymbolId,
    ) -> Option<TypeId> {
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

    /// Check if a call expression in a heritage clause has arguments with
    /// circular heritage dependencies back to `current_class_sym`.
    ///
    /// This detects patterns like:
    /// ```text
    /// declare class A extends Doc<typeof B> {}
    /// declare class B extends mixin(A) {}
    /// ```
    /// When building B's constructor type, the call `mixin(A)` is evaluated.
    /// A's type is already computed (eagerly), so its construct signatures exist.
    /// But A's heritage uses `typeof B`, creating a circular dependency.
    /// In tsc, this circularity causes `typeof A` to have no construct signatures
    /// during B's evaluation, producing TS2345. We detect this pattern and emit
    /// the same error.
    pub(super) fn check_circular_heritage_call_args(
        &mut self,
        call_expr_idx: NodeIndex,
        current_class_sym: tsz_binder::SymbolId,
    ) {
        use tsz_parser::parser::syntax_kind_ext as sk;

        let Some(call_node) = self.ctx.arena.get(call_expr_idx) else {
            return;
        };
        if call_node.kind != sk::CALL_EXPRESSION {
            return;
        }
        let Some(call) = self.ctx.arena.get_call_expr(call_node) else {
            return;
        };
        let Some(ref args) = call.arguments else {
            return;
        };

        for &arg_idx in &args.nodes {
            let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                continue;
            };
            if arg_node.kind != SyntaxKind::Identifier as u16 {
                continue;
            }
            // Get the argument's symbol
            let Some(arg_sym) = self.resolve_identifier_symbol(arg_idx) else {
                continue;
            };
            let Some(arg_symbol) = self.ctx.binder.get_symbol(arg_sym) else {
                continue;
            };
            // Only check class arguments
            if !arg_symbol.has_any_flags(tsz_binder::symbol_flags::CLASS) {
                continue;
            }

            // Check if this class's heritage type arguments reference the current class
            if self.class_heritage_references_symbol(arg_sym, current_class_sym) {
                // Circular dependency detected — emit TS2345
                // Use "typeof ClassName" format to match tsc's message
                let class_name = arg_symbol.escaped_name.clone();
                let msg = format!(
                    "Argument of type 'typeof {class_name}' is not assignable to parameter of type 'new (...args: any[]) => any'."
                );
                use crate::diagnostics::diagnostic_codes;
                self.ctx.error(
                    arg_node.pos,
                    arg_node.end - arg_node.pos,
                    msg,
                    diagnostic_codes::ARGUMENT_OF_TYPE_IS_NOT_ASSIGNABLE_TO_PARAMETER_OF_TYPE,
                );
            }
        }
    }

    /// Check if a class's heritage clause type arguments reference a specific symbol.
    fn class_heritage_references_symbol(
        &self,
        class_sym: tsz_binder::SymbolId,
        target_sym: tsz_binder::SymbolId,
    ) -> bool {
        use tsz_parser::parser::syntax_kind_ext as sk;

        let Some(symbol) = self.ctx.binder.get_symbol(class_sym) else {
            return false;
        };

        for &decl_idx in &symbol.declarations {
            let Some(node) = self.ctx.arena.get(decl_idx) else {
                continue;
            };
            if node.kind != sk::CLASS_DECLARATION {
                continue;
            }
            let Some(class) = self.ctx.arena.get_class(node) else {
                continue;
            };
            let Some(ref heritage_clauses) = class.heritage_clauses else {
                continue;
            };

            for &clause_idx in &heritage_clauses.nodes {
                let Some(heritage) = self.ctx.arena.get_heritage_clause_at(clause_idx) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }
                for &type_idx in &heritage.types.nodes {
                    let Some(type_node) = self.ctx.arena.get(type_idx) else {
                        continue;
                    };
                    let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) else {
                        continue;
                    };
                    let Some(ref type_args) = expr_type_args.type_arguments else {
                        continue;
                    };
                    for &arg_idx in &type_args.nodes {
                        // Check if this type argument is `typeof target_sym`
                        let Some(arg_node) = self.ctx.arena.get(arg_idx) else {
                            continue;
                        };
                        let Some(type_query) = self.ctx.arena.get_type_query(arg_node) else {
                            continue;
                        };
                        let Some(expr_node) = self.ctx.arena.get(type_query.expr_name) else {
                            continue;
                        };
                        if expr_node.kind != SyntaxKind::Identifier as u16 {
                            continue;
                        }
                        // Resolve the identifier to a symbol
                        if let Some(ref_sym) = self
                            .ctx
                            .binder
                            .node_symbols
                            .get(&type_query.expr_name.0)
                            .copied()
                            .or_else(|| {
                                let ident = self.ctx.arena.get_identifier(expr_node)?;
                                self.ctx.binder.file_locals.get(&ident.escaped_text)
                            })
                            && ref_sym == target_sym
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}
