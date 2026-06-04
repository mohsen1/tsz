use super::super::super::Printer;
use super::super::super::core::PropertyNameEmit;
use super::StaticFieldInit;
use super::emit_es6_private_accessors::PrivateAutoAccessorInfo;
use super::private_comma_items::PrivateCommaItems;
use super::replace_identifier;
use crate::emitter::core::{PrivateAccessorDef, PrivateMethodDef};
use std::sync::Arc;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{ClassData, Node};

pub(super) struct ClassEs6AfterBody<'a> {
    pub(super) node: &'a Node,
    pub(super) class: &'a ClassData,
    pub(super) class_name: &'a str,
    pub(super) class_name_is_real: bool,
    pub(super) assignment_prefix_is_some: bool,
    pub(super) static_initializer_self_alias: Option<&'a str>,
    pub(super) static_initializer_this_binding: Option<&'a str>,
    pub(super) static_initializer_super_base: Option<&'a str>,
    pub(super) externalized_static_initializer_uses_undefined_receiver: bool,
    pub(super) computed_side_effects_emitted_in_static_block: bool,
    pub(super) class_expr_comma_needs_parens: bool,
    pub(super) needs_native_computed_prop_evaluator_comma_expr: bool,
    pub(super) needs_computed_prop_comma_expr: bool,
    pub(super) needs_static_comma_expr: bool,
    pub(super) needs_private_comma_expr: bool,
    pub(super) has_any_private_lowering: bool,
    pub(super) private_member_def_needs_class_alias: bool,
    pub(super) lower_auto_accessors_to_weakmap: bool,
    pub(super) emit_auto_accessor_instance_inits_after_class: bool,
    pub(super) target_supports_native_private_names: bool,
    pub(super) has_legacy_private_name_member_decorators: bool,
    pub(super) class_expr_temp: Option<String>,
    pub(super) class_expr_static_temp: Option<String>,
    pub(super) default_export_set_function_name_temp: Option<String>,
    pub(super) static_initializer_class_alias: Option<String>,
    pub(super) class_value_alias: Option<String>,
    pub(super) auto_accessor_class_alias: Option<String>,
    pub(super) class_expr_set_function_name: Option<String>,
    pub(super) computed_prop_entries: Vec<(Option<String>, NodeIndex, NodeIndex)>,
    pub(super) computed_prop_entries_consumed_by_member_name: Vec<usize>,
    pub(super) computed_property_side_effects: Vec<NodeIndex>,
    pub(super) static_field_inits: Vec<StaticFieldInit>,
    pub(super) deferred_static_blocks: Vec<(NodeIndex, usize)>,
    pub(super) private_auto_accessors: &'a [PrivateAutoAccessorInfo],
    pub(super) auto_accessor_instance_inits: &'a [(String, Option<NodeIndex>)],
    pub(super) auto_accessor_static_inits: &'a [(String, Option<NodeIndex>)],
}

impl<'a> Printer<'a> {
    pub(super) fn emit_class_es6_after_body(&mut self, after: ClassEs6AfterBody<'_>) {
        let ClassEs6AfterBody {
            node,
            class,
            class_name,
            class_name_is_real,
            assignment_prefix_is_some,
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
            mut static_field_inits,
            mut deferred_static_blocks,
            private_auto_accessors,
            auto_accessor_instance_inits,
            auto_accessor_static_inits,
        } = after;

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
            } else if computed_prop_entries
                .iter()
                .enumerate()
                .any(|(entry_idx, _)| {
                    !computed_prop_entries_consumed_by_member_name.contains(&entry_idx)
                })
            {
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
        if needs_native_computed_prop_evaluator_comma_expr && class_expr_temp.is_none() {
            self.decrease_indent();
            if class_expr_comma_needs_parens {
                self.write(")");
            }
            if assignment_prefix_is_some {
                self.write(";");
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

        // Emit static field initializers after class body. Class expressions use
        // a comma expression; class declarations use separate statements.
        //
        // For a class *declaration* lowered to the WeakMap pattern, tsc always
        // emits the private-member init statements before the static field
        // assignment statements: a static initializer can instantiate the class,
        // whose constructor populates the WeakMaps, so the storage must exist
        // first. This holds even without a class/private self-reference, so the
        // gate fires whenever private lowering and static elements coexist on a
        // declaration, not only for the self-referential subset.
        let emit_private_inits_before_static_elements = !needs_private_comma_expr
            && has_any_private_lowering
            && (!static_field_inits.is_empty() || !deferred_static_blocks.is_empty());
        let mut emitted_private_auto_accessors_pre_static = false;
        if emit_private_inits_before_static_elements {
            let static_private_inits = std::mem::take(&mut self.pending_static_private_inits);
            let private_class_alias_pair = self.pending_private_class_alias.take();
            let instances_ws = self.pending_instances_weakset_add.take();
            let method_defs = std::mem::take(&mut self.pending_private_method_defs);
            let accessor_defs = std::mem::take(&mut self.pending_private_accessor_defs);
            // Consume the pending WeakMap inits here so the later post-class
            // emission path (which re-checks `pending_weakmap_inits`) does not
            // emit the same `_X = new WeakMap()` lines a second time.
            let weakmap_inits = std::mem::take(&mut self.pending_weakmap_inits);
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
                for def in &method_defs {
                    if !first {
                        self.write(", ");
                    }
                    self.emit_private_method_function_def(
                        def,
                        private_member_def_needs_class_alias,
                        class_value_alias.as_deref(),
                        &class_name,
                    );
                    first = false;
                }
                for def in &accessor_defs {
                    if !first {
                        self.write(", ");
                    }
                    self.emit_private_accessor_function_def(
                        def,
                        private_member_def_needs_class_alias,
                        class_value_alias.as_deref(),
                        &class_name,
                    );
                    first = false;
                }
                for accessor in private_auto_accessors {
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
                for init in &static_private_inits {
                    self.write_line();
                    self.emit_static_private_init(init, &class_name, true);
                }
            }
        }
        // Private helper/state initialization can be part of a class-expression
        // comma list. Gather it before static element scheduling so lowered
        // private state can be emitted before static field/block work observes it.
        let weakmap_inits = std::mem::take(&mut self.pending_weakmap_inits);
        let has_weakmap_inits = !weakmap_inits.is_empty();
        let static_private_inits = std::mem::take(&mut self.pending_static_private_inits);
        let private_class_alias_pair = self.pending_private_class_alias.take();
        let instances_ws = self.pending_instances_weakset_add.clone();
        let method_defs: Vec<PrivateMethodDef> =
            std::mem::take(&mut self.pending_private_method_defs);
        let accessor_defs: Vec<PrivateAccessorDef> =
            std::mem::take(&mut self.pending_private_accessor_defs);
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

        let class_expr_static_comma_had_scheduled_elements =
            !static_field_inits.is_empty() || !deferred_static_blocks.is_empty();
        let mut emitted_private_comma_items_before_static_items = false;
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

            if needs_private_comma_expr && has_post_class_inits {
                emitted_private_comma_items_before_static_items = true;
                self.emit_private_comma_items(PrivateCommaItems {
                    weakmap_inits: &weakmap_inits,
                    instances_ws: instances_ws.as_deref(),
                    private_auto_instance_storage_inits: &private_auto_instance_storage_inits,
                    method_defs: &method_defs,
                    accessor_defs: &accessor_defs,
                    private_member_def_needs_class_alias,
                    class_value_alias: class_value_alias.as_deref(),
                    class_name: &class_name,
                    emitted_private_auto_accessors_pre_static,
                    private_auto_accessors,
                    private_class_alias_pair: private_class_alias_pair.as_ref(),
                    set_function_name: class_expr_set_function_name
                        .as_deref()
                        .map(|name| (temp.as_str(), name)),
                    static_private_inits: &static_private_inits,
                });
            }

            for (_pos, item) in comma_items {
                match item {
                    CommaItem::SetFunctionName(name) => {
                        if emitted_private_comma_items_before_static_items {
                            continue;
                        }
                        self.emit_class_expr_set_function_name_comma_item(temp, &name);
                    }
                    CommaItem::Field((
                        name_emit,
                        init_idx,
                        _member_pos,
                        leading_comments,
                        trailing_comments,
                    )) => {
                        self.write(",");
                        self.write_line();
                        self.increase_indent();
                        for (comment_text, source_pos) in leading_comments {
                            self.write_comment_with_reindent(&comment_text, Some(source_pos));
                            self.write_line();
                        }
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
                        for comment_text in trailing_comments {
                            self.write_space();
                            self.write_comment(&comment_text);
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
                                Arc::<str>::from(class_name),
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
            if assignment_prefix_is_some {
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
                                Arc::<str>::from(class_name),
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
                    self.emit_static_field_initializer_with_inner_comments(
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
                    self.emit_static_field_initializer_with_inner_comments(
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
                            Arc::<str>::from(class_name),
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
            if assignment_prefix_is_some {
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

            for (storage_name, init_idx) in auto_accessor_static_inits {
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

        // For class expressions with private field lowering, emit the WeakMap/WeakSet/method
        // initializations as comma-separated items inside the wrapping expression:
        //   (_a = class C { ... },
        //       _C_field = new WeakMap(),
        //       _C_instances = new WeakSet(),
        //       _C_method = function _C_method() { },
        //       _a)
        // For class declarations, emit as separate statements after the class body.
        if needs_private_comma_expr
            && has_post_class_inits
            && !emitted_private_comma_items_before_static_items
        {
            self.emit_private_comma_items(PrivateCommaItems {
                weakmap_inits: &weakmap_inits,
                instances_ws: instances_ws.as_deref(),
                private_auto_instance_storage_inits: &private_auto_instance_storage_inits,
                method_defs: &method_defs,
                accessor_defs: &accessor_defs,
                private_member_def_needs_class_alias,
                class_value_alias: class_value_alias.as_deref(),
                class_name: &class_name,
                emitted_private_auto_accessors_pre_static,
                private_auto_accessors,
                private_class_alias_pair: private_class_alias_pair.as_ref(),
                set_function_name: if !needs_static_comma_expr
                    || class_expr_static_comma_has_no_scheduled_elements
                {
                    class_expr_temp
                        .as_deref()
                        .zip(class_expr_set_function_name.as_deref())
                } else {
                    None
                },
                static_private_inits: &static_private_inits,
            });

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
                if assignment_prefix_is_some {
                    self.write(";");
                }
            }
        } else if has_post_class_inits && !emitted_private_comma_items_before_static_items {
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
            for def in &method_defs {
                if !first {
                    self.write(", ");
                }
                self.emit_private_method_function_def(
                    def,
                    private_member_def_needs_class_alias,
                    class_value_alias.as_deref(),
                    &class_name,
                );
                first = false;
            }

            // Private accessor function definitions:
            // _C_prop_get = function _C_prop_get() { ... }
            // _C_prop_set = function _C_prop_set(param) { ... }
            for def in &accessor_defs {
                if !first {
                    self.write(", ");
                }
                self.emit_private_accessor_function_def(
                    def,
                    private_member_def_needs_class_alias,
                    class_value_alias.as_deref(),
                    &class_name,
                );
                first = false;
            }

            if !emitted_private_auto_accessors_pre_static {
                for accessor in private_auto_accessors {
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
            for init in &static_private_inits {
                self.write_line();
                self.emit_static_private_init(init, &class_name, true);
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
                    Arc::<str>::from(class_name),
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
            if assignment_prefix_is_some {
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
                    Arc::<str>::from(class_name),
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
    }
}
