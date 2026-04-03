use crate::class_checker::ClassMemberInfo;
use crate::flow_analysis::{ComputedKey, PropertyKey};
use crate::query_boundaries::common::{callable_shape_for_type, object_shape_for_type};
use crate::query_boundaries::definite_assignment::constructor_assigned_properties;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_lowering::TypeLowering;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;
use tsz_solver::{TypeId, Visibility};

#[derive(Clone)]
pub(crate) struct ClassPropertyInitializationInfo {
    pub(crate) name_idx: NodeIndex,
    pub(crate) key: Option<PropertyKey>,
    pub(crate) lookup_name: Option<String>,
    pub(crate) display_name: String,
    pub(crate) position: usize,
    pub(crate) has_no_initializer: bool,
    pub(crate) is_abstract: bool,
    pub(crate) requires_initialization: bool,
}

#[derive(Clone, Default)]
pub(crate) struct ClassInitializationSummary {
    pub(crate) requires_super: bool,
    pub(crate) constructor_body: Option<NodeIndex>,
    pub(crate) has_super_call_position_sensitive_members: bool,
    pub(crate) all_instance_field_keys: FxHashSet<PropertyKey>,
    pub(crate) required_instance_field_keys: FxHashSet<PropertyKey>,
    pub(crate) parameter_property_names: FxHashSet<String>,
    pub(crate) field_initializer_keys: FxHashSet<PropertyKey>,
    pub(crate) constructor_assigned_fields: FxHashSet<PropertyKey>,
    pub(crate) required_instance_fields: Vec<ClassPropertyInitializationInfo>,
    member_positions: FxHashMap<NodeIndex, usize>,
    instance_property_by_name: FxHashMap<String, usize>,
    ordered_instance_properties: Vec<ClassPropertyInitializationInfo>,
}

impl ClassInitializationSummary {
    pub(crate) fn member_position(&self, member_idx: NodeIndex) -> Option<usize> {
        self.member_positions.get(&member_idx).copied()
    }

    pub(crate) fn instance_property_named(
        &self,
        name: &str,
    ) -> Option<&ClassPropertyInitializationInfo> {
        self.instance_property_by_name
            .get(name)
            .and_then(|&idx| self.ordered_instance_properties.get(idx))
    }
}

/// Unified per-member entry that stores all attributes in one allocation.
/// Replaces 3 separate hashmaps (lookup, display_name, kind) per axis.
#[derive(Clone)]
pub(crate) struct MemberEntry {
    pub(crate) info: ClassMemberInfo,
    pub(crate) display_name: String,
    pub(crate) kind: ClassMemberKind,
    pub(crate) is_visible: bool,
}

#[derive(Clone, Default)]
struct ClassOwnMemberSummary {
    initialization: ClassInitializationSummary,
    /// Unified instance member map: name -> entry (replaces 6 separate maps)
    instance_members: FxHashMap<String, MemberEntry>,
    /// Unified static member map: name -> entry (replaces 6 separate maps)
    static_members: FxHashMap<String, MemberEntry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassMemberKind {
    MethodLike,
    FieldLike,
}

#[derive(Clone, Default)]
pub(crate) struct ClassChainSummary {
    /// Unified instance member map: name -> entry (replaces 6 maps + 1 set)
    instance_members: FxHashMap<String, MemberEntry>,
    /// Unified static member map: name -> entry (replaces 6 maps + 1 set)
    static_members: FxHashMap<String, MemberEntry>,
}

impl ClassChainSummary {
    pub(crate) fn lookup(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<&ClassMemberInfo> {
        let map = if target_is_static {
            &self.static_members
        } else {
            &self.instance_members
        };
        map.get(target_name).and_then(|entry| {
            if skip_private && !entry.is_visible {
                None
            } else {
                Some(&entry.info)
            }
        })
    }

    pub(crate) fn member_kind(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<ClassMemberKind> {
        let map = if target_is_static {
            &self.static_members
        } else {
            &self.instance_members
        };
        map.get(target_name).and_then(|entry| {
            if skip_private && !entry.is_visible {
                None
            } else {
                Some(entry.kind)
            }
        })
    }

    pub(crate) fn member_display_name(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<&str> {
        let map = if target_is_static {
            &self.static_members
        } else {
            &self.instance_members
        };
        map.get(target_name).and_then(|entry| {
            if skip_private && !entry.is_visible {
                None
            } else {
                Some(entry.display_name.as_str())
            }
        })
    }

    /// Get the set of visible instance member names.
    pub(crate) fn visible_instance_names(&self) -> impl Iterator<Item = &String> {
        self.instance_members
            .iter()
            .filter(|(_, entry)| entry.is_visible)
            .map(|(name, _)| name)
    }

    /// Get the set of visible static member names.
    pub(crate) fn visible_static_names(&self) -> impl Iterator<Item = &String> {
        self.static_members
            .iter()
            .filter(|(_, entry)| entry.is_visible)
            .map(|(name, _)| name)
    }
}

#[derive(Clone)]
struct JsImplicitMemberName {
    lookup_name: String,
    display_name: String,
}

impl<'a> CheckerState<'a> {
    pub(crate) fn summarize_class_initialization(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> ClassInitializationSummary {
        self.summarize_own_class_members(class_idx, class)
            .initialization
    }

    /// Collect only member info (names, types, visibility, kinds) for a class.
    /// Uses a single unified map per axis (instance/static) instead of 6 separate maps.
    fn collect_class_members_for_chain(
        &mut self,
        class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> ClassOwnMemberSummary {
        use crate::class_checker::MemberVisibility;

        let mut summary = ClassOwnMemberSummary::default();

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if let Some(info) = self.extract_class_member_info(member_idx, false) {
                let is_visible = info.visibility != MemberVisibility::Private;
                Self::record_unified_member(info, is_visible, &mut summary, self);
            }

            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                    continue;
                };
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
                    if let Some(info) = self.parameter_property_member_info(param_idx, param, false)
                    {
                        let is_visible = info.visibility != MemberVisibility::Private;
                        Self::record_unified_member(info, is_visible, &mut summary, self);
                    }
                }
            }
        }

        self.record_merged_interface_members_for_chain(class_idx, &mut summary);
        self.collect_js_implicit_member_kinds(class, &mut summary);
        summary
    }

    fn summarize_own_class_members(
        &mut self,
        _class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> ClassOwnMemberSummary {
        let requires_super = self.class_has_base(class);
        let constructor_body = self.find_constructor_body(&class.members);

        let mut summary = ClassOwnMemberSummary {
            initialization: ClassInitializationSummary {
                requires_super,
                constructor_body,
                ..ClassInitializationSummary::default()
            },
            ..ClassOwnMemberSummary::default()
        };

        for (position, &member_idx) in class.members.nodes.iter().enumerate() {
            summary
                .initialization
                .member_positions
                .insert(member_idx, position);

            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            if let Some(info) = self.extract_class_member_info(member_idx, false) {
                let is_visible = info.visibility != crate::class_checker::MemberVisibility::Private;
                Self::record_unified_member(info, is_visible, &mut summary, self);
            }

            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                    continue;
                };

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
                    summary
                        .initialization
                        .has_super_call_position_sensitive_members = true;
                    if let Some(name) = self.get_property_name(param.name) {
                        summary.initialization.parameter_property_names.insert(name);
                    }
                    if let Some(info) = self.parameter_property_member_info(param_idx, param, false)
                    {
                        let is_visible =
                            info.visibility != crate::class_checker::MemberVisibility::Private;
                        Self::record_unified_member(info, is_visible, &mut summary, self);
                    }
                }

                continue;
            }

            match member_node.kind {
                syntax_kind_ext::PROPERTY_DECLARATION => {}
                syntax_kind_ext::METHOD_DECLARATION => {
                    if let Some(method) = self.ctx.arena.get_method_decl(member_node)
                        && self.is_private_identifier_name(method.name)
                    {
                        summary
                            .initialization
                            .has_super_call_position_sensitive_members = true;
                    }
                    continue;
                }
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                    if let Some(accessor) = self.ctx.arena.get_accessor(member_node)
                        && self.is_private_identifier_name(accessor.name)
                    {
                        summary
                            .initialization
                            .has_super_call_position_sensitive_members = true;
                    }
                    continue;
                }
                _ => continue,
            }

            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };

            if self.is_private_identifier_name(prop.name) {
                summary
                    .initialization
                    .has_super_call_position_sensitive_members = true;
            }
            if !self.has_static_modifier(&prop.modifiers) && prop.initializer.is_some() {
                summary
                    .initialization
                    .has_super_call_position_sensitive_members = true;
            }

            if self.has_static_modifier(&prop.modifiers) {
                continue;
            }

            let key = self
                .property_key_from_name(prop.name)
                .or_else(|| self.synthetic_computed_property_key(prop.name));
            if let Some(ref key) = key {
                summary
                    .initialization
                    .all_instance_field_keys
                    .insert(key.clone());
                if prop.initializer.is_some() {
                    summary
                        .initialization
                        .field_initializer_keys
                        .insert(key.clone());
                }
            }

            let info = ClassPropertyInitializationInfo {
                name_idx: prop.name,
                key,
                lookup_name: self.get_property_name(prop.name),
                display_name: self.class_property_display_name(prop.name),
                position,
                has_no_initializer: prop.initializer.is_none() && !prop.exclamation_token,
                is_abstract: self.has_abstract_modifier(&prop.modifiers),
                requires_initialization: self.property_requires_initialization(
                    member_idx,
                    prop,
                    requires_super,
                ),
            };

            if let Some(ref name) = info.lookup_name {
                summary
                    .initialization
                    .instance_property_by_name
                    .entry(name.clone())
                    .or_insert(summary.initialization.ordered_instance_properties.len());
            }

            if info.requires_initialization {
                if let Some(ref key) = info.key {
                    summary
                        .initialization
                        .required_instance_field_keys
                        .insert(key.clone());
                }
                summary
                    .initialization
                    .required_instance_fields
                    .push(info.clone());
            }

            summary
                .initialization
                .ordered_instance_properties
                .push(info);
        }

        self.collect_js_implicit_member_kinds(class, &mut summary);

        summary.initialization.constructor_assigned_fields =
            if let Some(body_idx) = summary.initialization.constructor_body {
                constructor_assigned_properties(
                    self,
                    body_idx,
                    &summary.initialization.all_instance_field_keys,
                    summary.initialization.requires_super,
                )
            } else {
                FxHashSet::default()
            };

        summary
    }

    pub(crate) fn summarize_enclosing_class_initialization(
        &mut self,
    ) -> Option<ClassInitializationSummary> {
        let class_idx = self.ctx.enclosing_class.as_ref()?.class_idx;
        let class = self.ctx.arena.get_class_at(class_idx)?;
        Some(self.summarize_class_initialization(class_idx, class))
    }

    pub(crate) fn summarize_class_chain(
        &mut self,
        class_idx: NodeIndex,
    ) -> std::rc::Rc<ClassChainSummary> {
        // Check cache first
        {
            let cache = self.ctx.class_chain_summary_cache.borrow();
            if let Some(cached) = cache.get(&class_idx) {
                return std::rc::Rc::clone(cached);
            }
        }

        // Class chain summaries are used for type compatibility checks (e.g., TS2416).
        // Method return types must be widened (e.g., "base" → string) to avoid false
        // incompatibilities. Temporarily disable preserve_literal_types during summary
        // construction so that infer_return_type_from_body widens literal returns.
        let saved_preserve = self.ctx.preserve_literal_types;
        self.ctx.preserve_literal_types = false;

        let mut summary = ClassChainSummary::default();
        let mut visited = FxHashSet::default();
        let mut current = Some(class_idx);
        // Track a cumulative substitution from ancestor type parameters to the
        // root class's type parameter expressions.  When `L<RT> extends T<F(RT)>`,
        // the substitution maps T's type param `A` to `F(RT)` so that inherited
        // members from T are expressed in terms of L's type params.  This prevents
        // false TS2416 when the derived class overrides a property whose base type
        // is only correct after full substitution through the chain.
        let mut cumulative_substitution = tsz_solver::TypeSubstitution::new();
        let mut is_first = true;

        while let Some(current_idx) = current {
            if !visited.insert(current_idx) {
                break;
            }

            let Some(class) = self.ctx.arena.get_class_at(current_idx) else {
                break;
            };

            // Push the class's type parameters into scope so that type annotations
            // referencing them (e.g., `props: P & { children?: ReactNode }`) resolve
            // to proper TypeParameter types instead of falling back to `any`.
            // Without this, the substitution in check_property_inheritance_compatibility
            // cannot replace the type parameters, causing base member types to remain
            // as `any` and skipping TS2416 checks entirely.
            let (_, type_param_updates) = self.push_type_parameters(&class.type_parameters);

            let own_summary = self.collect_class_members_for_chain(current_idx, class);

            // Extract extends-clause type arguments while the current class's type
            // parameters are still in scope (so expressions like `RT[RT['a']]` resolve).
            let extends_info = self.get_extends_clause_type_args(current_idx);

            self.pop_type_parameters(type_param_updates);

            // For the first class (the class itself), members are already in terms of
            // its own type parameters — no substitution needed.  For inherited members
            // from ancestor classes, apply the cumulative substitution to rewrite their
            // types from the ancestor's type parameters to the root class's.
            if is_first {
                is_first = false;
                for (name, entry) in own_summary.instance_members {
                    summary.instance_members.entry(name).or_insert(entry);
                }
                for (name, entry) in own_summary.static_members {
                    summary.static_members.entry(name).or_insert(entry);
                }
            } else {
                for (name, mut entry) in own_summary.instance_members {
                    if !cumulative_substitution.is_empty() {
                        entry.info.type_id = crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            entry.info.type_id,
                            &cumulative_substitution,
                        );
                    }
                    summary.instance_members.entry(name).or_insert(entry);
                }
                for (name, mut entry) in own_summary.static_members {
                    if !cumulative_substitution.is_empty() {
                        entry.info.type_id = crate::query_boundaries::common::instantiate_type(
                            self.ctx.types,
                            entry.info.type_id,
                            &cumulative_substitution,
                        );
                    }
                    summary.static_members.entry(name).or_insert(entry);
                }
            }

            // Build the substitution for the next level: map the base class's type
            // parameters to the extends-clause type arguments, composed with the
            // existing cumulative substitution.
            if let Some((base_class_idx, type_arg_ids)) = extends_info {
                if let Some(base_class) = self.ctx.arena.get_class_at(base_class_idx) {
                    let (base_type_params, base_type_param_updates) =
                        self.push_type_parameters(&base_class.type_parameters);
                    self.pop_type_parameters(base_type_param_updates);

                    if !base_type_params.is_empty() && !type_arg_ids.is_empty() {
                        let level_sub = tsz_solver::TypeSubstitution::from_args(
                            self.ctx.types,
                            &base_type_params,
                            &type_arg_ids,
                        );
                        let mut new_cumulative = tsz_solver::TypeSubstitution::new();
                        for (&param_name, &arg_type) in level_sub.map() {
                            let instantiated = if !cumulative_substitution.is_empty() {
                                crate::query_boundaries::common::instantiate_type(
                                    self.ctx.types,
                                    arg_type,
                                    &cumulative_substitution,
                                )
                            } else {
                                arg_type
                            };
                            new_cumulative.insert(param_name, instantiated);
                        }
                        cumulative_substitution = new_cumulative;
                    }
                }
                current = Some(base_class_idx);
            } else {
                current = self.get_base_class_idx(current_idx);
            }
        }

        self.ctx.preserve_literal_types = saved_preserve;

        // Cache the result as Rc for cheap clone on subsequent lookups
        let summary = std::rc::Rc::new(summary);
        self.ctx
            .class_chain_summary_cache
            .borrow_mut()
            .insert(class_idx, std::rc::Rc::clone(&summary));

        summary
    }

    /// Extract extends-clause type arguments for a class, returning the resolved
    /// base class node index and evaluated type argument TypeIds.
    /// Must be called while the class's type parameters are in scope.
    fn get_extends_clause_type_args(
        &mut self,
        class_idx: NodeIndex,
    ) -> Option<(NodeIndex, Vec<TypeId>)> {
        let class = self.ctx.arena.get_class_at(class_idx)?;
        let heritage_clauses = class.heritage_clauses.as_ref()?;

        for &clause_idx in &heritage_clauses.nodes {
            let clause_node = self.ctx.arena.get(clause_idx)?;
            let heritage = self.ctx.arena.get_heritage_clause(clause_node)?;
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                continue;
            }
            let &type_idx = heritage.types.nodes.first()?;
            let type_node = self.ctx.arena.get(type_idx)?;

            let (expr_idx, type_arguments) =
                if let Some(expr_type_args) = self.ctx.arena.get_expr_type_args(type_node) {
                    (
                        expr_type_args.expression,
                        expr_type_args.type_arguments.as_ref(),
                    )
                } else {
                    (type_idx, None)
                };

            let base_sym_id = self.resolve_heritage_symbol(expr_idx)?;
            let base_class_idx = self.get_class_declaration_from_symbol(base_sym_id)?;

            let type_arg_ids: Vec<TypeId> = if let Some(args) = type_arguments {
                args.nodes
                    .iter()
                    .map(|&arg_idx| self.get_type_from_type_node(arg_idx))
                    .collect()
            } else {
                Vec::new()
            };

            return Some((base_class_idx, type_arg_ids));
        }

        None
    }

    pub(crate) fn parameter_property_member_info(
        &mut self,
        param_idx: NodeIndex,
        param: &tsz_parser::parser::node::ParameterData,
        skip_private: bool,
    ) -> Option<ClassMemberInfo> {
        if skip_private && self.has_private_modifier(&param.modifiers) {
            return None;
        }

        let name = self.get_property_name(param.name)?;
        let mut prop_type = if param.type_annotation.is_some() {
            self.get_type_from_type_node(param.type_annotation)
        } else {
            TypeId::ANY
        };
        if param.question_token && self.ctx.strict_null_checks() {
            prop_type = self
                .ctx
                .types
                .factory()
                .union2(prop_type, TypeId::UNDEFINED);
        }

        Some(ClassMemberInfo {
            name,
            type_id: prop_type,
            name_idx: param.name,
            visibility: if self.has_private_modifier(&param.modifiers) {
                crate::class_checker::MemberVisibility::Private
            } else if self.has_protected_modifier(&param.modifiers) {
                crate::class_checker::MemberVisibility::Protected
            } else {
                crate::class_checker::MemberVisibility::Public
            },
            is_method: false,
            is_static: false,
            is_accessor: false,
            is_abstract: false,
            has_override: self.has_override_modifier(&param.modifiers)
                || self.has_jsdoc_override_tag(param_idx),
            is_jsdoc_override: !self.has_override_modifier(&param.modifiers)
                && self.has_jsdoc_override_tag(param_idx),
            has_dynamic_name: false,
            has_computed_non_literal_name: false,
        })
    }

    /// Record a member into the unified map structure.
    /// Each member is stored once with all its attributes (info, display_name, kind, visibility).
    fn record_unified_member(
        info: ClassMemberInfo,
        is_visible: bool,
        summary: &mut ClassOwnMemberSummary,
        state: &Self,
    ) {
        let kind = Self::member_kind_from_info(&info);
        let name = info.name.clone();
        let display_name = state
            .get_member_name_display_text(info.name_idx)
            .unwrap_or_else(|| name.clone());
        let map = if info.is_static {
            &mut summary.static_members
        } else {
            &mut summary.instance_members
        };
        map.entry(name).or_insert(MemberEntry {
            info,
            display_name,
            kind,
            is_visible,
        });
    }

    fn record_merged_interface_members_for_chain(
        &mut self,
        class_idx: NodeIndex,
        summary: &mut ClassOwnMemberSummary,
    ) {
        let Some(sym_id) = self.ctx.binder.get_node_symbol(class_idx) else {
            return;
        };
        let Some(symbol) = self.ctx.binder.get_symbol(sym_id) else {
            return;
        };

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
        if interface_decls.is_empty() {
            return;
        }

        let type_param_bindings = self.get_type_param_bindings();
        let type_resolver = |node_idx: NodeIndex| self.resolve_type_symbol_for_lowering(node_idx);
        let value_resolver = |node_idx: NodeIndex| self.resolve_value_symbol_for_lowering(node_idx);
        let lowering = TypeLowering::with_resolvers(
            self.ctx.arena,
            self.ctx.types,
            &type_resolver,
            &value_resolver,
        )
        .with_type_param_bindings(type_param_bindings);
        let interface_type = lowering.lower_interface_declarations(&interface_decls);
        let interface_type = self.merge_interface_heritage_types(&interface_decls, interface_type);
        self.record_merged_interface_shape_members(interface_type, summary);
    }

    fn record_merged_interface_shape_members(
        &mut self,
        interface_type: TypeId,
        summary: &mut ClassOwnMemberSummary,
    ) {
        if let Some(shape) = object_shape_for_type(self.ctx.types, interface_type) {
            for prop in &shape.properties {
                self.record_merged_interface_property(prop, summary);
            }
            return;
        }

        if let Some(shape) = callable_shape_for_type(self.ctx.types, interface_type) {
            for prop in &shape.properties {
                self.record_merged_interface_property(prop, summary);
            }
        }
    }

    fn record_merged_interface_property(
        &mut self,
        prop: &tsz_solver::PropertyInfo,
        summary: &mut ClassOwnMemberSummary,
    ) {
        let visibility = match prop.visibility {
            Visibility::Private => crate::class_checker::MemberVisibility::Private,
            Visibility::Protected => crate::class_checker::MemberVisibility::Protected,
            Visibility::Public => crate::class_checker::MemberVisibility::Public,
        };
        let info = ClassMemberInfo {
            name: self.ctx.types.resolve_atom(prop.name),
            type_id: prop.type_id,
            name_idx: NodeIndex::NONE,
            visibility,
            is_method: prop.is_method,
            is_static: false,
            is_accessor: false,
            is_abstract: false,
            has_override: false,
            is_jsdoc_override: false,
            has_dynamic_name: false,
            has_computed_non_literal_name: false,
        };
        let is_visible = visibility != crate::class_checker::MemberVisibility::Private;
        Self::record_unified_member(info, is_visible, summary, self);
    }

    const fn member_kind_from_info(info: &ClassMemberInfo) -> ClassMemberKind {
        if info.is_method || info.is_accessor {
            ClassMemberKind::MethodLike
        } else {
            ClassMemberKind::FieldLike
        }
    }

    fn collect_js_implicit_member_kinds(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
        summary: &mut ClassOwnMemberSummary,
    ) {
        if !self.ctx.is_js_file() {
            return;
        }

        for &member_idx in &class.members.nodes {
            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

            match member_node.kind {
                syntax_kind_ext::CONSTRUCTOR => {
                    let Some(ctor) = self.ctx.arena.get_constructor(member_node) else {
                        continue;
                    };
                    if ctor.body.is_some() {
                        self.record_js_body_assigned_member_kinds(ctor.body, false, summary);
                    }
                }
                syntax_kind_ext::METHOD_DECLARATION => {
                    let Some(method) = self.ctx.arena.get_method_decl(member_node) else {
                        continue;
                    };
                    if method.body.is_some() {
                        self.record_js_body_assigned_member_kinds(
                            method.body,
                            self.has_static_modifier(&method.modifiers),
                            summary,
                        );
                    }
                }
                syntax_kind_ext::GET_ACCESSOR | syntax_kind_ext::SET_ACCESSOR => {
                    let Some(accessor) = self.ctx.arena.get_accessor(member_node) else {
                        continue;
                    };
                    if accessor.body.is_some() {
                        self.record_js_body_assigned_member_kinds(
                            accessor.body,
                            self.has_static_modifier(&accessor.modifiers),
                            summary,
                        );
                    }
                }
                syntax_kind_ext::PROPERTY_DECLARATION => {
                    let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                        continue;
                    };
                    if let Some(body_idx) = self.function_like_body(prop.initializer) {
                        self.record_js_body_assigned_member_kinds(
                            body_idx,
                            self.has_static_modifier(&prop.modifiers),
                            summary,
                        );
                    }
                }
                _ => {}
            }
        }

        self.collect_js_static_expando_member_kinds(class, summary);
    }

    fn collect_js_static_expando_member_kinds(
        &mut self,
        class: &tsz_parser::parser::node::ClassData,
        summary: &mut ClassOwnMemberSummary,
    ) {
        let Some(name_node) = self.ctx.arena.get(class.name) else {
            return;
        };
        let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
            return;
        };

        for name in self.collect_expando_properties_for_root(ident.escaped_text.as_str()) {
            Self::record_member_kind_unified(
                name.clone(),
                name,
                true,
                ClassMemberKind::FieldLike,
                summary,
            );
        }
    }

    fn record_js_body_assigned_member_kinds(
        &mut self,
        body_idx: NodeIndex,
        is_static: bool,
        summary: &mut ClassOwnMemberSummary,
    ) {
        let Some(body_node) = self.ctx.arena.get(body_idx) else {
            return;
        };
        let Some(block) = self.ctx.arena.get_block(body_node) else {
            return;
        };

        let statements = block.statements.nodes.clone();
        let this_aliases = self.collect_js_this_aliases(&statements);

        for stmt_idx in statements {
            let Some(name) = self.js_implicit_member_name(stmt_idx, &this_aliases) else {
                continue;
            };
            Self::record_member_kind_unified(
                name.lookup_name,
                name.display_name,
                is_static,
                ClassMemberKind::FieldLike,
                summary,
            );
        }
    }

    /// Record a JS-implicit member kind into the unified map structure.
    /// These members have no ClassMemberInfo (they come from `this.x = ...` patterns),
    /// so we create a minimal placeholder entry with just the kind and display_name.
    fn record_member_kind_unified(
        name: String,
        display_name: String,
        is_static: bool,
        kind: ClassMemberKind,
        summary: &mut ClassOwnMemberSummary,
    ) {
        use crate::class_checker::MemberVisibility;
        let map = if is_static {
            &mut summary.static_members
        } else {
            &mut summary.instance_members
        };
        map.entry(name.clone()).or_insert(MemberEntry {
            info: ClassMemberInfo {
                name: name.clone(),
                type_id: TypeId::ANY,
                name_idx: NodeIndex::NONE,
                visibility: MemberVisibility::Public,
                is_method: false,
                is_static,
                is_accessor: false,
                is_abstract: false,
                has_override: false,
                is_jsdoc_override: false,
                has_dynamic_name: false,
                has_computed_non_literal_name: false,
            },
            display_name,
            kind,
            is_visible: true,
        });
    }

    fn collect_js_this_aliases(&self, statements: &[NodeIndex]) -> FxHashSet<String> {
        let mut aliases = FxHashSet::default();

        for &stmt_idx in statements {
            let Some(stmt_node) = self.ctx.arena.get(stmt_idx) else {
                continue;
            };
            if stmt_node.kind != syntax_kind_ext::VARIABLE_STATEMENT {
                continue;
            }
            let Some(var_stmt) = self.ctx.arena.get_variable(stmt_node) else {
                continue;
            };
            for &decl_list_idx in &var_stmt.declarations.nodes {
                let Some(decl_list_node) = self.ctx.arena.get(decl_list_idx) else {
                    continue;
                };
                let Some(decl_list) = self.ctx.arena.get_variable(decl_list_node) else {
                    continue;
                };
                for &decl_idx in &decl_list.declarations.nodes {
                    let Some(decl_node) = self.ctx.arena.get(decl_idx) else {
                        continue;
                    };
                    let Some(var_decl) = self.ctx.arena.get_variable_declaration(decl_node) else {
                        continue;
                    };
                    let Some(init_node) = self.ctx.arena.get(var_decl.initializer) else {
                        continue;
                    };
                    if init_node.kind != SyntaxKind::ThisKeyword as u16 {
                        continue;
                    }
                    let Some(name_node) = self.ctx.arena.get(var_decl.name) else {
                        continue;
                    };
                    let Some(ident) = self.ctx.arena.get_identifier(name_node) else {
                        continue;
                    };
                    aliases.insert(ident.escaped_text.clone());
                }
            }
        }

        aliases
    }

    fn js_implicit_member_name(
        &mut self,
        stmt_idx: NodeIndex,
        this_aliases: &FxHashSet<String>,
    ) -> Option<JsImplicitMemberName> {
        let stmt_node = self.ctx.arena.get(stmt_idx)?;
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return None;
        }

        let expr_stmt = self.ctx.arena.get_expression_statement(stmt_node)?;
        let expr_node = self.ctx.arena.get(expr_stmt.expression)?;
        let (access, is_element_access) = if expr_node.kind == syntax_kind_ext::BINARY_EXPRESSION {
            let binary = self.ctx.arena.get_binary_expr(expr_node)?;
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                return None;
            }
            if self.js_assignment_rhs_is_void_zero(binary.right) {
                return None;
            }
            let lhs_node = self.ctx.arena.get(binary.left)?;
            if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                return None;
            }
            (
                self.ctx.arena.get_access_expr(lhs_node)?,
                lhs_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
            )
        } else {
            self.js_statement_declared_type(stmt_idx)?;
            if expr_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
                && expr_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
            {
                return None;
            }
            (
                self.ctx.arena.get_access_expr(expr_node)?,
                expr_node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION,
            )
        };

        let object_node = self.ctx.arena.get(access.expression)?;
        let is_this_or_alias = if object_node.kind == SyntaxKind::ThisKeyword as u16 {
            true
        } else if object_node.kind == SyntaxKind::Identifier as u16 {
            self.ctx
                .arena
                .get_identifier(object_node)
                .is_some_and(|ident| this_aliases.contains(ident.escaped_text.as_str()))
        } else {
            false
        };
        if !is_this_or_alias {
            return None;
        }

        if is_element_access {
            let prev_preserve = self.ctx.preserve_literal_types;
            self.ctx.preserve_literal_types = true;
            let key_type = self.get_type_of_node(access.name_or_argument);
            self.ctx.preserve_literal_types = prev_preserve;

            let lookup_name =
                crate::query_boundaries::type_computation::access::literal_property_name(
                    self.ctx.types,
                    key_type,
                )
                .map(|atom| self.ctx.types.resolve_atom(atom))?;
            let display_name = self
                .js_implicit_element_member_display_name(access.name_or_argument)
                .unwrap_or_else(|| lookup_name.clone());
            Some(JsImplicitMemberName {
                lookup_name,
                display_name,
            })
        } else {
            let name_node = self.ctx.arena.get(access.name_or_argument)?;
            if name_node.kind == SyntaxKind::PrivateIdentifier as u16 {
                return None;
            }
            let ident = self.ctx.arena.get_identifier(name_node)?;
            Some(JsImplicitMemberName {
                lookup_name: ident.escaped_text.clone(),
                display_name: ident.escaped_text.clone(),
            })
        }
    }

    fn js_implicit_element_member_display_name(&self, name_idx: NodeIndex) -> Option<String> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if let Some(lit) = self.ctx.arena.get_literal(name_node) {
            return match name_node.kind {
                k if k == SyntaxKind::StringLiteral as u16 => Some(format!("'{}'", lit.text)),
                k if k == SyntaxKind::NumericLiteral as u16 => Some(lit.text.clone()),
                _ => None,
            };
        }
        if let Some(ident) = self.ctx.arena.get_identifier(name_node) {
            return Some(format!("[{}]", ident.escaped_text));
        }
        None
    }

    fn function_like_body(&self, node_idx: NodeIndex) -> Option<NodeIndex> {
        let node = self.ctx.arena.get(node_idx)?;
        match node.kind {
            syntax_kind_ext::FUNCTION_EXPRESSION | syntax_kind_ext::ARROW_FUNCTION => self
                .ctx
                .arena
                .get_function(node)
                .and_then(|func| func.body.into_option()),
            _ => None,
        }
    }

    fn synthetic_computed_property_key(&self, name_idx: NodeIndex) -> Option<PropertyKey> {
        let name_node = self.ctx.arena.get(name_idx)?;
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return None;
        }
        let raw = self.node_text(name_idx)?;
        let normalized = raw.trim_end_matches(':').trim();
        let inner = normalized
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(normalized)
            .trim()
            .to_string();
        Some(PropertyKey::Computed(ComputedKey::Ident(inner)))
    }

    fn class_property_display_name(&self, name_idx: NodeIndex) -> String {
        let Some(key) = self
            .property_key_from_name(name_idx)
            .or_else(|| self.synthetic_computed_property_key(name_idx))
        else {
            return self.get_property_name(name_idx).unwrap_or_default();
        };

        let Some(name_node) = self.ctx.arena.get(name_idx) else {
            return self.get_property_name(name_idx).unwrap_or_default();
        };
        if name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return self
                .node_text(name_idx)
                .map(|raw| raw.trim_end_matches(':').trim().to_string())
                .unwrap_or_else(|| self.format_property_key_display(&key));
        }

        self.get_property_name(name_idx)
            .unwrap_or_else(|| self.format_property_key_display(&key))
    }

    fn format_property_key_display(&self, key: &PropertyKey) -> String {
        match key {
            PropertyKey::Computed(ComputedKey::Ident(value))
            | PropertyKey::Computed(ComputedKey::Number(value)) => format!("[{value}]"),
            PropertyKey::Computed(ComputedKey::String(value)) => format!("[\"{value}\"]"),
            PropertyKey::Private(value) => {
                if value.starts_with('#') {
                    value.clone()
                } else {
                    format!("#{value}")
                }
            }
            PropertyKey::Ident(value) => value.clone(),
        }
    }
}
