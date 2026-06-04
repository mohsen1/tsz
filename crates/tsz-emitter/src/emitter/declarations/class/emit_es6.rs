use super::super::super::{Printer, ScriptTarget};
use super::AutoAccessorInfo;
use super::duplicate_private_names::{
    PrivateDuplicateConflictPlan, collect_private_duplicate_conflicts,
};
use super::emit_es6_after_body::ClassEs6AfterBody;
use super::emit_es6_field_inits::ClassFieldInitCollection;
use super::emit_es6_members::ClassEs6MemberEmit;
use super::emit_es6_options::ClassEs6EmitOptions;
use super::emit_es6_private_accessors::{
    PrivateAutoAccessorInfo, collect_private_auto_accessors_with_reserved,
};
use crate::emitter::core::{
    PrivateFieldStorageKind, PrivateMemberInfo, PrivateMethodDef, StaticPrivateInit,
};
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, PrivateMethodInfo,
    collect_enclosing_source_binding_names, collect_private_members_with_reserved,
    get_private_field_name, is_private_identifier, make_unique_private_name, private_helper_base,
};
use rustc_hash::FxHashMap;
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::syntax::transform_utils::{
    contains_async_arrow_function, contains_super_reference, contains_this_reference,
};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_class_es6_with_options(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        suppress_modifiers: bool,
        assignment_prefix: Option<(&str, String)>,
        assignment_alias: Option<&str>,
        static_initializer_self_alias: Option<&str>,
        emit_assignment_static_elements_as_statements: bool,
    ) {
        self.emit_class_es6_with_emit_options(
            node,
            _idx,
            ClassEs6EmitOptions {
                suppress_modifiers,
                assignment_prefix,
                assignment_alias,
                static_initializer_self_alias,
                emit_assignment_static_elements_as_statements,
                assignment_suffix: None,
            },
        );
    }

    pub(in crate::emitter) fn emit_class_es6_assignment_with_suffix(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        assignment_target: String,
        assignment_suffix: &str,
    ) {
        self.emit_class_es6_with_emit_options(
            node,
            _idx,
            ClassEs6EmitOptions {
                suppress_modifiers: false,
                assignment_prefix: Some(("", assignment_target)),
                assignment_alias: None,
                static_initializer_self_alias: None,
                emit_assignment_static_elements_as_statements: false,
                assignment_suffix: Some(assignment_suffix),
            },
        );
    }

    fn emit_class_es6_with_emit_options(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        options: ClassEs6EmitOptions<'_>,
    ) {
        let ClassEs6EmitOptions {
            suppress_modifiers,
            assignment_prefix,
            assignment_alias,
            static_initializer_self_alias,
            emit_assignment_static_elements_as_statements,
            assignment_suffix,
        } = options;
        let Some(class) = self.arena.get_class(node) else {
            return;
        };
        let class_name_is_real = class.name.is_some();
        let class_name = if class.name.is_none() {
            assignment_prefix
                .as_ref()
                .map(|(_, binding_name)| binding_name.clone())
                .or_else(|| self.anonymous_default_export_name.clone())
                .or_else(|| {
                    // For anonymous class expressions used as variable initializers
                    // (e.g. `const C = class { #field... }`), resolve the binding name
                    // from the parent VariableDeclaration node. This is needed for
                    // private field WeakMap naming (e.g., `_C_field`).
                    if node.kind == syntax_kind_ext::CLASS_EXPRESSION {
                        self.resolve_class_expr_binding_name(_idx)
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else {
            self.get_identifier_text_idx(class.name)
        };

        self.emit_recovered_top_level_accessor_class_modifier(&class.modifiers, suppress_modifiers);

        if suppress_modifiers
            && self.ctx.options.legacy_decorators
            && let Some(ref modifiers) = class.modifiers
        {
            for &mod_idx in &modifiers.nodes {
                let Some(mod_node) = self.arena.get(mod_idx) else {
                    continue;
                };
                if mod_node.kind == syntax_kind_ext::DECORATOR {
                    self.skip_comments_for_erased_node(mod_node);
                }
            }
        }

        let emit_invalid_namespace_static =
            self.should_emit_invalid_namespace_static_modifier(node, &class.modifiers);
        if emit_invalid_namespace_static {
            self.write("static ");
        }

        // Emit modifiers (including decorators) - skip TS-only modifiers for JS output
        if !suppress_modifiers && let Some(ref modifiers) = class.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
                    if self.should_preserve_native_decorator_comments(&class.modifiers) {
                        self.emit_comments_before_pos(mod_node.pos);
                    }
                    if emit_invalid_namespace_static
                        && mod_node.kind == SyntaxKind::StaticKeyword as u16
                    {
                        continue;
                    }
                    // Skip export/default modifiers in CommonJS mode or namespace IIFE
                    if (self.ctx.is_commonjs() || self.in_namespace_iife)
                        && (mod_node.kind == SyntaxKind::ExportKeyword as u16
                            || mod_node.kind == SyntaxKind::DefaultKeyword as u16)
                    {
                        continue;
                    }
                    // Skip TypeScript-only modifiers (abstract, declare, etc.)
                    // Also skip `async` — it's an error on class declarations but
                    // TSC still emits the class without the modifier.
                    if mod_node.kind == SyntaxKind::AbstractKeyword as u16
                        || mod_node.kind == SyntaxKind::DeclareKeyword as u16
                        || mod_node.kind == SyntaxKind::AsyncKeyword as u16
                        || (self.ctx.options.legacy_decorators
                            && mod_node.kind == syntax_kind_ext::DECORATOR)
                    {
                        if self.ctx.options.legacy_decorators
                            && mod_node.kind == syntax_kind_ext::DECORATOR
                        {
                            self.skip_comments_for_erased_node(mod_node);
                        }
                        continue;
                    }
                    if mod_node.kind == SyntaxKind::ExportKeyword as u16 {
                        self.write("export");
                    } else if mod_node.kind == SyntaxKind::DefaultKeyword as u16 {
                        self.write("default");
                    } else if mod_node.kind == SyntaxKind::AccessorKeyword as u16
                        && self.ctx.options.target == ScriptTarget::ESNext
                    {
                        self.write("accessor");
                    } else {
                        self.emit(mod_idx);
                    }
                    // Add space or newline after decorator
                    if mod_node.kind == syntax_kind_ext::DECORATOR {
                        self.write_line();
                    } else {
                        self.write_space();
                    }
                }
            }
        }

        let target_needs_field_lowering = (self.ctx.options.target as u32)
            < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;

        let default_export_set_function_name_temp = if self.ctx.options.legacy_decorators
            && class.name.is_none()
            && assignment_prefix.as_ref().is_some_and(|(_, binding_name)| {
                self.anonymous_default_export_name
                    .as_deref()
                    .is_some_and(|default_name| default_name == binding_name)
            })
            && !self.collect_class_decorators(&class.modifiers).is_empty()
            && target_needs_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|member_node| {
                    member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self
                            .arena
                            .get_property_decl(member_node)
                            .is_some_and(|prop| {
                                self.arena.is_static(&prop.modifiers)
                                    && !self
                                        .arena
                                        .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                                    && !self
                                        .arena
                                        .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                                    && !prop.initializer.is_none()
                                    && self.class_property_initializer_has_equals(member_node, prop)
                            })
                })
            }) {
            Some(self.make_unique_name_hoisted())
        } else {
            None
        };

        if let Some((keyword, binding_name)) = assignment_prefix.as_ref() {
            if !keyword.is_empty() {
                self.write(keyword);
                self.write(" ");
            }
            self.write(binding_name);
            self.write(" = ");
            self.write_outer_alias_prefix(node, assignment_alias);
            if let Some(temp) = default_export_set_function_name_temp.as_ref() {
                self.write(temp);
                self.write(" = ");
            }
        }

        // Collect `accessor` fields to lower using one of two strategies:
        // - ES2022+ (except ESNext): emit native private storage + getter/setter.
        // - < ES2022: emit WeakMap-backed getter/setter pairs.
        let auto_accessor_target = self.ctx.options.target;
        let has_order_sensitive_instance_field_initializer = target_needs_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                let Some(member_node) = self.arena.get(member_idx) else {
                    return false;
                };
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    return false;
                }
                let Some(prop) = self.arena.get_property_decl(member_node) else {
                    return false;
                };
                prop.initializer.is_some()
                    && self.class_property_initializer_has_equals(member_node, prop)
                    && !self.has_effective_static_modifier_js(&prop.modifiers)
                    && !self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                    && !self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                    && !self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                    && !is_private_identifier(self.arena, prop.name)
            });
        let auto_accessor_target_supports_native_private_fields = auto_accessor_target
            == ScriptTarget::ESNext
            || (auto_accessor_target as u32) >= (ScriptTarget::ES2022 as u32);
        let lower_auto_accessors_to_private_fields =
            auto_accessor_target_supports_native_private_fields
                && (auto_accessor_target != ScriptTarget::ESNext
                    || has_order_sensitive_instance_field_initializer);
        let lower_auto_accessors_to_weakmap = auto_accessor_target != ScriptTarget::ESNext
            && (auto_accessor_target as u32) < (ScriptTarget::ES2022 as u32);
        let hoist_native_instance_order_inits = lower_auto_accessors_to_private_fields
            && has_order_sensitive_instance_field_initializer
            && !self.ctx.options.use_define_for_class_fields;

        let mut auto_accessor_members: Vec<AutoAccessorInfo> = Vec::new();
        let mut auto_accessor_instance_inits: Vec<(String, Option<NodeIndex>)> = Vec::new();
        let mut auto_accessor_static_inits: Vec<(String, Option<NodeIndex>)> = Vec::new();
        let mut auto_accessor_class_alias: Option<String> = None;
        let mut private_names_for_auto_accessors: Vec<String> = Vec::new();
        if lower_auto_accessors_to_private_fields {
            let mut nodes_to_visit: Vec<NodeIndex> = class.members.nodes.clone();
            while let Some(member_idx) = nodes_to_visit.pop() {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                if member_node.kind == syntax_kind_ext::CLASS_DECLARATION
                    || member_node.kind == syntax_kind_ext::CLASS_EXPRESSION
                {
                    continue;
                }
                if member_node.kind == SyntaxKind::PrivateIdentifier as u16
                    && let Some(name) = get_private_field_name(self.arena, member_idx)
                {
                    private_names_for_auto_accessors.push(name.trim_start_matches('#').to_string());
                }
                let mut children = self.arena.get_children(member_idx);
                nodes_to_visit.append(&mut children);
            }
        }

        let mut next_auto_accessor_name_index = if lower_auto_accessors_to_weakmap {
            self.next_auto_accessor_name_index
        } else {
            0
        };
        let mut next_auto_accessor_name = || -> String {
            let name = if next_auto_accessor_name_index < 26 {
                let offset = next_auto_accessor_name_index as u8;
                format!("_{}", (b'a' + offset) as char)
            } else {
                format!("_{}", next_auto_accessor_name_index - 26)
            };
            next_auto_accessor_name_index += 1;
            name
        };

        let mut uniquify_private_accessor_name = |base: &str| -> String {
            if !lower_auto_accessors_to_private_fields {
                return base.to_string();
            }

            let mut candidate = base.to_string();
            let mut candidate_with_storage = format!("{candidate}_accessor_storage");
            let mut suffix = 1usize;
            while private_names_for_auto_accessors
                .iter()
                .any(|name| name == &candidate_with_storage)
            {
                candidate = format!("{base}_{suffix}");
                candidate_with_storage = format!("{candidate}_accessor_storage");
                suffix += 1;
            }
            private_names_for_auto_accessors.push(format!("{candidate}_accessor_storage"));
            candidate
        };

        if lower_auto_accessors_to_private_fields || lower_auto_accessors_to_weakmap {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let Some(prop) = self.arena.get_property_decl(member_node).filter(|prop| {
                    self.arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                }) else {
                    continue;
                };
                if self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                {
                    continue;
                }
                if self
                    .arena
                    .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                {
                    continue;
                }
                if lower_auto_accessors_to_weakmap
                    && self
                        .arena
                        .get(prop.name)
                        .is_some_and(|name| name.kind == SyntaxKind::PrivateIdentifier as u16)
                {
                    continue;
                }
                if lower_auto_accessors_to_weakmap && class_name.is_empty() {
                    continue;
                }
                let is_static = self.arena.is_static(&prop.modifiers);
                let Some(name_node) = self.arena.get(prop.name) else {
                    continue;
                };
                let mut accessor_name = match name_node.kind {
                    k if k == SyntaxKind::Identifier as u16 => {
                        self.get_identifier_text_idx(prop.name)
                    }
                    k if k == SyntaxKind::PrivateIdentifier as u16 => {
                        if lower_auto_accessors_to_private_fields {
                            get_private_field_name(self.arena, prop.name)
                                .unwrap_or_default()
                                .trim_start_matches('#')
                                .to_string()
                        } else {
                            String::new()
                        }
                    }
                    _ => String::new(),
                };
                if accessor_name.is_empty() {
                    accessor_name = next_auto_accessor_name();
                }
                if accessor_name.is_empty() {
                    continue;
                }
                let accessor_name = uniquify_private_accessor_name(&accessor_name);
                let storage_name = if lower_auto_accessors_to_weakmap {
                    format!("_{class_name}_{accessor_name}_accessor_storage")
                } else {
                    format!("{accessor_name}_accessor_storage")
                };
                let init = if prop.initializer.is_none() {
                    None
                } else {
                    Some(prop.initializer)
                };
                auto_accessor_members.push((member_idx, storage_name.clone(), init, is_static));
                if is_static {
                    if lower_auto_accessors_to_weakmap && auto_accessor_class_alias.is_none() {
                        auto_accessor_class_alias = Some(self.make_unique_name());
                    }
                    auto_accessor_static_inits.push((storage_name, init));
                } else {
                    auto_accessor_instance_inits.push((storage_name, init));
                }
            }
        }
        if lower_auto_accessors_to_weakmap {
            self.next_auto_accessor_name_index = next_auto_accessor_name_index;
        }

        if !auto_accessor_members.is_empty() && lower_auto_accessors_to_weakmap {
            // Hoist auto-accessor storage vars to the top of the scope,
            // matching tsc behavior (emits all class-related vars before the first class).
            if let Some(alias) = auto_accessor_class_alias.as_ref() {
                self.hoisted_assignment_temps.push(alias.clone());
            }
            for (_, storage_name, _, _) in &auto_accessor_members {
                self.hoisted_assignment_temps.push(storage_name.clone());
            }
            self.emit_comments_before_pos(node.pos);
        }
        let auto_accessor_member_map: FxHashMap<NodeIndex, (String, bool)> = auto_accessor_members
            .iter()
            .map(|(member_idx, storage_name, _, is_static)| {
                (*member_idx, (storage_name.clone(), *is_static))
            })
            .collect();
        let auto_accessor_computed_storage_key_member = if lower_auto_accessors_to_weakmap {
            auto_accessor_members.iter().find_map(
                |(member_idx, _storage_name, _init, is_static)| {
                    if *is_static {
                        return None;
                    }
                    let member_node = self.arena.get(*member_idx)?;
                    let prop = self.arena.get_property_decl(member_node)?;
                    let name_node = self.arena.get(prop.name)?;
                    (name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                        .then_some(*member_idx)
                },
            )
        } else {
            None
        };
        let auto_accessor_instance_storage_inits_in_computed_key: Vec<String> =
            if auto_accessor_computed_storage_key_member.is_some() {
                auto_accessor_instance_inits
                    .iter()
                    .map(|(storage_name, _)| format!("{storage_name} = new WeakMap()"))
                    .collect()
            } else {
                Vec::new()
            };
        let emit_auto_accessor_instance_inits_after_class =
            auto_accessor_instance_storage_inits_in_computed_key.is_empty();

        let is_class_expression = node.kind == syntax_kind_ext::CLASS_EXPRESSION;

        // Private field lowering: when target < ES2022, transform #fields to WeakMap pattern
        let needs_private_field_lowering = !self.ctx.options.target.supports_es2022()
            && self.ctx.options.target != ScriptTarget::ESNext;
        let anonymous_class_expression_has_static_private = class.name.is_none()
            && is_class_expression
            && class.members.nodes.iter().any(|&member_idx| {
                let Some(member_node) = self.arena.get(member_idx) else {
                    return false;
                };
                let (modifiers, name_idx) = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(member_node)
                        .map(|prop| (&prop.modifiers, prop.name)),
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .map(|method| (&method.modifiers, method.name)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .map(|accessor| (&accessor.modifiers, accessor.name))
                    }
                    _ => None,
                }
                .unwrap_or((&None, NodeIndex::NONE));

                self.arena.is_static(modifiers) && is_private_identifier(self.arena, name_idx)
            });
        let private_helper_class_name = if class.name.is_none()
            && is_class_expression
            && !anonymous_class_expression_has_static_private
            && (class_name.is_empty() || self.class_expr_is_exported_variable_initializer(_idx))
        {
            ""
        } else {
            &class_name
        };
        // Generated private-helper names are uniquified against a single file-wide
        // set so nested or sibling classes that reuse a class name receive
        // `_N`-suffixed helpers instead of colliding (matches tsc's per-file name
        // generator). Seed once from the enclosing source bindings, then keep the
        // accumulated set on the emitter and reuse it for every later class.
        let mut used_private_names = if needs_private_field_lowering {
            self.generated_private_names
                .take()
                .unwrap_or_else(|| collect_enclosing_source_binding_names(self.arena, _idx))
        } else {
            rustc_hash::FxHashSet::default()
        };
        let (private_fields, private_methods, private_accessors): (
            Vec<PrivateFieldInfo>,
            Vec<PrivateMethodInfo>,
            Vec<PrivateAccessorInfo>,
        ) = if needs_private_field_lowering {
            collect_private_members_with_reserved(
                self.arena,
                _idx,
                private_helper_class_name,
                &mut used_private_names,
            )
        } else {
            (Vec::new(), Vec::new(), Vec::new())
        };
        let private_auto_accessors: Vec<PrivateAutoAccessorInfo> =
            if needs_private_field_lowering && lower_auto_accessors_to_weakmap {
                collect_private_auto_accessors_with_reserved(
                    self,
                    class,
                    private_helper_class_name,
                    &mut used_private_names,
                )
            } else {
                Vec::new()
            };
        let private_duplicate_conflicts = if needs_private_field_lowering {
            collect_private_duplicate_conflicts(self, class, &private_fields)
        } else {
            PrivateDuplicateConflictPlan::default()
        };
        let constructor_auto_accessor_instance_inits: Vec<(String, Option<NodeIndex>)> =
            auto_accessor_instance_inits
                .iter()
                .cloned()
                .chain(
                    private_auto_accessors
                        .iter()
                        .filter(|a| !a.is_static)
                        .map(|a| (a.storage_name.clone(), a.initializer)),
                )
                .collect();

        // Determine if we need a WeakSet for instance methods/accessors
        let has_instance_methods_or_accessors = private_methods.iter().any(|m| !m.is_static)
            || private_accessors.iter().any(|a| !a.is_static);
        let has_instance_methods_or_accessors = has_instance_methods_or_accessors
            || private_auto_accessors.iter().any(|a| !a.is_static);
        let instances_weakset_name = if has_instance_methods_or_accessors {
            Some(make_unique_private_name(
                &private_helper_base(private_helper_class_name, "instances"),
                &mut used_private_names,
            ))
        } else {
            None
        };

        // Publish the accumulated generated-name set back onto the emitter so any
        // nested classes emitted while printing this class body (and later sibling
        // classes) uniquify their private helpers against the names allocated here.
        if needs_private_field_lowering {
            self.generated_private_names = Some(std::mem::take(&mut used_private_names));
        }

        let target_needs_static_block_lowering =
            (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);
        let static_initializer_alias_source_nodes: Vec<NodeIndex> =
            if target_needs_static_block_lowering {
                class
                    .members
                    .nodes
                    .iter()
                    .filter_map(|&member_idx| {
                        let member_node = self.arena.get(member_idx)?;
                        if member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION {
                            return Some(member_idx);
                        }
                        if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                            return None;
                        }
                        let prop = self.arena.get_property_decl(member_node)?;
                        if !self.arena.is_static(&prop.modifiers)
                            || self
                                .arena
                                .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                            || self
                                .arena
                                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                            || prop.initializer.is_none()
                        {
                            return None;
                        }
                        Some(prop.initializer)
                    })
                    .collect()
            } else {
                Vec::new()
            };
        let static_initializer_needs_this_alias = !static_initializer_alias_source_nodes.is_empty()
            && static_initializer_alias_source_nodes
                .iter()
                .any(|init_idx| {
                    contains_this_reference(self.arena, *init_idx)
                        || contains_async_arrow_function(self.arena, *init_idx)
                });
        let private_member_def_needs_class_alias = !class_name.is_empty()
            && (private_methods.iter().any(|method| {
                method
                    .body
                    .is_some_and(|body| self.node_text_contains_identifier(body, &class_name))
            }) || private_accessors.iter().any(|accessor| {
                accessor
                    .getter_body
                    .is_some_and(|body| self.node_text_contains_identifier(body, &class_name))
                    || accessor
                        .setter_body
                        .is_some_and(|body| self.node_text_contains_identifier(body, &class_name))
            }));

        let has_static_privates = private_fields.iter().any(|f| f.is_static)
            || private_methods.iter().any(|m| m.is_static)
            || private_accessors.iter().any(|a| a.is_static)
            || private_auto_accessors.iter().any(|a| a.is_static);
        let static_initializer_contains_class_name = !class_name.is_empty()
            && static_initializer_alias_source_nodes
                .iter()
                .any(|idx| self.node_text_contains_identifier(*idx, &class_name));
        let static_initializer_needs_class_alias = static_initializer_contains_class_name
            && (static_initializer_needs_this_alias
                || has_static_privates
                || private_member_def_needs_class_alias);
        let has_legacy_class_decorators = self.ctx.options.legacy_decorators
            && !self.collect_class_decorators(&class.modifiers).is_empty();
        let externalized_static_initializer_uses_undefined_receiver = !is_class_expression
            && target_needs_static_block_lowering
            && has_legacy_class_decorators;
        let static_initializer_needs_live_class_alias =
            !externalized_static_initializer_uses_undefined_receiver
                && (static_initializer_needs_this_alias || static_initializer_needs_class_alias);

        // Determine if we need a class alias for static private fields.
        let class_value_alias = if has_static_privates
            || static_initializer_needs_live_class_alias
            || private_member_def_needs_class_alias
        {
            Some(self.make_class_static_temp_name(_idx))
        } else {
            None
        };
        let private_class_alias = if has_static_privates {
            class_value_alias.clone()
        } else {
            None
        };
        let static_initializer_class_alias =
            if static_initializer_needs_live_class_alias || private_member_def_needs_class_alias {
                class_value_alias.clone()
            } else {
                None
            };

        // Save the previous private-name maps (for nested classes). Private
        // names are lexically scoped, so nested classes must still be able to
        // lower accesses to outer private names unless shadowed by their own.
        let prev_private_field_weakmaps = self.private_field_weakmaps.clone();
        let prev_pending_weakmap_inits = std::mem::take(&mut self.pending_weakmap_inits);
        let prev_pending_static_private_inits =
            std::mem::take(&mut self.pending_static_private_inits);
        let prev_pending_private_class_alias = self.pending_private_class_alias.take();
        let prev_private_member_info = self.private_member_info.clone();
        let prev_pending_private_field_constructor_inits =
            std::mem::take(&mut self.pending_private_field_constructor_inits);
        let prev_pending_instances_weakset_add = self.pending_instances_weakset_add.take();
        let prev_pending_private_method_defs =
            std::mem::take(&mut self.pending_private_method_defs);
        let prev_pending_private_accessor_defs =
            std::mem::take(&mut self.pending_private_accessor_defs);
        let prev_private_members_to_skip = std::mem::take(&mut self.private_members_to_skip);
        let prev_private_static_class_alias = self.private_static_class_alias.clone();

        let has_any_private_lowering = !private_fields.is_empty()
            || !private_methods.is_empty()
            || !private_accessors.is_empty()
            || !private_auto_accessors.is_empty();

        if has_any_private_lowering {
            // Collect all variable names needed for declaration.
            //
            // tsc's order, see e.g. `privateNameInInExpressionTransform`:
            //   1. WeakSet for instance methods/accessors (`_C_instances`)
            //   2. Class alias for static members (`_a`)
            //   3. Private members in *source* order (per-class)
            //
            // Grouping by category (all instance fields → all static fields →
            // all methods → all accessors) does not match tsc — tsc walks the
            // class body once and emits each var as it encounters the member.
            let mut var_names: Vec<String> = Vec::new();

            // WeakSet for instance methods/accessors (first in tsc's emit)
            if let Some(ref ws_name) = instances_weakset_name {
                var_names.push(ws_name.clone());
            }

            // Class alias for static elements and extracted private member bodies.
            if let Some(ref alias) = class_value_alias {
                var_names.push(alias.clone());
            }

            // Private members in source order. Walk `class.members.nodes`
            // once and look up each member's pre-computed info entry by source
            // member identity, not by private clean-name. When several members
            // share a clean-name (an instance and a static `#foo`, or duplicate
            // declarations in an error case), each member owns a distinct
            // `_N`-suffixed helper; matching by identity keeps this hoisted
            // var-decl list in agreement with the initializer/assignment paths
            // that iterate the same collections. Accessors collapse a get/set
            // pair into one entry, so we emit each accessor entry once when its
            // first member is reached.
            let mut emitted_accessor_entries: rustc_hash::FxHashSet<usize> =
                rustc_hash::FxHashSet::default();
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        if let Some(accessor) = private_auto_accessors
                            .iter()
                            .find(|a| a.member_idx == member_idx)
                        {
                            var_names.push(accessor.get_var_name.clone());
                            var_names.push(accessor.set_var_name.clone());
                        } else if let Some(field) =
                            private_fields.iter().find(|f| f.member_idx == member_idx)
                        {
                            var_names.push(field.weakmap_name.clone());
                        }
                    }
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        if let Some(method) =
                            private_methods.iter().find(|m| m.member_idx == member_idx)
                        {
                            var_names.push(method.fn_var_name.clone());
                        }
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        if let Some((entry_pos, accessor)) = private_accessors
                            .iter()
                            .enumerate()
                            .find(|(_, a)| a.member_indices.contains(&member_idx))
                        {
                            if !emitted_accessor_entries.insert(entry_pos) {
                                continue;
                            }
                            for &accessor_member_idx in &accessor.member_indices {
                                let Some(accessor_node) = self.arena.get(accessor_member_idx)
                                else {
                                    continue;
                                };
                                if accessor_node.kind == syntax_kind_ext::GET_ACCESSOR {
                                    if let Some(ref name) = accessor.get_var_name {
                                        var_names.push(name.clone());
                                    }
                                } else if accessor_node.kind == syntax_kind_ext::SET_ACCESSOR
                                    && let Some(ref name) = accessor.set_var_name
                                {
                                    var_names.push(name.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            for accessor in &private_auto_accessors {
                var_names.push(accessor.storage_name.clone());
            }

            if !var_names.is_empty() {
                if node.kind == syntax_kind_ext::CLASS_DECLARATION
                    && (self.ctx.options.target as u32) < (ScriptTarget::ES2015 as u32)
                {
                } else if node.kind == syntax_kind_ext::CLASS_EXPRESSION
                    && self.class_expression_is_in_loop_body(_idx)
                {
                    self.block_scoped_private_temps.extend(var_names);
                } else {
                    self.hoisted_assignment_temps.extend(var_names);
                }
            }

            // Set up the private field map for expression lowering
            for field in &private_fields {
                self.private_field_weakmaps
                    .insert(field.name.clone(), field.weakmap_name.clone());
                self.private_member_info.insert(
                    field.name.clone(),
                    PrivateMemberInfo {
                        kind: "f",
                        fn_ref: field.is_static.then(|| field.weakmap_name.clone()),
                        setter_ref: None,
                        state_var: if field.is_static {
                            private_class_alias.clone()
                        } else {
                            None
                        },
                    },
                );
            }

            // Register methods
            for method in &private_methods {
                self.private_field_weakmaps
                    .insert(method.name.clone(), method.fn_var_name.clone());
                self.private_member_info.insert(
                    method.name.clone(),
                    PrivateMemberInfo {
                        kind: "m",
                        fn_ref: Some(method.fn_var_name.clone()),
                        setter_ref: None,
                        state_var: if method.is_static {
                            private_class_alias.clone()
                        } else {
                            instances_weakset_name.clone()
                        },
                    },
                );
            }

            // Register accessors
            for accessor in &private_accessors {
                // Use the instances weakset name as the weakmap entry for lookup
                let weakmap_entry = if accessor.is_static {
                    private_class_alias.clone().unwrap_or_default()
                } else {
                    instances_weakset_name.clone().unwrap_or_default()
                };
                self.private_field_weakmaps
                    .insert(accessor.name.clone(), weakmap_entry);
                self.private_member_info.insert(
                    accessor.name.clone(),
                    PrivateMemberInfo {
                        kind: "a",
                        fn_ref: accessor
                            .get_var_name
                            .as_ref()
                            .filter(|_| accessor.has_getter)
                            .cloned(),
                        setter_ref: accessor
                            .set_var_name
                            .as_ref()
                            .filter(|_| accessor.has_setter)
                            .cloned(),
                        state_var: if accessor.is_static {
                            private_class_alias.clone()
                        } else {
                            instances_weakset_name.clone()
                        },
                    },
                );
            }
            for accessor in &private_auto_accessors {
                let weakmap_entry = if accessor.is_static {
                    private_class_alias.clone().unwrap_or_default()
                } else {
                    instances_weakset_name.clone().unwrap_or_default()
                };
                self.private_field_weakmaps
                    .insert(accessor.name.clone(), weakmap_entry);
                self.private_member_info.insert(
                    accessor.name.clone(),
                    PrivateMemberInfo {
                        kind: "a",
                        fn_ref: Some(accessor.get_var_name.clone()),
                        setter_ref: Some(accessor.set_var_name.clone()),
                        state_var: if accessor.is_static {
                            private_class_alias.clone()
                        } else {
                            instances_weakset_name.clone()
                        },
                    },
                );
            }

            // Prepare WeakMap initializations for after the class body
            self.pending_weakmap_inits = private_fields
                .iter()
                .filter(|f| !f.is_static)
                .map(|f| format!("{} = new WeakMap()", f.weakmap_name))
                .collect();

            // Prepare static private field value initializations for after the class body.
            // tsc emits: `_A_field = { value: <init> };` for each static private field.
            self.pending_static_private_inits = private_fields
                .iter()
                .filter(|f| f.is_static)
                .filter_map(|f| {
                    let (storage_name, storage_kind) = if private_duplicate_conflicts
                        .is_conflicting(f.member_idx)
                    {
                        let selected = private_duplicate_conflicts.selected_field_for(&f.name)?;
                        (selected.helper_name.clone(), selected.storage_kind)
                    } else {
                        (f.weakmap_name.clone(), PrivateFieldStorageKind::Value)
                    };
                    Some(StaticPrivateInit {
                        storage_name,
                        initializer: f.initializer,
                        storage_kind,
                    })
                })
                .collect();

            // Store class alias for static elements/private bodies:
            // emit `_a = ClassName;` after the class body, before extracted members.
            if let Some(ref alias) = class_value_alias
                && !class_name.is_empty()
            {
                self.pending_private_class_alias = Some((alias.clone(), class_name.clone()));
                if has_static_privates {
                    self.private_static_class_alias = Some((class_name.clone(), alias.clone()));
                }
            }

            // Prepare private field constructor inits (WeakMap.set calls)
            self.pending_private_field_constructor_inits = private_fields
                .iter()
                .filter(|f| !f.is_static)
                .filter_map(|f| {
                    let (storage_name, storage_kind) = if private_duplicate_conflicts
                        .is_conflicting(f.member_idx)
                    {
                        let selected = private_duplicate_conflicts.selected_field_for(&f.name)?;
                        (selected.helper_name.clone(), selected.storage_kind)
                    } else {
                        (f.weakmap_name.clone(), PrivateFieldStorageKind::WeakMap)
                    };
                    let Some(member_pos) = class
                        .members
                        .nodes
                        .iter()
                        .position(|&member_idx| member_idx == f.member_idx)
                    else {
                        return Some((
                            storage_name,
                            f.has_initializer,
                            f.initializer,
                            Vec::new(),
                            Vec::new(),
                            u32::MAX,
                            storage_kind,
                        ));
                    };
                    let member_node = self.arena.get(f.member_idx);
                    let source_order = member_node.map_or(u32::MAX, |n| n.pos);
                    let leading_comments = if !self.ctx.options.remove_comments {
                        let prev_end = if member_pos > 0 {
                            class
                                .members
                                .nodes
                                .get(member_pos - 1)
                                .and_then(|&prev_idx| self.arena.get(prev_idx))
                                .map_or(source_order, |prev| {
                                    self.find_token_end_before_trivia(prev.pos, prev.end)
                                })
                        } else {
                            source_order.saturating_sub(64)
                        };
                        self.collect_leading_comments_in_range(prev_end, source_order)
                    } else {
                        Vec::new()
                    };
                    let trailing_comments = if !self.ctx.options.remove_comments {
                        let skip_end = class
                            .members
                            .nodes
                            .get(member_pos + 1)
                            .and_then(|&next_idx| self.arena.get(next_idx))
                            .map_or_else(
                                || member_node.map_or(source_order, |n| n.end),
                                |next| next.pos,
                            );
                        member_node.map_or_else(Vec::new, |node| {
                            let actual_end = self.find_token_end_before_trivia(node.pos, skip_end);
                            self.collect_trailing_comments_in_range(actual_end)
                        })
                    } else {
                        Vec::new()
                    };
                    Some((
                        storage_name,
                        f.has_initializer,
                        f.initializer,
                        leading_comments,
                        trailing_comments,
                        source_order,
                        storage_kind,
                    ))
                })
                .collect();

            // Prepare WeakSet instances.add(this) for constructor
            if let Some(ref ws_name) = instances_weakset_name {
                self.pending_instances_weakset_add = Some(ws_name.clone());
            }

            // Prepare private method function defs for after the class body
            // Both instance and static private methods are extracted.
            for method in &private_methods {
                if Self::is_reserved_private_constructor_name(&method.name) {
                    continue;
                }
                if private_duplicate_conflicts.is_conflicting(method.member_idx) {
                    continue;
                }
                if let Some(body_idx) = method.body {
                    self.pending_private_method_defs.push(PrivateMethodDef {
                        var_name: method.fn_var_name.clone(),
                        body: body_idx,
                        params: method.parameters.clone(),
                        is_async: method.is_async,
                        is_generator: method.is_generator,
                    });
                }
            }

            // Prepare private accessor function defs for after the class body
            // Both instance and static private accessors are extracted.
            for accessor in &private_accessors {
                if accessor
                    .member_indices
                    .iter()
                    .any(|&idx| private_duplicate_conflicts.is_conflicting(idx))
                {
                    continue;
                }
                for &accessor_member_idx in &accessor.member_indices {
                    let Some(accessor_node) = self.arena.get(accessor_member_idx) else {
                        continue;
                    };
                    if accessor_node.kind == syntax_kind_ext::GET_ACCESSOR {
                        if let Some(ref var_name) = accessor.get_var_name {
                            self.pending_private_accessor_defs.push(
                                crate::emitter::core::PrivateAccessorDef {
                                    var_name: var_name.clone(),
                                    body: accessor.getter_body,
                                    param: None,
                                    is_async: accessor.getter_is_async,
                                },
                            );
                        }
                    } else if accessor_node.kind == syntax_kind_ext::SET_ACCESSOR
                        && let Some(ref var_name) = accessor.set_var_name
                    {
                        self.pending_private_accessor_defs.push(
                            crate::emitter::core::PrivateAccessorDef {
                                var_name: var_name.clone(),
                                body: accessor.setter_body,
                                param: accessor.setter_param,
                                is_async: accessor.setter_is_async,
                            },
                        );
                    }
                }
            }

            // Mark all private methods and accessors (instance + static) to skip from class body
            for method in &private_methods {
                if Self::is_reserved_private_constructor_name(&method.name) {
                    continue;
                }
                if !private_duplicate_conflicts.is_conflicting(method.member_idx) {
                    self.private_members_to_skip.insert(method.member_idx);
                }
            }
            for accessor in &private_accessors {
                if accessor
                    .member_indices
                    .iter()
                    .all(|&idx| !private_duplicate_conflicts.is_conflicting(idx))
                {
                    self.private_members_to_skip
                        .extend(accessor.member_indices.iter().copied());
                }
            }
            for accessor in &private_auto_accessors {
                self.private_members_to_skip.insert(accessor.member_idx);
            }
        }

        // For class expressions with private field lowering, we need to wrap the class
        // in a comma expression: `(_a = class C { ... }, _WeakMap = new WeakMap(), ..., _a)`
        // tsc uses this pattern so the WeakMap/WeakSet initialization happens inline.
        let emits_as_class_expression = is_class_expression || assignment_prefix.is_some();
        let needs_private_comma_expr = is_class_expression && has_any_private_lowering;

        // Class-expression tails that run after the class body use a comma wrapper.
        let has_static_field_comma_expr = self.class_has_static_field_comma_expr(
            class,
            target_needs_field_lowering,
            target_needs_static_block_lowering,
            needs_private_field_lowering,
        );
        let has_static_block_comma_expr =
            self.class_has_static_block_comma_expr(class, target_needs_static_block_lowering);
        // A computed-named *static method or accessor* is emitted inline in the
        // class body, so it only requires the `(_tmp = class {...}, ..., _tmp)`
        // comma wrapping when the binding *also* loses JS named evaluation --
        // i.e. a `using`/`await using` declaration lowered to
        // `__addDisposableResource`, which moves the class out of
        // direct-assignment position. A plain `var X = class {...}` keeps named
        // evaluation and needs no wrapping for inline computed method names.
        let has_static_computed_method_or_accessor = self
            .class_has_static_computed_method_or_accessor_comma_expr(
                class,
                _idx,
                emits_as_class_expression,
            );
        let needs_static_comma_expr = emits_as_class_expression
            && !emit_assignment_static_elements_as_statements
            && (has_static_field_comma_expr
                || has_static_block_comma_expr
                || has_static_computed_method_or_accessor);
        let preplanned_class_expr_temp = if needs_static_comma_expr
            && private_class_alias.is_none()
            && self.file_level_class_temp_reservations.contains_key(&_idx)
        {
            Some(self.make_class_static_temp_name_hoisted(_idx))
        } else {
            None
        };

        // Computed property name hoisting for class fields that will be lowered.
        // tsc hoists non-constant computed property name expressions to temp variables
        // (e.g., `_a = n, _b = s + n`) so that the evaluation order is preserved and
        // the class body can reference the temp instead of the original expression.
        //
        // Only PROPERTY DECLARATIONS with computed names participate in hoisting.
        // Methods and accessors keep their computed names inline in ES6+.
        // After the class body, a comma expression joins all assignments and side effects.
        let needs_computed_prop_hoisting = target_needs_field_lowering;
        // For an *erased* computed-name member (a type-only field with no runtime
        // slot, e.g. `static [N.s]: "b";`) whose key expression has observable side
        // effects, tsc evaluates the key inside a native `static { ... }` block when
        // the target supports static blocks (>= ES2022). Field lowering driven by
        // `useDefineForClassFields: false` forces `needs_computed_prop_hoisting` on
        // even at ES2022+, but that hoisting machinery (bare after-body statements /
        // class-expression comma evaluator) must NOT swallow these side effects for a
        // plain class declaration — they belong in the static block. Class
        // expressions keep using the comma/evaluator path #12126 introduced.
        let erased_computed_side_effects_use_static_block =
            !target_needs_static_block_lowering && !emits_as_class_expression;
        // Each entry: (Option<temp_name>, expr_idx, member_idx) — None means side-effect only
        let mut computed_prop_entries: Vec<(Option<String>, NodeIndex, NodeIndex)> = Vec::new();
        if needs_computed_prop_hoisting {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let (modifiers, name_idx, property_is_erased) = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        let Some(prop) = self.arena.get_property_decl(member_node) else {
                            continue;
                        };
                        let is_erased = if self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                            || self
                                .arena
                                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                        {
                            true
                        } else {
                            let is_private = self
                                .arena
                                .get(prop.name)
                                .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16);
                            let has_accessor = self
                                .arena
                                .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                            // A no-initializer field is erased *unless* it is still
                            // runtime-materialized as a defined field (define
                            // semantics). Use the shared predicate so this stays in
                            // lockstep with the runtime field-lowering site, which
                            // would otherwise reference a temp that was never recorded.
                            prop.initializer.is_none()
                                && !is_private
                                && !has_accessor
                                && !self.no_init_property_is_runtime_materialized(prop)
                        };
                        (&prop.modifiers, prop.name, Some(is_erased))
                    }
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        let Some(method) = self.arena.get_method_decl(member_node) else {
                            continue;
                        };
                        if !method.body.is_some() {
                            continue;
                        }
                        (&method.modifiers, method.name, None)
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        let Some(accessor) = self.arena.get_accessor(member_node) else {
                            continue;
                        };
                        (&accessor.modifiers, accessor.name, None)
                    }
                    _ => continue,
                };
                let Some(name_node) = self.arena.get(name_idx) else {
                    continue;
                };
                if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = self.arena.get_computed_property(name_node) else {
                    continue;
                };
                let Some(expr_node) = self.arena.get(computed.expression) else {
                    continue;
                };
                // Check if expression is a constant that doesn't need hoisting
                let is_constant = expr_node.kind == SyntaxKind::StringLiteral as u16
                    || expr_node.kind == SyntaxKind::NumericLiteral as u16
                    || expr_node.kind == SyntaxKind::NoSubstitutionTemplateLiteral as u16;
                if is_constant {
                    continue;
                }
                let has_legacy_decorators = self.ctx.options.legacy_decorators
                    && !self.collect_class_decorators(modifiers).is_empty();
                if property_is_erased.is_none() {
                    if has_legacy_decorators {
                        let temp = self.make_unique_name_hoisted();
                        self.legacy_decorator_computed_name_temp_map
                            .insert(computed.expression, temp);
                    }
                    continue;
                }
                if property_is_erased == Some(true) {
                    // Side-effect only: expression is emitted for its effects but no temp.
                    if has_legacy_decorators {
                        // Only `declare`/`abstract` fields are truly erased at
                        // runtime — for those, a side-effect-free key expression
                        // (plain identifier, literal) can be emitted directly in
                        // the `__decorate` call without a hoisted temp.
                        //
                        // Implicitly-erased fields (no initializer +
                        // `use_define_for_class_fields: false`) are still
                        // runtime-visible through the decorator, so `tsc` always
                        // allocates a temp for their key to guarantee stable
                        // evaluation order at class-definition time.
                        let is_explicitly_erased = self
                            .arena
                            .has_modifier(modifiers, SyntaxKind::AbstractKeyword)
                            || self
                                .arena
                                .has_modifier(modifiers, SyntaxKind::DeclareKeyword);
                        let needs_temp = !is_explicitly_erased
                            || !self.is_computed_name_expr_side_effect_free(computed.expression);
                        if needs_temp {
                            let temp = self.make_unique_name_hoisted();
                            self.computed_prop_temp_map
                                .insert(computed.expression, temp.clone());
                            self.legacy_decorator_computed_name_temp_map
                                .insert(computed.expression, temp.clone());
                            computed_prop_entries.push((
                                Some(temp),
                                computed.expression,
                                member_idx,
                            ));
                        }
                        // `declare`/`abstract` + side-effect-free key: no temp;
                        // `emit_decorator_member_name` emits the expression directly.
                    } else if !self.is_computed_name_expr_side_effect_free(computed.expression)
                        && !erased_computed_side_effects_use_static_block
                    {
                        computed_prop_entries.push((None, computed.expression, member_idx));
                    }
                } else {
                    // Allocate a temp variable for this computed property name
                    let use_class_static_temp = (preplanned_class_expr_temp.is_some()
                        && self.file_level_class_temp_reservations.contains_key(&_idx))
                        || (self.ctx.options.legacy_decorators && !has_legacy_decorators);
                    let temp = if use_class_static_temp {
                        self.make_class_static_temp_name_hoisted(_idx)
                    } else {
                        self.make_unique_name_hoisted()
                    };
                    self.computed_prop_temp_map
                        .insert(computed.expression, temp.clone());
                    if has_legacy_decorators {
                        self.legacy_decorator_computed_name_temp_map
                            .insert(computed.expression, temp.clone());
                    }
                    computed_prop_entries.push((Some(temp), computed.expression, member_idx));
                }
            }
        }

        let mut computed_prop_entries_consumed_by_member_name: Vec<usize> = Vec::new();
        if needs_computed_prop_hoisting && !computed_prop_entries.is_empty() {
            let mut pending_computed_entries = Vec::new();
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };

                if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION {
                    if let Some(entry_idx) = computed_prop_entries
                        .iter()
                        .position(|(_, _, entry_member_idx)| *entry_member_idx == member_idx)
                    {
                        pending_computed_entries.push(entry_idx);
                    }
                    continue;
                }

                let computed_name = match member_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .and_then(|method| self.arena.get(method.name)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .and_then(|accessor| self.arena.get(accessor.name))
                    }
                    _ => None,
                };
                let Some(computed_name) = computed_name else {
                    continue;
                };
                if computed_name.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                    continue;
                }
                let Some(computed) = self.arena.get_computed_property(computed_name) else {
                    continue;
                };
                let decorated_key_temp = self
                    .legacy_decorator_computed_name_temp_map
                    .get(&computed.expression)
                    .cloned();
                if pending_computed_entries.is_empty() && decorated_key_temp.is_none() {
                    continue;
                }

                let mut comma_parts = Vec::new();
                for entry_idx in pending_computed_entries.drain(..) {
                    let (temp_name, expr_idx, _) = computed_prop_entries[entry_idx].clone();
                    let expr_text = self.capture_emit(expr_idx);
                    if let Some(temp) = temp_name {
                        comma_parts.push(format!("{temp} = {expr_text}"));
                    } else {
                        comma_parts.push(expr_text);
                    }
                    computed_prop_entries_consumed_by_member_name.push(entry_idx);
                }
                let expr_text = self.capture_emit(computed.expression);
                if let Some(temp) = decorated_key_temp {
                    comma_parts.push(format!("{temp} = {expr_text}"));
                } else {
                    comma_parts.push(expr_text);
                }
                let replacement = if comma_parts.len() == 1 {
                    comma_parts.pop().unwrap_or_default()
                } else {
                    format!("({})", comma_parts.join(", "))
                };
                self.computed_prop_temp_map
                    .insert(computed.expression, replacement);
            }
        }
        if let Some(member_idx) = auto_accessor_computed_storage_key_member
            && let Some(entry_idx) = computed_prop_entries
                .iter()
                .position(|(_, _, entry_member_idx)| *entry_member_idx == member_idx)
            && !computed_prop_entries_consumed_by_member_name.contains(&entry_idx)
        {
            computed_prop_entries_consumed_by_member_name.push(entry_idx);
        }

        let native_computed_prop_evaluator = if emits_as_class_expression
            && needs_computed_prop_hoisting
            && !target_needs_static_block_lowering
        {
            computed_prop_entries
                .iter()
                .enumerate()
                .any(|(entry_idx, _)| {
                    !computed_prop_entries_consumed_by_member_name.contains(&entry_idx)
                })
                .then(|| self.make_unique_name_hoisted())
        } else {
            None
        };
        if native_computed_prop_evaluator.is_some() {
            for entry_idx in 0..computed_prop_entries.len() {
                if !computed_prop_entries_consumed_by_member_name.contains(&entry_idx) {
                    computed_prop_entries_consumed_by_member_name.push(entry_idx);
                }
            }
        }

        let needs_computed_prop_comma_expr = emits_as_class_expression
            && computed_prop_entries
                .iter()
                .enumerate()
                .any(|(entry_idx, _)| {
                    !computed_prop_entries_consumed_by_member_name.contains(&entry_idx)
                });
        let needs_native_computed_prop_evaluator_comma_expr =
            native_computed_prop_evaluator.is_some();
        let needs_class_expr_temp =
            needs_static_comma_expr || needs_private_comma_expr || needs_computed_prop_comma_expr;
        let needs_any_comma_expr =
            needs_class_expr_temp || needs_native_computed_prop_evaluator_comma_expr;
        let class_expr_comma_needs_parens = needs_any_comma_expr
            && !self.emitting_concise_arrow_return_argument
            && self
                .arena
                .get_extended(_idx)
                .and_then(|ext| self.arena.get(ext.parent))
                .is_none_or(|parent| {
                    parent.kind != syntax_kind_ext::RETURN_STATEMENT
                        && parent.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION
                });
        if needs_native_computed_prop_evaluator_comma_expr {
            if class_expr_comma_needs_parens {
                self.write("(");
            }
            if let Some(evaluator) = native_computed_prop_evaluator.as_ref() {
                self.write(evaluator);
                self.write(" = () => { ");
                let mut emitted_entry = false;
                for (entry_idx, (temp_name, expr_idx, _)) in
                    computed_prop_entries.iter().enumerate()
                {
                    if !computed_prop_entries_consumed_by_member_name.contains(&entry_idx) {
                        continue;
                    }
                    if emitted_entry {
                        self.write(", ");
                    }
                    emitted_entry = true;
                    if let Some(temp) = temp_name {
                        self.write(temp);
                        self.write(" = ");
                    }
                    self.emit_expression(*expr_idx);
                }
                self.write("; },");
                self.write_line();
                self.increase_indent();
            }
        }
        let class_expr_temp = if needs_class_expr_temp {
            let temp = if let Some(ref alias) = private_class_alias {
                alias.clone()
            } else if let Some(temp) = preplanned_class_expr_temp {
                temp
            } else {
                self.make_class_static_temp_name_hoisted(_idx)
            };
            if class_expr_comma_needs_parens && !needs_native_computed_prop_evaluator_comma_expr {
                self.write("(");
            }
            self.write(&temp);
            self.write(" = ");
            Some(temp)
        } else {
            None
        };
        let class_expr_static_temp = if needs_static_comma_expr {
            class_expr_temp.clone()
        } else {
            None
        };
        // tsc emits setFunctionName only when the comma wrapper carries
        // real static state; recovery-only and instance-private tails do not.
        let has_static_private_member = needs_private_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|m| {
                    m.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self.arena.get_property_decl(m).is_some_and(|p| {
                            self.arena.is_static(&p.modifiers)
                                && is_private_identifier(self.arena, p.name)
                        })
                })
            });
        let needs_set_function_name_comma_item = has_static_private_member
            || (needs_static_comma_expr
                && (has_static_field_comma_expr
                    || has_static_block_comma_expr
                    || has_static_computed_method_or_accessor));
        let class_expr_set_function_name = class_expr_temp.as_ref().and_then(|_| {
            if class.name.is_none() && needs_set_function_name_comma_item {
                self.resolve_class_expr_binding_name(_idx)
            } else {
                None
            }
        });

        let has_extends = class.heritage_clauses.as_ref().is_some_and(|clauses| {
            clauses.nodes.iter().any(|&idx| {
                self.arena
                    .get(idx)
                    .and_then(|n| self.arena.get_heritage(n))
                    .is_some_and(|h| h.token == SyntaxKind::ExtendsKeyword as u16)
            })
        });
        let extends_null = crate::transforms::emit_utils::extends_null_literal(
            self.arena,
            &class.heritage_clauses,
        );
        let needs_static_block_lowering = target_needs_static_block_lowering;
        let static_initializer_needs_super_alias = has_extends
            && !extends_null
            && !static_initializer_alias_source_nodes.is_empty()
            && static_initializer_alias_source_nodes
                .iter()
                .any(|init_idx| contains_super_reference(self.arena, *init_idx));
        if !has_any_private_lowering
            && let Some(alias) = static_initializer_class_alias.as_ref()
            && !self
                .hoisted_assignment_temps
                .iter()
                .any(|name| name == alias)
            && !self
                .hoisted_file_level_class_temps
                .iter()
                .any(|name| name == alias)
        {
            self.hoisted_assignment_temps.push(alias.clone());
        }
        let static_this_alias = if static_initializer_needs_this_alias
            && !externalized_static_initializer_uses_undefined_receiver
        {
            static_initializer_class_alias
                .clone()
                .or_else(|| Some(self.make_class_static_temp_name_hoisted(_idx)))
        } else {
            None
        };
        let static_super_base_alias = if static_initializer_needs_super_alias
            && !externalized_static_initializer_uses_undefined_receiver
        {
            Some(self.make_class_static_temp_name_hoisted(_idx))
        } else {
            None
        };
        self.finish_file_level_class_temp_reservation(_idx);
        let static_initializer_this_binding =
            if externalized_static_initializer_uses_undefined_receiver
                && static_initializer_needs_this_alias
            {
                Some("(void 0)")
            } else {
                static_this_alias.as_deref()
            };
        let static_initializer_super_base =
            if externalized_static_initializer_uses_undefined_receiver
                && static_initializer_needs_super_alias
            {
                Some("(void 0)")
            } else {
                static_super_base_alias.as_deref()
            };

        self.emit_class_es6_header_and_open_body(
            node,
            class,
            assignment_prefix.is_some(),
            static_super_base_alias.as_deref(),
            class_expr_temp.is_some(),
        );

        // Store auto-accessor inits for constructor emission.
        let prev_auto_accessor_inits = std::mem::take(&mut self.pending_auto_accessor_inits);
        if !constructor_auto_accessor_instance_inits.is_empty() && lower_auto_accessors_to_weakmap {
            self.pending_auto_accessor_inits = constructor_auto_accessor_instance_inits.clone();
        }

        // Private field WeakMap.set inits are handled via pending_private_field_constructor_inits
        // which is emitted in emit_constructor_prologue and the synthesized constructor path.

        // Check if we need to lower class fields to constructor.
        // This is needed when target < ES2022 OR when useDefineForClassFields is false
        // (legacy behavior where fields are assigned in the constructor).
        let needs_class_field_lowering = (self.ctx.options.target as u32)
            < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.use_define_for_class_fields;

        let mut deferred_static_blocks: Vec<(NodeIndex, usize)> = Vec::new();
        // Collect computed property name expressions from erased type-only members.
        // tsc emits these as standalone side-effect statements after the class body
        // (e.g., `[Symbol.iterator]: Type` → erased member, but `Symbol.iterator;` emitted).
        let mut computed_property_side_effects: Vec<NodeIndex> = Vec::new();

        let ClassFieldInitCollection {
            mut field_inits,
            mut static_field_inits,
            hoisted_native_private_members,
            hoisted_native_auto_accessor_members,
        } = self.collect_class_es6_field_inits(
            class,
            needs_class_field_lowering,
            needs_private_field_lowering,
            hoist_native_instance_order_inits,
            &auto_accessor_member_map,
            needs_static_block_lowering,
            &private_fields,
        );

        // Check if class has an explicit constructor with a body.
        // A constructor without a body (e.g., broken syntax `constructor` with no
        // parens/braces) should not prevent synthesis of a constructor for field
        // initialization (matches tsc error-recovery behavior).
        let has_constructor = class.members.nodes.iter().any(|&idx| {
            self.arena.get(idx).is_some_and(|n| {
                n.kind == syntax_kind_ext::CONSTRUCTOR
                    && self
                        .arena
                        .get_constructor(n)
                        .is_some_and(|ctor| ctor.body.is_some())
            })
        });

        // Store field inits for constructor emission
        let prev_field_inits = std::mem::take(&mut self.pending_class_field_inits);
        if !field_inits.is_empty() {
            self.pending_class_field_inits = field_inits.clone();
        }

        // If no constructor but we have field inits, synthesize one
        let has_private_field_inits = !self.pending_private_field_constructor_inits.is_empty();
        let has_instances_weakset = self.pending_instances_weakset_add.is_some();
        let synthesize_constructor = !has_constructor
            && (!field_inits.is_empty()
                || (lower_auto_accessors_to_weakmap && !auto_accessor_instance_inits.is_empty())
                || (lower_auto_accessors_to_weakmap
                    && !constructor_auto_accessor_instance_inits.is_empty())
                || has_private_field_inits
                || has_instances_weakset);

        let prev_scoped_class_expression_self_alias =
            self.scoped_class_expression_self_alias.take();
        let scoped_class_expression_self_alias_ancestor_len =
            self.scoped_class_expression_self_alias_ancestors.len();
        if let Some((prev_class_name, prev_class_alias)) =
            prev_scoped_class_expression_self_alias.clone()
        {
            let shadows_prev_alias = class_name_is_real
                && !class_name.is_empty()
                && class_name == prev_class_name.as_ref();
            if !shadows_prev_alias {
                self.scoped_class_expression_self_alias_ancestors
                    .push((prev_class_name, prev_class_alias));
            }
        }
        if let Some(alias) = assignment_alias {
            if class_name_is_real && !class_name.is_empty() && class_name != alias {
                self.scoped_class_expression_self_alias = Some((
                    Arc::<str>::from(class_name.as_str()),
                    Arc::<str>::from(alias),
                ));
            }
        } else if let Some(temp) = class_expr_temp.as_ref() {
            if class_name_is_real && !class_name.is_empty() && class_name != *temp {
                self.scoped_class_expression_self_alias = Some((
                    Arc::<str>::from(class_name.as_str()),
                    Arc::<str>::from(temp.as_str()),
                ));
            }
        } else if let Some(alias) = class_value_alias.as_ref() {
            if class_name_is_real && !class_name.is_empty() && class_name != *alias {
                self.scoped_class_expression_self_alias = Some((
                    Arc::<str>::from(class_name.as_str()),
                    Arc::<str>::from(alias.as_str()),
                ));
            }
        } else if let Some((static_class_name, static_class_alias)) =
            self.private_static_class_alias.clone()
            && class_name_is_real
            && !class_name.is_empty()
            && class_name == static_class_name
            && class_name != static_class_alias
        {
            self.scoped_class_expression_self_alias = Some((
                Arc::<str>::from(class_name.as_str()),
                Arc::<str>::from(static_class_alias.as_str()),
            ));
        }

        if synthesize_constructor {
            // Increment function_scope_depth so async arrow functions inside
            // the synthesized constructor use `this` instead of `void 0` as
            // the __awaiter first argument.
            self.function_scope_depth += 1;
            let prev_es5_super_home_depth = self.es5_super_home_function_depth;
            let prev_es5_super_home_static = self.es5_super_home_is_static;
            if self.ctx.target_es5 {
                self.es5_super_home_function_depth = Some(self.function_scope_depth);
                self.es5_super_home_is_static = false;
            }
            if has_extends && !extends_null {
                self.write("constructor() {");
                self.write_line();
                self.increase_indent();
                self.write("super(...arguments);");
                self.write_line();
            } else {
                self.write("constructor() {");
                self.write_line();
                self.increase_indent();
            }
            // Temps allocated while emitting field initializers in this synthesized
            // constructor body (e.g. a class-expression lowered inside a private-field
            // initializer needing `_a`) must be declared in this constructor's scope,
            // not leak to the enclosing function/file hoist list. Push a body temp
            // scope and capture the insertion anchor for the `var` line.
            self.push_temp_scope();
            let synth_ctor_hoist_anchor = self.capture_hoist_anchor();
            self.emit_constructor_prologue(
                &[],
                &field_inits,
                if lower_auto_accessors_to_weakmap {
                    &constructor_auto_accessor_instance_inits
                } else {
                    &[]
                },
            );
            // Insert any temps hoisted while emitting field initializers (e.g.
            // `_a` for a class expression lowered inside a private-field init, or
            // a `??=` read-cache temp) at the top of this synthesized constructor
            // body, then drop the scope. Value temps are inserted last so they
            // land on the first line.
            let synth_ctor_indent = self
                .writer
                .indent_string_at(synth_ctor_hoist_anchor.indent_level);
            let assignment_temps = std::mem::take(&mut self.hoisted_assignment_temps);
            self.insert_hoisted_var_line(
                &assignment_temps,
                &synth_ctor_hoist_anchor,
                &synth_ctor_indent,
            );
            let value_temps = std::mem::take(&mut self.hoisted_assignment_value_temps);
            self.insert_hoisted_var_line(
                &value_temps,
                &synth_ctor_hoist_anchor,
                &synth_ctor_indent,
            );
            self.pop_temp_scope();
            self.decrease_indent();
            self.write("}");
            self.write_line();
            self.es5_super_home_function_depth = prev_es5_super_home_depth;
            self.es5_super_home_is_static = prev_es5_super_home_static;
            self.function_scope_depth -= 1;
        }

        let target_supports_native_fields =
            (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32);
        let target_supports_native_private_names =
            (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32);
        let has_legacy_private_name_member_decorators = self.ctx.options.legacy_decorators
            && !class_name.is_empty()
            && class.members.nodes.iter().any(|&member_idx| {
                self.legacy_member_decorator_needs_private_name_scope(member_idx)
            });

        // When useDefineForClassFields is true AND target >= ES2022 (native class fields),
        // emit parameter property field declarations (e.g. `foo;`) at the beginning of
        // the class body. TSC emits these before any other class members.
        // When target < ES2022, fields are lowered to the constructor body, so no
        // class-body declaration is needed.
        self.emit_class_es6_members(ClassEs6MemberEmit {
            node,
            class,
            assignment_alias,
            class_name: &class_name,
            needs_class_field_lowering,
            needs_static_block_lowering,
            lower_auto_accessors_to_private_fields,
            target_supports_native_fields,
            target_supports_native_private_names,
            has_legacy_private_name_member_decorators,
            needs_computed_prop_hoisting,
            erased_computed_side_effects_use_static_block,
            native_computed_prop_evaluator: native_computed_prop_evaluator.as_deref(),
            private_fields: &private_fields,
            private_duplicate_conflicts: &private_duplicate_conflicts,
            auto_accessor_member_map: &auto_accessor_member_map,
            auto_accessor_class_alias: auto_accessor_class_alias.as_deref(),
            auto_accessor_computed_storage_key_member,
            auto_accessor_instance_storage_inits_in_computed_key:
                &auto_accessor_instance_storage_inits_in_computed_key,
            hoisted_native_auto_accessor_members: &hoisted_native_auto_accessor_members,
            hoisted_native_private_members: &hoisted_native_private_members,
            field_inits: &mut field_inits,
            static_field_inits: &mut static_field_inits,
            deferred_static_blocks: &mut deferred_static_blocks,
            computed_property_side_effects: &mut computed_property_side_effects,
        });

        self.scoped_class_expression_self_alias_ancestors
            .truncate(scoped_class_expression_self_alias_ancestor_len);
        self.scoped_class_expression_self_alias = prev_scoped_class_expression_self_alias;

        let computed_side_effects_emitted_in_static_block =
            !computed_property_side_effects.is_empty() && class_expr_temp.is_none();
        if computed_side_effects_emitted_in_static_block {
            self.write("static { ");
            for (i, expr_idx) in computed_property_side_effects.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.emit_expression(*expr_idx);
            }
            self.write("; }");
            self.write_line();
        }

        // Skip orphaned comments inside the class body.
        // When class members are erased (type-only properties, abstract members, etc.),
        // comments on lines between erased members or between the last erased member
        // and the closing `}` are left unconsumed. Without this, they leak into the
        // output as spurious comments after the class.
        // Find the closing `}` position and skip any remaining comments before it.
        {
            let class_body_end = self.find_token_end_before_trivia(node.pos, node.end);
            while self.comment_emit_idx < self.all_comments.len() {
                let c = &self.all_comments[self.comment_emit_idx];
                if c.end <= class_body_end {
                    self.comment_emit_idx += 1;
                } else {
                    break;
                }
            }
        }

        // Restore field inits
        self.pending_class_field_inits = prev_field_inits;
        self.pending_auto_accessor_inits = prev_auto_accessor_inits;

        // Undo the extra indent level added for comma expression wrapper
        if class_expr_temp.is_some() {
            self.decrease_indent();
        }
        self.decrease_indent();
        if class_expr_temp.is_some() {
            // Indent the closing brace inside the comma-expression context
            self.increase_indent();
            self.write("}");
            self.decrease_indent();
        } else {
            self.write("}");
        }
        if assignment_prefix.is_some() && class_expr_temp.is_none() {
            self.write(assignment_suffix.unwrap_or(";"));
        }

        if class_expr_temp.is_none() {
            for stmt in self.recovered_class_body_statements(node) {
                self.write_line();
                self.write(&stmt);
            }
            if self.class_has_recovered_void_extends(&class.heritage_clauses) {
                self.write_line();
                self.write("void {};");
            }
        }

        if self
            .pending_commonjs_class_export_name
            .as_ref()
            .is_some_and(|(class_idx, _, _)| *class_idx == _idx)
        {
            let (_, local_name, export_names) = self
                .pending_commonjs_class_export_name
                .take()
                .expect("pending class export should be present");
            for export_name in export_names {
                self.write_line();
                self.write("exports.");
                self.write(&export_name);
                self.write(" = ");
                self.write(&local_name);
                self.write(";");
            }
        }

        self.emit_class_es6_after_body(ClassEs6AfterBody {
            node,
            class,
            class_name: &class_name,
            class_name_is_real,
            assignment_prefix_is_some: assignment_prefix.is_some(),
            static_initializer_self_alias,
            static_initializer_this_binding,
            static_initializer_super_base,
            externalized_static_initializer_uses_undefined_receiver,
            computed_side_effects_emitted_in_static_block,
            class_expr_comma_needs_parens,
            needs_native_computed_prop_evaluator_comma_expr,
            needs_computed_prop_comma_expr,
            needs_static_comma_expr,
            needs_private_comma_expr,
            has_any_private_lowering,
            private_member_def_needs_class_alias,
            lower_auto_accessors_to_weakmap,
            emit_auto_accessor_instance_inits_after_class,
            target_supports_native_private_names,
            has_legacy_private_name_member_decorators,
            class_expr_temp,
            class_expr_static_temp,
            default_export_set_function_name_temp,
            static_initializer_class_alias,
            class_value_alias,
            auto_accessor_class_alias,
            class_expr_set_function_name,
            computed_prop_entries,
            computed_prop_entries_consumed_by_member_name,
            computed_property_side_effects,
            static_field_inits,
            deferred_static_blocks,
            private_auto_accessors: &private_auto_accessors,
            auto_accessor_instance_inits: &auto_accessor_instance_inits,
            auto_accessor_static_inits: &auto_accessor_static_inits,
        });

        // Restore private field state (for nested classes)
        self.private_field_weakmaps = prev_private_field_weakmaps;
        self.pending_weakmap_inits = prev_pending_weakmap_inits;
        self.pending_static_private_inits = prev_pending_static_private_inits;
        self.pending_private_class_alias = prev_pending_private_class_alias;
        self.private_member_info = prev_private_member_info;
        self.pending_private_field_constructor_inits = prev_pending_private_field_constructor_inits;
        self.pending_instances_weakset_add = prev_pending_instances_weakset_add;
        self.pending_private_method_defs = prev_pending_private_method_defs;
        self.pending_private_accessor_defs = prev_pending_private_accessor_defs;
        self.private_members_to_skip = prev_private_members_to_skip;
        self.private_static_class_alias = prev_private_static_class_alias;

        // Clear computed property temp map to avoid leaking to the next class.
        self.computed_prop_temp_map.clear();

        // Track class name to prevent duplicate var declarations for merged namespaces.
        // When a class and namespace have the same name (declaration merging), the class
        // provides the declaration, so the namespace shouldn't emit `var name;`.
        if class.name.is_some() {
            let class_name = self.get_identifier_text_idx(class.name);
            if !class_name.is_empty() {
                self.declared_namespace_names.insert(class_name);
            }
        }
    }
}
