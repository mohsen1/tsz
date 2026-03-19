use crate::class_checker::ClassMemberInfo;
use crate::flow_analysis::{ComputedKey, PropertyKey};
use crate::query_boundaries::definite_assignment::constructor_assigned_properties;
use crate::state::CheckerState;
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;

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
    pub(crate) all_instance_field_keys: FxHashSet<PropertyKey>,
    pub(crate) parameter_property_keys: FxHashSet<PropertyKey>,
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

#[derive(Clone, Default)]
pub(crate) struct ClassChainSummary {
    visible_instance_lookup: FxHashMap<String, ClassMemberInfo>,
    visible_static_lookup: FxHashMap<String, ClassMemberInfo>,
    all_instance_lookup: FxHashMap<String, ClassMemberInfo>,
    all_static_lookup: FxHashMap<String, ClassMemberInfo>,
    pub(crate) visible_instance_names: FxHashSet<String>,
    pub(crate) visible_static_names: FxHashSet<String>,
}

impl ClassChainSummary {
    pub(crate) fn lookup(
        &self,
        target_name: &str,
        target_is_static: bool,
        skip_private: bool,
    ) -> Option<&ClassMemberInfo> {
        let map = match (target_is_static, skip_private) {
            (false, true) => &self.visible_instance_lookup,
            (true, true) => &self.visible_static_lookup,
            (false, false) => &self.all_instance_lookup,
            (true, false) => &self.all_static_lookup,
        };
        map.get(target_name)
    }
}

impl<'a> CheckerState<'a> {
    pub(crate) fn summarize_class_initialization(
        &mut self,
        _class_idx: NodeIndex,
        class: &tsz_parser::parser::node::ClassData,
    ) -> ClassInitializationSummary {
        let requires_super = self.class_has_base(class);
        let constructor_body = self.find_constructor_body(&class.members);

        let mut summary = ClassInitializationSummary {
            requires_super,
            constructor_body,
            ..ClassInitializationSummary::default()
        };

        for (position, &member_idx) in class.members.nodes.iter().enumerate() {
            summary.member_positions.insert(member_idx, position);

            let Some(member_node) = self.ctx.arena.get(member_idx) else {
                continue;
            };

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
                    if let Some(key) = self.property_key_from_name(param.name) {
                        summary.parameter_property_keys.insert(key);
                    }
                    if let Some(name) = self.get_property_name(param.name) {
                        summary.parameter_property_names.insert(name);
                    }
                }

                continue;
            }

            if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                continue;
            }

            let Some(prop) = self.ctx.arena.get_property_decl(member_node) else {
                continue;
            };
            if self.has_static_modifier(&prop.modifiers) {
                continue;
            }

            let key = self
                .property_key_from_name(prop.name)
                .or_else(|| self.synthetic_computed_property_key(prop.name));
            if let Some(ref key) = key {
                summary.all_instance_field_keys.insert(key.clone());
                if prop.initializer.is_some() {
                    summary.field_initializer_keys.insert(key.clone());
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
                    .instance_property_by_name
                    .entry(name.clone())
                    .or_insert(summary.ordered_instance_properties.len());
            }

            if info.requires_initialization {
                summary.required_instance_fields.push(info.clone());
            }

            summary.ordered_instance_properties.push(info);
        }

        summary.constructor_assigned_fields = if let Some(body_idx) = summary.constructor_body {
            constructor_assigned_properties(
                self,
                body_idx,
                &summary.all_instance_field_keys,
                summary.requires_super,
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

    pub(crate) fn summarize_class_chain(&mut self, class_idx: NodeIndex) -> ClassChainSummary {
        let mut summary = ClassChainSummary::default();
        let mut visited = FxHashSet::default();
        let mut current = Some(class_idx);

        while let Some(current_idx) = current {
            if !visited.insert(current_idx) {
                break;
            }

            let Some(class) = self.ctx.arena.get_class_at(current_idx) else {
                break;
            };

            for &member_idx in &class.members.nodes {
                if let Some(info) = self.extract_class_member_info(member_idx, true) {
                    let name = info.name.clone();
                    if info.is_static {
                        summary.visible_static_names.insert(name.clone());
                        summary.visible_static_lookup.entry(name).or_insert(info);
                    } else {
                        summary.visible_instance_names.insert(name.clone());
                        summary.visible_instance_lookup.entry(name).or_insert(info);
                    }
                }

                if let Some(info) = self.extract_class_member_info(member_idx, false) {
                    let name = info.name.clone();
                    if info.is_static {
                        summary.all_static_lookup.entry(name).or_insert(info);
                    } else {
                        summary.all_instance_lookup.entry(name).or_insert(info);
                    }
                }
            }

            for skip_private in [true, false] {
                self.collect_constructor_parameter_properties_into_summary(
                    current_idx,
                    skip_private,
                    &mut summary,
                );
            }

            current = self.get_base_class_idx(current_idx);
        }

        summary
    }

    fn collect_constructor_parameter_properties_into_summary(
        &mut self,
        class_idx: NodeIndex,
        skip_private: bool,
        summary: &mut ClassChainSummary,
    ) {
        let Some(class) = self.ctx.arena.get_class_at(class_idx) else {
            return;
        };

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
                if skip_private && self.has_private_modifier(&param.modifiers) {
                    continue;
                }
                let Some(name) = self.get_property_name(param.name) else {
                    continue;
                };
                let prop_type = if param.type_annotation.is_some() {
                    self.get_type_from_type_node(param.type_annotation)
                } else {
                    tsz_solver::TypeId::ANY
                };
                let info = ClassMemberInfo {
                    name: name.clone(),
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
                };

                if skip_private {
                    summary.visible_instance_names.insert(name.clone());
                    summary.visible_instance_lookup.entry(name).or_insert(info);
                } else {
                    summary.all_instance_lookup.entry(name).or_insert(info);
                }
            }
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
