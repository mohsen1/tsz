use super::super::super::core::PropertyNameEmit;
use super::super::super::{Printer, ScriptTarget};
use super::replace_identifier;
use super::{AutoAccessorEmitOptions, AutoAccessorInfo, StaticFieldInit};
use crate::emitter::core::PrivateMemberInfo;
use crate::transforms::private_fields_es5::{
    PrivateAccessorInfo, PrivateFieldInfo, PrivateMethodInfo,
    collect_enclosing_source_binding_names, collect_private_accessors_with_reserved,
    collect_private_fields_with_reserved, collect_private_methods_with_reserved,
    get_private_field_name, is_private_identifier, make_unique_private_name,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::Arc;
use tsz_parser::parser::node::{ClassData, Node, NodeAccess};
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_parser::syntax::transform_utils::{
    contains_async_arrow_function, contains_super_reference, contains_this_reference,
};
use tsz_scanner::SyntaxKind;

#[derive(Debug, Clone)]
struct PrivateAutoAccessorInfo {
    member_idx: NodeIndex,
    name: String,
    get_var_name: String,
    set_var_name: String,
    storage_name: String,
    initializer: Option<NodeIndex>,
    is_static: bool,
}

fn collect_private_auto_accessors_with_reserved(
    printer: &Printer<'_>,
    class: &ClassData,
    class_name: &str,
    used_names: &mut rustc_hash::FxHashSet<String>,
) -> Vec<PrivateAutoAccessorInfo> {
    if class_name.is_empty() {
        return Vec::new();
    }

    let mut accessors = Vec::new();
    for &member_idx in &class.members.nodes {
        let Some(member_node) = printer.arena.get(member_idx) else {
            continue;
        };
        let Some(prop) = printer.arena.get_property_decl(member_node) else {
            continue;
        };
        if !printer
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
        {
            continue;
        }
        if printer
            .arena
            .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
            || printer
                .arena
                .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
        {
            continue;
        }
        if !is_private_identifier(printer.arena, prop.name) {
            continue;
        }

        let Some(field_name) = get_private_field_name(printer.arena, prop.name) else {
            continue;
        };
        let clean_name = field_name.strip_prefix('#').unwrap_or(&field_name);
        let base = format!("_{class_name}_{clean_name}");
        let get_var_name = make_unique_private_name(&format!("{base}_get"), used_names);
        let set_var_name = make_unique_private_name(&format!("{base}_set"), used_names);
        let storage_name = if printer.ctx.options.legacy_decorators {
            used_names.insert(base.clone());
            let storage_stem = make_unique_private_name(&base, used_names);
            make_unique_private_name(&format!("{storage_stem}_accessor_storage"), used_names)
        } else {
            make_unique_private_name(&format!("{base}_accessor_storage"), used_names)
        };
        accessors.push(PrivateAutoAccessorInfo {
            member_idx,
            name: clean_name.to_string(),
            get_var_name,
            set_var_name,
            storage_name,
            initializer: if prop.initializer.is_none() {
                None
            } else {
                Some(prop.initializer)
            },
            is_static: printer.has_effective_static_modifier_js(&prop.modifiers),
        });
    }
    accessors
}

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn class_expression_is_in_loop_body(&self, idx: NodeIndex) -> bool {
        let mut current = idx;
        while let Some(ext) = self.arena.get_extended(current) {
            let parent = ext.parent;
            if parent.is_none() {
                return false;
            }

            let Some(current_node) = self.arena.get(current) else {
                return false;
            };
            let Some(parent_node) = self.arena.get(parent) else {
                return false;
            };

            if current_node.kind == syntax_kind_ext::BLOCK
                && (parent_node.kind == syntax_kind_ext::FOR_STATEMENT
                    || parent_node.kind == syntax_kind_ext::FOR_IN_STATEMENT
                    || parent_node.kind == syntax_kind_ext::FOR_OF_STATEMENT
                    || parent_node.kind == syntax_kind_ext::WHILE_STATEMENT
                    || parent_node.kind == syntax_kind_ext::DO_STATEMENT)
            {
                return true;
            }

            if parent_node.kind == syntax_kind_ext::SOURCE_FILE
                || parent_node.kind == syntax_kind_ext::FUNCTION_DECLARATION
                || parent_node.kind == syntax_kind_ext::FUNCTION_EXPRESSION
                || parent_node.kind == syntax_kind_ext::ARROW_FUNCTION
            {
                return false;
            }

            current = parent;
        }

        false
    }

    fn is_reserved_private_constructor_name(name: &str) -> bool {
        name == "constructor"
    }

    fn emit_private_auto_accessor_function_def(
        &mut self,
        var_name: &str,
        storage_name: &str,
        is_static: bool,
        is_get: bool,
        class_alias: Option<&str>,
    ) {
        self.write(var_name);
        self.write(" = function ");
        self.write(var_name);
        self.write("(");
        if !is_get {
            self.write("value");
        }
        self.write(") { ");
        if is_get {
            self.write("return ");
            self.write_helper("__classPrivateFieldGet");
        } else {
            self.write_helper("__classPrivateFieldSet");
        }
        self.write("(");
        if is_static {
            let alias = class_alias.unwrap_or("this");
            self.write(alias);
            self.write(", ");
            self.write(alias);
            if is_get {
                self.write(", \"f\", ");
                self.write(storage_name);
            } else {
                self.write(", value, \"f\", ");
                self.write(storage_name);
            }
        } else {
            self.write("this, ");
            self.write(storage_name);
            if is_get {
                self.write(", \"f\"");
            } else {
                self.write(", value, \"f\"");
            }
        }
        self.write("); }");
    }

    /// Emit a class using ES6 native class syntax (no transforms).
    /// This is the pure emission logic that can be reused by both the old API
    /// and the new transform system.
    pub(in crate::emitter) fn emit_class_es6(&mut self, node: &Node, idx: NodeIndex) {
        self.emit_class_es6_with_options(node, idx, false, None, None, false);
    }

    pub(in crate::emitter) fn emit_class_es6_with_options(
        &mut self,
        node: &Node,
        _idx: NodeIndex,
        suppress_modifiers: bool,
        assignment_prefix: Option<(&str, String)>,
        static_initializer_self_alias: Option<&str>,
        emit_assignment_static_elements_as_statements: bool,
    ) {
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

        if !suppress_modifiers
            && self.ctx.options.target == ScriptTarget::ESNext
            && self.has_recovered_accessor_modifier(node)
        {
            self.write("accessor ");
        }

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

        // Emit modifiers (including decorators) - skip TS-only modifiers for JS output
        if !suppress_modifiers && let Some(ref modifiers) = class.modifiers {
            for &mod_idx in &modifiers.nodes {
                if let Some(mod_node) = self.arena.get(mod_idx) {
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

        // Private field lowering: when target < ES2022, transform #fields to WeakMap pattern
        let needs_private_field_lowering = !self.ctx.options.target.supports_es2022()
            && self.ctx.options.target != ScriptTarget::ESNext;
        let mut used_private_names = if needs_private_field_lowering {
            collect_enclosing_source_binding_names(self.arena, _idx)
        } else {
            rustc_hash::FxHashSet::default()
        };
        let private_fields: Vec<PrivateFieldInfo> = if needs_private_field_lowering {
            collect_private_fields_with_reserved(
                self.arena,
                _idx,
                &class_name,
                &mut used_private_names,
            )
        } else {
            Vec::new()
        };
        let private_methods: Vec<PrivateMethodInfo> = if needs_private_field_lowering {
            collect_private_methods_with_reserved(
                self.arena,
                _idx,
                &class_name,
                &mut used_private_names,
            )
        } else {
            Vec::new()
        };
        let private_accessors: Vec<PrivateAccessorInfo> = if needs_private_field_lowering {
            collect_private_accessors_with_reserved(
                self.arena,
                _idx,
                &class_name,
                &mut used_private_names,
            )
        } else {
            Vec::new()
        };
        let private_auto_accessors: Vec<PrivateAutoAccessorInfo> =
            if needs_private_field_lowering && lower_auto_accessors_to_weakmap {
                collect_private_auto_accessors_with_reserved(
                    self,
                    class,
                    &class_name,
                    &mut used_private_names,
                )
            } else {
                Vec::new()
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
                &format!("_{class_name}_instances"),
                &mut used_private_names,
            ))
        } else {
            None
        };

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

        // Determine if we need a class alias for static private fields.
        let class_value_alias = if has_static_privates
            || static_initializer_needs_this_alias
            || static_initializer_needs_class_alias
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
        let static_initializer_class_alias = if static_initializer_needs_this_alias
            || static_initializer_needs_class_alias
            || private_member_def_needs_class_alias
        {
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
            // once and look up each member's pre-computed info entry by
            // private-identifier name. Accessors share a private name across
            // get/set, so we dedupe within an accessor pair.
            let mut emitted_accessor_names: rustc_hash::FxHashSet<String> =
                rustc_hash::FxHashSet::default();
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                let private_name = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(member_node)
                        .and_then(|p| get_private_field_name(self.arena, p.name)),
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .and_then(|m| get_private_field_name(self.arena, m.name)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .and_then(|a| get_private_field_name(self.arena, a.name))
                    }
                    _ => None,
                };
                let Some(private_name) = private_name else {
                    continue;
                };
                let clean_name = private_name.strip_prefix('#').unwrap_or(&private_name);
                match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        if let Some(accessor) = private_auto_accessors
                            .iter()
                            .find(|a| a.member_idx == member_idx)
                        {
                            var_names.push(accessor.get_var_name.clone());
                            var_names.push(accessor.set_var_name.clone());
                        } else if let Some(field) =
                            private_fields.iter().find(|f| f.name == clean_name)
                        {
                            var_names.push(field.weakmap_name.clone());
                        }
                    }
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        if let Some(method) = private_methods.iter().find(|m| m.name == clean_name)
                        {
                            var_names.push(method.fn_var_name.clone());
                        }
                    }
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        if !emitted_accessor_names.insert(clean_name.to_string()) {
                            continue;
                        }
                        if let Some(accessor) =
                            private_accessors.iter().find(|a| a.name == clean_name)
                        {
                            if let Some(ref name) = accessor.get_var_name
                                && accessor.getter_body.is_some()
                            {
                                var_names.push(name.clone());
                            }
                            if let Some(ref name) = accessor.set_var_name
                                && accessor.setter_body.is_some()
                            {
                                var_names.push(name.clone());
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
                // Hoist private field vars to the top of the scope (after "use strict"
                // and CJS preamble), matching tsc behavior. tsc emits all private field
                // WeakMap/method vars before the first class in the scope.
                if node.kind == syntax_kind_ext::CLASS_EXPRESSION
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
                            .filter(|_| accessor.getter_body.is_some())
                            .cloned(),
                        setter_ref: accessor
                            .set_var_name
                            .as_ref()
                            .filter(|_| accessor.setter_body.is_some())
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
                .map(|f| (f.weakmap_name.clone(), f.initializer))
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
                .map(|f| (f.weakmap_name.clone(), f.has_initializer, f.initializer))
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
                if let Some(body_idx) = method.body {
                    self.pending_private_method_defs.push((
                        method.fn_var_name.clone(),
                        body_idx,
                        method.parameters.clone(),
                    ));
                }
            }

            // Prepare private accessor function defs for after the class body
            // Both instance and static private accessors are extracted.
            for accessor in &private_accessors {
                if let Some(body_idx) = accessor.getter_body
                    && let Some(ref var_name) = accessor.get_var_name
                {
                    self.pending_private_accessor_defs.push(
                        crate::emitter::core::PrivateAccessorDef {
                            var_name: var_name.clone(),
                            body: body_idx,
                            param: None,
                        },
                    );
                }
                if let Some(body_idx) = accessor.setter_body
                    && let Some(ref var_name) = accessor.set_var_name
                {
                    self.pending_private_accessor_defs.push(
                        crate::emitter::core::PrivateAccessorDef {
                            var_name: var_name.clone(),
                            body: body_idx,
                            param: accessor.setter_param,
                        },
                    );
                }
            }

            // Mark all private methods and accessors (instance + static) to skip from class body
            for method in &private_methods {
                if Self::is_reserved_private_constructor_name(&method.name) {
                    continue;
                }
                self.private_members_to_skip.insert(method.name.clone());
            }
            for accessor in &private_accessors {
                self.private_members_to_skip.insert(accessor.name.clone());
            }
            for accessor in &private_auto_accessors {
                self.private_members_to_skip.insert(accessor.name.clone());
            }
        }

        // For class expressions with private field lowering, we need to wrap the class
        // in a comma expression: `(_a = class C { ... }, _WeakMap = new WeakMap(), ..., _a)`
        // tsc uses this pattern so the WeakMap/WeakSet initialization happens inline.
        let is_class_expression = node.kind == syntax_kind_ext::CLASS_EXPRESSION;
        let emits_as_class_expression = is_class_expression || assignment_prefix.is_some();
        let needs_private_comma_expr = is_class_expression && has_any_private_lowering;

        // Computed property name hoisting for targets < ES2022.
        // tsc hoists non-constant computed property name expressions to temp variables
        // (e.g., `_a = n, _b = s + n`) so that the evaluation order is preserved and
        // the class body can reference the temp instead of the original expression.
        //
        // Only PROPERTY DECLARATIONS with computed names participate in hoisting.
        // Methods and accessors keep their computed names inline in ES6+.
        // After the class body, a comma expression joins all assignments and side effects.
        let needs_computed_prop_hoisting =
            (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32);
        // Each entry: (Option<temp_name>, expr_idx, member_idx) — None means side-effect only
        let mut computed_prop_entries: Vec<(Option<String>, NodeIndex, NodeIndex)> = Vec::new();
        if needs_computed_prop_hoisting {
            for &member_idx in &class.members.nodes {
                let Some(member_node) = self.arena.get(member_idx) else {
                    continue;
                };
                // Only property declarations participate in computed property hoisting
                if member_node.kind != syntax_kind_ext::PROPERTY_DECLARATION {
                    continue;
                }
                let Some(prop) = self.arena.get_property_decl(member_node) else {
                    continue;
                };
                let Some(name_node) = self.arena.get(prop.name) else {
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
                // Check if this property is erased (type-only, abstract, declared).
                // `declare` fields have no runtime effect even when an
                // initializer is present (the initializer is part of the
                // declaration and is dropped). tsc still emits the computed
                // expression for its side effects, but does not allocate a
                // temp — see the `esDecorators-classDeclaration-fields-staticAmbient`
                // baseline where `static declare [field3] = 3;` produces
                // no `var _a; _a = field3;` pair.
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
                    prop.initializer.is_none() && !is_private && !has_accessor
                };
                if is_erased {
                    // Side-effect only: expression is emitted for its effects but no temp.
                    let is_side_effect_free =
                        self.is_computed_name_expr_side_effect_free(computed.expression);
                    if !is_side_effect_free {
                        computed_prop_entries.push((None, computed.expression, member_idx));
                    }
                } else {
                    // Allocate a temp variable for this computed property name
                    let temp = self.make_unique_name_hoisted();
                    self.computed_prop_temp_map
                        .insert(computed.expression, temp.clone());
                    computed_prop_entries.push((Some(temp), computed.expression, member_idx));
                }
            }
        }

        // For class expressions with static field initializers, we need to wrap
        // in a comma expression: `(_a = class C {}, _a.a = 1, _a)`.
        // Allocate the class-expression temp after computed-name temps so the
        // generated `_a`, `_b`, `_c` ordering matches tsc.
        // Positive-form predicate reads more clearly than clippy's inverted De Morgan form.
        #[allow(clippy::nonminimal_bool)]
        let has_static_field_comma_expr = target_needs_field_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena.get(member_idx).is_some_and(|m| {
                    m.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && self.arena.get_property_decl(m).is_some_and(|p| {
                            self.arena.is_static(&p.modifiers)
                                && !self
                                    .arena
                                    .has_modifier(&p.modifiers, SyntaxKind::AbstractKeyword)
                                && !self
                                    .arena
                                    .has_modifier(&p.modifiers, SyntaxKind::DeclareKeyword)
                                && !(needs_private_field_lowering
                                    && is_private_identifier(self.arena, p.name))
                        })
                })
            });
        let has_static_block_comma_expr = target_needs_static_block_lowering
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena
                    .get(member_idx)
                    .is_some_and(|m| m.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION)
            });
        let has_static_computed_method_or_accessor = emits_as_class_expression
            && class.name.is_none()
            && self.resolve_class_expr_binding_name(_idx).is_some()
            && class.members.nodes.iter().any(|&member_idx| {
                self.arena
                    .get(member_idx)
                    .is_some_and(|member| match member.kind {
                        k if k == syntax_kind_ext::METHOD_DECLARATION => {
                            self.arena.get_method_decl(member).is_some_and(|method| {
                                self.arena.is_static(&method.modifiers)
                                    && self.arena.get(method.name).is_some_and(|name| {
                                        name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                                    })
                            })
                        }
                        k if k == syntax_kind_ext::GET_ACCESSOR
                            || k == syntax_kind_ext::SET_ACCESSOR =>
                        {
                            self.arena.get_accessor(member).is_some_and(|accessor| {
                                self.arena.is_static(&accessor.modifiers)
                                    && self.arena.get(accessor.name).is_some_and(|name| {
                                        name.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                                    })
                            })
                        }
                        _ => false,
                    })
            });
        let needs_static_comma_expr = emits_as_class_expression
            && !emit_assignment_static_elements_as_statements
            && (has_static_field_comma_expr
                || has_static_block_comma_expr
                || has_static_computed_method_or_accessor);
        let needs_computed_prop_comma_expr =
            emits_as_class_expression && !computed_prop_entries.is_empty();
        let needs_any_comma_expr =
            needs_static_comma_expr || needs_private_comma_expr || needs_computed_prop_comma_expr;
        let class_expr_comma_needs_parens = needs_any_comma_expr
            && self
                .arena
                .get_extended(_idx)
                .and_then(|ext| self.arena.get(ext.parent))
                .is_none_or(|parent| {
                    parent.kind != syntax_kind_ext::RETURN_STATEMENT
                        && parent.kind != syntax_kind_ext::PARENTHESIZED_EXPRESSION
                });
        let class_expr_temp = if needs_any_comma_expr {
            let temp = if let Some(ref alias) = private_class_alias {
                alias.clone()
            } else {
                self.make_class_static_temp_name_hoisted(_idx)
            };
            if class_expr_comma_needs_parens {
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
        // tsc emits `__setFunctionName(temp, "C")` for an anonymous class
        // expression only when the comma wrapper carries *static* state
        // (a static field initializer, a static block, or a static
        // private field that lowers into the same comma). Instance-only
        // private comma forms — e.g.
        // `(_a = class { #x; }, _C_x = new WeakMap(), _a)` — keep the
        // engine's automatic assignment-based naming and tsc does not
        // emit the helper. Mirror this so the helper inclusion decision
        // in lowering and the comma-item emission in the printer agree.
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
        let needs_set_function_name_comma_item =
            needs_static_comma_expr || has_static_private_member;
        let class_expr_set_function_name = class_expr_temp.as_ref().and_then(|_| {
            if class.name.is_none() && needs_set_function_name_comma_item {
                self.resolve_class_expr_binding_name(_idx)
            } else {
                None
            }
        });

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
                if pending_computed_entries.is_empty() {
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
                comma_parts.push(self.capture_emit(computed.expression));
                self.computed_prop_temp_map
                    .insert(computed.expression, format!("({})", comma_parts.join(", ")));
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
        let has_legacy_class_decorators = self.ctx.options.legacy_decorators
            && !self.collect_class_decorators(&class.modifiers).is_empty();
        let externalized_static_initializer_uses_undefined_receiver =
            !is_class_expression && needs_static_block_lowering && has_legacy_class_decorators;

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

        self.write("class");

        // Determine the class expression name.
        // When assignment_prefix is provided (e.g., `let C = class C {}`), a named class
        // keeps its name on the expression, but an anonymous class stays anonymous
        // (`let default_1 = class {}`), even if anonymous_default_export_name is set.
        if class.name.is_some() {
            self.write_space();
            self.emit_decl_name(class.name);
        } else if assignment_prefix.is_none() {
            // No assignment prefix — use anonymous_default_export_name if available
            // (e.g., `export default class {}` → `class default_1 {}`)
            let override_name = self.anonymous_default_export_name.clone();
            if let Some(name) = override_name
                && !name.is_empty()
            {
                self.write_space();
                self.write(&name);
            }
        }

        if let Some(ref heritage_clauses) = class.heritage_clauses {
            for &clause_idx in &heritage_clauses.nodes {
                let Some(clause_node) = self.arena.get(clause_idx) else {
                    continue;
                };
                let Some(heritage) = self.arena.get_heritage(clause_node) else {
                    continue;
                };
                if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                    continue;
                }

                if !heritage.types.nodes.is_empty() {
                    self.write(" extends ");
                    for (i, &extends_type) in heritage.types.nodes.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        if let Some(base_alias) = static_super_base_alias.as_ref() {
                            self.write("(");
                            self.write(base_alias);
                            self.write(" = ");
                            self.emit_heritage_expression(extends_type);
                            self.write(")");
                        } else {
                            self.emit_heritage_expression(extends_type);
                        }
                    }
                } else {
                    // Error recovery: source has `extends` with no base type.
                    // Preserve the keyword to match tsc output.
                    self.write(" extends ");
                }
            }
        }

        self.write(" {");
        // Suppress trailing comments on class body opening brace.
        // tsc drops same-line comments on `{` for class bodies, just like function
        // bodies (e.g. `class C { // error` → `class C {`).
        if !self.ctx.options.remove_comments
            && let Some(text) = self.source_text
        {
            let bytes = text.as_bytes();
            let start = node.pos as usize;
            let end = (node.end as usize).min(bytes.len());
            if let Some(offset) = bytes[start..end].iter().position(|&b| b == b'{') {
                let brace_end = (start + offset + 1) as u32;
                // Only suppress if there's a newline between `{` and the first
                // member (or the closing `}` if empty).  Single-line class bodies
                // like `class C { x: T; } // error` have the comment after `}`,
                // so we must NOT suppress it.
                // For empty classes like `class C {} // comment`, scan_end must
                // be the closing `}` position, not node.end — otherwise a newline
                // after `}` (before the next statement) causes us to incorrectly
                // suppress the trailing comment that belongs to `}`.
                let scan_end = class
                    .members
                    .nodes
                    .first()
                    .and_then(|&idx| self.arena.get(idx))
                    .map_or_else(
                        || {
                            // Empty class: find the closing `}` to use as scan_end
                            let be = brace_end as usize;
                            if be <= end {
                                bytes[be..end]
                                    .iter()
                                    .position(|&b| b == b'}')
                                    .map_or(end, |p| be + p)
                            } else {
                                end
                            }
                        },
                        |m| m.pos as usize,
                    );
                let brace_end_usize = brace_end as usize;
                let scan_end_clamped = scan_end.min(end);
                let has_newline = if brace_end_usize <= scan_end_clamped {
                    bytes[brace_end_usize..scan_end_clamped]
                        .iter()
                        .any(|&b| b == b'\n' || b == b'\r')
                } else {
                    // Malformed source: first member pos precedes the opening
                    // brace we found — skip the suppression heuristic.
                    false
                };
                if has_newline {
                    self.skip_trailing_same_line_comments(brace_end, node.end);
                }
            }
        }
        self.write_line();
        self.increase_indent();
        // When inside a comma expression wrapper (class expression with private fields
        // or static fields), add one extra indent level for the class body to match tsc.
        if class_expr_temp.is_some() {
            self.increase_indent();
        }

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

        // Collect property initializers that need lowering
        // (name, initializer_idx, init_end, leading_comments, trailing_comments)
        // Comments are collected eagerly here so they're available even
        // when the constructor appears before the property in source order.
        let mut field_inits: Vec<crate::emitter::core::FieldInit> = Vec::new();
        let mut static_field_inits: Vec<StaticFieldInit> = Vec::new();
        let mut hoisted_native_private_members: FxHashSet<NodeIndex> = FxHashSet::default();
        let mut hoisted_native_auto_accessor_members: FxHashSet<NodeIndex> = FxHashSet::default();
        if needs_class_field_lowering {
            let members = &class.members.nodes;
            for (member_i, &member_idx) in members.iter().enumerate() {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.arena.get_property_decl(member_node)
                {
                    // With useDefineForClassFields, fields without initializers
                    // are still materialized at runtime as
                    // `Object.defineProperty(this, "name", { value: void 0 })`.
                    // Without that flag the typed-only declaration has no
                    // runtime effect, so skip it.
                    let no_initializer_node = prop.initializer.is_none();
                    let materialize_no_init =
                        no_initializer_node && self.ctx.options.use_define_for_class_fields;
                    if !materialize_no_init
                        && (no_initializer_node
                            || !self.class_property_initializer_has_equals(member_node, prop))
                    {
                        continue;
                    }
                    if self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                        || self
                            .arena
                            .has_modifier(&prop.modifiers, SyntaxKind::DeclareKeyword)
                    {
                        continue;
                    }
                    // Skip private fields when they're being lowered to WeakMap pattern.
                    // They're handled separately via pending_private_field_constructor_inits.
                    if !private_fields.is_empty() && is_private_identifier(self.arena, prop.name) {
                        continue;
                    }
                    let is_private_name = is_private_identifier(self.arena, prop.name);
                    let is_auto_accessor = self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword);
                    if !needs_private_field_lowering && is_private_name {
                        if !hoist_native_instance_order_inits
                            || self.has_effective_static_modifier_js(&prop.modifiers)
                        {
                            continue;
                        }
                    }
                    if is_auto_accessor
                        && (!hoist_native_instance_order_inits
                            || self.has_effective_static_modifier_js(&prop.modifiers))
                    {
                        continue;
                    }
                    // If the property has a computed name with a hoisted temp, use the temp
                    // variable name. This takes priority over get_property_name_emit because
                    // the temp captures the expression value at class-evaluation time.
                    let mut name_emit = if let Some(name_node) = self.arena.get(prop.name)
                        && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.arena.get_computed_property(name_node)
                        && let Some(temp) = self.computed_prop_temp_map.get(&computed.expression)
                    {
                        Some(PropertyNameEmit::Bracket(temp.clone()))
                    } else {
                        self.get_property_name_emit(prop.name)
                    };
                    if hoist_native_instance_order_inits && is_auto_accessor {
                        if let Some((storage_name, _)) = auto_accessor_member_map.get(&member_idx) {
                            name_emit = Some(PropertyNameEmit::Dot(format!("#{storage_name}")));
                            hoisted_native_auto_accessor_members.insert(member_idx);
                        }
                    } else if hoist_native_instance_order_inits && is_private_name {
                        if let Some(private_name) = get_private_field_name(self.arena, prop.name) {
                            name_emit = Some(PropertyNameEmit::Dot(private_name));
                            hoisted_native_private_members.insert(member_idx);
                        }
                    }
                    let Some(name_emit) = name_emit else {
                        continue;
                    };

                    // Pre-collect leading comments for this property declaration.
                    // Use the actual token end of the previous member (not its
                    // `end` field which can overshoot into the next member's trivia)
                    // so the range doesn't invert.
                    let leading_comments = if !self.ctx.options.remove_comments {
                        let prev_end = if member_i > 0 {
                            members
                                .get(member_i - 1)
                                .and_then(|&prev_idx| self.arena.get(prev_idx))
                                .map_or(member_node.pos, |prev| {
                                    self.find_token_end_before_trivia(prev.pos, prev.end)
                                })
                        } else {
                            member_node.pos.saturating_sub(64)
                        };
                        self.collect_leading_comments_in_range(prev_end, member_node.pos)
                    } else {
                        Vec::new()
                    };

                    // Pre-collect trailing comments for this property declaration.
                    let trailing_comments = if !self.ctx.options.remove_comments {
                        let skip_end = members
                            .get(member_i + 1)
                            .and_then(|&next_idx| self.arena.get(next_idx))
                            .map_or(member_node.end, |next| next.pos);
                        let actual_end =
                            self.find_token_end_before_trivia(member_node.pos, skip_end);
                        self.collect_trailing_comments_in_range(actual_end)
                    } else {
                        Vec::new()
                    };

                    if self.has_effective_static_modifier_js(&prop.modifiers) {
                        // At ES2022+, static fields are emitted as `static { this.f = v; }`
                        // blocks inside the class body, not as external assignments.
                        if !needs_static_block_lowering {
                            // Don't collect for external emission; these will be
                            // emitted inline as static initialization blocks.
                        } else {
                            static_field_inits.push((
                                name_emit,
                                prop.initializer,
                                member_node.pos,
                                Vec::new(), // leading_comments filled during class body emission
                                Vec::new(), // trailing_comments filled during class body emission
                            ));
                        }
                    } else {
                        // Non-static field inits use String names for `this.name = val`,
                        // `this["name"] = val`, or `this[0] = val`. Bracket names use
                        // a `[` prefix to signal bracket notation at emit time.
                        let ident_name = match &name_emit {
                            PropertyNameEmit::Dot(s) => s.clone(),
                            PropertyNameEmit::Bracket(s) | PropertyNameEmit::BracketNumeric(s) => {
                                format!("[{s}]")
                            }
                        };
                        let init_end = self
                            .arena
                            .get(prop.initializer)
                            .map_or(member_node.end, |n| n.end);
                        field_inits.push((
                            ident_name,
                            prop.initializer,
                            init_end,
                            leading_comments,
                            trailing_comments,
                        ));
                    }
                }
            }
        }

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
        let has_private_field_inits = private_fields.iter().any(|f| !f.is_static);
        let has_instances_weakset = self.pending_instances_weakset_add.is_some();
        let synthesize_constructor = !has_constructor
            && (!field_inits.is_empty()
                || (lower_auto_accessors_to_weakmap && !auto_accessor_instance_inits.is_empty())
                || (lower_auto_accessors_to_weakmap
                    && !constructor_auto_accessor_instance_inits.is_empty())
                || has_private_field_inits
                || has_instances_weakset);

        if synthesize_constructor {
            // Increment function_scope_depth so async arrow functions inside
            // the synthesized constructor use `this` instead of `void 0` as
            // the __awaiter first argument.
            self.function_scope_depth += 1;
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
            // Emit _X_instances.add(this) for private methods/accessors
            if let Some(ref ws_name) = self.pending_instances_weakset_add.clone() {
                self.write(ws_name);
                self.write(".add(this);");
                self.write_line();
            }
            // Private field WeakMap.set inits first (before non-private field inits)
            for field in &private_fields {
                if !field.is_static {
                    self.write(&field.weakmap_name);
                    self.write(".set(this, ");
                    if field.has_initializer {
                        self.emit_expression(field.initializer);
                    } else {
                        self.write("void 0");
                    }
                    self.write(");");
                    self.write_line();
                }
            }
            if lower_auto_accessors_to_weakmap {
                for (storage_name, init_idx) in &constructor_auto_accessor_instance_inits {
                    self.write(storage_name);
                    self.write(".set(this, ");
                    match init_idx {
                        Some(init) => {
                            self.with_scoped_static_initializer_context_cleared(|this| {
                                this.emit_expression(*init);
                            });
                        }
                        None => self.write("void 0"),
                    }
                    self.write(");");
                    self.write_line();
                }
            }
            // Non-private field inits after WeakMap.set calls
            for (name, init_idx, init_end, leading, trailing) in &field_inits {
                // Emit leading comments from the original property declaration
                for comment in leading {
                    self.write_comment(comment);
                    self.write_line();
                }
                if self.ctx.options.use_define_for_class_fields {
                    self.write("Object.defineProperty(this, ");
                    if name.starts_with('[') && name.ends_with(']') {
                        self.write(&name[1..name.len() - 1]);
                    } else {
                        self.emit_string_literal_text(name);
                    }
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("enumerable: true,");
                    self.write_line();
                    self.write("configurable: true,");
                    self.write_line();
                    self.write("writable: true,");
                    self.write_line();
                    self.write("value: ");
                    if init_idx.is_none() {
                        self.write("void 0");
                    } else {
                        if let Some(init_node) = self.arena.get(*init_idx) {
                            while self.comment_emit_idx < self.all_comments.len()
                                && self.all_comments[self.comment_emit_idx].end <= init_node.pos
                            {
                                self.comment_emit_idx += 1;
                            }
                        }
                        self.with_scoped_static_initializer_context_cleared(|this| {
                            this.emit_expression(*init_idx);
                        });
                    }
                    self.write_line();
                    self.decrease_indent();
                    self.write("});");
                } else {
                    if name.starts_with('[') {
                        self.write("this");
                        self.write(name);
                    } else {
                        self.write("this.");
                        self.write(name);
                    }
                    self.write(" = ");
                    if init_idx.is_none() {
                        self.write("void 0");
                    } else {
                        if let Some(init_node) = self.arena.get(*init_idx) {
                            while self.comment_emit_idx < self.all_comments.len()
                                && self.all_comments[self.comment_emit_idx].end <= init_node.pos
                            {
                                self.comment_emit_idx += 1;
                            }
                        }
                        self.with_scoped_static_initializer_context_cleared(|this| {
                            this.emit_expression(*init_idx);
                        });
                    }
                    self.write(";");
                    if !trailing.is_empty() {
                        for comment in trailing {
                            self.write_space();
                            self.write_comment(comment);
                        }
                    } else {
                        self.emit_trailing_comments(*init_end);
                    }
                }
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
            self.write_line();
            self.function_scope_depth -= 1;
        }

        // When useDefineForClassFields is true AND target >= ES2022 (native class fields),
        // emit parameter property field declarations (e.g. `foo;`) at the beginning of
        // the class body. TSC emits these before any other class members.
        // When target < ES2022, fields are lowered to the constructor body, so no
        // class-body declaration is needed.
        let mut emitted_any_member = false;
        let target_supports_native_fields =
            (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32);
        let target_supports_native_private_names =
            (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32);
        let has_legacy_private_name_member_decorators = self.ctx.options.legacy_decorators
            && !class_name.is_empty()
            && class.members.nodes.iter().any(|&member_idx| {
                self.legacy_member_decorator_needs_private_name_scope(member_idx)
            });
        if self.ctx.options.use_define_for_class_fields && target_supports_native_fields {
            // Find the constructor and collect its parameter properties
            for &member_idx in &class.members.nodes {
                if let Some(member_node) = self.arena.get(member_idx)
                    && member_node.kind == syntax_kind_ext::CONSTRUCTOR
                    && let Some(ctor) = self.arena.get_constructor(member_node)
                    && ctor.body.is_some()
                {
                    let param_props = self.collect_parameter_properties(&ctor.parameters.nodes);
                    for name in &param_props {
                        self.write(name);
                        self.write(";");
                        self.write_line();
                        emitted_any_member = true;
                    }
                    break;
                }
            }
        }
        // Compute the class body's closing `}` position so the last member's
        // trailing comment scan doesn't overshoot into comments belonging to
        // the closing brace line (same pattern as namespace IIFE emitter).
        let class_body_close_pos = self
            .source_text
            .map(|text| {
                let end = std::cmp::min(node.end as usize, text.len());
                let bytes = text.as_bytes();
                let mut pos = end;
                while pos > 0 {
                    pos -= 1;
                    if bytes[pos] == b'}' {
                        return pos as u32;
                    }
                }
                node.end
            })
            .unwrap_or(node.end);

        let mut field_init_comment_idx = 0usize;
        let prev_scoped_class_expression_self_alias =
            self.scoped_class_expression_self_alias.take();
        if let Some(temp) = class_expr_temp.as_ref() {
            if class_name_is_real && !class_name.is_empty() && class_name != *temp {
                self.scoped_class_expression_self_alias = Some((
                    Arc::<str>::from(class_name.as_str()),
                    Arc::<str>::from(temp.as_str()),
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
        for (member_i, &member_idx) in class.members.nodes.iter().enumerate() {
            // Skip private field declarations entirely when lowering to WeakMap pattern
            if !private_fields.is_empty()
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(member_node)
                && self
                    .arena
                    .get(prop.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
            {
                // Skip comments that belong to this erased member
                if let Some(mn) = self.arena.get(member_idx) {
                    let skip_end = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(mn.end, |next| next.pos);
                    while self.comment_emit_idx < self.all_comments.len()
                        && self.all_comments[self.comment_emit_idx].end <= skip_end
                    {
                        self.comment_emit_idx += 1;
                    }
                }
                continue;
            }
            // Skip private methods and accessors that are extracted as standalone functions
            if !self.private_members_to_skip.is_empty()
                && let Some(member_node) = self.arena.get(member_idx)
            {
                let should_skip = match member_node.kind {
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                        .arena
                        .get_property_decl(member_node)
                        .filter(|p| {
                            self.arena
                                .has_modifier(&p.modifiers, SyntaxKind::AccessorKeyword)
                        })
                        .and_then(|p| get_private_field_name(self.arena, p.name))
                        .map(|n| n.strip_prefix('#').unwrap_or(&n).to_string())
                        .is_some_and(|n| self.private_members_to_skip.contains(&n)),
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .and_then(|m| get_private_field_name(self.arena, m.name))
                        .map(|n| n.strip_prefix('#').unwrap_or(&n).to_string())
                        .is_some_and(|n| self.private_members_to_skip.contains(&n)),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .and_then(|a| get_private_field_name(self.arena, a.name))
                            .map(|n| n.strip_prefix('#').unwrap_or(&n).to_string())
                            .is_some_and(|n| self.private_members_to_skip.contains(&n))
                    }
                    _ => false,
                };
                if should_skip {
                    // When source has trailing `;` after private method/accessor
                    // (e.g., `#foo() { };`), tsc preserves the semicolon.
                    if let Some(mn) = self.arena.get(member_idx) {
                        let has_trailing_semi = self.source_text.is_some_and(|text| {
                            let start = mn.pos as usize;
                            let end = std::cmp::min(mn.end as usize, text.len());
                            if start >= end {
                                return false;
                            }
                            let member_text = text[start..end].trim_end();
                            if let Some(before_semi) = member_text.strip_suffix(';') {
                                before_semi.trim_end().ends_with('}')
                            } else {
                                false
                            }
                        });
                        if has_trailing_semi {
                            if !self.writer.is_at_line_start() {
                                self.write_line();
                            }
                            self.write(";");
                            self.write_line();
                            emitted_any_member = true;
                        }
                    }
                    if let Some(mn) = self.arena.get(member_idx) {
                        let skip_end = class
                            .members
                            .nodes
                            .get(member_i + 1)
                            .and_then(|&next_idx| self.arena.get(next_idx))
                            .map_or(mn.end, |next| next.pos);
                        while self.comment_emit_idx < self.all_comments.len()
                            && self.all_comments[self.comment_emit_idx].end <= skip_end
                        {
                            self.comment_emit_idx += 1;
                        }
                    }
                    continue;
                }
            }
            // Skip property declarations that were lowered
            if needs_class_field_lowering
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                    && let Some(prop) = self.arena.get_property_decl(member_node)
                    && !auto_accessor_member_map.contains_key(&member_idx)
                    && prop.initializer.is_some()
                    && !self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AbstractKeyword)
                    // Auto-accessor properties (`accessor x = 1`) that are NOT being
                    // lowered (e.g. at esnext target) must be preserved verbatim — they
                    // are not regular field declarations.
                    && !self
                        .arena
                        .has_modifier(&prop.modifiers, SyntaxKind::AccessorKeyword)
                // Private fields (#name) are emitted verbatim at ES2022+ — they
                // use native private field syntax and are unaffected by
                // useDefineForClassFields.  Only skip them for lowering when the
                // target actually requires WeakMap-based lowering (< ES2022).
                && !(self.arena.get(prop.name).is_some_and(|n| {
                    n.kind == SyntaxKind::PrivateIdentifier as u16
                }) && (self.ctx.options.target as u32) >= (ScriptTarget::ES2022 as u32))
                // Static fields at ES2022+ are emitted inline as `static { this.f = v; }`
                // blocks, not deferred to external assignments.
                && (!self.has_effective_static_modifier_js(&prop.modifiers)
                    || needs_static_block_lowering)
            {
                // For static properties, save leading and trailing comments before
                // skipping so they can be emitted when the initialization is moved
                // after the class body.
                let is_static = self.has_effective_static_modifier_js(&prop.modifiers);
                if is_static {
                    let leading = self.collect_leading_comments(member_node.pos);
                    if let Some(entry) = static_field_inits
                        .iter_mut()
                        .find(|e| e.2 == member_node.pos)
                    {
                        entry.3 = leading;
                    }
                }
                if let Some(member_node) = self.arena.get(member_idx) {
                    // Use a tighter bound for property declarations to avoid
                    // consuming comments that belong to the next class member.
                    // Property node.end can extend past newlines into the next
                    // member's territory, so we bound by the next member's pos.
                    let skip_end = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map_or(member_node.end, |next| next.pos);
                    // Find the actual end of the property's content
                    let actual_end = self.find_token_end_before_trivia(member_node.pos, skip_end);
                    // Find line end from actual_end
                    let line_end = if let Some(text) = self.source_text {
                        let bytes = text.as_bytes();
                        let mut pos = actual_end as usize;
                        while pos < bytes.len() && bytes[pos] != b'\n' && bytes[pos] != b'\r' {
                            pos += 1;
                        }
                        pos as u32
                    } else {
                        actual_end
                    };
                    // Collect trailing comments on the same line for both static and
                    // non-static fields. Static comments are stored on static_field_inits;
                    // non-static comments are stored on field_inits for replay in the
                    // constructor prologue.
                    if let Some(text) = self.source_text {
                        let mut trailing = Vec::new();
                        let mut idx = self.comment_emit_idx;
                        while idx < self.all_comments.len() {
                            let c = &self.all_comments[idx];
                            if c.pos >= actual_end
                                && c.end <= line_end
                                && let Ok(comment_text) =
                                    crate::safe_slice::slice(text, c.pos as usize, c.end as usize)
                            {
                                trailing.push(comment_text.to_string());
                            }
                            if c.end > line_end {
                                break;
                            }
                            idx += 1;
                        }
                        if is_static {
                            if let Some(entry) = static_field_inits
                                .iter_mut()
                                .find(|e| e.2 == member_node.pos)
                            {
                                entry.4 = trailing;
                            }
                        } else if !trailing.is_empty() {
                            if let Some(entry) = field_inits.get_mut(field_init_comment_idx) {
                                entry.4 = trailing.clone();
                            }
                            // Also update pending_class_field_inits so existing constructors
                            // that read from it during the member loop get the comments
                            if let Some(entry) = self
                                .pending_class_field_inits
                                .get_mut(field_init_comment_idx)
                            {
                                entry.4 = trailing;
                            }
                        }
                    }
                    if !is_static {
                        field_init_comment_idx += 1;
                    }
                    while self.comment_emit_idx < self.all_comments.len() {
                        let c = &self.all_comments[self.comment_emit_idx];
                        if c.end <= line_end {
                            self.comment_emit_idx += 1;
                        } else {
                            break;
                        }
                    }
                }
                continue;
            }

            // Skip static blocks that need lowering to IIFEs after the class
            if needs_static_block_lowering
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::CLASS_STATIC_BLOCK_DECLARATION
            {
                // Find the opening `{` of the static block to determine where
                // inner (body) comments start. We skip leading comments but save
                // the index of the first inner comment for replay during IIFE emission.
                let brace_pos = if let Some(text) = self.source_text {
                    let bytes = text.as_bytes();
                    let start = member_node.pos as usize;
                    let end = (member_node.end as usize).min(bytes.len());
                    bytes[start..end]
                        .iter()
                        .position(|&b| b == b'{')
                        .map(|off| (start + off + 1) as u32)
                        .unwrap_or(member_node.end)
                } else {
                    member_node.end
                };
                // Skip comments preceding the block opening `{`
                while self.comment_emit_idx < self.all_comments.len()
                    && self.all_comments[self.comment_emit_idx].end <= brace_pos
                {
                    self.comment_emit_idx += 1;
                }
                // Save index pointing at the first inner comment (if any)
                let inner_comment_idx = self.comment_emit_idx;
                // Skip remaining inner comments so they don't leak as leading
                // comments of subsequent class members
                self.skip_comments_for_erased_node(member_node);
                deferred_static_blocks.push((member_idx, inner_comment_idx));
                continue;
            }

            // Check if this member is erased (no runtime representation)
            if let Some(member_node) = self.arena.get(member_idx) {
                let is_erased = match member_node.kind {
                    // Bodyless methods are erased (abstract methods without body,
                    // overload signatures). Abstract methods WITH a body (an error
                    // in TS) are still emitted by tsc, so we must not erase them.
                    k if k == syntax_kind_ext::METHOD_DECLARATION => {
                        self.arena.get_method_decl(member_node).is_some_and(|m| {
                            m.body.is_none()
                                && !self.has_recovered_declaration_trailing_comma(member_node)
                        })
                    }
                    // Abstract accessors without body are erased. Bodyless non-abstract
                    // accessors (error case) are kept — tsc emits them as `{}`.
                    // Abstract accessors WITH a body (error case) are also kept.
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena.get_accessor(member_node).is_some_and(|a| {
                            self.arena
                                .has_modifier(&a.modifiers, SyntaxKind::AbstractKeyword)
                                && a.body.is_none()
                        })
                    }
                    k if k == syntax_kind_ext::PROPERTY_DECLARATION => {
                        if let Some(p) = self.arena.get_property_decl(member_node) {
                            // Abstract properties: erased
                            if self
                                .arena
                                .has_modifier(&p.modifiers, SyntaxKind::AbstractKeyword)
                            {
                                true
                            } else {
                                // Type-only properties (no initializer, not private, not accessor): erased.
                                // Native class-field emit keeps uninitialised properties only
                                // when the target can represent class fields in the class body.
                                if self.ctx.options.use_define_for_class_fields
                                    && target_supports_native_fields
                                {
                                    false
                                } else {
                                    let is_private = self.arena.get(p.name).is_some_and(|n| {
                                        n.kind == SyntaxKind::PrivateIdentifier as u16
                                    });
                                    let has_accessor = self
                                        .arena
                                        .has_modifier(&p.modifiers, SyntaxKind::AccessorKeyword);
                                    p.initializer.is_none() && !is_private && !has_accessor
                                }
                            }
                        } else {
                            false
                        }
                    }
                    // Bodyless constructor overloads are erased
                    k if k == syntax_kind_ext::CONSTRUCTOR => self
                        .arena
                        .get_constructor(member_node)
                        .is_some_and(|c| c.body.is_none()),
                    // Index signatures are TypeScript-only
                    k if k == syntax_kind_ext::INDEX_SIGNATURE => true,
                    // Semicolon class elements are preserved in JS output (valid JS syntax)
                    k if k == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT => false,
                    _ => false,
                };
                if is_erased {
                    // When an erased property has a computed name whose expression
                    // could have runtime side effects, tsc emits the expression as
                    // a standalone statement after the class body.
                    // e.g., `[Symbol.iterator]: Type` → `Symbol.iterator;`
                    // Only expressions that might have observable effects are emitted:
                    // property accesses, element accesses, calls, assignments, etc.
                    // Simple identifiers and literals are NOT emitted (no side effects).
                    // Skip this when computed property hoisting is active — the comma
                    // expression already handles side effects.
                    if !needs_computed_prop_hoisting
                        && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                        && let Some(p) = self.arena.get_property_decl(member_node)
                        && let Some(name_node) = self.arena.get(p.name)
                        && name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
                        && let Some(computed) = self.arena.get_computed_property(name_node)
                        && let Some(expr_node) = self.arena.get(computed.expression)
                    {
                        let k = expr_node.kind;
                        let is_side_effect_free = k == SyntaxKind::Identifier as u16
                            || k == SyntaxKind::StringLiteral as u16
                            || k == SyntaxKind::NumericLiteral as u16
                            || k == SyntaxKind::NoSubstitutionTemplateLiteral as u16
                            || k == SyntaxKind::PrivateIdentifier as u16;
                        if !is_side_effect_free {
                            computed_property_side_effects.push(computed.expression);
                        }
                    }
                    self.skip_comments_for_erased_node(member_node);
                    continue;
                }
            }

            // Emit leading comments before this member
            if let Some(member_node) = self.arena.get(member_idx) {
                self.emit_comments_before_pos(member_node.pos);
            }

            let before_len = self.writer.len();
            let auto_accessor = auto_accessor_member_map.get(&member_idx).cloned();
            if let Some(member_node) = self.arena.get(member_idx) {
                let property_end = if auto_accessor.is_some() {
                    let upper = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos)
                        .unwrap_or(member_node.end);
                    Some(self.find_token_end_before_trivia(member_node.pos, upper))
                } else {
                    None
                };

                if let Some((storage_name, is_static)) = auto_accessor {
                    let computed_storage_inits =
                        if Some(member_idx) == auto_accessor_computed_storage_key_member {
                            auto_accessor_instance_storage_inits_in_computed_key.as_slice()
                        } else {
                            &[]
                        };
                    self.emit_auto_accessor_methods(
                        member_node,
                        &storage_name,
                        is_static,
                        AutoAccessorEmitOptions {
                            static_accessor_alias: auto_accessor_class_alias.as_deref(),
                            lower_to_private_fields: lower_auto_accessors_to_private_fields,
                            class_name: &class_name,
                            property_end: property_end.unwrap_or(member_node.end),
                            omit_storage_initializer: hoisted_native_auto_accessor_members
                                .contains(&member_idx),
                            computed_storage_inits,
                        },
                    );
                } else if hoisted_native_private_members.contains(&member_idx) {
                    if let Some(prop) = self.arena.get_property_decl(member_node) {
                        self.emit_class_member_modifiers_js(&prop.modifiers);
                        if let Some(private_name) = get_private_field_name(self.arena, prop.name) {
                            self.write(&private_name);
                        }
                        self.write_semicolon();
                    }
                } else {
                    self.class_member_emit_depth = self.class_member_emit_depth.saturating_add(1);
                    self.emit(member_idx);
                    self.class_member_emit_depth = self.class_member_emit_depth.saturating_sub(1);
                }
            }
            let mut emit_standalone_class_semicolon = false;
            if let Some(member_node) = self.arena.get(member_idx)
                && (member_node.kind == syntax_kind_ext::GET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::SET_ACCESSOR
                    || member_node.kind == syntax_kind_ext::METHOD_DECLARATION)
            {
                let next_is_semicolon_member = class
                    .members
                    .nodes
                    .get(member_i + 1)
                    .and_then(|&idx| self.arena.get(idx))
                    .is_some_and(|n| n.kind == syntax_kind_ext::SEMICOLON_CLASS_ELEMENT);

                // Check if the member has a body (method/accessor with `{}`).
                let member_has_body_for_semi = match member_node.kind {
                    k if k == syntax_kind_ext::METHOD_DECLARATION => self
                        .arena
                        .get_method_decl(member_node)
                        .is_some_and(|m| m.body.is_some()),
                    k if k == syntax_kind_ext::GET_ACCESSOR
                        || k == syntax_kind_ext::SET_ACCESSOR =>
                    {
                        self.arena
                            .get_accessor(member_node)
                            .is_some_and(|a| a.body.is_some())
                    }
                    _ => false,
                };
                if !next_is_semicolon_member {
                    let has_source_semicolon = self.source_text.is_some_and(|text| {
                        let member_end = std::cmp::min(member_node.end as usize, text.len());
                        // For members WITHOUT bodies, check the gap after the member.
                        if !member_has_body_for_semi {
                            let gap_end = class
                                .members
                                .nodes
                                .get(member_i + 1)
                                .and_then(|&idx| self.arena.get(idx))
                                .map_or_else(
                                    || {
                                        let search_end =
                                            std::cmp::min(node.end as usize, text.len());
                                        text[member_end..search_end]
                                            .rfind('}')
                                            .map_or(search_end, |pos| member_end + pos)
                                    },
                                    |n| n.pos as usize,
                                );
                            let gap_end = std::cmp::min(gap_end, text.len());
                            if member_end < gap_end && text[member_end..gap_end].contains(';') {
                                return true;
                            }
                        }
                        // For members WITH bodies, the parser may absorb trailing `;`
                        // into the member span (e.g., `get x() { ... };`).
                        // Check if the member source ends with `} ;` pattern.
                        if member_has_body_for_semi && member_end >= 2 {
                            let tail = &text[member_node.pos as usize..member_end];
                            let trimmed = tail.trim_end();
                            if let Some(before_semi) = trimmed.strip_suffix(';')
                                && before_semi.trim_end().ends_with('}')
                            {
                                return true;
                            }
                        }
                        false
                    });
                    emit_standalone_class_semicolon = has_source_semicolon;
                }

                // Some parser recoveries include the semicolon in member.end without
                // creating a separate SEMICOLON_CLASS_ELEMENT; preserve it from source.
                // Only check this for methods/accessors that DON'T have a body (i.e.,
                // abstract methods or overload signatures like `foo(): void;`).
                if !member_has_body_for_semi
                    && self.source_text.is_some_and(|text| {
                        let start = std::cmp::min(member_node.pos as usize, text.len());
                        let end = std::cmp::min(member_node.end as usize, text.len());
                        if start >= end {
                            return false;
                        }
                        let member_text = text[start..end].trim_end();
                        member_text.ends_with(';')
                    })
                {
                    emit_standalone_class_semicolon = true;
                }
            }
            if self.writer.len() == before_len
                && let (Some(member_node), Some(text)) =
                    (self.arena.get(member_idx), self.source_text)
            {
                let start = std::cmp::min(member_node.pos as usize, text.len());
                let end = std::cmp::min(member_node.end as usize, text.len());
                if start < end {
                    let raw = &text[start..end];
                    let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                    if compact.starts_with("*(){") {
                        self.write("*() { }");
                    }
                }
            }
            // Only add newline if something was actually emitted
            if self.writer.len() > before_len && !self.writer.is_at_line_start() {
                emitted_any_member = true;
                // Emit trailing comments on the same line as the member.
                // For property declarations, member_node.end can include the leading trivia
                // of the next member (because the parser records token_end() = scanner.pos
                // which is after the lookahead token). Use the AST initializer/name end
                // to get the true end of the property's last token.
                if let Some(member_node) = self.arena.get(member_idx) {
                    // Use the next member's pos as upper bound to avoid scanning
                    // past the current member into the next member's trivia.
                    // For the last member, use the class body's closing `}` position
                    // so we don't steal comments that belong on the closing brace line.
                    let next_member_pos = class
                        .members
                        .nodes
                        .get(member_i + 1)
                        .and_then(|&next_idx| self.arena.get(next_idx))
                        .map(|n| n.pos);
                    let upper = next_member_pos.unwrap_or(member_node.end);
                    let token_end = self.find_token_end_before_trivia(member_node.pos, upper);
                    // For the last member, cap trailing comment scan at the class
                    // body's closing `}` to avoid stealing comments that belong
                    // on the closing brace line.
                    if next_member_pos.is_none() {
                        self.emit_trailing_comments_before(token_end, class_body_close_pos);
                    } else {
                        self.emit_trailing_comments(token_end);
                    }
                }
                self.write_line();
                if emit_standalone_class_semicolon {
                    self.write(";");
                    self.write_line();
                }
                if target_supports_native_private_names
                    && has_legacy_private_name_member_decorators
                    && self.legacy_member_decorator_needs_private_name_scope(member_idx)
                {
                    self.write("static {");
                    self.write_line();
                    self.increase_indent();
                    self.emit_legacy_member_decorator_calls_requiring_private_name_scope(
                        &class_name,
                        &[member_idx],
                    );
                    self.decrease_indent();
                    self.write("}");
                    self.write_line();
                }
            }
        }
        self.scoped_class_expression_self_alias = prev_scoped_class_expression_self_alias;

        if !emitted_any_member && let Some(text) = self.source_text {
            let start = std::cmp::min(node.pos as usize, text.len());
            let end = std::cmp::min(node.end as usize, text.len());
            if start < end {
                let raw = &text[start..end];
                let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
                if compact.contains("*(){}") {
                    self.write("*() { }");
                    self.write_line();
                }
            }
        }

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
            self.write(";");
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
            .is_some_and(|(class_idx, _)| *class_idx == _idx)
        {
            let (_, class_name) = self
                .pending_commonjs_class_export_name
                .take()
                .expect("pending class export should be present");
            self.write_line();
            self.write("exports.");
            self.write(&class_name);
            self.write(" = ");
            self.write(&class_name);
            self.write(";");
        }

        // Emit computed property name hoisting comma expression or standalone side effects.
        if !computed_prop_entries.is_empty() {
            if class_expr_temp.is_some() {
                for (entry_idx, (temp_name, expr_idx, _)) in
                    computed_prop_entries.iter().enumerate()
                {
                    if computed_prop_entries_consumed_by_member_name.contains(&entry_idx) {
                        continue;
                    }
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    if let Some(temp) = temp_name {
                        self.write(temp);
                        self.write(" = ");
                    }
                    self.emit_expression(*expr_idx);
                    self.decrease_indent();
                }
            } else {
                // Emit as a single comma expression: `_a = expr1, sideEffect, _b = expr2;`
                self.write_line();
                let mut emitted_entry = false;
                for (entry_idx, (temp_name, expr_idx, _)) in
                    computed_prop_entries.iter().enumerate()
                {
                    if computed_prop_entries_consumed_by_member_name.contains(&entry_idx) {
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
                if emitted_entry {
                    self.write(";");
                }
            }
            if needs_computed_prop_comma_expr
                && !needs_static_comma_expr
                && !needs_private_comma_expr
                && let Some(temp) = class_expr_temp.as_ref()
            {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(temp);
                if class_expr_comma_needs_parens {
                    self.write(")");
                }
                self.decrease_indent();
            }
        } else if !computed_side_effects_emitted_in_static_block {
            // Emit computed property name side-effect statements for erased members
            // (when hoisting is not active, e.g., ES2022+ targets).
            // e.g., `[Symbol.iterator]: Type` → `Symbol.iterator;`
            for expr_idx in &computed_property_side_effects {
                if class_expr_temp.is_some() {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    self.emit_expression(*expr_idx);
                    self.decrease_indent();
                } else {
                    self.write_line();
                    self.emit_expression(*expr_idx);
                    self.write(";");
                }
            }
        }

        if let Some(recovery_name) = self.class_var_function_recovery_name(node) {
            self.write_line();
            self.write("var ");
            self.write(&recovery_name);
            self.write(";");
            self.write_line();
            self.write("() => { };");
        }

        // Emit static field initializers after class body
        // For class expressions: use comma expression `(_a = class C {}, _a.field = value, _a)`
        // For class declarations: use separate statements `ClassName.field = value;`
        let emit_private_inits_before_static_elements = !needs_private_comma_expr
            && has_any_private_lowering
            && (static_initializer_class_alias.is_some() || has_static_privates)
            && (!static_field_inits.is_empty() || !deferred_static_blocks.is_empty());
        let mut emitted_private_auto_accessors_pre_static = false;
        if emit_private_inits_before_static_elements {
            let static_private_inits = std::mem::take(&mut self.pending_static_private_inits);
            let private_class_alias_pair = self.pending_private_class_alias.take();
            let instances_ws = self.pending_instances_weakset_add.take();
            let method_defs = std::mem::take(&mut self.pending_private_method_defs);
            let accessor_defs = std::mem::take(&mut self.pending_private_accessor_defs);
            let weakmap_inits = self.pending_weakmap_inits.clone();
            let private_auto_instance_storage_inits: Vec<String> = private_auto_accessors
                .iter()
                .filter(|a| !a.is_static)
                .map(|a| format!("{} = new WeakMap()", a.storage_name))
                .collect();
            let has_pre_static_private_inits = private_class_alias_pair.is_some()
                || !weakmap_inits.is_empty()
                || instances_ws.is_some()
                || !method_defs.is_empty()
                || !accessor_defs.is_empty()
                || !private_auto_accessors.is_empty()
                || !private_auto_instance_storage_inits.is_empty()
                || !static_private_inits.is_empty();

            if has_pre_static_private_inits {
                self.write_line();
                let mut first = true;
                if let Some((ref alias, ref cls_name)) = private_class_alias_pair {
                    self.write(alias);
                    self.write(" = ");
                    self.write(cls_name);
                    first = false;
                }
                for init in &weakmap_inits {
                    if !first {
                        self.write(", ");
                    }
                    self.write(init);
                    first = false;
                }
                if let Some(ref ws_name) = instances_ws {
                    if !first {
                        self.write(", ");
                    }
                    self.write(ws_name);
                    self.write(" = new WeakSet()");
                    first = false;
                }
                for init in &private_auto_instance_storage_inits {
                    if !first {
                        self.write(", ");
                    }
                    self.write(init);
                    first = false;
                }
                for (var_name, body_idx, params) in &method_defs {
                    if !first {
                        self.write(", ");
                    }
                    self.write(var_name);
                    self.write(" = function ");
                    self.write(var_name);
                    self.write("(");
                    for (i, &param_idx) in params.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        if let Some(param_node) = self.arena.get(param_idx)
                            && let Some(param_data) = self.arena.get_parameter(param_node)
                        {
                            self.emit(param_data.name);
                        }
                    }
                    self.write(") ");
                    let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                    if private_member_def_needs_class_alias
                        && let Some(alias) = class_value_alias.as_ref()
                        && !class_name.is_empty()
                    {
                        self.scoped_class_expression_self_alias = Some((
                            Arc::<str>::from(class_name.as_str()),
                            Arc::<str>::from(alias.as_str()),
                        ));
                    }
                    self.emit_single_line_block(*body_idx);
                    self.scoped_class_expression_self_alias = prev_self_alias;
                    first = false;
                }
                for def in &accessor_defs {
                    if !first {
                        self.write(", ");
                    }
                    self.write(&def.var_name);
                    self.write(" = function ");
                    self.write(&def.var_name);
                    self.write("(");
                    if let Some(param_idx) = def.param
                        && let Some(param_node) = self.arena.get(param_idx)
                        && let Some(param_data) = self.arena.get_parameter(param_node)
                    {
                        self.emit(param_data.name);
                    }
                    self.write(") ");
                    let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                    if private_member_def_needs_class_alias
                        && let Some(alias) = class_value_alias.as_ref()
                        && !class_name.is_empty()
                    {
                        self.scoped_class_expression_self_alias = Some((
                            Arc::<str>::from(class_name.as_str()),
                            Arc::<str>::from(alias.as_str()),
                        ));
                    }
                    self.emit_single_line_block(def.body);
                    self.scoped_class_expression_self_alias = prev_self_alias;
                    first = false;
                }
                for accessor in &private_auto_accessors {
                    if !first {
                        self.write(", ");
                    }
                    self.emit_private_auto_accessor_function_def(
                        &accessor.get_var_name,
                        &accessor.storage_name,
                        accessor.is_static,
                        true,
                        private_class_alias_pair
                            .as_ref()
                            .map(|(alias, _)| alias.as_str())
                            .or(class_value_alias.as_deref()),
                    );
                    self.write(", ");
                    self.emit_private_auto_accessor_function_def(
                        &accessor.set_var_name,
                        &accessor.storage_name,
                        accessor.is_static,
                        false,
                        private_class_alias_pair
                            .as_ref()
                            .map(|(alias, _)| alias.as_str())
                            .or(class_value_alias.as_deref()),
                    );
                    first = false;
                }
                if !private_auto_accessors.is_empty() {
                    emitted_private_auto_accessors_pre_static = true;
                }
                self.write(";");
                for (var_name, init_idx) in &static_private_inits {
                    self.write_line();
                    self.write(var_name);
                    self.write(" = { value: ");
                    if init_idx.is_some() {
                        self.emit_expression(*init_idx);
                    } else {
                        self.write("void 0");
                    }
                    self.write(" };");
                }
            }
        }
        let class_expr_static_comma_had_scheduled_elements =
            !static_field_inits.is_empty() || !deferred_static_blocks.is_empty();
        if !static_field_inits.is_empty()
            && let Some(temp) = class_expr_static_temp.as_ref()
        {
            // Class expression comma-expression: `(_a = class C {}, _a.a = 1, _a)`
            // The `(_a = ` prefix was already emitted before the `class` keyword.
            //
            // Static field initializers and (when not deferred) static blocks
            // must be interleaved by source position so that observable
            // evaluation order matches the source — e.g.
            // `static a = 1; static { console.log(this.a); } static b = 2;`
            // must emit the static block AFTER `_a.a = 1` and BEFORE `_a.b = 2`.
            // Devin review: <https://github.com/mohsen1/tsz/pull/2279#discussion_r3176494185>
            //
            // We build a single position-keyed list. `field` items reuse the
            // owned `StaticFieldInit` entries; `block` items consume the
            // `(NodeIndex, usize)` deferred entries. When static blocks are
            // deferred (via `--useDefineForClassFields` lowering deferral),
            // they're emitted in their existing trailing batch instead.
            let interleave_blocks = !self.defer_class_static_blocks;
            enum CommaItem {
                SetFunctionName(String),
                Field(StaticFieldInit),
                Block(NodeIndex, usize),
            }
            let owned_field_inits = std::mem::take(&mut static_field_inits);
            let mut comma_items: Vec<(u32, CommaItem)> = Vec::new();
            if let Some(name) = class_expr_set_function_name.as_ref() {
                comma_items.push((node.pos, CommaItem::SetFunctionName(name.clone())));
            }
            comma_items.extend(
                owned_field_inits
                    .into_iter()
                    .map(|init| (init.2, CommaItem::Field(init))),
            );
            if interleave_blocks {
                let blocks = std::mem::take(&mut deferred_static_blocks);
                for (block_idx, comment_idx) in blocks {
                    let pos = self.arena.get(block_idx).map_or(u32::MAX, |node| node.pos);
                    comma_items.push((pos, CommaItem::Block(block_idx, comment_idx)));
                }
            }
            comma_items.sort_by_key(|(pos, _)| *pos);

            for (_pos, item) in comma_items {
                match item {
                    CommaItem::SetFunctionName(name) => {
                        self.emit_class_expr_set_function_name_comma_item(temp, &name);
                    }
                    CommaItem::Field((
                        name_emit,
                        init_idx,
                        _member_pos,
                        _leading_comments,
                        _trailing_comments,
                    )) => {
                        self.write(",");
                        self.write_line();
                        self.increase_indent();
                        if self.ctx.options.use_define_for_class_fields {
                            let define_name = match &name_emit {
                                PropertyNameEmit::Dot(s) => format!("\"{s}\""),
                                PropertyNameEmit::Bracket(s)
                                | PropertyNameEmit::BracketNumeric(s) => s.clone(),
                            };
                            self.write("Object.defineProperty(");
                            self.write(temp);
                            self.write(", ");
                            self.write(&define_name);
                            self.write(", {");
                            self.write_line();
                            self.increase_indent();
                            self.write("enumerable: true,");
                            self.write_line();
                            self.write("configurable: true,");
                            self.write_line();
                            self.write("writable: true,");
                            self.write_line();
                            self.write("value: ");
                            // Emit the initializer, then substitute class name with temp var
                            let before = self.writer.len();
                            self.with_scoped_static_initializer_context_cleared(|this| {
                                this.emit_expression(init_idx);
                            });
                            let after = self.writer.len();
                            if !class_name.is_empty() && class_name != *temp {
                                let full = self.writer.get_output().to_string();
                                let segment = &full[before..after];
                                let replaced = replace_identifier(segment, &class_name, temp);
                                if replaced != segment {
                                    self.writer.truncate(before);
                                    self.write(&replaced);
                                }
                            }
                            self.write_line();
                            self.decrease_indent();
                            self.write("})");
                        } else {
                            self.write(temp);
                            match &name_emit {
                                PropertyNameEmit::Dot(name) => {
                                    self.write(".");
                                    self.write(name);
                                }
                                PropertyNameEmit::Bracket(name)
                                | PropertyNameEmit::BracketNumeric(name) => {
                                    self.write("[");
                                    self.write(name);
                                    self.write("]");
                                }
                            }
                            self.write(" = ");
                            let before = self.writer.len();
                            self.with_scoped_static_initializer_context_cleared(|this| {
                                this.emit_expression(init_idx);
                            });
                            let after = self.writer.len();
                            if !class_name.is_empty() && class_name != *temp {
                                let full = self.writer.get_output().to_string();
                                let segment = &full[before..after];
                                let replaced = replace_identifier(segment, &class_name, temp);
                                if replaced != segment {
                                    self.writer.truncate(before);
                                    self.write(&replaced);
                                }
                            }
                        }
                        self.decrease_indent();
                    }
                    CommaItem::Block(block_idx, comment_idx) => {
                        self.write(",");
                        self.write_line();
                        self.increase_indent();
                        let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                        if class_name_is_real && !class_name.is_empty() && class_name != *temp {
                            self.scoped_class_expression_self_alias = Some((
                                Arc::<str>::from(class_name.as_str()),
                                Arc::<str>::from(temp.as_str()),
                            ));
                        }
                        self.emit_static_block_iife_expression(block_idx, comment_idx);
                        self.scoped_class_expression_self_alias = prev_self_alias;
                        self.decrease_indent();
                    }
                }
            }
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(temp);
            if class_expr_comma_needs_parens {
                self.write(")");
            }
            self.decrease_indent();
            if assignment_prefix.is_some() {
                self.write(";");
            }
        } else if !static_field_inits.is_empty() && !class_name.is_empty() {
            self.write_line();
            if let Some(temp) = default_export_set_function_name_temp.as_ref() {
                self.write_helper("__setFunctionName");
                self.write("(");
                self.write(temp);
                self.write(", \"default\");");
                self.write_line();
            }
            // If lowered static elements need a stable class value, emit
            // `_a = ClassName;` so `this` and class-name references can use it.
            if !emit_private_inits_before_static_elements
                && let Some(ref alias) = static_initializer_class_alias
            {
                self.write(alias);
                self.write(" = ");
                self.write(&class_name);
                self.write(";");
                self.write_line();
            }
            let mut next_static_block = 0usize;
            for (name_emit, init_idx, _member_pos, leading_comments, trailing_comments) in
                &static_field_inits
            {
                if !self.defer_class_static_blocks {
                    while next_static_block < deferred_static_blocks.len() {
                        let (block_idx, comment_idx) = deferred_static_blocks[next_static_block];
                        let block_pos = self.arena.get(block_idx).map_or(u32::MAX, |node| node.pos);
                        if block_pos >= *_member_pos {
                            break;
                        }
                        let prev_this_alias = self.scoped_static_this_alias.clone();
                        let prev_super_alias = self.scoped_static_super_base_alias.clone();
                        self.scoped_static_this_alias =
                            static_initializer_this_binding.map(std::sync::Arc::from);
                        self.scoped_static_super_base_alias =
                            static_initializer_super_base.map(std::sync::Arc::from);
                        let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                        if let Some(alias) = static_initializer_class_alias.as_ref() {
                            self.scoped_class_expression_self_alias = Some((
                                Arc::<str>::from(class_name.as_str()),
                                Arc::<str>::from(alias.as_str()),
                            ));
                        }
                        self.emit_static_block_iife_expression(block_idx, comment_idx);
                        self.scoped_class_expression_self_alias = prev_self_alias;
                        self.scoped_static_this_alias = prev_this_alias;
                        self.scoped_static_super_base_alias = prev_super_alias;
                        self.write(";");
                        self.write_line();
                        next_static_block += 1;
                    }
                }

                // Emit saved leading comments from the original static property declaration
                for (comment_text, source_pos) in leading_comments {
                    self.write_comment_with_reindent(comment_text, Some(*source_pos));
                    self.write_line();
                }
                if self.ctx.options.use_define_for_class_fields {
                    let define_name = match name_emit {
                        PropertyNameEmit::Dot(s) => format!("\"{s}\""),
                        PropertyNameEmit::Bracket(s) | PropertyNameEmit::BracketNumeric(s) => {
                            s.clone()
                        }
                    };
                    self.write("Object.defineProperty(");
                    self.write(&class_name);
                    self.write(", ");
                    self.write(&define_name);
                    self.write(", {");
                    self.write_line();
                    self.increase_indent();
                    self.write("enumerable: true,");
                    self.write_line();
                    self.write("configurable: true,");
                    self.write_line();
                    self.write("writable: true,");
                    self.write_line();
                    self.write("value: ");
                    let before = self.writer.len();
                    self.emit_expression_with_scoped_static_initializer_mode(
                        *init_idx,
                        static_initializer_this_binding,
                        static_initializer_super_base,
                        externalized_static_initializer_uses_undefined_receiver,
                    );
                    let after = self.writer.len();
                    if let Some(alias) =
                        static_initializer_self_alias.or(static_initializer_class_alias.as_deref())
                        && !class_name.is_empty()
                        && class_name != alias
                    {
                        let full = self.writer.get_output().to_string();
                        let segment = &full[before..after];
                        let replaced = replace_identifier(segment, &class_name, alias);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
                    }
                    self.write_line();
                    self.decrease_indent();
                    self.write("});");
                } else {
                    self.write(&class_name);
                    match name_emit {
                        PropertyNameEmit::Dot(name) => {
                            self.write(".");
                            self.write(name);
                        }
                        PropertyNameEmit::Bracket(name)
                        | PropertyNameEmit::BracketNumeric(name) => {
                            self.write("[");
                            self.write(name);
                            self.write("]");
                        }
                    }
                    self.write(" = ");
                    let before = self.writer.len();
                    self.emit_expression_with_scoped_static_initializer_mode(
                        *init_idx,
                        static_initializer_this_binding,
                        static_initializer_super_base,
                        externalized_static_initializer_uses_undefined_receiver,
                    );
                    let after = self.writer.len();
                    if let Some(alias) =
                        static_initializer_self_alias.or(static_initializer_class_alias.as_deref())
                        && !class_name.is_empty()
                        && class_name != alias
                    {
                        let full = self.writer.get_output().to_string();
                        let segment = &full[before..after];
                        let replaced = replace_identifier(segment, &class_name, alias);
                        if replaced != segment {
                            self.writer.truncate(before);
                            self.write(&replaced);
                        }
                    }
                    self.write(";");
                }
                // Emit saved trailing comments (e.g. `// ok` from
                // `static intance = new C3(); // ok`)
                for comment_text in trailing_comments {
                    self.write_space();
                    self.write_comment(comment_text);
                }
                self.write_line();
            }
            if !self.defer_class_static_blocks {
                while next_static_block < deferred_static_blocks.len() {
                    let (block_idx, comment_idx) = deferred_static_blocks[next_static_block];
                    let prev_this_alias = self.scoped_static_this_alias.clone();
                    let prev_super_alias = self.scoped_static_super_base_alias.clone();
                    self.scoped_static_this_alias =
                        static_initializer_this_binding.map(std::sync::Arc::from);
                    self.scoped_static_super_base_alias =
                        static_initializer_super_base.map(std::sync::Arc::from);
                    let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                    if let Some(alias) = static_initializer_class_alias.as_ref() {
                        self.scoped_class_expression_self_alias = Some((
                            Arc::<str>::from(class_name.as_str()),
                            Arc::<str>::from(alias.as_str()),
                        ));
                    }
                    self.emit_static_block_iife_expression(block_idx, comment_idx);
                    self.scoped_class_expression_self_alias = prev_self_alias;
                    self.scoped_static_this_alias = prev_this_alias;
                    self.scoped_static_super_base_alias = prev_super_alias;
                    self.write(";");
                    self.write_line();
                    next_static_block += 1;
                }
                if next_static_block > 0 {
                    deferred_static_blocks.clear();
                }
            }
        }

        let class_expr_static_comma_has_no_scheduled_elements =
            class_expr_static_temp.is_some() && !class_expr_static_comma_had_scheduled_elements;
        if class_expr_static_comma_has_no_scheduled_elements
            && !needs_private_comma_expr
            && let Some(temp) = class_expr_static_temp.as_ref()
        {
            if let Some(name) = class_expr_set_function_name.as_ref() {
                self.emit_class_expr_set_function_name_comma_item(temp, name);
            }
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(temp);
            if class_expr_comma_needs_parens {
                self.write(")");
            }
            self.decrease_indent();
            if assignment_prefix.is_some() {
                self.write(";");
            }
        }

        // Emit auto-accessor WeakMap initializations after class body:
        // var _Class_prop_accessor_storage;
        // ...
        // _Class_prop_accessor_storage = new WeakMap();
        if lower_auto_accessors_to_weakmap
            && ((emit_auto_accessor_instance_inits_after_class
                && !auto_accessor_instance_inits.is_empty())
                || !auto_accessor_static_inits.is_empty()
                || auto_accessor_class_alias.is_some())
        {
            self.write_line();
            let mut wrote_alias_line = false;

            if let Some(alias) = auto_accessor_class_alias.as_ref()
                && !alias.is_empty()
                && !class_name.is_empty()
            {
                self.write(alias);
                self.write(" = ");
                self.write(&class_name);
                wrote_alias_line = true;
            }

            if emit_auto_accessor_instance_inits_after_class
                && !auto_accessor_instance_inits.is_empty()
            {
                if wrote_alias_line {
                    self.write(", ");
                }
                let mut wrote_instance_line = false;
                for (i, (storage_name, _init_idx)) in
                    auto_accessor_instance_inits.iter().enumerate()
                {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(storage_name);
                    self.write(" = new WeakMap()");
                    wrote_instance_line = true;
                }
                if wrote_alias_line || wrote_instance_line {
                    self.write(";");
                    self.write_line();
                }
            } else if wrote_alias_line {
                self.write(";");
                self.write_line();
            }

            for (storage_name, init_idx) in &auto_accessor_static_inits {
                self.write(storage_name);
                self.write(" = { value: ");
                if let Some(init) = init_idx {
                    self.emit_expression(*init);
                } else {
                    self.write("void 0");
                }
                self.write(" };");
                self.write_line();
            }
        }

        // Emit private field WeakMap initializations after class body:
        // _C_field1 = new WeakMap();
        let has_weakmap_inits = !self.pending_weakmap_inits.is_empty();
        let static_private_inits = std::mem::take(&mut self.pending_static_private_inits);
        let private_class_alias_pair = self.pending_private_class_alias.take();

        // Emit combined initialization line after class body.
        // tsc joins class alias, WeakMap inits, WeakSet init, and private method/accessor defs
        // with commas on a single line, e.g.:
        // _a = A, _A_foo = new WeakMap(), _A_instances = new WeakSet(), _A_m = function _A_m() { };
        let instances_ws = self.pending_instances_weakset_add.clone();
        let method_defs = std::mem::take(&mut self.pending_private_method_defs);
        let accessor_defs = std::mem::take(&mut self.pending_private_accessor_defs);
        let private_auto_instance_storage_inits: Vec<String> = private_auto_accessors
            .iter()
            .filter(|_| !emitted_private_auto_accessors_pre_static)
            .filter(|a| !a.is_static)
            .map(|a| format!("{} = new WeakMap()", a.storage_name))
            .collect();
        let has_post_class_inits = private_class_alias_pair.is_some()
            || has_weakmap_inits
            || instances_ws.is_some()
            || !method_defs.is_empty()
            || !accessor_defs.is_empty()
            || !private_auto_instance_storage_inits.is_empty();

        // For class expressions with private field lowering, emit the WeakMap/WeakSet/method
        // initializations as comma-separated items inside the wrapping expression:
        //   (_a = class C { ... },
        //       _C_field = new WeakMap(),
        //       _C_instances = new WeakSet(),
        //       _C_method = function _C_method() { },
        //       _a)
        // For class declarations, emit as separate statements after the class body.
        if needs_private_comma_expr && has_post_class_inits {
            // Emit comma-separated inits inline in the expression.
            // The `(_a = ` prefix was already emitted before the `class` keyword.

            if (!needs_static_comma_expr || class_expr_static_comma_has_no_scheduled_elements)
                && let Some(temp) = class_expr_temp.as_ref()
                && let Some(name) = class_expr_set_function_name.as_ref()
            {
                self.emit_class_expr_set_function_name_comma_item(temp, name);
            }

            // WeakMap inits: _X_field = new WeakMap()
            let weakmap_inits = self.pending_weakmap_inits.clone();
            for init in &weakmap_inits {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(init);
                self.decrease_indent();
            }

            // WeakSet: _X_instances = new WeakSet()
            if let Some(ref ws_name) = instances_ws {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(ws_name);
                self.write(" = new WeakSet()");
                self.decrease_indent();
            }

            for init in &private_auto_instance_storage_inits {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(init);
                self.decrease_indent();
            }

            // Private method function definitions
            for (var_name, body_idx, params) in &method_defs {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(var_name);
                self.write(" = function ");
                self.write(var_name);
                self.write("(");
                for (i, &param_idx) in params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    if let Some(param_node) = self.arena.get(param_idx)
                        && let Some(param_data) = self.arena.get_parameter(param_node)
                    {
                        self.emit(param_data.name);
                    }
                }
                self.write(") ");
                let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                if private_member_def_needs_class_alias
                    && let Some(alias) = class_value_alias.as_ref()
                    && !class_name.is_empty()
                {
                    self.scoped_class_expression_self_alias = Some((
                        Arc::<str>::from(class_name.as_str()),
                        Arc::<str>::from(alias.as_str()),
                    ));
                }
                self.emit_single_line_block(*body_idx);
                self.scoped_class_expression_self_alias = prev_self_alias;
                self.decrease_indent();
            }

            // Private accessor function definitions
            for def in &accessor_defs {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(&def.var_name);
                self.write(" = function ");
                self.write(&def.var_name);
                self.write("(");
                if let Some(param_idx) = def.param
                    && let Some(param_node) = self.arena.get(param_idx)
                    && let Some(param_data) = self.arena.get_parameter(param_node)
                {
                    self.emit(param_data.name);
                }
                self.write(") ");
                let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                if private_member_def_needs_class_alias
                    && let Some(alias) = class_value_alias.as_ref()
                    && !class_name.is_empty()
                {
                    self.scoped_class_expression_self_alias = Some((
                        Arc::<str>::from(class_name.as_str()),
                        Arc::<str>::from(alias.as_str()),
                    ));
                }
                self.emit_single_line_block(def.body);
                self.scoped_class_expression_self_alias = prev_self_alias;
                self.decrease_indent();
            }

            if !emitted_private_auto_accessors_pre_static {
                for accessor in &private_auto_accessors {
                    self.write(",");
                    self.write_line();
                    self.increase_indent();
                    self.emit_private_auto_accessor_function_def(
                        &accessor.get_var_name,
                        &accessor.storage_name,
                        accessor.is_static,
                        true,
                        private_class_alias_pair
                            .as_ref()
                            .map(|(alias, _)| alias.as_str())
                            .or(class_value_alias.as_deref()),
                    );
                    self.write(",");
                    self.write_line();
                    self.emit_private_auto_accessor_function_def(
                        &accessor.set_var_name,
                        &accessor.storage_name,
                        accessor.is_static,
                        false,
                        private_class_alias_pair
                            .as_ref()
                            .map(|(alias, _)| alias.as_str())
                            .or(class_value_alias.as_deref()),
                    );
                    self.decrease_indent();
                }
            }

            // Emit static private field value initializations as comma items
            // (e.g., `_D_field = { value: __classPrivateFieldGet(...) }`)
            for (var_name, init_idx) in &static_private_inits {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(var_name);
                self.write(" = { value: ");
                if init_idx.is_some() {
                    self.emit_expression(*init_idx);
                } else {
                    self.write("void 0");
                }
                self.write(" }");
                self.decrease_indent();
            }
            for accessor in private_auto_accessors.iter().filter(|a| a.is_static) {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(&accessor.storage_name);
                self.write(" = { value: ");
                if let Some(init) = accessor.initializer {
                    self.emit_expression(init);
                } else {
                    self.write("void 0");
                }
                self.write(" }");
                self.decrease_indent();
            }

            if !target_supports_native_private_names && has_legacy_private_name_member_decorators {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write("(() => {");
                self.write_line();
                self.increase_indent();
                self.emit_legacy_member_decorator_calls_requiring_private_name_scope(
                    &class_name,
                    &class.members.nodes,
                );
                self.decrease_indent();
                self.write("})()");
                self.decrease_indent();
            }

            // Close the comma expression with the temp var, unless the static field
            // comma expr path will handle the closing.
            if (!needs_static_comma_expr || class_expr_static_comma_has_no_scheduled_elements)
                && let Some(ref temp) = class_expr_temp
            {
                self.write(",");
                self.write_line();
                self.increase_indent();
                self.write(temp);
                if class_expr_comma_needs_parens {
                    self.write(")");
                }
                self.decrease_indent();
                if assignment_prefix.is_some() {
                    self.write(";");
                }
            }
        } else if has_post_class_inits {
            self.write_line();
            let mut first = true;

            // Class alias: _a = ClassName
            if let Some((ref alias, ref cls_name)) = private_class_alias_pair {
                self.write(alias);
                self.write(" = ");
                self.write(cls_name);
                first = false;
            }

            // WeakMap inits first (tsc order): _X_field = new WeakMap()
            let weakmap_inits = self.pending_weakmap_inits.clone();
            for init in &weakmap_inits {
                if !first {
                    self.write(", ");
                }
                self.write(init);
                first = false;
            }

            // WeakSet: _X_instances = new WeakSet()
            if let Some(ref ws_name) = instances_ws {
                if !first {
                    self.write(", ");
                }
                self.write(ws_name);
                self.write(" = new WeakSet()");
                first = false;
            }

            for init in &private_auto_instance_storage_inits {
                if !first {
                    self.write(", ");
                }
                self.write(init);
                first = false;
            }

            // Private method function definitions:
            // _C_method = function _C_method(params) { ... }
            for (var_name, body_idx, params) in &method_defs {
                if !first {
                    self.write(", ");
                }
                self.write(var_name);
                self.write(" = function ");
                self.write(var_name);
                self.write("(");
                for (i, &param_idx) in params.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    // Emit parameter name (identifier or pattern)
                    if let Some(param_node) = self.arena.get(param_idx)
                        && let Some(param_data) = self.arena.get_parameter(param_node)
                    {
                        self.emit(param_data.name);
                    }
                }
                self.write(") ");
                let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                if private_member_def_needs_class_alias
                    && let Some(alias) = class_value_alias.as_ref()
                    && !class_name.is_empty()
                {
                    self.scoped_class_expression_self_alias = Some((
                        Arc::<str>::from(class_name.as_str()),
                        Arc::<str>::from(alias.as_str()),
                    ));
                }
                self.emit_single_line_block(*body_idx);
                self.scoped_class_expression_self_alias = prev_self_alias;
                first = false;
            }

            // Private accessor function definitions:
            // _C_prop_get = function _C_prop_get() { ... }
            // _C_prop_set = function _C_prop_set(param) { ... }
            for def in &accessor_defs {
                if !first {
                    self.write(", ");
                }
                self.write(&def.var_name);
                self.write(" = function ");
                self.write(&def.var_name);
                self.write("(");
                if let Some(param_idx) = def.param {
                    // Emit setter parameter name
                    if let Some(param_node) = self.arena.get(param_idx)
                        && let Some(param_data) = self.arena.get_parameter(param_node)
                    {
                        self.emit(param_data.name);
                    }
                }
                self.write(") ");
                let prev_self_alias = self.scoped_class_expression_self_alias.clone();
                if private_member_def_needs_class_alias
                    && let Some(alias) = class_value_alias.as_ref()
                    && !class_name.is_empty()
                {
                    self.scoped_class_expression_self_alias = Some((
                        Arc::<str>::from(class_name.as_str()),
                        Arc::<str>::from(alias.as_str()),
                    ));
                }
                self.emit_single_line_block(def.body);
                self.scoped_class_expression_self_alias = prev_self_alias;
                first = false;
            }

            if !emitted_private_auto_accessors_pre_static {
                for accessor in &private_auto_accessors {
                    if !first {
                        self.write(", ");
                    }
                    self.emit_private_auto_accessor_function_def(
                        &accessor.get_var_name,
                        &accessor.storage_name,
                        accessor.is_static,
                        true,
                        private_class_alias_pair
                            .as_ref()
                            .map(|(alias, _)| alias.as_str())
                            .or(class_value_alias.as_deref()),
                    );
                    self.write(", ");
                    self.emit_private_auto_accessor_function_def(
                        &accessor.set_var_name,
                        &accessor.storage_name,
                        accessor.is_static,
                        false,
                        private_class_alias_pair
                            .as_ref()
                            .map(|(alias, _)| alias.as_str())
                            .or(class_value_alias.as_deref()),
                    );
                    first = false;
                }
            }

            self.write(";");
        }

        if !needs_private_comma_expr
            && !target_supports_native_private_names
            && has_legacy_private_name_member_decorators
        {
            if !self.writer.is_at_line_start() {
                self.write_line();
            }
            self.write("(() => {");
            self.write_line();
            self.increase_indent();
            self.emit_legacy_member_decorator_calls_requiring_private_name_scope(
                &class_name,
                &class.members.nodes,
            );
            self.decrease_indent();
            self.write("})();");
        }

        // Emit static private field value initializations after class body:
        // `_A_field = { value: 10 };`
        // For class expressions with private lowering, these are already emitted
        // as comma items above in the private comma expr block.
        if needs_private_comma_expr {
            // Already emitted above in the comma expression block.
        } else {
            for (var_name, init_idx) in &static_private_inits {
                self.write_line();
                self.write(var_name);
                self.write(" = { value: ");
                if init_idx.is_some() {
                    self.emit_expression(*init_idx);
                } else {
                    self.write("void 0");
                }
                self.write(" };");
            }
            for accessor in private_auto_accessors.iter().filter(|a| a.is_static) {
                self.write_line();
                self.write(&accessor.storage_name);
                self.write(" = { value: ");
                if let Some(init) = accessor.initializer {
                    self.emit_expression(init);
                } else {
                    self.write("void 0");
                }
                self.write(" };");
            }
        }

        // Emit deferred static blocks as IIFEs after the class body.
        // Class expressions lowered to comma expressions must keep static block
        // evaluation inside that expression before returning the temp.
        if let Some(temp) = class_expr_static_temp.as_ref()
            && static_field_inits.is_empty()
            && !self.defer_class_static_blocks
            && !deferred_static_blocks.is_empty()
        {
            if let Some(name) = class_expr_set_function_name.as_ref() {
                self.emit_class_expr_set_function_name_comma_item(temp, name);
            }
            let prev_self_alias = self.scoped_class_expression_self_alias.clone();
            if class_name_is_real && !class_name.is_empty() && class_name != *temp {
                self.scoped_class_expression_self_alias = Some((
                    Arc::<str>::from(class_name.as_str()),
                    Arc::<str>::from(temp.as_str()),
                ));
            }
            self.emit_static_block_iife_comma_items_with_context(
                deferred_static_blocks,
                static_initializer_this_binding,
                static_initializer_super_base,
            );
            self.scoped_class_expression_self_alias = prev_self_alias;
            self.write(",");
            self.write_line();
            self.increase_indent();
            self.write(temp);
            if class_expr_comma_needs_parens {
                self.write(")");
            }
            self.decrease_indent();
            if assignment_prefix.is_some() {
                self.write(";");
            }
        } else if self.defer_class_static_blocks {
            self.deferred_class_static_blocks
                .extend(deferred_static_blocks);
        } else {
            if static_field_inits.is_empty()
                && !deferred_static_blocks.is_empty()
                && !emit_private_inits_before_static_elements
                && !class_name.is_empty()
                && let Some(alias) = static_initializer_class_alias.as_ref()
            {
                self.write_line();
                self.write(alias);
                self.write(" = ");
                self.write(&class_name);
                self.write(";");
            }
            let prev_self_alias = self.scoped_class_expression_self_alias.clone();
            if let Some(alias) = static_initializer_class_alias.as_ref()
                && !class_name.is_empty()
            {
                self.scoped_class_expression_self_alias = Some((
                    Arc::<str>::from(class_name.as_str()),
                    Arc::<str>::from(alias.as_str()),
                ));
            }
            self.emit_static_block_iifes_with_context(
                deferred_static_blocks,
                static_initializer_this_binding,
                static_initializer_super_base,
            );
            self.scoped_class_expression_self_alias = prev_self_alias;
        }

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

    fn class_property_initializer_has_equals(
        &self,
        member_node: &Node,
        prop: &tsz_parser::parser::node::PropertyDeclData,
    ) -> bool {
        let Some(text) = self.source_text else {
            return true;
        };
        let Some(init_node) = self.arena.get(prop.initializer) else {
            return true;
        };
        if prop.type_annotation.is_none() {
            return true;
        }
        let start = member_node.pos as usize;
        let end = (init_node.pos as usize).min(text.len());
        if start >= end {
            return false;
        }
        let segment = &text.as_bytes()[start..end];
        let search_from = segment
            .iter()
            .rposition(|&byte| byte == b':')
            .map_or(0, |idx| idx + 1);
        segment[search_from..].contains(&b'=')
    }

    fn node_text_contains_identifier(&self, idx: NodeIndex, name: &str) -> bool {
        if name.is_empty() {
            return false;
        }
        let (Some(text), Some(node)) = (self.source_text, self.arena.get(idx)) else {
            return false;
        };
        let start = (node.pos as usize).min(text.len());
        let end = (node.end as usize).min(text.len());
        if start >= end {
            return false;
        }
        let value_text = crate::import_usage::strip_type_only_content(&text[start..end]);
        super::text_contains_identifier(&value_text, name)
    }

    fn recovered_class_body_statements(&self, node: &Node) -> Vec<String> {
        let Some(text) = self.source_text else {
            return Vec::new();
        };
        let start = node.pos as usize;
        let end = (node.end as usize).min(text.len());
        let Some(source) = text.get(start..end) else {
            return Vec::new();
        };

        let mut depth = 0_i32;
        let mut recovered = Vec::new();
        let mut pending_empty_enum: Option<(String, bool)> = None;
        let mut pending_empty_class: Option<(String, bool)> = None;
        for line in source.lines() {
            let trimmed = line.trim();
            if let Some((class_name, has_body)) = pending_empty_class.as_mut() {
                if depth == 2 && trimmed == "}" {
                    if !*has_body {
                        recovered.push(format!("class {class_name} {{"));
                        recovered.push("}".to_string());
                    }
                    pending_empty_class = None;
                } else if depth >= 2 && !trimmed.is_empty() {
                    *has_body = true;
                }
            }
            if let Some((enum_name, has_body)) = pending_empty_enum.as_mut() {
                if depth == 2 && trimmed == "}" {
                    if !*has_body {
                        let declaration = if self.in_namespace_iife && !self.ctx.target_es5 {
                            "let"
                        } else {
                            "var"
                        };
                        recovered.push(format!("{declaration} {enum_name};"));
                        recovered.push(format!("(function ({enum_name}) {{"));
                        recovered.push(format!("}})({enum_name} || ({enum_name} = {{}}));"));
                    }
                    pending_empty_enum = None;
                } else if depth >= 2 && !trimmed.is_empty() {
                    *has_body = true;
                }
            }
            if depth == 1
                && (trimmed.starts_with("function ")
                    || (trimmed.starts_with("var ")
                        && !trimmed.contains("//")
                        && !trimmed.contains("()")))
            {
                recovered.push(trimmed.replace("{}", "{ }"));
            } else if depth == 1
                && let Some(enum_name) = self.recovered_class_body_empty_enum_name(trimmed)
            {
                pending_empty_enum = Some((enum_name, false));
            } else if depth == 1
                && let Some(class_name) = self.recovered_class_body_empty_class_name(trimmed)
            {
                pending_empty_class = Some((class_name, false));
            } else if depth == 1
                && let Some(stmt) = self.recovered_public_class_block(trimmed)
            {
                recovered.push(stmt);
            }
            for ch in line.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
        }
        recovered
    }

    fn recovered_class_body_empty_enum_name(&self, trimmed: &str) -> Option<String> {
        let rest = trimmed.strip_prefix("enum ")?;
        let name: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() {
            return None;
        }
        let after_name = rest.get(name.len()..)?.trim_start();
        (after_name == "{").then_some(name)
    }

    fn recovered_class_body_empty_class_name(&self, trimmed: &str) -> Option<String> {
        let rest = trimmed.strip_prefix("class ")?;
        let name: String = rest
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect();
        if name.is_empty() {
            return None;
        }
        let after_name = rest.get(name.len()..)?.trim_start();
        (after_name == "{").then_some(name)
    }

    fn class_has_recovered_void_extends(&self, heritage_clauses: &Option<NodeList>) -> bool {
        let (Some(text), Some(clauses)) = (self.source_text, heritage_clauses.as_ref()) else {
            return false;
        };

        clauses.nodes.iter().any(|&clause_idx| {
            let Some(clause_node) = self.arena.get(clause_idx) else {
                return false;
            };
            let Some(heritage) = self.arena.get_heritage(clause_node) else {
                return false;
            };
            if heritage.token != SyntaxKind::ExtendsKeyword as u16 {
                return false;
            }

            heritage.types.nodes.iter().any(|&type_idx| {
                let Some(type_node) = self.arena.get(type_idx) else {
                    return false;
                };
                if type_node.kind != SyntaxKind::Unknown as u16 {
                    return false;
                }
                let start = (type_node.pos as usize).min(text.len());
                let end = (type_node.end as usize).min(text.len());
                start <= end && text.get(start..end).is_some_and(|raw| raw.trim() == "void")
            })
        })
    }

    fn recovered_public_class_block(&self, trimmed: &str) -> Option<String> {
        let after_public = trimmed.strip_prefix("public")?.trim_start();
        if !after_public.starts_with('{') {
            return None;
        }
        let close = after_public.rfind('}')?;
        let inner = after_public[1..close].trim();
        if inner.is_empty() {
            return Some("{ }".to_string());
        }

        if let Some(after_open_bracket) = inner.strip_prefix('[')
            && let Some((index_params, value_type)) = after_open_bracket.split_once("]:")
        {
            let index_expr = index_params.replace(':', ", ");
            let value_type = value_type.trim();
            return Some(format!("{{\n    [{index_expr}];\n    {value_type};\n}}"));
        }

        Some(format!("{{\n    {inner};\n}}"))
    }
}
