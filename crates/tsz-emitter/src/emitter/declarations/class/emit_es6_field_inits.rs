use super::super::super::Printer;
use super::super::super::core::{FieldInit, PropertyNameEmit};
use super::StaticFieldInit;
use super::static_field_erasure::static_no_init_field_is_erased;
use crate::transforms::private_fields_es5::{
    PrivateFieldInfo, get_private_field_name, is_private_identifier,
};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::ClassData;
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

#[derive(Default)]
pub(super) struct ClassFieldInitCollection {
    pub(super) field_inits: Vec<FieldInit>,
    pub(super) static_field_inits: Vec<StaticFieldInit>,
    pub(super) hoisted_native_private_members: FxHashSet<NodeIndex>,
    pub(super) hoisted_native_auto_accessor_members: FxHashSet<NodeIndex>,
}

impl<'a> Printer<'a> {
    pub(super) fn collect_class_es6_field_inits(
        &mut self,
        class: &ClassData,
        needs_class_field_lowering: bool,
        needs_private_field_lowering: bool,
        hoist_native_instance_order_inits: bool,
        auto_accessor_member_map: &FxHashMap<NodeIndex, (String, bool)>,
        needs_static_block_lowering: bool,
        private_fields: &[PrivateFieldInfo],
    ) -> ClassFieldInitCollection {
        // Collect property initializers that need lowering
        // (name, initializer_idx, init_end, leading_comments, trailing_comments)
        // Comments are collected eagerly here so they're available even
        // when the constructor appears before the property in source order.
        let mut collection = ClassFieldInitCollection::default();
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
                    let materialize_no_init = self.no_init_property_is_runtime_materialized(prop);
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
                    if !needs_private_field_lowering
                        && is_private_name
                        && (!hoist_native_instance_order_inits
                            || self.has_effective_static_modifier_js(&prop.modifiers))
                    {
                        continue;
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
                            collection
                                .hoisted_native_auto_accessor_members
                                .insert(member_idx);
                        }
                    } else if hoist_native_instance_order_inits
                        && is_private_name
                        && let Some(private_name) = get_private_field_name(self.arena, prop.name)
                    {
                        name_emit = Some(PropertyNameEmit::Dot(private_name));
                        collection.hoisted_native_private_members.insert(member_idx);
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
                        // `tsc` erases a no-init static field (see `static_field_erasure`).
                        let erased = static_no_init_field_is_erased(
                            prop.initializer.is_none(),
                            self.ctx.options.use_define_for_class_fields,
                        );
                        if needs_static_block_lowering && !erased {
                            collection.static_field_inits.push((
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
                        collection.field_inits.push((
                            ident_name,
                            prop.initializer,
                            init_end,
                            leading_comments,
                            trailing_comments,
                            member_node.pos,
                        ));
                    }
                }
            }
        }

        collection
    }
}
