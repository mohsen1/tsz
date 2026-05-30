//! Class body and constructor emit for TC39 decorator emission.

use super::TC39DecoratorEmitter;
#[allow(unused_imports)]
use super::helpers::*;
#[allow(unused_imports)]
use super::{
    AutoAccessorClassCtx, AutoAccessorMemberEmitCtx, ClassBodyCtx, ClassBodyFlags,
    ClassDecoratorInstancePrivateFieldInfo, ClassDecoratorVars, CtorInitFlags, CtorMembersCtx,
    CtorOutputCtx, DecoratorApplicationCtx, DecoratorReceiverState, EsDecorateMemberCtx,
    EsDecorateVars, PlainComputedInstanceFieldInfo,
};
#[allow(unused_imports)]
use crate::transforms::emit_utils::hygienic_temp_name;
#[allow(unused_imports)]
use rustc_hash::FxHashMap;
#[allow(unused_imports)]
use tsz_parser::parser::node::{NodeAccess, NodeArena};
#[allow(unused_imports)]
use tsz_parser::parser::syntax_kind_ext;
#[allow(unused_imports)]
use tsz_parser::parser::{NodeIndex, NodeList};
#[allow(unused_imports)]
use tsz_scanner::SyntaxKind;

impl<'a> TC39DecoratorEmitter<'a> {
    pub(super) fn emit_class_body(
        &self,
        ctx: &ClassBodyCtx<'_>,
        flags: &ClassBodyFlags<'_>,
        indent: &str,
        inner_indent: &str,
        out: &mut String,
    ) -> (Vec<String>, Vec<String>) {
        let ClassBodyCtx {
            class_node,
            class_data,
            decorated_members,
            member_vars,
            source_order_decorator_members,
            computed_key_vars,
            plain_computed_instance_fields,
            class_decorator_instance_private_fields,
            class_decorator_static_private_methods,
            class_decorator_auto_accessor_infos,
            class_decorator_static_private_fields,
        } = ctx;
        let ClassBodyFlags {
            has_any_instance,
            class_alias: _class_alias,
            class_name,
            defer_class_extra_init,
            class_this_var,
            class_extra_initializers_var,
            instance_extra_initializers_var,
            static_extra_initializers_var,
            has_static_method,
            instance_private_brand_var,
        } = flags;
        let has_any_instance = *has_any_instance;
        let defer_class_extra_init = *defer_class_extra_init;
        let has_static_method = *has_static_method;
        let run_init = self.helper("__runInitializers");
        let fields_in_class_body = self.use_static_blocks && self.use_define_for_class_fields;

        let propkey_map: std::collections::HashMap<NodeIndex, &str> = computed_key_vars
            .iter()
            .filter_map(|(mi, var)| {
                decorated_members
                    .get(*mi)
                    .map(|m| (m.member_idx, var.as_str()))
            })
            .collect();

        let decorated_field_idx_set: std::collections::HashSet<NodeIndex> = decorated_members
            .iter()
            .filter(|m| m.kind == MemberKind::Field)
            .map(|m| m.member_idx)
            .collect();
        let decorated_auto_accessor_idx_set: std::collections::HashSet<NodeIndex> =
            decorated_members
                .iter()
                .filter(|m| m.kind == MemberKind::Accessor)
                .map(|m| m.member_idx)
                .collect();
        let plain_computed_instance_field_idx_set: std::collections::HashSet<NodeIndex> =
            plain_computed_instance_fields
                .iter()
                .map(|info| info.member_idx)
                .collect();
        let class_decorator_instance_private_field_idx_set: std::collections::HashSet<NodeIndex> =
            class_decorator_instance_private_fields
                .iter()
                .map(|info| info.member_idx)
                .collect();
        let class_decorator_static_private_method_map: std::collections::HashMap<
            NodeIndex,
            &ClassDecoratorStaticPrivateMethodInfo,
        > = class_decorator_static_private_methods
            .iter()
            .map(|info| (info.member_idx, info))
            .collect();
        let class_decorator_auto_accessor_map: std::collections::HashMap<
            NodeIndex,
            &ClassDecoratorAutoAccessorInfo,
        > = class_decorator_auto_accessor_infos
            .iter()
            .map(|info| (info.member.member_idx, info))
            .collect();
        let class_decorator_static_private_field_map: std::collections::HashMap<
            NodeIndex,
            &ClassDecoratorStaticPrivateFieldInfo,
        > = class_decorator_static_private_fields
            .iter()
            .map(|info| (info.member_idx, info))
            .collect();
        let field_infos = self.collect_decorated_field_info(decorated_members, computed_key_vars);
        let class_decorator_static_private_auto_accessor_indices: std::collections::HashSet<
            NodeIndex,
        > = class_decorator_auto_accessor_infos
            .iter()
            .filter(|info| info.is_decorated && info.member.is_static && info.member.is_private)
            .map(|info| info.member.member_idx)
            .collect();
        let reserved_auto_accessor_storage_bases: Vec<String> = class_decorator_auto_accessor_infos
            .iter()
            .filter(|info| info.is_decorated && info.member.is_static && info.member.is_private)
            .filter_map(|info| match &info.member.name {
                MemberName::Private(name) | MemberName::Identifier(name) => {
                    Some(name.trim_start_matches('#').to_string())
                }
                MemberName::StringLiteral(_) | MemberName::Computed(_) => None,
            })
            .collect();
        let auto_accessor_infos = self.collect_decorated_auto_accessor_info(
            decorated_members,
            computed_key_vars,
            &reserved_auto_accessor_storage_bases,
            &class_decorator_static_private_auto_accessor_indices,
        );
        let parameter_properties = self.collect_constructor_parameter_properties(class_data);
        let has_parameter_properties = !parameter_properties.is_empty();
        let source_ctor = self.get_constructor_info(class_data);
        let has_instance_fields = field_infos
            .iter()
            .any(|fi| !decorated_members[fi.member_var_index].is_static)
            || !plain_computed_instance_fields.is_empty()
            || !class_decorator_instance_private_fields.is_empty();
        let has_instance_auto_accessors = auto_accessor_infos
            .iter()
            .any(|info| !decorated_members[info.member_var_index].is_static);
        let has_instance_method = decorated_members
            .iter()
            .any(|m| !m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));

        let all_members: Vec<_> = class_data
            .members
            .nodes
            .iter()
            .filter_map(|&idx| self.arena.get(idx).map(|n| (idx, n)))
            .collect();
        let class_close = self.find_class_close_brace(class_node);

        let mut plain_static_field_idx_set: std::collections::HashSet<NodeIndex> =
            std::collections::HashSet::new();
        let mut plain_static_field_assignments: Vec<String> = Vec::new();
        let decorated_static_fields_emit_in_source_order =
            self.use_static_blocks && !self.use_define_for_class_fields;
        if !self.use_static_blocks {
            for (member_i, (member_idx, member_node)) in all_members.iter().enumerate() {
                if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                    let has_class_decorator_private_rewrites =
                        !class_decorator_static_private_methods.is_empty()
                            || !class_decorator_static_private_fields.is_empty()
                            || class_decorator_auto_accessor_infos
                                .iter()
                                .any(|info| info.member.is_private);
                    let next_boundary = if member_i + 1 < all_members.len() {
                        all_members[member_i + 1].1.pos as usize
                    } else {
                        class_close
                    };
                    let text = if has_class_decorator_private_rewrites {
                        self.emit_member_bounded(member_node, next_boundary.min(class_close))
                    } else if let Some(text) = self.static_block_texts.get(member_idx) {
                        text.clone()
                    } else {
                        self.emit_member_bounded(member_node, next_boundary.min(class_close))
                    };
                    let text = self.rewrite_class_decorator_static_private_accesses(
                        &text,
                        class_decorator_static_private_methods,
                        class_decorator_auto_accessor_infos,
                        class_decorator_static_private_fields,
                        class_this_var,
                    );
                    let statement = self
                        .lower_static_block_text_to_iife(&text)
                        .unwrap_or_else(|| text.trim().trim_end_matches(';').to_string());
                    if !statement.is_empty() {
                        plain_static_field_idx_set.insert(*member_idx);
                        plain_static_field_assignments.push(statement);
                    }
                    continue;
                }
                if let Some(info) = class_decorator_static_private_field_map.get(member_idx) {
                    if info.is_decorated {
                        continue;
                    }
                    let value = if info.initializer_text.is_empty() {
                        "void 0".to_string()
                    } else {
                        info.initializer_text.clone()
                    };
                    plain_static_field_idx_set.insert(*member_idx);
                    plain_static_field_assignments
                        .push(format!("{} = {{ value: {value} }}", info.storage_name));
                    continue;
                }
                if let Some(info) = class_decorator_auto_accessor_map.get(member_idx)
                    && info.member.is_static
                {
                    if info.is_decorated {
                        continue;
                    }
                    let value =
                        self.class_decorator_auto_accessor_initializer_value(info, _class_alias);
                    plain_static_field_assignments
                        .push(format!("{} = {{ value: {value} }}", info.storage_name));
                    continue;
                }
                if decorated_field_idx_set.contains(member_idx) {
                    continue;
                }
                let Some(assignment) = self.plain_static_field_assignment(
                    *member_idx,
                    member_node,
                    _class_alias,
                    indent,
                ) else {
                    continue;
                };
                plain_static_field_idx_set.insert(*member_idx);
                plain_static_field_assignments.push(assignment);
            }
        }

        // Build assignment injection map
        let mut assignment_queue: Vec<String> = Vec::new();
        let mut injected_assignments: std::collections::HashMap<NodeIndex, Vec<String>> =
            std::collections::HashMap::new();
        let mut computed_key_passthrough_sinks: std::collections::HashSet<NodeIndex> =
            std::collections::HashSet::new();
        let mut computed_key_sink_value_initializers: std::collections::HashMap<
            NodeIndex,
            Vec<String>,
        > = std::collections::HashMap::new();
        let instance_auto_accessor_storage_sink = if !self.use_static_blocks {
            auto_accessor_infos.iter().find_map(|info| {
                let member = &decorated_members[info.member_var_index];
                if member.is_static || member.is_private {
                    return None;
                }
                matches!(
                    member.name,
                    MemberName::StringLiteral(_) | MemberName::Computed(_)
                )
                .then_some(member.member_idx)
            })
        } else {
            None
        };
        let instance_auto_accessor_storage_assignments: Vec<String> =
            if instance_auto_accessor_storage_sink.is_some() {
                auto_accessor_infos
                    .iter()
                    .filter_map(|info| {
                        let member = &decorated_members[info.member_var_index];
                        (!member.is_static && !member.is_private).then(|| {
                            format!(
                                "{} = new WeakMap()",
                                self.auto_accessor_weakmap_storage_name(class_name, info)
                            )
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            };
        if let Some(member_idx) = instance_auto_accessor_storage_sink
            && !instance_auto_accessor_storage_assignments.is_empty()
        {
            injected_assignments.insert(member_idx, instance_auto_accessor_storage_assignments);
        }

        let decorated_member_indices: std::collections::HashMap<NodeIndex, usize> =
            decorated_members
                .iter()
                .enumerate()
                .map(|(i, member)| (member.member_idx, i))
                .collect();
        let mut pending_value_initializers: Vec<String> = Vec::new();
        for (member_idx, member_node) in &all_members {
            if let Some(&i) = decorated_member_indices.get(member_idx) {
                let member = &decorated_members[i];
                let var_info = &member_vars[i];
                let dec_exprs = member.decorator_exprs.join(", ");
                assignment_queue.push(format!("{} = [{}]", var_info.decorators_var, dec_exprs));
                if source_order_decorator_members.contains(&member.member_idx)
                    && let Some(extra_var) = var_info.extra_initializers_var.as_deref()
                {
                    let receiver = if member.is_static {
                        *_class_alias
                    } else {
                        "this"
                    };
                    pending_value_initializers.push(format!("{run_init}({receiver}, {extra_var})"));
                }

                let is_field_being_removed =
                    !fields_in_class_body && member.kind == MemberKind::Field;
                if propkey_map.contains_key(&member.member_idx) {
                    if let MemberName::Computed(expr_idx) = &member.name
                        && let Some((_, var_name)) =
                            computed_key_vars.iter().find(|(mi, _)| *mi == i)
                    {
                        assignment_queue.push(format!(
                            "{var_name} = {}({})",
                            self.helper("__propKey"),
                            self.node_text(*expr_idx)
                        ));
                    }
                    if !is_field_being_removed {
                        injected_assignments
                            .insert(member.member_idx, std::mem::take(&mut assignment_queue));
                    }
                }
                continue;
            }

            if !assignment_queue.is_empty()
                && self.class_member_name_is_computed(member_node)
                && source_order_decorator_members
                    .iter()
                    .any(|member_idx| decorated_member_indices.contains_key(member_idx))
            {
                injected_assignments.insert(*member_idx, std::mem::take(&mut assignment_queue));
                computed_key_passthrough_sinks.insert(*member_idx);
                if !pending_value_initializers.is_empty() {
                    computed_key_sink_value_initializers
                        .insert(*member_idx, std::mem::take(&mut pending_value_initializers));
                }
            }
        }
        let remaining_assignments = assignment_queue;
        let needs_ctor = source_ctor.is_some()
            || has_any_instance
            || has_parameter_properties
            || has_instance_fields;
        let constructor_output = if needs_ctor {
            Some(self.render_decorated_constructor(
                source_ctor.as_ref(),
                &CtorMembersCtx {
                    parameter_properties: &parameter_properties,
                    field_infos: &field_infos,
                    auto_accessor_infos: &auto_accessor_infos,
                    plain_computed_instance_fields,
                    class_decorator_instance_private_fields,
                    computed_key_sink_value_initializers: &computed_key_sink_value_initializers,
                    decorated_members,
                    member_vars,
                    source_order_decorator_members,
                },
                &CtorInitFlags {
                    fields_in_class_body,
                    has_instance_fields,
                    has_instance_auto_accessors,
                    has_instance_method,
                    has_extends: self.has_extends_clause(&class_data.heritage_clauses),
                },
                &CtorOutputCtx {
                    class_name,
                    indent,
                    inner_indent,
                    instance_extra_initializers_var,
                    instance_private_brand_var: instance_private_brand_var.as_deref(),
                },
            ))
        } else {
            None
        };

        // Emittable members: exclude constructors, index sigs, semicolons, and removed fields
        let emittable: Vec<usize> = all_members
            .iter()
            .enumerate()
            .filter(|(_, (idx, node))| {
                node.kind != syntax_kind_ext::INDEX_SIGNATURE
                    && node.kind != syntax_kind_ext::SEMICOLON_CLASS_ELEMENT
                    && (fields_in_class_body
                        || !decorated_field_idx_set.contains(idx)
                        || self
                            .decorated_member_for(*idx, decorated_members)
                            .is_some_and(|member| {
                                decorated_static_fields_emit_in_source_order && member.is_static
                            }))
                    && !plain_computed_instance_field_idx_set.contains(idx)
                    && !class_decorator_instance_private_field_idx_set.contains(idx)
                    && !plain_static_field_idx_set.contains(idx)
            })
            .map(|(i, _)| i)
            .collect();

        for &emit_i in &emittable {
            let (member_idx, member_node) = all_members[emit_i];
            if member_node.kind == syntax_kind_ext::CONSTRUCTOR {
                if let Some(output) = &constructor_output {
                    out.push_str(output);
                }
                continue;
            }
            if let Some(info) = class_decorator_static_private_method_map.get(&member_idx) {
                if info.needs_wrapper {
                    self.emit_class_decorator_static_private_wrapper(info, indent, out);
                }
                continue;
            }
            if let Some(info) = class_decorator_auto_accessor_map.get(&member_idx) {
                if info.is_decorated {
                    if self.use_static_blocks
                        && info.member.is_static
                        && let Some(accessor_info) = class_decorator_auto_accessor_infos
                            .iter()
                            .find(|class_info| class_info.member.member_idx == member_idx)
                        && let Some(member_var_index) = decorated_members
                            .iter()
                            .position(|member| member.member_idx == member_idx)
                    {
                        let var_info = &member_vars[member_var_index];
                        let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                        let init_arg = if accessor_info.initializer_text.is_empty() {
                            ", void 0".to_string()
                        } else {
                            format!(", {}", accessor_info.initializer_text)
                        };
                        let value = self
                            .previous_decorated_element_extra_initializers(
                                decorated_members,
                                member_vars,
                                member_var_index,
                            )
                            .map(|prev_extra| {
                                format!(
                                    "({run_init}({class_this_var}, {prev_extra}), {run_init}({class_this_var}, {init_var}{init_arg}))"
                                )
                            })
                            .unwrap_or_else(|| {
                                format!("{run_init}({class_this_var}, {init_var}{init_arg})")
                            });
                        out.push_str(&format!(
                            "{indent}static {{\n{inner_indent}{} = {{ value: {value} }};\n{indent}}}\n",
                            accessor_info.storage_name
                        ));
                    }
                    continue;
                }
                self.emit_class_decorator_auto_accessor_member(info, _class_alias, indent, out);
                continue;
            }
            if let Some(info) = class_decorator_static_private_field_map.get(&member_idx) {
                if info.is_decorated {
                    if self.use_static_blocks
                        && let Some(fi) = field_infos.iter().find(|field| {
                            decorated_members[field.member_var_index].member_idx == member_idx
                        })
                    {
                        let member_var_index = fi.member_var_index;
                        let var_info = &member_vars[member_var_index];
                        let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                        let init_arg = if fi.initializer_text.is_empty() {
                            ", void 0".to_string()
                        } else {
                            format!(", {}", fi.initializer_text)
                        };
                        let value = if has_static_method {
                            format!(
                                "({run_init}({class_this_var}, {static_extra_initializers_var}), {run_init}({class_this_var}, {init_var}{init_arg}))"
                            )
                        } else {
                            format!("{run_init}({class_this_var}, {init_var}{init_arg})")
                        };
                        let comment = self.leading_member_comment(member_idx);
                        out.push_str(&format!(
                            "{indent}static {{\n{}{inner_indent}{} = {{ value: {value} }};\n{indent}}}\n",
                            comment
                                .map(|comment| format!("{inner_indent}{comment}\n"))
                                .unwrap_or_default(),
                            info.storage_name
                        ));
                    }
                    continue;
                }
                if self.use_static_blocks {
                    let value = if info.initializer_text.is_empty() {
                        "void 0".to_string()
                    } else {
                        info.initializer_text.clone()
                    };
                    out.push_str(&format!(
                        "{indent}static {{\n{indent}    {} = {{ value: {value} }};\n{indent}}}\n",
                        info.storage_name
                    ));
                }
                continue;
            }
            let next_boundary = if emit_i + 1 < all_members.len() {
                all_members[emit_i + 1].1.pos as usize
            } else {
                class_close
            };
            let raw_member_text =
                if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                    let text = self
                        .static_block_texts
                        .get(&member_idx)
                        .cloned()
                        .unwrap_or_else(|| {
                            self.emit_member_bounded(member_node, next_boundary.min(class_close))
                        });
                    self.rewrite_class_decorator_static_private_accesses(
                        &text,
                        class_decorator_static_private_methods,
                        class_decorator_auto_accessor_infos,
                        class_decorator_static_private_fields,
                        class_this_var,
                    )
                } else if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                    self.static_member_texts
                        .get(&member_idx)
                        .cloned()
                        .unwrap_or_else(|| {
                            self.emit_member_bounded(member_node, next_boundary.min(class_close))
                        })
                } else {
                    self.emit_member_bounded(member_node, next_boundary.min(class_close))
                };
            let member_text = self.member_text_with_leading_comment(member_idx, &raw_member_text);

            let is_decorated_field = decorated_field_idx_set.contains(&member_idx)
                && (fields_in_class_body
                    || self
                        .decorated_member_for(member_idx, decorated_members)
                        .is_some_and(|member| {
                            decorated_static_fields_emit_in_source_order && member.is_static
                        }));
            let is_decorated_auto_accessor = decorated_auto_accessor_idx_set.contains(&member_idx);
            let private_decorated_member_index = decorated_members.iter().position(|member| {
                member.member_idx == member_idx
                    && member.is_private
                    && matches!(
                        member.kind,
                        MemberKind::Method | MemberKind::Getter | MemberKind::Setter
                    )
                    && (self.use_static_blocks || self.needs_es2015_private_descriptor(member))
            });

            if let Some(member_var_index) = private_decorated_member_index {
                let member = &decorated_members[member_var_index];
                let var_info = &member_vars[member_var_index];
                if let Some(assignments) = injected_assignments.get(&member_idx) {
                    let injected = assignments.join(", ");
                    out.push_str(&format!("{indent}static {{ {injected}; }}\n"));
                }
                if self.use_static_blocks {
                    self.emit_private_decorated_member_wrapper(member, var_info, indent, out);
                }
            } else if is_decorated_auto_accessor {
                if let Some(info) = auto_accessor_infos
                    .iter()
                    .find(|info| decorated_members[info.member_var_index].member_idx == member_idx)
                {
                    let member = &decorated_members[info.member_var_index];
                    if self.needs_es2015_private_descriptor(member) {
                        continue;
                    }
                    let var_info = &member_vars[info.member_var_index];
                    let previous_extra_initializers = if self.use_static_blocks {
                        self.previous_decorated_element_extra_initializers(
                            decorated_members,
                            member_vars,
                            info.member_var_index,
                        )
                        .or_else(|| {
                            decorated_members[info.member_var_index]
                                .is_static
                                .then_some(*static_extra_initializers_var)
                                .filter(|_| has_static_method)
                        })
                    } else {
                        self.previous_decorated_element_extra_initializers(
                            decorated_members,
                            member_vars,
                            info.member_var_index,
                        )
                        .or_else(|| {
                            self.previous_auto_accessor_extra_initializers(
                                &auto_accessor_infos,
                                decorated_members,
                                member_vars,
                                info,
                            )
                        })
                    };
                    self.emit_decorated_auto_accessor_member(
                        member,
                        info,
                        var_info,
                        &AutoAccessorMemberEmitCtx {
                            previous_extra_initializers,
                            injected_assignments: injected_assignments
                                .get(&member_idx)
                                .map(Vec::as_slice),
                            class: AutoAccessorClassCtx {
                                class_name,
                                class_alias: _class_alias,
                            },
                            indent,
                        },
                        out,
                    );
                }
            } else if is_decorated_field {
                if let Some(fi) = field_infos
                    .iter()
                    .find(|f| decorated_members[f.member_var_index].member_idx == member_idx)
                {
                    let is_static = decorated_members[fi.member_var_index].is_static;
                    let static_prefix = if is_static { "static " } else { "" };
                    let var_info = &member_vars[fi.member_var_index];
                    let init_var = var_info
                        .initializers_var
                        .as_deref()
                        .unwrap_or("_initializers");

                    // Group by static/instance for chaining
                    let same_group: Vec<usize> = field_infos
                        .iter()
                        .enumerate()
                        .filter(|(_, f)| {
                            decorated_members[f.member_var_index].is_static == is_static
                        })
                        .map(|(idx, _)| idx)
                        .collect();
                    let group_idx = same_group
                        .iter()
                        .position(|&idx| {
                            decorated_members[field_infos[idx].member_var_index].member_idx
                                == member_idx
                        })
                        .unwrap_or(0);

                    let init_arg = if fi.initializer_text.is_empty() {
                        ", void 0".to_string()
                    } else {
                        format!(", {}", fi.initializer_text)
                    };

                    let init_receiver = if is_static { *_class_alias } else { "this" };
                    let group_extra_initializers = if is_static {
                        has_static_method.then_some(*static_extra_initializers_var)
                    } else {
                        has_instance_method.then_some(*instance_extra_initializers_var)
                    };
                    let run_init_expr = if group_idx == 0 {
                        if let Some(extra_var) = group_extra_initializers {
                            format!(
                                "({run_init}({init_receiver}, {extra_var}), {run_init}({init_receiver}, {init_var}{init_arg}))"
                            )
                        } else {
                            format!("{run_init}({init_receiver}, {init_var}{init_arg})")
                        }
                    } else {
                        let prev_fi = &field_infos[same_group[group_idx - 1]];
                        let prev_extra = member_vars[prev_fi.member_var_index]
                            .extra_initializers_var
                            .as_deref()
                            .unwrap_or("_extra");
                        format!(
                            "({run_init}({init_receiver}, {prev_extra}), {run_init}({init_receiver}, {init_var}{init_arg}))"
                        )
                    };

                    self.push_leading_member_comment(member_idx, indent, out);
                    if decorated_static_fields_emit_in_source_order && is_static {
                        let lhs = if fi.is_bracket_access {
                            format!("{init_receiver}[{}]", fi.access_expr)
                        } else {
                            format!("{init_receiver}.{}", fi.access_expr)
                        };
                        out.push_str(&format!("{indent}static {{ {lhs} = {run_init_expr}; }}\n"));
                    } else if let Some(assignments) = injected_assignments.get(&member_idx) {
                        let injected = assignments.join(", ");
                        out.push_str(&format!(
                            "{indent}{static_prefix}[({injected})] = {run_init_expr};\n"
                        ));
                    } else if fi.is_bracket_access {
                        out.push_str(&format!(
                            "{indent}{static_prefix}[{}] = {run_init_expr};\n",
                            fi.access_expr
                        ));
                    } else {
                        out.push_str(&format!(
                            "{indent}{static_prefix}{} = {run_init_expr};\n",
                            fi.access_expr
                        ));
                    }
                } else {
                    push_indented_lines(out, indent, &member_text);
                }
            } else if let Some(assignments) = injected_assignments.get(&member_idx) {
                let injected = assignments.join(", ");
                if let Some(bracket_start) = member_text.find('[') {
                    let before = &member_text[..bracket_start + 1];
                    let after = &member_text[bracket_start + 1..];
                    if let Some(bracket_end) = self.find_matching_bracket(after) {
                        let key = &after[..bracket_end];
                        let rest = &after[bracket_end + 1..];
                        let injected_key = if computed_key_passthrough_sinks.contains(&member_idx) {
                            format!("({injected}, {})", key.trim())
                        } else {
                            format!("({injected})")
                        };
                        let member_text = if let Some(value_initializers) =
                            computed_key_sink_value_initializers.get(&member_idx)
                        {
                            self.member_text_with_value_initializers(
                                &format!("{before}{injected_key}]{rest}"),
                                value_initializers,
                            )
                        } else {
                            format!("{before}{injected_key}]{rest}")
                        };
                        push_indented_lines(out, indent, &member_text);
                    } else {
                        push_indented_lines(out, indent, &format!("{before}({injected})]() {{ }}"));
                    }
                } else {
                    push_indented_lines(out, indent, &member_text);
                }
            } else {
                push_indented_lines(out, indent, &member_text);
            }
        }
        if source_ctor.is_none()
            && let Some(output) = &constructor_output
        {
            out.push_str(output);
        }

        // Handle remaining assignments
        let mut external_assignments: Vec<String> = Vec::new();
        let mut post_iife_assignments: Vec<String> = Vec::new();
        post_iife_assignments.extend(plain_static_field_assignments);
        if !self.use_static_blocks {
            for info in class_decorator_instance_private_fields.iter() {
                external_assignments.push(format!("{} = new WeakMap()", info.storage_name));
            }
            for info in auto_accessor_infos
                .iter()
                .filter(|info| !decorated_members[info.member_var_index].is_static)
            {
                let member = &decorated_members[info.member_var_index];
                if instance_auto_accessor_storage_sink.is_some() && !member.is_private {
                    continue;
                }
                external_assignments.push(format!(
                    "{} = new WeakMap()",
                    self.auto_accessor_weakmap_storage_name(class_name, info)
                ));
            }
            for member in decorated_members.iter().filter(|member| {
                !member.is_static && member.is_private && member.kind == MemberKind::Field
            }) {
                external_assignments.push(format!(
                    "{} = new WeakMap()",
                    self.private_field_storage_name(class_name, member)
                ));
            }
            for info in plain_computed_instance_fields.iter() {
                let key_expr = if let Some(assignments) = injected_assignments.get(&info.member_idx)
                {
                    format!(
                        "({}, {})",
                        assignments.join(", "),
                        self.node_text(info.key_expr)
                    )
                } else {
                    self.node_text(info.key_expr)
                };
                external_assignments.push(format!("{} = {key_expr}", info.key_var));
            }
            if let Some(brand_var) = instance_private_brand_var.as_deref() {
                external_assignments.push(format!("{brand_var} = new WeakSet()"));
            }
            for (member, var_info) in decorated_members.iter().zip(member_vars.iter()) {
                if self.needs_es2015_private_descriptor(member) {
                    external_assignments.extend(
                        self.es2015_private_access_assignments(class_name, member, var_info),
                    );
                }
            }
        }
        let has_computed_method_sink = computed_key_vars.iter().any(|(mi, _)| {
            decorated_members.get(*mi).is_some_and(|m| {
                matches!(
                    m.kind,
                    MemberKind::Method | MemberKind::Getter | MemberKind::Setter
                )
            })
        });
        let es2015_class_decorators = !self.use_static_blocks && *_class_alias == "_classThis";
        let skip_sink = if self.use_static_blocks {
            !has_computed_method_sink && !decorated_members.is_empty()
        } else if es2015_class_decorators {
            true
        } else {
            if !remaining_assignments.is_empty() && !computed_key_vars.is_empty() {
                external_assignments = remaining_assignments.clone();
            }
            true
        };
        if !remaining_assignments.is_empty() && !skip_sink {
            let sink_expr = remaining_assignments.join(", ");
            let sink_is_static = decorated_members.iter().any(|m| m.is_static);
            let static_prefix = if sink_is_static { "static " } else { "" };
            out.push_str(&format!("{indent}{static_prefix}[({sink_expr})]() {{ }}\n"));
        }

        if !self.use_static_blocks {
            let static_auto_accessors: Vec<&DecoratedAutoAccessorInfo> = auto_accessor_infos
                .iter()
                .filter(|info| decorated_members[info.member_var_index].is_static)
                .collect();
            for (static_accessor_idx, info) in static_auto_accessors.iter().enumerate() {
                let member = &decorated_members[info.member_var_index];
                let var_info = &member_vars[info.member_var_index];
                let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                let init_arg = self.auto_accessor_initializer_arg(info);
                let storage_name = self.auto_accessor_weakmap_storage_name(class_name, info);
                let value = if static_accessor_idx == 0 {
                    format!("{run_init}({_class_alias}, {init_var}{init_arg})")
                } else {
                    let prev_info = static_auto_accessors[static_accessor_idx - 1];
                    let prev_extra = member_vars[prev_info.member_var_index]
                        .extra_initializers_var
                        .as_deref()
                        .unwrap_or("_extra");
                    format!(
                        "({run_init}({_class_alias}, {prev_extra}), {run_init}({_class_alias}, {init_var}{init_arg}))"
                    )
                };
                let post_indent = indent.strip_prefix("    ").unwrap_or(indent);
                let mut assignment = self
                    .leading_member_comment_block(member.member_idx, post_indent)
                    .unwrap_or_default();
                assignment.push_str(&format!("{storage_name} = {{ value: {value} }}"));
                post_iife_assignments.push(assignment);
            }
            if let Some(info) = static_auto_accessors.last()
                && let Some(extra_var) = member_vars[info.member_var_index]
                    .extra_initializers_var
                    .as_deref()
            {
                post_iife_assignments.push(format!(
                    "__EXTRA_INIT_IIFE__:{run_init}({_class_alias}, {extra_var})"
                ));
            }
        }

        // Static field initialization
        let static_fields: Vec<&DecoratedFieldInfo> = field_infos
            .iter()
            .filter(|fi| decorated_members[fi.member_var_index].is_static)
            .collect();
        let class_decorator_static_auto_accessors: Vec<&ClassDecoratorAutoAccessorInfo> =
            class_decorator_auto_accessor_infos
                .iter()
                .filter(|info| info.is_decorated && info.member.is_static && info.member.is_private)
                .collect();
        let static_field_extra_handles_class_extra = defer_class_extra_init
            && static_fields.last().is_some_and(|last_fi| {
                member_vars[last_fi.member_var_index]
                    .extra_initializers_var
                    .is_some()
                    && !self.has_following_class_decorator_auto_accessor(
                        &class_decorator_static_auto_accessors,
                        decorated_members[last_fi.member_var_index].member_idx,
                    )
                    && !self.has_following_auto_accessor_info(
                        &auto_accessor_infos,
                        decorated_members,
                        last_fi.member_var_index,
                    )
            });
        let class_decorator_auto_extra_handles_class_extra = defer_class_extra_init
            && class_decorator_static_auto_accessors
                .last()
                .and_then(|info| {
                    decorated_members
                        .iter()
                        .position(|member| member.member_idx == info.member.member_idx)
                })
                .and_then(|idx| member_vars[idx].extra_initializers_var.as_deref())
                .is_some();

        if !static_fields.is_empty() {
            if self.use_static_blocks && !self.use_define_for_class_fields {
                if let Some(last_fi) = static_fields.last()
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                    && !self.has_following_decorated_auto_accessor(
                        decorated_members,
                        last_fi.member_var_index,
                    )
                    && !self.has_following_class_decorator_auto_accessor(
                        &class_decorator_static_auto_accessors,
                        decorated_members[last_fi.member_var_index].member_idx,
                    )
                    && !self.has_following_auto_accessor_info(
                        &auto_accessor_infos,
                        decorated_members,
                        last_fi.member_var_index,
                    )
                {
                    let static_init_receiver = *_class_alias;
                    out.push_str(&format!(
                        "{indent}static {{\n{inner_indent}{run_init}({static_init_receiver}, {extra_var});\n"
                    ));
                    if defer_class_extra_init {
                        out.push_str(&format!(
                            "{inner_indent}{run_init}({class_this_var}, {class_extra_initializers_var});\n"
                        ));
                    }
                    out.push_str(&format!("{indent}}}\n"));
                }
            } else if self.use_static_blocks && self.use_define_for_class_fields {
                // ES2022 + useDefine=true: last static field's extra-initializers in static block
                if let Some(last_fi) = static_fields.last()
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                    && !self.has_following_decorated_auto_accessor(
                        decorated_members,
                        last_fi.member_var_index,
                    )
                    && !self.has_following_class_decorator_auto_accessor(
                        &class_decorator_static_auto_accessors,
                        decorated_members[last_fi.member_var_index].member_idx,
                    )
                    && !self.has_following_auto_accessor_info(
                        &auto_accessor_infos,
                        decorated_members,
                        last_fi.member_var_index,
                    )
                {
                    let static_init_receiver = *_class_alias;
                    out.push_str(&format!(
                        "{indent}static {{\n{inner_indent}{run_init}({static_init_receiver}, {extra_var});\n"
                    ));
                    if defer_class_extra_init {
                        out.push_str(&format!(
                            "{inner_indent}{run_init}({class_this_var}, {class_extra_initializers_var});\n"
                        ));
                    }
                    out.push_str(&format!("{indent}}}\n"));
                }
            } else {
                // ES2015: static field inits as comma expressions (post-IIFE)
                let class_ref = _class_alias;
                for (sf_idx, fi) in static_fields.iter().enumerate() {
                    let member = &decorated_members[fi.member_var_index];
                    let var_info = &member_vars[fi.member_var_index];
                    let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                    let init_arg = if fi.initializer_text.is_empty() {
                        ", void 0".to_string()
                    } else {
                        format!(", {}", fi.initializer_text)
                    };
                    let rhs = if sf_idx == 0 {
                        if has_static_method {
                            format!(
                                "({run_init}({class_ref}, {static_extra_initializers_var}), {run_init}({class_ref}, {init_var}{init_arg}))"
                            )
                        } else {
                            format!("{run_init}({class_ref}, {init_var}{init_arg})")
                        }
                    } else {
                        let prev_extra = member_vars[static_fields[sf_idx - 1].member_var_index]
                            .extra_initializers_var
                            .as_deref()
                            .unwrap_or("_extra");
                        format!(
                            "({run_init}({class_ref}, {prev_extra}), {run_init}({class_ref}, {init_var}{init_arg}))"
                        )
                    };
                    if member.is_private {
                        let storage_name = self.static_private_field_storage_name(
                            class_name,
                            member,
                            class_node
                                .pos
                                .try_into()
                                .ok()
                                .and_then(|start: usize| {
                                    self.source_text.map(|source| {
                                        let end = (class_node.end as usize).min(source.len());
                                        if start <= end {
                                            &source[start..end]
                                        } else {
                                            ""
                                        }
                                    })
                                })
                                .unwrap_or(""),
                        );
                        let post_indent = indent.strip_prefix("    ").unwrap_or(indent);
                        let mut assignment = self
                            .leading_member_comment_block(member.member_idx, post_indent)
                            .unwrap_or_default();
                        assignment.push_str(&format!("{storage_name} = {{ value: {rhs} }}"));
                        post_iife_assignments.push(assignment);
                        continue;
                    }
                    if self.use_define_for_class_fields {
                        let key_expr = if fi.is_bracket_access {
                            fi.access_expr.clone()
                        } else {
                            format!("\"{}\"", fi.access_expr)
                        };
                        post_iife_assignments.push(format!(
                            "Object.defineProperty({class_ref}, {key_expr}, {{\n{indent}    enumerable: true,\n{indent}    configurable: true,\n{indent}    writable: true,\n{indent}    value: {rhs}\n{indent}}})"
                        ));
                    } else {
                        let lhs = if fi.is_bracket_access {
                            format!("{class_ref}[{}]", fi.access_expr)
                        } else {
                            format!("{class_ref}.{}", fi.access_expr)
                        };
                        post_iife_assignments.push(format!("{lhs} = {rhs}"));
                    }
                }
                if let Some(last_fi) = static_fields.last()
                    && let Some(ref extra_var) =
                        member_vars[last_fi.member_var_index].extra_initializers_var
                    && !self.has_following_decorated_auto_accessor(
                        decorated_members,
                        last_fi.member_var_index,
                    )
                    && !self.has_following_class_decorator_auto_accessor(
                        &class_decorator_static_auto_accessors,
                        decorated_members[last_fi.member_var_index].member_idx,
                    )
                    && !self.has_following_auto_accessor_info(
                        &auto_accessor_infos,
                        decorated_members,
                        last_fi.member_var_index,
                    )
                {
                    let mut expr = format!("{run_init}({class_ref}, {extra_var})");
                    if defer_class_extra_init {
                        expr.push_str(&format!(
                            ";\n{indent}{run_init}({class_this_var}, {class_extra_initializers_var})"
                        ));
                    }
                    post_iife_assignments.push(format!("__EXTRA_INIT_IIFE__:{expr}"));
                }
            }
        }

        if !class_decorator_static_auto_accessors.is_empty() {
            if self.use_static_blocks {
                if let Some(info) = class_decorator_static_auto_accessors.last()
                    && let Some(member_var_index) = decorated_members
                        .iter()
                        .position(|member| member.member_idx == info.member.member_idx)
                    && let Some(extra_var) = member_vars[member_var_index]
                        .extra_initializers_var
                        .as_deref()
                {
                    out.push_str(&format!(
                        "{indent}static {{\n{inner_indent}{run_init}({class_this_var}, {extra_var});\n"
                    ));
                    if defer_class_extra_init {
                        out.push_str(&format!(
                            "{inner_indent}{run_init}({class_this_var}, {class_extra_initializers_var});\n"
                        ));
                    }
                    out.push_str(&format!("{indent}}}\n"));
                }
            } else {
                for info in &class_decorator_static_auto_accessors {
                    let Some(member_var_index) = decorated_members
                        .iter()
                        .position(|member| member.member_idx == info.member.member_idx)
                    else {
                        continue;
                    };
                    let var_info = &member_vars[member_var_index];
                    let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                    let init_arg = if info.initializer_text.is_empty() {
                        ", void 0".to_string()
                    } else {
                        format!(", {}", info.initializer_text)
                    };
                    let value = self
                        .previous_decorated_element_extra_initializers(
                            decorated_members,
                            member_vars,
                            member_var_index,
                        )
                        .map(|prev_extra| {
                            format!(
                                "({run_init}({_class_alias}, {prev_extra}), {run_init}({_class_alias}, {init_var}{init_arg}))"
                            )
                        })
                        .unwrap_or_else(|| {
                            format!("{run_init}({_class_alias}, {init_var}{init_arg})")
                        });
                    post_iife_assignments
                        .push(format!("{} = {{ value: {value} }}", info.storage_name));
                }
                if let Some(info) = class_decorator_static_auto_accessors.last()
                    && let Some(member_var_index) = decorated_members
                        .iter()
                        .position(|member| member.member_idx == info.member.member_idx)
                    && let Some(extra_var) = member_vars[member_var_index]
                        .extra_initializers_var
                        .as_deref()
                {
                    let mut expr = format!("{run_init}({_class_alias}, {extra_var})");
                    if defer_class_extra_init {
                        expr.push_str(&format!(
                            ";\n{indent}{run_init}({class_this_var}, {class_extra_initializers_var})"
                        ));
                    }
                    post_iife_assignments.push(format!("__EXTRA_INIT_IIFE__:{expr}"));
                }
            }
        }

        // ES2022 + class decorators: deferred __runInitializers static block
        if defer_class_extra_init
            && !static_field_extra_handles_class_extra
            && !class_decorator_auto_extra_handles_class_extra
        {
            if self.use_static_blocks {
                out.push_str(&format!(
                    "{indent}static {{\n{inner_indent}{run_init}({class_this_var}, {class_extra_initializers_var});\n{indent}}}\n"
                ));
            } else {
                post_iife_assignments.push(format!(
                    "__EXTRA_INIT_IIFE__:{run_init}({class_this_var}, {class_extra_initializers_var})"
                ));
            }
        }

        if self.use_static_blocks
            && let Some(info) = auto_accessor_infos
                .iter()
                .rev()
                .find(|info| decorated_members[info.member_var_index].is_static)
            && let Some(extra_var) = member_vars[info.member_var_index]
                .extra_initializers_var
                .as_deref()
        {
            out.push_str(&format!(
                "{indent}static {{\n{inner_indent}{run_init}(this, {extra_var});\n{indent}}}\n"
            ));
        }

        (external_assignments, post_iife_assignments)
    }
}
