//! Decorator application emit for TC39 decorator emission.

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
    /// Emit the decorator application code (metadata, __esDecorate calls, etc.)
    pub(super) fn emit_decorator_application(
        &self,
        ctx: &DecoratorApplicationCtx<'_>,
        indent: &str,
        out: &mut String,
        defer_class_extra_init: bool,
        vars: &ClassDecoratorVars<'_>,
    ) {
        let DecoratorApplicationCtx {
            decorated_members,
            member_vars,
            source_order_decorator_members,
            class_decorators,
            class_name,
            ctor_ref,
            computed_key_vars,
            class_decorator_static_private_methods,
            class_decorator_auto_accessor_infos,
            class_decorator_static_private_fields,
            has_extends,
        } = ctx;
        let ClassDecoratorVars {
            class_descriptor,
            class_this_var,
            class_super_var,
            class_decorators_var,
            class_extra_initializers_var,
            instance_extra_initializers_var,
            static_extra_initializers_var,
            metadata_var,
            metadata_super_temp_var,
        } = vars;
        let has_extends = *has_extends;
        // Metadata
        if has_extends {
            if self.use_static_blocks {
                out.push_str(&format!("{indent}const {metadata_var} = typeof Symbol === \"function\" && Symbol.metadata ? Object.create({class_super_var}[Symbol.metadata] ?? null) : void 0;\n"));
            } else {
                out.push_str(&format!("{indent}var {metadata_super_temp_var};\n"));
                out.push_str(&format!("{indent}const {metadata_var} = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(({metadata_super_temp_var} = {class_super_var}[Symbol.metadata]) !== null && {metadata_super_temp_var} !== void 0 ? {metadata_super_temp_var} : null) : void 0;\n"));
            }
        } else {
            out.push_str(&format!("{indent}const {metadata_var} = typeof Symbol === \"function\" && Symbol.metadata ? Object.create(null) : void 0;\n"));
        }

        // Emit decorator assignment expressions before __esDecorate calls when
        // assignments can't go in a computed member sink:
        // - ES2022 static blocks without computed method sinks (field-only decorators)
        // - ES2015 + class decorators (assignments go in the IIFE, not a sink member)
        let has_computed_method_sink = computed_key_vars.iter().any(|(mi, _)| {
            decorated_members.get(*mi).is_some_and(|m| {
                matches!(
                    m.kind,
                    MemberKind::Method | MemberKind::Getter | MemberKind::Setter
                )
            })
        });
        let has_computed_field_keys_app = !computed_key_vars.is_empty();
        let emit_assignments_here = if self.use_static_blocks {
            // ES2022: emit here only when no computed keys and no computed method sinks
            !has_computed_field_keys_app
                && !has_computed_method_sink
                && !decorated_members.is_empty()
        } else {
            // ES2015: assignments without a computed-name sink go in the IIFE.
            !class_decorators.is_empty()
                || (!has_computed_field_keys_app && !decorated_members.is_empty())
        };
        if emit_assignments_here {
            for (i, member) in decorated_members.iter().enumerate() {
                if source_order_decorator_members.contains(&member.member_idx) {
                    continue;
                }
                let var_info = &member_vars[i];
                let dec_exprs = member.captured_decorator_exprs.join(", ");
                out.push_str(&format!(
                    "{indent}{} = [{}];\n",
                    var_info.decorators_var, dec_exprs
                ));
            }
        }

        // __esDecorate calls for each member
        // In ES2022 static blocks, use `this` for the class ref (it IS the class in static blocks)
        let member_class_ref = if self.use_static_blocks {
            "this"
        } else {
            ctor_ref
        };
        let member_private_ref = if self.use_static_blocks && !class_decorators.is_empty() {
            *class_this_var
        } else {
            member_class_ref
        };
        for i in self.decorator_application_order(decorated_members) {
            let member = &decorated_members[i];
            let var_info = &member_vars[i];
            self.emit_es_decorate_call(
                member,
                var_info,
                &EsDecorateMemberCtx {
                    member_index: i,
                    class_alias: member_class_ref,
                    class_private_ref: member_private_ref,
                    class_name,
                    computed_key_vars,
                    class_decorator_static_private_methods,
                    class_decorator_auto_accessor_infos,
                    class_decorator_static_private_fields,
                },
                indent,
                out,
                &EsDecorateVars {
                    instance_extra_initializers_var,
                    static_extra_initializers_var,
                    metadata_var,
                },
            );
        }

        // Class-level __esDecorate if needed
        let es_decorate = self.helper("__esDecorate");
        let run_initializers = self.helper("__runInitializers");
        if !class_decorators.is_empty() {
            out.push_str(&format!("{indent}{es_decorate}(null, {class_descriptor} = {{ value: {class_this_var} }}, {class_decorators_var}, {{ kind: \"class\", name: {class_this_var}.name, metadata: {metadata_var} }}, null, {class_extra_initializers_var});\n"));
            out.push_str(&format!(
                "{indent}{class_name} = {class_this_var} = {class_descriptor}.value;\n"
            ));
        }

        // Metadata assignment
        out.push_str(&format!("{indent}if ({metadata_var}) Object.defineProperty({ctor_ref}, Symbol.metadata, {{ enumerable: true, configurable: true, writable: true, value: {metadata_var} }});\n"));

        // Static extra initializers — only for static method/getter/setter decorators
        let has_static_method_decorators = decorated_members
            .iter()
            .any(|m| m.is_static && !matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        let has_static_field_initializers = decorated_members
            .iter()
            .any(|m| m.is_static && matches!(m.kind, MemberKind::Field | MemberKind::Accessor));
        if has_static_method_decorators && !has_static_field_initializers {
            out.push_str(&format!(
                "{indent}{run_initializers}({ctor_ref}, {static_extra_initializers_var});\n"
            ));
        }

        // Class extra initializers: deferred when user static members exist.
        if !class_decorators.is_empty() && !defer_class_extra_init {
            out.push_str(&format!(
                "{indent}{run_initializers}({ctor_ref}, {class_extra_initializers_var});\n"
            ));
        }
    }
}
