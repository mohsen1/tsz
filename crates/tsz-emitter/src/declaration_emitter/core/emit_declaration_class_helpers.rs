use rustc_hash::FxHashSet;
use tsz_parser::parser::{NodeIndex, NodeList, syntax_kind_ext};

use crate::declaration_emitter::core::emit_members::ClassMemberKind;

use super::DeclarationEmitter;

impl<'a> DeclarationEmitter<'a> {
    pub(in crate::declaration_emitter) fn class_member_emit_order(
        &self,
        members: &NodeList,
    ) -> Vec<NodeIndex> {
        // tsc preserves source order for TS classes whose members are
        // already declaration-shaped (`[s]: any;`, `[s](): void;`,
        // `accessor a: any`, etc.).  It only re-orders (statics
        // first) when at least one method body forces a *conversion*
        // to `[name]: () => T;` property-arrow form — that happens
        // when the method's computed name *cannot* preserve method
        // syntax in d.ts.  Methods with `unique symbol`-typed
        // computed keys keep the method shape and source order,
        // matching `uniqueSymbolsDeclarationsErrors` /
        // `autoAccessor8`; methods with regular-typed keys (e.g.
        // `[const fieldName: string]() { … }`) get rewritten to
        // property-arrow form and emitted statics-first, matching
        // `declarationEmitSimpleComputedNames1`.
        let has_method_converted_to_property_arrow = members.nodes.iter().any(|&member_idx| {
            let Some(node) = self.arena.get(member_idx) else {
                return false;
            };
            if node.kind != syntax_kind_ext::METHOD_DECLARATION {
                return false;
            }
            let Some(method) = self.arena.get_method_decl(node) else {
                return false;
            };
            if method.body.is_none() {
                return false;
            }
            self.arena
                .get(method.name)
                .is_some_and(|name_node| name_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME)
                && !self.computed_method_name_can_preserve_method_syntax(method.name)
        });
        if !self.source_is_js_file && !has_method_converted_to_property_arrow {
            return members.nodes.clone();
        }

        let mut static_members = Vec::new();
        let mut constructors = Vec::new();
        let mut instance_members = Vec::new();

        for &member_idx in &members.nodes {
            // For TS classes with computed names, tsc keeps the
            // constructor in its source position among the non-static
            // members (so `[a]: number; constructor();` round-trips).
            // The JS path still treats constructors specially because
            // the JS member shape is synthesised from `this.x = …`
            // assignments and prototype writes whose source positions
            // we can't trust the same way.
            let Some(member_info) = self.class_member_info(member_idx) else {
                continue;
            };

            let is_constructor_special =
                self.source_is_js_file && member_info.kind == ClassMemberKind::Constructor;
            if is_constructor_special {
                constructors.push(member_idx);
                continue;
            }

            if member_info.is_static {
                static_members.push(member_idx);
            } else {
                instance_members.push(member_idx);
            }
        }

        static_members.extend(constructors);
        if self.source_is_js_file {
            static_members.extend(self.js_class_instance_member_emit_order(instance_members));
        } else {
            static_members.extend(instance_members);
        }
        static_members
    }

    fn js_class_instance_member_emit_order(&self, members: Vec<NodeIndex>) -> Vec<NodeIndex> {
        let mut backing_field_keys = FxHashSet::default();
        for &member_idx in &members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if (member_node.kind == syntax_kind_ext::GET_ACCESSOR
                || member_node.kind == syntax_kind_ext::SET_ACCESSOR)
                && let Some(key_text) = self.accessor_this_element_key_text(member_idx)
            {
                backing_field_keys.insert(key_text);
            }
        }

        let mut deferred_backing_fields = Vec::new();
        let mut emitted = FxHashSet::default();
        let mut ordered = Vec::new();

        for &member_idx in &members {
            let Some(member_node) = self.arena.get(member_idx) else {
                continue;
            };
            if member_node.kind == syntax_kind_ext::GET_ACCESSOR
                && let Some(name) = self.member_name_source_text(member_idx)
                && self.class_members_have_setter_named(&members, &name)
            {
                continue;
            }

            if !emitted.insert(member_idx) {
                continue;
            }

            if member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && self
                    .class_computed_property_key_text(member_idx)
                    .is_some_and(|key| backing_field_keys.contains(&key))
            {
                deferred_backing_fields.push(member_idx);
                continue;
            }

            ordered.push(member_idx);

            if member_node.kind == syntax_kind_ext::SET_ACCESSOR
                && let Some(name) = self.member_name_source_text(member_idx)
                && let Some(getter_idx) = self.class_members_getter_named(&members, &name)
                && emitted.insert(getter_idx)
            {
                ordered.push(getter_idx);
            }
        }

        ordered.extend(deferred_backing_fields);
        ordered
    }

    fn member_name_source_text(&self, member_idx: NodeIndex) -> Option<String> {
        let name_idx = self.get_member_name_idx(member_idx)?;
        let name_node = self.arena.get(name_idx)?;
        self.get_source_slice(name_node.pos, name_node.end)
    }

    fn class_members_have_setter_named(&self, members: &[NodeIndex], name: &str) -> bool {
        self.class_members_getter_or_setter_named(members, name, syntax_kind_ext::SET_ACCESSOR)
            .is_some()
    }

    fn class_members_getter_named(&self, members: &[NodeIndex], name: &str) -> Option<NodeIndex> {
        self.class_members_getter_or_setter_named(members, name, syntax_kind_ext::GET_ACCESSOR)
    }

    fn class_members_getter_or_setter_named(
        &self,
        members: &[NodeIndex],
        name: &str,
        kind: u16,
    ) -> Option<NodeIndex> {
        members.iter().copied().find(|&member_idx| {
            self.arena
                .get(member_idx)
                .is_some_and(|node| node.kind == kind)
                && self.member_name_source_text(member_idx).as_deref() == Some(name)
        })
    }

    /// Pre-scan class members: when a computed property name appears on both
    /// a method implementation and a get/set accessor, tsc suppresses the
    /// method in the .d.ts output (the accessor wins). This returns the set
    /// of computed name texts that should be treated as "already declared"
    /// so the method implementation is skipped.
    pub(in crate::declaration_emitter) fn computed_names_shadowed_by_accessors(
        &self,
        members: &NodeList,
    ) -> FxHashSet<String> {
        let mut accessor_names = FxHashSet::default();
        let mut method_impl_names: Vec<String> = Vec::new();
        for &m in &members.nodes {
            let Some(mn) = self.arena.get(m) else {
                continue;
            };
            let Some(info) = self.class_member_info(m) else {
                continue;
            };
            let is_accessor = info.kind == ClassMemberKind::Accessor;
            let is_method = info.kind == ClassMemberKind::Method;
            if !is_accessor && !is_method {
                continue;
            }
            let name_idx = info.name;
            let Some(name_idx) = name_idx else {
                continue;
            };
            let Some(name_node) = self.arena.get(name_idx) else {
                continue;
            };
            if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
                continue;
            }
            let Some(text) = self.get_source_slice(name_node.pos, name_node.end) else {
                continue;
            };
            if is_accessor {
                accessor_names.insert(text);
            } else if self
                .arena
                .get_method_decl(mn)
                .is_some_and(|md| md.body.is_some())
            {
                method_impl_names.push(text);
            }
        }
        let mut result = FxHashSet::default();
        for name in method_impl_names {
            if accessor_names.contains(&name) {
                result.insert(name);
            }
        }
        result
    }
}
