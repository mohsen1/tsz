use super::super::super::Printer;
use super::super::super::ScriptTarget;
use super::super::super::core::FieldInit;
use super::duplicate_private_names::PrivateDuplicateConflictPlan;
use super::{AutoAccessorEmitOptions, StaticFieldInit};
use crate::transforms::private_fields_es5::{PrivateFieldInfo, get_private_field_name};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{ClassData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

pub(super) struct ClassEs6MemberEmit<'a> {
    pub(super) node: &'a Node,
    pub(super) class: &'a ClassData,
    pub(super) assignment_alias: Option<&'a str>,
    pub(super) class_name: &'a str,
    pub(super) needs_class_field_lowering: bool,
    pub(super) needs_static_block_lowering: bool,
    pub(super) lower_auto_accessors_to_private_fields: bool,
    pub(super) target_supports_native_fields: bool,
    pub(super) target_supports_native_private_names: bool,
    pub(super) has_legacy_private_name_member_decorators: bool,
    pub(super) needs_computed_prop_hoisting: bool,
    pub(super) erased_computed_side_effects_use_static_block: bool,
    pub(super) native_computed_prop_evaluator: Option<&'a str>,
    pub(super) private_fields: &'a [PrivateFieldInfo],
    pub(super) private_duplicate_conflicts: &'a PrivateDuplicateConflictPlan,
    pub(super) auto_accessor_member_map: &'a FxHashMap<NodeIndex, (String, bool)>,
    pub(super) auto_accessor_class_alias: Option<&'a str>,
    pub(super) auto_accessor_computed_storage_key_member: Option<NodeIndex>,
    pub(super) auto_accessor_instance_storage_inits_in_computed_key: &'a [String],
    pub(super) hoisted_native_auto_accessor_members: &'a FxHashSet<NodeIndex>,
    pub(super) hoisted_native_private_members: &'a FxHashSet<NodeIndex>,
    pub(super) field_inits: &'a mut Vec<FieldInit>,
    pub(super) static_field_inits: &'a mut Vec<StaticFieldInit>,
    pub(super) deferred_static_blocks: &'a mut Vec<(NodeIndex, usize)>,
    pub(super) computed_property_side_effects: &'a mut Vec<NodeIndex>,
}

impl<'a> Printer<'a> {
    pub(super) fn emit_class_es6_members(&mut self, member_emit: ClassEs6MemberEmit<'_>) {
        let ClassEs6MemberEmit {
            node,
            class,
            assignment_alias,
            class_name,
            needs_class_field_lowering,
            needs_static_block_lowering,
            lower_auto_accessors_to_private_fields,
            target_supports_native_fields,
            target_supports_native_private_names,
            has_legacy_private_name_member_decorators,
            needs_computed_prop_hoisting,
            erased_computed_side_effects_use_static_block,
            native_computed_prop_evaluator,
            private_fields,
            private_duplicate_conflicts,
            auto_accessor_member_map,
            auto_accessor_class_alias,
            auto_accessor_computed_storage_key_member,
            auto_accessor_instance_storage_inits_in_computed_key,
            hoisted_native_auto_accessor_members,
            hoisted_native_private_members,
            field_inits,
            static_field_inits,
            deferred_static_blocks,
            computed_property_side_effects,
        } = member_emit;

        let mut emitted_any_member = false;
        emitted_any_member |= self.emit_synthetic_static_self_alias_block(node, assignment_alias);
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
        let mut emitted_native_computed_prop_evaluator_call = false;
        for (member_i, &member_idx) in class.members.nodes.iter().enumerate() {
            if let Some(evaluator) = native_computed_prop_evaluator
                && !emitted_native_computed_prop_evaluator_call
                && self.class_member_uses_computed_prop_temp(member_idx)
            {
                self.write("static { ");
                self.write(evaluator);
                self.write("(); }");
                self.write_line();
                emitted_native_computed_prop_evaluator_call = true;
            }
            // Skip private field declarations entirely when lowering to WeakMap pattern
            if !private_fields.is_empty()
                && let Some(member_node) = self.arena.get(member_idx)
                && member_node.kind == syntax_kind_ext::PROPERTY_DECLARATION
                && let Some(prop) = self.arena.get_property_decl(member_node)
                && self
                    .arena
                    .get(prop.name)
                    .is_some_and(|n| n.kind == SyntaxKind::PrivateIdentifier as u16)
                && !private_duplicate_conflicts.is_conflicting(member_idx)
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
            if !self.private_members_to_skip.is_empty() {
                let should_skip = self.private_members_to_skip.contains(&member_idx);
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
                && !(self.arena.get(prop.name).is_some_and(|n| {
                    n.kind == SyntaxKind::PrivateIdentifier as u16
                }) && private_duplicate_conflicts.is_conflicting(member_idx))
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
                                && !self.is_recovered_optional_bodyless_class_method(member_node)
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
                    // expression already handles side effects — UNLESS the target has
                    // native static blocks, where these side effects belong in a
                    // `static { ... }` block (and the hoisting collection deliberately
                    // left them out of the comma expression for that reason).
                    if (!needs_computed_prop_hoisting
                        || erased_computed_side_effects_use_static_block)
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
                let comments_end_with_line_break =
                    self.pending_comments_before_pos_end_with_line_break(member_node.pos);
                self.emit_comments_before_pos(member_node.pos);
                if comments_end_with_line_break && !self.writer.is_at_line_start() {
                    self.write_line();
                }
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
                            auto_accessor_instance_storage_inits_in_computed_key
                        } else {
                            &[]
                        };
                    self.emit_auto_accessor_methods(
                        member_node,
                        &storage_name,
                        is_static,
                        AutoAccessorEmitOptions {
                            static_accessor_alias: auto_accessor_class_alias,
                            lower_to_private_fields: lower_auto_accessors_to_private_fields,
                            class_name,
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
    }

    fn class_member_uses_computed_prop_temp(&self, member_idx: NodeIndex) -> bool {
        let Some(member_node) = self.arena.get(member_idx) else {
            return false;
        };
        let name_idx = match member_node.kind {
            k if k == syntax_kind_ext::PROPERTY_DECLARATION => self
                .arena
                .get_property_decl(member_node)
                .map(|prop| prop.name),
            _ => None,
        };
        let Some(name_idx) = name_idx else {
            return false;
        };
        let Some(name_node) = self.arena.get(name_idx) else {
            return false;
        };
        if name_node.kind != syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            return false;
        }
        let Some(computed) = self.arena.get_computed_property(name_node) else {
            return false;
        };
        let expression = self
            .arena
            .get(computed.expression)
            .filter(|node| node.kind == syntax_kind_ext::PARENTHESIZED_EXPRESSION)
            .and_then(|node| self.arena.get_parenthesized(node))
            .map_or(computed.expression, |paren| paren.expression);
        self.computed_prop_temp_map.contains_key(&expression)
    }
}
