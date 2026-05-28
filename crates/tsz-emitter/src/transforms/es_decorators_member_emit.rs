//! Auto accessor and member helper emit for TC39 decorator emission.

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
    pub(super) fn render_decorated_constructor(
        &self,
        source_ctor: Option<&ConstructorInfo>,
        members: &CtorMembersCtx<'_>,
        flags: &CtorInitFlags,
        output_ctx: &CtorOutputCtx<'_>,
    ) -> String {
        let CtorMembersCtx {
            parameter_properties,
            field_infos,
            auto_accessor_infos,
            plain_computed_instance_fields,
            class_decorator_instance_private_fields,
            computed_key_sink_value_initializers,
            decorated_members,
            member_vars,
            source_order_decorator_members,
        } = members;
        let CtorInitFlags {
            fields_in_class_body,
            has_instance_fields,
            has_instance_auto_accessors,
            has_instance_method,
            has_extends,
        } = *flags;
        let CtorOutputCtx {
            class_name,
            indent,
            inner_indent,
            instance_extra_initializers_var,
            instance_private_brand_var,
        } = output_ctx;
        let run_init = self.helper("__runInitializers");
        let parameter_properties_run_instance_initializers =
            has_instance_method && !parameter_properties.is_empty();
        let mut output = String::new();
        let mut ctor_init_calls: Vec<String> = Vec::new();

        if let Some(brand_var) = instance_private_brand_var {
            ctor_init_calls.push(format!("{inner_indent}{brand_var}.add(this);\n"));
        }

        if self.use_static_blocks && self.use_define_for_class_fields {
            for (idx, prop) in parameter_properties.iter().enumerate() {
                if idx == 0 && parameter_properties_run_instance_initializers {
                    output.push_str(&format!(
                        "{indent}{} = {run_init}(this, {instance_extra_initializers_var});\n",
                        prop.name
                    ));
                } else {
                    output.push_str(&format!("{indent}{};\n", prop.name));
                }
                ctor_init_calls.push(format!("{inner_indent}this.{0} = {0};\n", prop.name));
            }
        } else {
            for (idx, prop) in parameter_properties.iter().enumerate() {
                let value = if idx == 0 && parameter_properties_run_instance_initializers {
                    format!(
                        "({run_init}(this, {instance_extra_initializers_var}), {})",
                        prop.name
                    )
                } else {
                    prop.name.clone()
                };
                if self.use_define_for_class_fields {
                    ctor_init_calls.push(format!(
                        "{inner_indent}Object.defineProperty(this, \"{}\", {{\n{inner_indent}    enumerable: true,\n{inner_indent}    configurable: true,\n{inner_indent}    writable: true,\n{inner_indent}    value: {value}\n{inner_indent}}});\n",
                        prop.name
                    ));
                } else {
                    ctor_init_calls.push(format!("{inner_indent}this.{0} = {value};\n", prop.name));
                }
            }
        }

        if !fields_in_class_body && has_instance_fields {
            enum InstanceCtorInit {
                DecoratedField(usize),
                PlainComputed(usize),
                PrivateField(usize),
            }

            let mut instance_inits: Vec<(u32, InstanceCtorInit)> = Vec::new();
            for (fi_idx, fi) in field_infos.iter().enumerate() {
                if decorated_members[fi.member_var_index].is_static {
                    continue;
                }
                if let Some(member_node) = self
                    .arena
                    .get(decorated_members[fi.member_var_index].member_idx)
                {
                    instance_inits
                        .push((member_node.pos, InstanceCtorInit::DecoratedField(fi_idx)));
                }
            }
            for (idx, info) in plain_computed_instance_fields.iter().enumerate() {
                if let Some(member_node) = self.arena.get(info.member_idx) {
                    instance_inits.push((member_node.pos, InstanceCtorInit::PlainComputed(idx)));
                }
            }
            for (idx, info) in class_decorator_instance_private_fields.iter().enumerate() {
                if let Some(member_node) = self.arena.get(info.member_idx) {
                    instance_inits.push((member_node.pos, InstanceCtorInit::PrivateField(idx)));
                }
            }
            instance_inits.sort_by_key(|(pos, _)| *pos);

            let mut previous_decorated_field_member_var_index: Option<usize> = None;
            for (_, init) in instance_inits {
                match init {
                    InstanceCtorInit::DecoratedField(fi_idx) => {
                        let fi = &field_infos[fi_idx];
                        let var_info = &member_vars[fi.member_var_index];
                        let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                        let init_arg = if fi.initializer_text.is_empty() {
                            ", void 0".to_string()
                        } else {
                            format!(", {}", fi.initializer_text)
                        };

                        let rhs = if let Some(prev_member_var_index) =
                            previous_decorated_field_member_var_index
                        {
                            let prev_extra = member_vars[prev_member_var_index]
                                .extra_initializers_var
                                .as_deref()
                                .unwrap_or("_extra");
                            format!(
                                "({run_init}(this, {prev_extra}), {run_init}(this, {init_var}{init_arg}))"
                            )
                        } else if has_instance_method {
                            format!(
                                "({run_init}(this, {instance_extra_initializers_var}), {run_init}(this, {init_var}{init_arg}))"
                            )
                        } else {
                            format!("{run_init}(this, {init_var}{init_arg})")
                        };

                        let member = &decorated_members[fi.member_var_index];
                        let mut assignment = String::new();
                        if let Some(comment) = self.leading_member_comment(member.member_idx) {
                            assignment.push_str(inner_indent);
                            assignment.push_str(&comment);
                            assignment.push('\n');
                        }
                        if self.use_define_for_class_fields && !self.use_static_blocks {
                            let key_expr = if fi.is_bracket_access {
                                fi.access_expr.clone()
                            } else {
                                format!("\"{}\"", fi.access_expr)
                            };
                            assignment.push_str(&format!(
                                "{inner_indent}Object.defineProperty(this, {key_expr}, {{\n{inner_indent}    enumerable: true,\n{inner_indent}    configurable: true,\n{inner_indent}    writable: true,\n{inner_indent}    value: {rhs}\n{inner_indent}}});\n"
                            ));
                        } else if !self.use_static_blocks && member.is_private {
                            let storage_name = self.private_field_storage_name(class_name, member);
                            assignment.push_str(&format!(
                                "{inner_indent}{storage_name}.set(this, {rhs});\n"
                            ));
                        } else {
                            let lhs = if fi.is_bracket_access {
                                format!("this[{}]", fi.access_expr)
                            } else {
                                format!("this.{}", fi.access_expr)
                            };
                            assignment.push_str(&format!("{inner_indent}{lhs} = {rhs};\n"));
                        }
                        ctor_init_calls.push(assignment);
                        previous_decorated_field_member_var_index = Some(fi.member_var_index);
                    }
                    InstanceCtorInit::PlainComputed(info_idx) => {
                        let info = &plain_computed_instance_fields[info_idx];
                        let value = if info.initializer_text.is_empty() {
                            "void 0".to_string()
                        } else {
                            info.initializer_text.clone()
                        };
                        let rhs = if let Some(initializers) =
                            computed_key_sink_value_initializers.get(&info.member_idx)
                        {
                            let mut parts = initializers.clone();
                            parts.push(value);
                            format!("({})", parts.join(", "))
                        } else {
                            value
                        };
                        let mut assignment = String::new();
                        if let Some(comment) = self.leading_member_comment(info.member_idx) {
                            assignment.push_str(inner_indent);
                            assignment.push_str(&comment);
                            assignment.push('\n');
                        }
                        assignment
                            .push_str(&format!("{inner_indent}this[{}] = {rhs};\n", info.key_var));
                        ctor_init_calls.push(assignment);
                    }
                    InstanceCtorInit::PrivateField(info_idx) => {
                        let info = &class_decorator_instance_private_fields[info_idx];
                        let value = if info.initializer_text.is_empty() {
                            "void 0".to_string()
                        } else {
                            info.initializer_text.clone()
                        };
                        ctor_init_calls.push(format!(
                            "{inner_indent}{}.set(this, {value});\n",
                            info.storage_name
                        ));
                    }
                }
            }
            // Last instance field's extra-initializers
            if let Some(last_fi) = field_infos.iter().rev().find(|f| {
                let member = &decorated_members[f.member_var_index];
                !member.is_static && !source_order_decorator_members.contains(&member.member_idx)
            }) && let Some(ref extra_var) =
                member_vars[last_fi.member_var_index].extra_initializers_var
                && !self.has_following_decorated_auto_accessor(
                    decorated_members,
                    last_fi.member_var_index,
                )
            {
                ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
            }
        } else if fields_in_class_body && has_instance_fields {
            // Fields in class body: only last instance field's extra-initializers in constructor
            if let Some(last_fi) = field_infos.iter().rev().find(|f| {
                let member = &decorated_members[f.member_var_index];
                !member.is_static && !source_order_decorator_members.contains(&member.member_idx)
            }) && let Some(ref extra_var) =
                member_vars[last_fi.member_var_index].extra_initializers_var
                && !self.has_following_decorated_auto_accessor(
                    decorated_members,
                    last_fi.member_var_index,
                )
                && !self.has_following_auto_accessor_info(
                    auto_accessor_infos,
                    decorated_members,
                    last_fi.member_var_index,
                )
            {
                ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
            }
        } else if has_instance_method && !parameter_properties_run_instance_initializers {
            ctor_init_calls.push(format!(
                "{inner_indent}{run_init}(this, {instance_extra_initializers_var});\n"
            ));
        }

        if has_instance_auto_accessors {
            if self.use_static_blocks {
                if !self.use_define_for_class_fields {
                    for info in auto_accessor_infos
                        .iter()
                        .filter(|info| !decorated_members[info.member_var_index].is_static)
                    {
                        let var_info = &member_vars[info.member_var_index];
                        let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                        let init_arg = self.auto_accessor_initializer_arg(info);
                        let previous_extra = self
                            .previous_decorated_element_extra_initializers(
                                decorated_members,
                                member_vars,
                                info.member_var_index,
                            )
                            .or_else(|| {
                                has_instance_method.then_some(instance_extra_initializers_var)
                            });
                        let value = if let Some(prev_extra) = previous_extra {
                            format!(
                                "({run_init}(this, {prev_extra}), {run_init}(this, {init_var}{init_arg}))"
                            )
                        } else {
                            format!("{run_init}(this, {init_var}{init_arg})")
                        };
                        ctor_init_calls.push(format!(
                            "{inner_indent}this.{} = {value};\n",
                            self.native_auto_accessor_storage_name(info)
                        ));
                    }
                }
                if let Some(info) = auto_accessor_infos
                    .iter()
                    .rev()
                    .find(|info| !decorated_members[info.member_var_index].is_static)
                    && let Some(extra_var) = member_vars[info.member_var_index]
                        .extra_initializers_var
                        .as_deref()
                {
                    ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
                }
            } else {
                let instance_auto_accessors: Vec<&DecoratedAutoAccessorInfo> = auto_accessor_infos
                    .iter()
                    .filter(|info| !decorated_members[info.member_var_index].is_static)
                    .collect();
                for info in instance_auto_accessors {
                    let var_info = &member_vars[info.member_var_index];
                    let init_var = var_info.initializers_var.as_deref().unwrap_or("_init");
                    let init_arg = self.auto_accessor_initializer_arg(info);
                    let storage_name = self.auto_accessor_weakmap_storage_name(class_name, info);
                    let value = if let Some(prev_extra) = self
                        .previous_decorated_element_extra_initializers(
                            decorated_members,
                            member_vars,
                            info.member_var_index,
                        ) {
                        format!(
                            "({run_init}(this, {prev_extra}), {run_init}(this, {init_var}{init_arg}))"
                        )
                    } else {
                        format!("{run_init}(this, {init_var}{init_arg})")
                    };
                    ctor_init_calls.push(format!(
                        "{inner_indent}{storage_name}.set(this, {value});\n"
                    ));
                }
                if let Some(info) = auto_accessor_infos
                    .iter()
                    .rev()
                    .find(|info| !decorated_members[info.member_var_index].is_static)
                    && let Some(extra_var) = member_vars[info.member_var_index]
                        .extra_initializers_var
                        .as_deref()
                {
                    ctor_init_calls.push(format!("{inner_indent}{run_init}(this, {extra_var});\n"));
                }
            }
        }

        if source_ctor.is_none() && !has_extends && ctor_init_calls.is_empty() {
            return output;
        }

        output.push_str(&format!("{indent}constructor("));
        if let Some(ctor) = source_ctor {
            output.push_str(&ctor.params);
            output.push_str(") {\n");
            let split_at = if has_extends {
                ctor.body_lines
                    .iter()
                    .position(|line| line.contains("super("))
                    .map_or(0, |idx| idx + 1)
            } else {
                0
            };
            for line in &ctor.body_lines[..split_at] {
                output.push_str(&format!("{inner_indent}{}\n", line.trim()));
            }
            for call in &ctor_init_calls {
                output.push_str(call);
            }
            for line in &ctor.body_lines[split_at..] {
                output.push_str(&format!("{inner_indent}{}\n", line.trim()));
            }
            output.push_str(&format!("{indent}}}\n"));
        } else {
            // Synthetic ctor for a derived decorated class: tsc emits a
            // zero-parameter `constructor() { super(...arguments); }`, not a rest param.
            if has_extends {
                output.push_str(") {\n");
                output.push_str(&format!("{inner_indent}super(...arguments);\n"));
            } else {
                output.push_str(") {\n");
            }
            for call in &ctor_init_calls {
                output.push_str(call);
            }
            output.push_str(&format!("{indent}}}\n"));
        }
        output
    }

    pub(super) fn previous_auto_accessor_extra_initializers<'b>(
        &self,
        auto_accessor_infos: &'b [DecoratedAutoAccessorInfo],
        decorated_members: &[DecoratedMember],
        member_vars: &'b [MemberVarInfo],
        current_info: &DecoratedAutoAccessorInfo,
    ) -> Option<&'b str> {
        let current_member = &decorated_members[current_info.member_var_index];
        let mut previous: Option<&DecoratedAutoAccessorInfo> = None;
        for info in auto_accessor_infos {
            if std::ptr::eq(info, current_info) {
                break;
            }
            if decorated_members[info.member_var_index].is_static == current_member.is_static {
                previous = Some(info);
            }
        }
        previous.and_then(|info| {
            member_vars[info.member_var_index]
                .extra_initializers_var
                .as_deref()
        })
    }

    pub(super) fn previous_decorated_element_extra_initializers<'b>(
        &self,
        decorated_members: &[DecoratedMember],
        member_vars: &'b [MemberVarInfo],
        current_member_var_index: usize,
    ) -> Option<&'b str> {
        let current_member = decorated_members.get(current_member_var_index)?;
        decorated_members
            .iter()
            .enumerate()
            .take(current_member_var_index)
            .rev()
            .find(|(_, member)| {
                member.is_static == current_member.is_static
                    && matches!(member.kind, MemberKind::Field | MemberKind::Accessor)
            })
            .and_then(|(idx, _)| member_vars[idx].extra_initializers_var.as_deref())
    }

    pub(super) fn has_following_decorated_auto_accessor(
        &self,
        decorated_members: &[DecoratedMember],
        member_var_index: usize,
    ) -> bool {
        let Some(current_member) = decorated_members.get(member_var_index) else {
            return false;
        };
        decorated_members.iter().any(|member| {
            member.is_static == current_member.is_static
                && member.kind == MemberKind::Accessor
                && self.member_pos_after(member.member_idx, current_member.member_idx)
        })
    }

    pub(super) fn has_following_class_decorator_auto_accessor(
        &self,
        auto_accessors: &[&ClassDecoratorAutoAccessorInfo],
        member_idx: NodeIndex,
    ) -> bool {
        auto_accessors
            .iter()
            .any(|info| self.member_pos_after(info.member.member_idx, member_idx))
    }

    pub(super) fn has_following_auto_accessor_info(
        &self,
        auto_accessors: &[DecoratedAutoAccessorInfo],
        decorated_members: &[DecoratedMember],
        member_var_index: usize,
    ) -> bool {
        let Some(current_member) = decorated_members.get(member_var_index) else {
            return false;
        };
        auto_accessors.iter().any(|info| {
            decorated_members
                .get(info.member_var_index)
                .is_some_and(|member| {
                    member.is_static == current_member.is_static
                        && self.member_pos_after(member.member_idx, current_member.member_idx)
                })
        })
    }

    pub(super) fn member_pos_after(&self, later_idx: NodeIndex, earlier_idx: NodeIndex) -> bool {
        let Some(later) = self.arena.get(later_idx) else {
            return false;
        };
        let Some(earlier) = self.arena.get(earlier_idx) else {
            return false;
        };
        later.pos > earlier.pos
    }

    pub(super) fn is_es2015_storage_setup_assignment(&self, assignment: &str) -> bool {
        assignment.ends_with(" = new WeakMap()") || assignment.ends_with(" = new WeakSet()")
    }

    pub(super) fn decorated_member_for<'b>(
        &self,
        member_idx: NodeIndex,
        decorated_members: &'b [DecoratedMember],
    ) -> Option<&'b DecoratedMember> {
        decorated_members
            .iter()
            .find(|member| member.member_idx == member_idx)
    }

    pub(super) fn emit_decorated_auto_accessor_member(
        &self,
        member: &DecoratedMember,
        info: &DecoratedAutoAccessorInfo,
        var_info: &MemberVarInfo,
        ctx: &AutoAccessorMemberEmitCtx<'_>,
        out: &mut String,
    ) {
        let AutoAccessorMemberEmitCtx {
            previous_extra_initializers,
            injected_assignments,
            class,
            indent,
        } = ctx;
        let AutoAccessorClassCtx {
            class_name,
            class_alias,
        } = class;
        let run_init = self.helper("__runInitializers");
        let init_var = var_info
            .initializers_var
            .as_deref()
            .unwrap_or("_initializers");
        let init_arg = self.auto_accessor_initializer_arg(info);
        let getter_name = self.auto_accessor_member_name(member, info, *injected_assignments);
        let setter_name = self.auto_accessor_member_name(member, info, None);
        let static_prefix = if member.is_static { "static " } else { "" };

        if self.use_static_blocks {
            let storage_name = self.native_auto_accessor_storage_name(info);
            let value = if let Some(prev_extra) = previous_extra_initializers {
                format!("({run_init}(this, {prev_extra}), {run_init}(this, {init_var}{init_arg}))")
            } else {
                format!("{run_init}(this, {init_var}{init_arg})")
            };
            if self.use_define_for_class_fields || member.is_static {
                out.push_str(&format!(
                    "{indent}{static_prefix}{storage_name} = {value};\n"
                ));
            } else {
                out.push_str(&format!("{indent}{storage_name};\n"));
            }

            self.push_leading_member_comment(member.member_idx, indent, out);

            if member.is_private {
                let descriptor_var = var_info.descriptor_var.as_deref().unwrap_or("_descriptor");
                out.push_str(&format!(
                    "{indent}{static_prefix}get {getter_name}() {{ return {descriptor_var}.get.call(this); }}\n"
                ));
                out.push_str(&format!(
                    "{indent}{static_prefix}set {setter_name}(value) {{ return {descriptor_var}.set.call(this, value); }}\n"
                ));
                return;
            }

            if member.is_static {
                let class_ref = if class_name.is_empty() {
                    "this"
                } else {
                    class_name
                };
                out.push_str(&format!(
                    "{indent}static get {getter_name}() {{ return {class_ref}.{storage_name}; }}\n"
                ));
                out.push_str(&format!(
                    "{indent}static set {setter_name}(value) {{ {class_ref}.{storage_name} = value; }}\n"
                ));
            } else {
                out.push_str(&format!(
                    "{indent}get {getter_name}() {{ return this.{storage_name}; }}\n"
                ));
                out.push_str(&format!(
                    "{indent}set {setter_name}(value) {{ this.{storage_name} = value; }}\n"
                ));
            }
            return;
        }

        self.push_leading_member_comment(member.member_idx, indent, out);

        let storage_name = self.auto_accessor_weakmap_storage_name(class_name, info);
        let get_helper = self.helper("__classPrivateFieldGet");
        let set_helper = self.helper("__classPrivateFieldSet");
        if member.is_static {
            out.push_str(&format!(
                "{indent}static get {getter_name}() {{ return {get_helper}({class_alias}, {class_alias}, \"f\", {storage_name}); }}\n"
            ));
            out.push_str(&format!(
                "{indent}static set {setter_name}(value) {{ {set_helper}({class_alias}, {class_alias}, value, \"f\", {storage_name}); }}\n"
            ));
        } else {
            out.push_str(&format!(
                "{indent}get {getter_name}() {{ return {get_helper}(this, {storage_name}, \"f\"); }}\n"
            ));
            out.push_str(&format!(
                "{indent}set {setter_name}(value) {{ {set_helper}(this, {storage_name}, value, \"f\"); }}\n"
            ));
        }
    }

    pub(super) fn emit_class_decorator_auto_accessor_member(
        &self,
        info: &ClassDecoratorAutoAccessorInfo,
        class_ref: &str,
        indent: &str,
        out: &mut String,
    ) {
        let member = &info.member;
        let static_prefix = if member.is_static { "static " } else { "" };
        let member_name = self.auto_accessor_member_syntax(member);
        let get_helper = self.helper("__classPrivateFieldGet");
        let set_helper = self.helper("__classPrivateFieldSet");

        if self.use_static_blocks && member.is_static {
            let value = self.class_decorator_auto_accessor_initializer_value(info, class_ref);
            out.push_str(&format!(
                "{indent}static {{\n{indent}    {} = {{ value: {value} }};\n{indent}}}\n",
                info.storage_name
            ));
        }

        if member.is_private && info.getter_temp_var.is_some() && info.setter_temp_var.is_some() {
            return;
        }

        if member.is_static {
            out.push_str(&format!(
                "{indent}{static_prefix}get {member_name}() {{ return {get_helper}({class_ref}, {class_ref}, \"f\", {}); }}\n",
                info.storage_name
            ));
            out.push_str(&format!(
                "{indent}{static_prefix}set {member_name}(value) {{ {set_helper}({class_ref}, {class_ref}, value, \"f\", {}); }}\n",
                info.storage_name
            ));
        }
    }

    pub(super) fn class_decorator_static_private_temp_assignments(
        &self,
        method_infos: &[ClassDecoratorStaticPrivateMethodInfo],
        auto_accessor_infos: &[ClassDecoratorAutoAccessorInfo],
        decorated_members: &[DecoratedMember],
        member_vars: &[MemberVarInfo],
        class_ref: &str,
    ) -> String {
        self.class_decorator_static_private_temp_assignment_list(
            method_infos,
            auto_accessor_infos,
            decorated_members,
            member_vars,
            class_ref,
        )
        .join(", ")
    }

    pub(super) fn class_decorator_static_private_temp_assignment_list(
        &self,
        method_infos: &[ClassDecoratorStaticPrivateMethodInfo],
        auto_accessor_infos: &[ClassDecoratorAutoAccessorInfo],
        decorated_members: &[DecoratedMember],
        member_vars: &[MemberVarInfo],
        class_ref: &str,
    ) -> Vec<String> {
        let mut assignments: Vec<String> = method_infos
            .iter()
            .map(|info| {
                if info.is_decorated
                    && let Some(descriptor_var) = info.descriptor_var.as_deref()
                {
                    return match info.kind {
                        MemberKind::Method => format!(
                            "{} = function {}() {{ return {descriptor_var}.value; }}",
                            info.temp_var, info.function_name
                        ),
                        MemberKind::Getter => format!(
                            "{} = function {}() {{ return {descriptor_var}.get.call(this); }}",
                            info.temp_var, info.function_name
                        ),
                        MemberKind::Setter => {
                            let param = info
                                .params
                                .split(',')
                                .next()
                                .map(str::trim)
                                .unwrap_or("value");
                            let param = if param.is_empty() { "value" } else { param };
                            format!(
                                "{} = function {}({param}) {{ return {descriptor_var}.set.call(this, {param}); }}",
                                info.temp_var, info.function_name
                            )
                        }
                        MemberKind::Field | MemberKind::Accessor => String::new(),
                    };
                }
                format!(
                    "{} = function {}({}) {}",
                    info.temp_var, info.function_name, info.params, info.body
                )
            })
            .collect();

        let get_helper = self.helper("__classPrivateFieldGet");
        let set_helper = self.helper("__classPrivateFieldSet");
        for info in auto_accessor_infos {
            if !info.member.is_private {
                continue;
            }
            let (Some(getter), Some(setter)) = (
                info.getter_temp_var.as_deref(),
                info.setter_temp_var.as_deref(),
            ) else {
                continue;
            };
            if info.is_decorated
                && let Some(descriptor_var) = decorated_members
                    .iter()
                    .position(|member| member.member_idx == info.member.member_idx)
                    .and_then(|index| member_vars[index].descriptor_var.as_deref())
            {
                assignments.push(format!(
                    "{getter} = function {getter}() {{ return {descriptor_var}.get.call(this); }}"
                ));
                assignments.push(format!(
                    "{setter} = function {setter}(value) {{ return {descriptor_var}.set.call(this, value); }}"
                ));
            } else {
                assignments.push(format!(
                    "{getter} = function {getter}() {{ return {get_helper}({class_ref}, {class_ref}, \"f\", {}); }}",
                    info.storage_name
                ));
                assignments.push(format!(
                    "{setter} = function {setter}(value) {{ {set_helper}({class_ref}, {class_ref}, value, \"f\", {}); }}",
                    info.storage_name
                ));
            }
        }

        assignments
    }

    pub(super) fn emit_class_decorator_static_private_wrapper(
        &self,
        info: &ClassDecoratorStaticPrivateMethodInfo,
        indent: &str,
        out: &mut String,
    ) {
        match info.kind {
            MemberKind::Method => {
                out.push_str(&format!(
                    "{indent}static get {}() {{ return {}; }}\n",
                    info.member_name, info.temp_var
                ));
            }
            MemberKind::Getter => {
                out.push_str(&format!(
                    "{indent}static get {}() {{ return {}.call(this); }}\n",
                    info.member_name, info.temp_var
                ));
            }
            MemberKind::Setter => {
                let param = info
                    .params
                    .split(',')
                    .next()
                    .map(str::trim)
                    .unwrap_or("value");
                let param = if param.is_empty() { "value" } else { param };
                out.push_str(&format!(
                    "{indent}static set {}({param}) {{ return {}.call(this, {param}); }}\n",
                    info.member_name, info.temp_var
                ));
            }
            MemberKind::Field | MemberKind::Accessor => {}
        }
    }

    pub(super) fn rewrite_class_decorator_static_private_accesses(
        &self,
        text: &str,
        infos: &[ClassDecoratorStaticPrivateMethodInfo],
        auto_accessor_infos: &[ClassDecoratorAutoAccessorInfo],
        field_infos: &[ClassDecoratorStaticPrivateFieldInfo],
        class_ref: &str,
    ) -> String {
        if infos.is_empty() && auto_accessor_infos.is_empty() && field_infos.is_empty() {
            return text.to_string();
        }

        let mut accessors: std::collections::HashMap<String, (Option<&str>, Option<&str>)> =
            std::collections::HashMap::new();
        let mut fields: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
        for info in infos {
            let entry = accessors
                .entry(info.member_name.clone())
                .or_insert((None, None));
            match info.kind {
                MemberKind::Getter => entry.0 = Some(info.temp_var.as_str()),
                MemberKind::Setter => entry.1 = Some(info.temp_var.as_str()),
                MemberKind::Method | MemberKind::Field | MemberKind::Accessor => {}
            }
        }
        for info in auto_accessor_infos {
            if !info.member.is_private {
                continue;
            }
            let (Some(getter), Some(setter)) = (
                info.getter_temp_var.as_deref(),
                info.setter_temp_var.as_deref(),
            ) else {
                continue;
            };
            let entry = accessors
                .entry(self.auto_accessor_member_syntax(&info.member))
                .or_insert((None, None));
            entry.0 = Some(getter);
            entry.1 = Some(setter);
        }
        for info in field_infos {
            fields.insert(info.member_name.clone(), info.storage_name.as_str());
        }
        if accessors.is_empty() && fields.is_empty() {
            return text.to_string();
        }

        let get_helper = self.helper("__classPrivateFieldGet");
        let set_helper = self.helper("__classPrivateFieldSet");
        let mut out = String::new();
        for line in text.lines() {
            let trimmed = line.trim_start();
            let leading = &line[..line.len() - trimmed.len()];
            let mut rewritten = None;
            for (member_name, storage_name) in &fields {
                for receiver in [class_ref, "this"] {
                    let access = format!("{receiver}.{member_name}");
                    let read_stmt = format!("{access};");
                    if trimmed == read_stmt {
                        rewritten = Some(format!(
                            "{leading}{get_helper}({class_ref}, {class_ref}, \"f\", {storage_name});"
                        ));
                        break;
                    }
                    let assign_prefix = format!("{access} = ");
                    if let Some(value) = trimmed.strip_prefix(&assign_prefix)
                        && let Some(value) = value.strip_suffix(';')
                    {
                        rewritten = Some(format!(
                            "{leading}{set_helper}({class_ref}, {class_ref}, {}, \"f\", {storage_name});",
                            value.trim()
                        ));
                        break;
                    }
                }
                if rewritten.is_some() {
                    break;
                }
            }
            if rewritten.is_some() {
                out.push_str(rewritten.as_deref().unwrap_or(line));
                out.push('\n');
                continue;
            }
            for (member_name, (getter, setter)) in &accessors {
                for receiver in [class_ref, "this"] {
                    let access = format!("{receiver}.{member_name}");
                    if let Some(getter) = getter {
                        let read_stmt = format!("{access};");
                        if trimmed == read_stmt {
                            rewritten = Some(format!(
                                "{leading}{get_helper}({class_ref}, {class_ref}, \"a\", {getter});"
                            ));
                            break;
                        }
                    }
                    if let Some(setter) = setter {
                        let assign_prefix = format!("{access} = ");
                        if let Some(value) = trimmed.strip_prefix(&assign_prefix)
                            && let Some(value) = value.strip_suffix(';')
                        {
                            rewritten = Some(format!(
                                "{leading}{set_helper}({class_ref}, {class_ref}, {}, \"a\", {setter});",
                                value.trim()
                            ));
                            break;
                        }
                    }
                }
                if rewritten.is_some() {
                    break;
                }
            }
            out.push_str(rewritten.as_deref().unwrap_or(line));
            out.push('\n');
        }
        if text.ends_with('\n') {
            out
        } else {
            out.pop();
            out
        }
    }

    pub(super) fn auto_accessor_member_name(
        &self,
        member: &DecoratedMember,
        info: &DecoratedAutoAccessorInfo,
        injected_assignments: Option<&[String]>,
    ) -> String {
        match &member.name {
            MemberName::Computed(_) => {
                if let Some(assignments) = injected_assignments
                    && !assignments.is_empty()
                {
                    return format!("[({})]", assignments.join(", "));
                }
                format!("[{}]", info.name)
            }
            MemberName::StringLiteral(name) => {
                if let Some(assignments) = injected_assignments
                    && !assignments.is_empty()
                {
                    return format!("[({}, \"{name}\")]", assignments.join(", "));
                }
                format!("[\"{name}\"]")
            }
            _ => info.name.clone(),
        }
    }

    pub(super) fn auto_accessor_member_syntax(&self, member: &DecoratedMember) -> String {
        match &member.name {
            MemberName::Identifier(name) | MemberName::Private(name) => name.clone(),
            MemberName::StringLiteral(name) => format!("[\"{name}\"]"),
            MemberName::Computed(expr_idx) => format!("[{}]", self.node_text(*expr_idx)),
        }
    }

    pub(super) fn native_auto_accessor_storage_name(
        &self,
        info: &DecoratedAutoAccessorInfo,
    ) -> String {
        format!("#{}_accessor_storage", info.storage_base)
    }

    pub(super) fn auto_accessor_weakmap_storage_name(
        &self,
        class_name: &str,
        info: &DecoratedAutoAccessorInfo,
    ) -> String {
        let class_prefix = if class_name.is_empty() {
            "class"
        } else {
            class_name
        };
        format!("_{class_prefix}_{}_accessor_storage", info.storage_base)
    }

    pub(super) fn auto_accessor_initializer_arg(&self, info: &DecoratedAutoAccessorInfo) -> String {
        if info.initializer_text.is_empty() {
            ", void 0".to_string()
        } else {
            format!(", {}", info.initializer_text)
        }
    }

    pub(super) fn class_decorator_auto_accessor_initializer_value(
        &self,
        info: &ClassDecoratorAutoAccessorInfo,
        class_ref: &str,
    ) -> String {
        if info.initializer_idx == NodeIndex::NONE {
            "void 0".to_string()
        } else if class_ref == "_classThis" && self.node_is_this_keyword(info.initializer_idx) {
            class_ref.to_string()
        } else {
            info.initializer_text.clone()
        }
    }

    pub(super) fn member_text_with_leading_comment(
        &self,
        member_idx: NodeIndex,
        member_text: &str,
    ) -> String {
        let text = self.strip_trailing_standalone_comments(member_text);
        let Some(comment) = self.leading_member_comment(member_idx) else {
            return text;
        };
        if text.trim_start().starts_with(&comment) {
            return text;
        }
        if text.is_empty() {
            comment
        } else {
            format!("{comment}\n{text}")
        }
    }

    pub(super) fn member_text_with_value_initializers(
        &self,
        member_text: &str,
        value_initializers: &[String],
    ) -> String {
        if value_initializers.is_empty() {
            return member_text.to_string();
        }
        let Some(eq_pos) = member_text.rfind(" = ") else {
            return member_text.to_string();
        };
        let before_value = &member_text[..eq_pos + 3];
        let value = &member_text[eq_pos + 3..];
        let value = value.trim();
        let value = value.strip_suffix(';').unwrap_or(value).trim();
        let mut parts = value_initializers.to_vec();
        parts.push(value.to_string());
        format!("{before_value}({});", parts.join(", "))
    }

    pub(super) fn leading_member_comment_block(
        &self,
        member_idx: NodeIndex,
        indent: &str,
    ) -> Option<String> {
        self.leading_member_comment(member_idx)
            .map(|comment| format!("{comment}\n{indent}"))
    }

    pub(super) fn push_leading_member_comment(
        &self,
        member_idx: NodeIndex,
        indent: &str,
        out: &mut String,
    ) {
        if let Some(comment) = self.leading_member_comment(member_idx) {
            out.push_str(indent);
            out.push_str(&comment);
            out.push('\n');
        }
    }

    pub(super) fn leading_member_comment(&self, member_idx: NodeIndex) -> Option<String> {
        let member_node = self.arena.get(member_idx)?;
        let source = self.source_text?;
        let start = member_node.pos as usize;
        if start >= source.len() {
            return None;
        }

        let mut comments: Vec<String> = Vec::new();
        for line in source[..start].lines().rev() {
            let line = line.trim();
            if line.is_empty() {
                if comments.is_empty() {
                    continue;
                }
                break;
            }
            if is_comment_line(line) {
                comments.push(line.to_string());
                continue;
            }
            break;
        }
        if !comments.is_empty() {
            comments.reverse();
            return comments.into_iter().next();
        }

        let end = self.find_member_clean_start(member_node).min(source.len());
        if start < end {
            return source[start..end]
                .lines()
                .map(str::trim)
                .find(|line| is_comment_line(line))
                .map(ToOwned::to_owned);
        }

        None
    }

    pub(super) fn strip_trailing_standalone_comments(&self, text: &str) -> String {
        let mut lines: Vec<&str> = text.lines().collect();
        while lines.last().is_some_and(|line| line.trim().is_empty()) {
            lines.pop();
        }
        let mut stripped_comment = false;
        while lines
            .last()
            .is_some_and(|line| is_comment_line(line.trim()))
        {
            stripped_comment = true;
            lines.pop();
            while lines.last().is_some_and(|line| line.trim().is_empty()) {
                lines.pop();
            }
        }
        if stripped_comment {
            let stripped = lines.join("\n").trim_end().to_string();
            if let Some(prefix) = stripped.strip_suffix("{}") {
                format!("{prefix}{{ }}")
            } else {
                stripped
            }
        } else {
            text.to_string()
        }
    }
}
