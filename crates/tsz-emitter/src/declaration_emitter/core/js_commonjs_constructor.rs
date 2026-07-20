use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

use super::DeclarationEmitter;

enum JsPrototypeObjectTypeArm {
    Structured {
        members: FxHashMap<String, JsPrototypeObjectMember>,
        property_order: Vec<String>,
    },
    Rendered(String),
}

struct JsPrototypeObjectMember {
    emitted_name: String,
    readonly: bool,
    text: String,
}

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn emit_js_commonjs_constructor_prototype_class(
        &mut self,
        name_idx: NodeIndex,
    ) -> bool {
        let Some(export_name) = self.js_commonjs_export_name_text(name_idx) else {
            return false;
        };
        let prototype_members = self.js_prototype_object_members_for_export_name(&export_name);
        if prototype_members.is_empty() {
            return false;
        }

        self.write_indent();
        self.write("export class ");
        self.write(&export_name);
        self.write(" {");
        self.write_line();
        self.increase_indent();

        for member_idx in prototype_members {
            self.emit_js_prototype_object_type_member(member_idx);
        }

        self.decrease_indent();
        self.write_indent();
        self.write("}");
        self.write_line();
        self.emitted_module_indicator = true;
        true
    }

    pub(in crate::declaration_emitter) fn emit_js_prototype_object_type_member(
        &mut self,
        member_idx: NodeIndex,
    ) {
        self.emit_js_prototype_object_type_member_with_type(member_idx, None, false);
    }

    fn emit_js_prototype_object_type_member_with_type(
        &mut self,
        member_idx: NodeIndex,
        precomputed_type_text: Option<&str>,
        force_readonly: bool,
    ) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };
        let before_jsdoc_len = self.writer.len();
        let saved_comment_idx = self.comment_emit_idx;
        self.emit_leading_jsdoc_comments(member_node.pos);
        let before_member_len = self.writer.len();

        if let Some(prop) = self.arena.get_property_assignment(member_node) {
            if let Some(type_text) = precomputed_type_text.map(str::to_owned).or_else(|| {
                self.resolve_declaration_type_text(&[prop.initializer], Some(prop.initializer))
                    .map(|resolved| resolved.emitted_type_text)
                    .or_else(|| self.allowlisted_initializer_type_text(prop.initializer))
            }) {
                self.write_indent();
                if force_readonly || self.jsdoc_has_readonly_for_node(member_idx) {
                    self.write("readonly ");
                }
                self.emit_js_prototype_object_member_name(prop.name);
                self.write(": ");
                self.write(&type_text);
                self.write(";");
                self.write_line();
            }
        } else if let Some(shorthand) = self.arena.get_shorthand_property(member_node) {
            let initializer = if shorthand.object_assignment_initializer == NodeIndex::NONE {
                shorthand.name
            } else {
                shorthand.object_assignment_initializer
            };
            if let Some(type_text) = precomputed_type_text.map(str::to_owned).or_else(|| {
                self.preferred_object_member_initializer_type_text(
                    initializer,
                    self.indent_level + 1,
                )
                .or_else(|| {
                    self.resolve_declaration_type_text(&[initializer], Some(initializer))
                        .map(|resolved| resolved.emitted_type_text)
                })
                .or_else(|| self.allowlisted_initializer_type_text(initializer))
            }) {
                self.write_indent();
                if force_readonly || self.jsdoc_has_readonly_for_node(member_idx) {
                    self.write("readonly ");
                }
                self.emit_js_prototype_object_member_name(shorthand.name);
                self.write(": ");
                self.write(&type_text);
                self.write(";");
                self.write_line();
            }
        } else {
            self.emit_class_member(member_idx);
        }

        if self.writer.len() == before_member_len {
            self.writer.truncate(before_jsdoc_len);
            self.comment_emit_idx = saved_comment_idx;
            self.skip_comments_in_node(member_node.pos, member_node.end);
        }
    }

    fn emit_js_prototype_object_member_name(&mut self, name_idx: NodeIndex) {
        if let Some(name) = self.object_literal_member_name_text(name_idx) {
            self.write(&name);
        } else {
            self.emit_node(name_idx);
        }
    }

    fn emit_js_prototype_object_proto_property(
        &mut self,
        member_idx: NodeIndex,
        precomputed_type_text: Option<&str>,
    ) {
        let Some(member_node) = self.arena.get(member_idx) else {
            return;
        };
        let Some(prop) = self.arena.get_property_assignment(member_node) else {
            return;
        };
        let before_jsdoc_len = self.writer.len();
        let saved_comment_idx = self.comment_emit_idx;
        self.emit_leading_jsdoc_comments(member_node.pos);
        let before_member_len = self.writer.len();
        if let Some(type_text) = precomputed_type_text
            .map(str::to_owned)
            .or_else(|| self.js_proto_property_assignment_type_text(member_idx))
        {
            self.write_indent();
            self.emit_js_prototype_object_member_name(prop.name);
            self.write(": ");
            self.write(&type_text);
            self.write(";");
            self.write_line();
        }
        if self.writer.len() == before_member_len {
            self.writer.truncate(before_jsdoc_len);
            self.comment_emit_idx = saved_comment_idx;
            self.skip_comments_in_node(member_node.pos, member_node.end);
        }
    }

    fn emit_js_prototype_object_accessor_property(
        &mut self,
        comment_anchor_idx: NodeIndex,
        emitted_name: &str,
        getter_idx: Option<NodeIndex>,
        setter_idx: Option<NodeIndex>,
    ) {
        let type_text = getter_idx
            .and_then(|idx| self.js_prototype_object_getter_type_text(idx))
            .or_else(|| setter_idx.and_then(|idx| self.js_prototype_object_setter_type_text(idx)))
            .unwrap_or_else(|| "any".to_string());

        let Some(member_node) = self.arena.get(comment_anchor_idx) else {
            return;
        };
        self.emit_leading_jsdoc_comments(member_node.pos);
        self.write_indent();
        if getter_idx.is_some() && setter_idx.is_none() {
            self.write("readonly ");
        }
        self.write(emitted_name);
        self.write(": ");
        self.write(&type_text);
        self.write(";");
        self.write_line();

        for accessor_idx in [getter_idx, setter_idx].into_iter().flatten() {
            if let Some(accessor_node) = self.arena.get(accessor_idx)
                && let Some(accessor) = self.arena.get_accessor(accessor_node)
                && let Some(body_node) = self.arena.get(accessor.body)
            {
                self.skip_comments_in_node(body_node.pos, body_node.end);
            }
        }
    }

    fn js_prototype_object_getter_type_text(&mut self, getter_idx: NodeIndex) -> Option<String> {
        let getter_node = self.arena.get(getter_idx)?;
        let getter = self.arena.get_accessor(getter_node)?;
        self.emit_type_node_text(getter.type_annotation)
            .or_else(|| self.jsdoc_return_type_text_for_node(getter_idx))
            .or_else(|| self.jsdoc_type_text_for_node(getter_idx))
            .or_else(|| self.matching_setter_parameter_type_text(getter_idx))
            .or_else(|| {
                let type_id = self.get_node_type_or_names(&[getter_idx, getter.name])?;
                if type_id == tsz_solver::types::TypeId::ANY && getter.body.is_some() {
                    if self.body_returns_void(getter.body) {
                        return Some("void".to_string());
                    }
                    if let Some(return_text) =
                        self.function_body_preferred_return_type_text(getter.body)
                    {
                        return Some(return_text);
                    }
                }
                Some(self.print_type_id(type_id))
            })
            .or_else(|| {
                getter.body.is_some().then(|| {
                    if self.body_returns_void(getter.body) {
                        "void".to_string()
                    } else {
                        self.function_body_preferred_return_type_text(getter.body)
                            .unwrap_or_else(|| "any".to_string())
                    }
                })
            })
    }

    fn js_prototype_object_setter_type_text(&self, setter_idx: NodeIndex) -> Option<String> {
        let setter_node = self.arena.get(setter_idx)?;
        let setter = self.arena.get_accessor(setter_node)?;
        let param_idx = setter.parameters.nodes.first().copied()?;
        let param_node = self.arena.get(param_idx)?;
        let param = self.arena.get_parameter(param_node)?;
        self.emit_type_node_text(param.type_annotation)
            .or_else(|| {
                self.jsdoc_param_decl_for_parameter(param_idx, 0)
                    .map(|decl| decl.type_text)
            })
            .or_else(|| {
                self.get_node_type_or_names(&[param_idx, param.name])
                    .map(|type_id| self.print_type_id(type_id))
            })
    }

    fn js_prototype_object_member_key(&self, name_idx: NodeIndex) -> Option<String> {
        self.property_name_text_from_arena(self.arena, name_idx)
            .or_else(|| self.resolved_computed_property_name_text(name_idx))
            .or_else(|| self.object_literal_member_name_text(name_idx))
    }

    fn js_prototype_object_emitted_member_name(&self, name_idx: NodeIndex) -> Option<String> {
        self.object_literal_member_name_text(name_idx)
            .or_else(|| self.property_name_text_from_arena(self.arena, name_idx))
    }

    fn js_prototype_object_simple_member_type_text(&self, member_idx: NodeIndex) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        if let Some(prop) = self.arena.get_property_assignment(member_node) {
            return self
                .resolve_declaration_type_text(&[prop.initializer], Some(prop.initializer))
                .map(|resolved| resolved.emitted_type_text)
                .or_else(|| {
                    self.preferred_object_member_initializer_type_text(
                        prop.initializer,
                        self.indent_level + 1,
                    )
                })
                .or_else(|| self.allowlisted_initializer_type_text(prop.initializer));
        }
        let shorthand = self.arena.get_shorthand_property(member_node)?;
        let initializer = if shorthand.object_assignment_initializer == NodeIndex::NONE {
            shorthand.name
        } else {
            shorthand.object_assignment_initializer
        };
        self.local_variable_initializer_type_text(initializer)
            .or_else(|| {
                self.preferred_object_member_initializer_type_text(
                    initializer,
                    self.indent_level + 1,
                )
            })
            .or_else(|| {
                self.resolve_declaration_type_text(&[initializer], Some(initializer))
                    .map(|resolved| resolved.emitted_type_text)
            })
            .or_else(|| self.allowlisted_initializer_type_text(initializer))
    }

    fn js_prototype_object_scratch_emitter(
        &self,
        depth: u32,
        comment_floor: u32,
        comment_ceiling: u32,
        emitted_comment_floor: u32,
    ) -> DeclarationEmitter<'a> {
        let mut scratch = self.scratch_declaration_emitter();
        scratch.indent_level = depth + 1;
        scratch.set_remove_comments(self.remove_comments);
        scratch.set_strip_internal(self.strip_internal);
        scratch.strict_null_checks = self.strict_null_checks;

        // A scratch member needs its own JSDoc for signature inference and
        // output, but copying the file-wide comment table once per property
        // turns a large prototype object into quadratic work. Comments are
        // source ordered, so retain only the interval owned by this member (or
        // accessor pair) and start output at the selected accessor's interval.
        let comment_start = self
            .all_comments
            .partition_point(|comment| comment.end <= comment_floor);
        let comment_end = self
            .all_comments
            .partition_point(|comment| comment.end <= comment_ceiling);
        scratch.all_comments = self.all_comments[comment_start..comment_end].to_vec();
        scratch.comment_emit_idx = scratch
            .all_comments
            .partition_point(|comment| comment.end <= emitted_comment_floor);
        scratch
    }

    fn js_prototype_object_member_has_leading_jsdoc(
        &self,
        member_idx: NodeIndex,
        comment_floor: u32,
    ) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let Some(source) = self.source_file_text.as_deref() else {
            return false;
        };
        let comment_start = self
            .all_comments
            .partition_point(|comment| comment.end <= comment_floor);
        let comment_end = self
            .all_comments
            .partition_point(|comment| comment.end <= member_node.pos);
        self.all_comments[comment_start..comment_end]
            .iter()
            .any(|comment| {
                source
                    .get(comment.pos as usize..comment.end as usize)
                    .is_some_and(|text| text.starts_with("/**") && text != "/**/")
            })
    }

    fn js_prototype_object_rendered_type_text(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<String> {
        self.resolve_declaration_type_text(&[initializer], Some(initializer))
            .map(|resolved| resolved.emitted_type_text)
            .or_else(|| self.preferred_object_member_initializer_type_text(initializer, depth))
            .or_else(|| self.allowlisted_initializer_type_text(initializer))
    }

    fn js_prototype_object_type_arm(
        &self,
        initializer: NodeIndex,
        depth: u32,
    ) -> Option<JsPrototypeObjectTypeArm> {
        let initializer_node = self.arena.get(initializer)?;
        let Some(object) = self.arena.get_literal_expr(initializer_node) else {
            return self
                .js_prototype_object_rendered_type_text(initializer, depth)
                .map(JsPrototypeObjectTypeArm::Rendered);
        };

        // Object spread needs the existing declaration-inference projection so
        // spread sources are flattened, collisions are resolved, and readonly
        // source members become mutable on the new object.
        if object.elements.nodes.iter().any(|&member_idx| {
            self.arena
                .get(member_idx)
                .is_some_and(|member| member.kind == syntax_kind_ext::SPREAD_ASSIGNMENT)
        }) {
            return self
                .infer_object_literal_type_text_at(initializer, depth)
                .or_else(|| {
                    self.get_node_type_or_names(&[initializer])
                        .map(|type_id| self.print_type_id_for_inferred_declaration(type_id))
                })
                .map(JsPrototypeObjectTypeArm::Rendered);
        }

        let mut accessor_pairs =
            FxHashMap::<String, (Option<NodeIndex>, Option<NodeIndex>)>::default();
        let mut preceding_member_end = FxHashMap::<NodeIndex, u32>::default();
        let mut preceding_end = initializer_node.pos;
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            preceding_member_end.insert(member_idx, preceding_end);
            preceding_end = member_node.end;
            if member_node.kind != syntax_kind_ext::GET_ACCESSOR
                && member_node.kind != syntax_kind_ext::SET_ACCESSOR
            {
                continue;
            }
            let name_idx = self.get_member_name_idx(member_idx)?;
            let name = self.js_prototype_object_member_key(name_idx)?;
            let pair = accessor_pairs.entry(name).or_default();
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR {
                pair.0 = Some(member_idx);
            } else {
                pair.1 = Some(member_idx);
            }
        }
        // A complementary getter/setter pair is anchored at the earlier of
        // the selected declarations. Repeated same-kind accessors are anchored
        // at the last declaration of that kind, matching the symbol order that
        // TypeScript exposes in declaration output.
        let accessor_emission_indices = accessor_pairs
            .iter()
            .filter_map(|(name, &(getter_idx, setter_idx))| {
                let emission_idx = match (getter_idx, setter_idx) {
                    (Some(getter_idx), Some(setter_idx)) => {
                        let getter_pos = self.arena.get(getter_idx)?.pos;
                        let setter_pos = self.arena.get(setter_idx)?.pos;
                        if getter_pos <= setter_pos {
                            getter_idx
                        } else {
                            setter_idx
                        }
                    }
                    (Some(getter_idx), None) => getter_idx,
                    (None, Some(setter_idx)) => setter_idx,
                    (None, None) => return None,
                };
                Some((name.clone(), emission_idx))
            })
            .collect::<FxHashMap<_, _>>();

        let mut members = FxHashMap::default();
        let mut property_order = Vec::new();
        for &member_idx in &object.elements.nodes {
            let member_node = self.arena.get(member_idx)?;
            let member_comment_floor = preceding_member_end
                .get(&member_idx)
                .copied()
                .unwrap_or(initializer_node.pos);

            let (name, emitted_name, readonly, scratch) = if let Some(prop) =
                self.arena.get_property_assignment(member_node)
            {
                let name = self.js_prototype_object_member_key(prop.name)?;
                let emitted_name = self.js_prototype_object_emitted_member_name(prop.name)?;
                let force_readonly =
                    accessor_pairs
                        .get(&name)
                        .is_some_and(|(getter_idx, setter_idx)| {
                            getter_idx.is_some() && setter_idx.is_none()
                        });
                let mut scratch = self.js_prototype_object_scratch_emitter(
                    depth,
                    member_comment_floor,
                    member_node.pos,
                    member_comment_floor,
                );
                let type_text = self.js_prototype_object_simple_member_type_text(member_idx);
                if name == "__proto__" {
                    scratch
                        .emit_js_prototype_object_proto_property(member_idx, type_text.as_deref());
                } else {
                    scratch.emit_js_prototype_object_type_member_with_type(
                        member_idx,
                        type_text.as_deref(),
                        force_readonly,
                    );
                }
                let readonly = scratch.jsdoc_has_readonly_for_node(member_idx);
                (name, emitted_name, readonly, scratch)
            } else if let Some(shorthand) = self.arena.get_shorthand_property(member_node) {
                let name = self.js_prototype_object_member_key(shorthand.name)?;
                let emitted_name = self.js_prototype_object_emitted_member_name(shorthand.name)?;
                let force_readonly =
                    accessor_pairs
                        .get(&name)
                        .is_some_and(|(getter_idx, setter_idx)| {
                            getter_idx.is_some() && setter_idx.is_none()
                        });
                let mut scratch = self.js_prototype_object_scratch_emitter(
                    depth,
                    member_comment_floor,
                    member_node.pos,
                    member_comment_floor,
                );
                let type_text = self.js_prototype_object_simple_member_type_text(member_idx);
                scratch.emit_js_prototype_object_type_member_with_type(
                    member_idx,
                    type_text.as_deref(),
                    force_readonly,
                );
                let readonly = scratch.jsdoc_has_readonly_for_node(member_idx);
                (name, emitted_name, readonly, scratch)
            } else if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR
            {
                let name_idx = self.get_member_name_idx(member_idx)?;
                let name = self.js_prototype_object_member_key(name_idx)?;
                if accessor_emission_indices.get(&name).copied() != Some(member_idx) {
                    continue;
                }
                let (getter_idx, setter_idx) =
                    accessor_pairs.get(&name).copied().unwrap_or_default();
                let emitted_names = [getter_idx, setter_idx]
                    .into_iter()
                    .flatten()
                    .filter_map(|idx| {
                        self.get_member_name_idx(idx)
                            .and_then(|idx| self.js_prototype_object_emitted_member_name(idx))
                    })
                    .collect::<Vec<_>>();
                let emitted_name = match emitted_names.as_slice() {
                    [] => self.js_prototype_object_emitted_member_name(name_idx)?,
                    [only] => only.clone(),
                    [first, rest @ ..] if rest.iter().all(|name| name == first) => first.clone(),
                    _ => Self::format_property_name_literal_text(&name),
                };
                let accessor_indices = [getter_idx, setter_idx];
                let comment_floor = accessor_indices
                    .into_iter()
                    .flatten()
                    .filter_map(|idx| preceding_member_end.get(&idx).copied())
                    .min()
                    .unwrap_or(member_comment_floor);
                let comment_ceiling = accessor_indices
                    .into_iter()
                    .flatten()
                    .filter_map(|idx| self.arena.get(idx).map(|node| node.pos))
                    .max()
                    .unwrap_or(member_node.pos);
                let comment_anchor_idx = accessor_indices
                    .into_iter()
                    .flatten()
                    .find(|idx| {
                        self.js_prototype_object_member_has_leading_jsdoc(
                            *idx,
                            preceding_member_end
                                .get(idx)
                                .copied()
                                .unwrap_or(comment_floor),
                        )
                    })
                    .or(getter_idx)
                    .or(setter_idx)
                    .unwrap_or(member_idx);
                let emitted_comment_floor = preceding_member_end
                    .get(&comment_anchor_idx)
                    .copied()
                    .unwrap_or(comment_floor);
                let mut scratch = self.js_prototype_object_scratch_emitter(
                    depth,
                    comment_floor,
                    comment_ceiling,
                    emitted_comment_floor,
                );
                scratch.emit_js_prototype_object_accessor_property(
                    comment_anchor_idx,
                    &emitted_name,
                    getter_idx,
                    setter_idx,
                );
                (
                    name,
                    emitted_name,
                    getter_idx.is_some() && setter_idx.is_none(),
                    scratch,
                )
            } else {
                let name_idx = self.get_member_name_idx(member_idx)?;
                let name = self.js_prototype_object_member_key(name_idx)?;
                let emitted_name = self.js_prototype_object_emitted_member_name(name_idx)?;
                let mut scratch = self.js_prototype_object_scratch_emitter(
                    depth,
                    member_comment_floor,
                    member_node.pos,
                    member_comment_floor,
                );
                scratch.emit_js_prototype_object_type_member(member_idx);
                let readonly = scratch.jsdoc_has_readonly_for_node(member_idx);
                (name, emitted_name, readonly, scratch)
            };

            let text = scratch.writer.take_output();
            if !text.trim().is_empty() {
                // TypeScript retains getter-only readonly semantics even when
                // a later data declaration supplies the property's final type.
                let readonly = readonly
                    || accessor_pairs
                        .get(&name)
                        .is_some_and(|(getter_idx, setter_idx)| {
                            getter_idx.is_some() && setter_idx.is_none()
                        });
                if !members.contains_key(&name) {
                    property_order.push(name.clone());
                }
                members.insert(
                    name,
                    JsPrototypeObjectMember {
                        emitted_name,
                        readonly,
                        text,
                    },
                );
            }
        }

        Some(JsPrototypeObjectTypeArm::Structured {
            members,
            property_order,
        })
    }

    fn render_js_prototype_object_structured_arm(
        members: &FxHashMap<String, JsPrototypeObjectMember>,
        property_order: &[String],
        property_templates: &FxHashMap<String, (String, bool)>,
        depth: u32,
    ) -> String {
        if members.is_empty() && property_order.is_empty() {
            return "{}".to_string();
        }

        let mut output = String::from("{\n");
        let member_indent = "    ".repeat((depth + 1) as usize);
        for name in property_order {
            if let Some(member) = members.get(name) {
                output.push_str(member.text.trim_end());
                output.push('\n');
            } else {
                output.push_str(&member_indent);
                let (emitted_name, readonly) = property_templates
                    .get(name)
                    .map(|(emitted_name, readonly)| (emitted_name.as_str(), *readonly))
                    .unwrap_or((name.as_str(), false));
                if readonly {
                    output.push_str("readonly ");
                }
                output.push_str(emitted_name);
                output.push_str("?: undefined;\n");
            }
        }
        output.push_str(&"    ".repeat(depth as usize));
        output.push('}');
        output
    }

    fn js_prototype_object_union_type_text(
        &self,
        initializers: &[NodeIndex],
        depth: u32,
    ) -> Option<String> {
        let arms = initializers
            .iter()
            .copied()
            .map(|initializer| self.js_prototype_object_type_arm(initializer, depth))
            .collect::<Option<Vec<_>>>()?;

        if arms
            .iter()
            .all(|arm| matches!(arm, JsPrototypeObjectTypeArm::Structured { .. }))
        {
            let mut property_order = Vec::new();
            let mut seen_properties = FxHashSet::default();
            let mut property_templates = FxHashMap::<String, (String, bool)>::default();
            for arm in &arms {
                let JsPrototypeObjectTypeArm::Structured {
                    members,
                    property_order: arm_property_order,
                } = arm
                else {
                    unreachable!();
                };
                for name in arm_property_order {
                    if seen_properties.insert(name.clone()) {
                        property_order.push(name.clone());
                    }
                    if let Some(member) = members.get(name) {
                        property_templates
                            .entry(name.clone())
                            .and_modify(|(_, readonly)| *readonly &= member.readonly)
                            .or_insert_with(|| (member.emitted_name.clone(), member.readonly));
                    }
                }
            }

            let mut distinct = Vec::new();
            let mut seen_arms = FxHashSet::default();
            for arm in &arms {
                let JsPrototypeObjectTypeArm::Structured { members, .. } = arm else {
                    unreachable!();
                };
                let rendered = Self::render_js_prototype_object_structured_arm(
                    members,
                    &property_order,
                    &property_templates,
                    depth,
                );
                if seen_arms.insert(rendered.clone()) {
                    distinct.push(rendered);
                }
            }
            return (!distinct.is_empty()).then(|| distinct.join(" | "));
        }

        let rendered = arms
            .into_iter()
            .map(|arm| match arm {
                JsPrototypeObjectTypeArm::Structured {
                    members,
                    property_order,
                } => Self::render_js_prototype_object_structured_arm(
                    &members,
                    &property_order,
                    &FxHashMap::default(),
                    depth,
                ),
                JsPrototypeObjectTypeArm::Rendered(text) => {
                    Self::parenthesize_type_text_in_array_element_position(&text)
                }
            })
            .collect::<Vec<_>>();
        Self::normalized_object_literal_union_text(rendered)
    }

    /// Plan the combined prototype assignment shape once. The namespace event
    /// scheduler can then reuse this text for every source-ordered assignment.
    pub(in crate::declaration_emitter) fn js_prototype_object_namespace_type_text(
        &self,
        initializers: &[NodeIndex],
    ) -> Option<String> {
        self.js_prototype_object_union_type_text(initializers, self.indent_level)
    }

    pub(in crate::declaration_emitter) fn emit_js_prototype_object_namespace_member_type_text(
        &mut self,
        type_text: &str,
        emit_export: bool,
    ) {
        self.write_indent();
        if emit_export {
            self.write("export ");
        }
        self.write("var prototype: ");
        self.write(type_text);
        self.write(";");
        self.write_line();
    }

    pub(super) fn js_prototype_object_members_for_export_name(&self, name: &str) -> Vec<NodeIndex> {
        let Some(initializer) = self.js_prototype_object_initializer_for_export_name(name) else {
            return Vec::new();
        };
        let Some(object) = self
            .arena
            .get(initializer)
            .and_then(|node| self.arena.get_literal_expr(node))
        else {
            return Vec::new();
        };
        object.elements.nodes.clone()
    }

    pub(in crate::declaration_emitter) fn js_prototype_object_initializer_for_export_name(
        &self,
        name: &str,
    ) -> Option<NodeIndex> {
        self.js_prototype_object_initializers_for_export_name(name)
            .into_iter()
            .next()
    }

    pub(in crate::declaration_emitter) fn js_prototype_object_initializers_for_export_name(
        &self,
        name: &str,
    ) -> Vec<NodeIndex> {
        self.js_prototype_object_initializers
            .get(name)
            .cloned()
            .unwrap_or_default()
    }

    pub(in crate::declaration_emitter) fn collect_js_prototype_object_initializers(
        &self,
        source_file: &tsz_parser::parser::node::SourceFileData,
    ) -> FxHashMap<String, Vec<NodeIndex>> {
        let mut initializers = FxHashMap::<String, Vec<NodeIndex>>::default();
        if !self.source_is_js_file {
            return initializers;
        }
        for &stmt_idx in &source_file.statements.nodes {
            for (receiver_name, initializer) in
                self.js_prototype_object_assignments_for_statement(stmt_idx)
            {
                initializers
                    .entry(receiver_name)
                    .or_default()
                    .push(initializer);
            }
        }
        initializers
    }

    fn js_prototype_object_assignments_for_statement(
        &self,
        stmt_idx: NodeIndex,
    ) -> Vec<(String, NodeIndex)> {
        let Some(stmt_node) = self.arena.get(stmt_idx) else {
            return Vec::new();
        };
        if stmt_node.kind != syntax_kind_ext::EXPRESSION_STATEMENT {
            return Vec::new();
        }
        let Some(expr_stmt) = self.arena.get_expression_statement(stmt_node) else {
            return Vec::new();
        };
        let mut current = self
            .arena
            .skip_parenthesized_and_assertions_and_comma(expr_stmt.expression);
        let mut receiver_names = Vec::new();
        loop {
            let Some(expr_node) = self.arena.get(current) else {
                return Vec::new();
            };
            if expr_node.kind != syntax_kind_ext::BINARY_EXPRESSION {
                break;
            }
            let Some(binary) = self.arena.get_binary_expr(expr_node) else {
                return Vec::new();
            };
            if binary.operator_token != SyntaxKind::EqualsToken as u16 {
                break;
            }
            if let Some(receiver_name) =
                self.js_prototype_object_assignment_receiver_name(binary.left)
            {
                receiver_names.push(receiver_name);
            }
            current = self
                .arena
                .skip_parenthesized_and_assertions_and_comma(binary.right);
        }

        receiver_names
            .into_iter()
            .map(|receiver_name| (receiver_name, current))
            .collect()
    }

    fn js_prototype_object_assignment_receiver_name(&self, lhs: NodeIndex) -> Option<String> {
        let lhs = self.arena.skip_parenthesized_and_assertions_and_comma(lhs);
        let lhs_node = self.arena.get(lhs)?;
        if lhs_node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && lhs_node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let lhs_access = self.arena.get_access_expr(lhs_node)?;
        if self
            .js_prototype_object_member_key(lhs_access.name_or_argument)
            .as_deref()
            != Some("prototype")
        {
            return None;
        }
        self.get_identifier_text(lhs_access.expression)
            .or_else(|| self.module_exports_property_reference_name(lhs_access.expression))
    }
}
