//! ES5 destructuring - function parameter prologue and parameter binding patterns.

use super::super::{ParamTransformPlan, Printer};
use super::bindings_patterns::ES5RestProp;
use crate::transforms::emit_utils;
use tsz_parser::parser::NodeIndex;
use tsz_parser::parser::node::{BindingElementData, ForInOfData, Node};
use tsz_parser::parser::syntax_kind_ext;
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn emit_param_prologue(&mut self, transforms: &ParamTransformPlan) {
        self.emit_param_binding_prologue(transforms);
        self.emit_rest_param_prologue(transforms);
    }

    pub(in crate::emitter) fn emit_param_binding_prologue(
        &mut self,
        transforms: &ParamTransformPlan,
    ) {
        for param in &transforms.params {
            if let Some(initializer) = param.initializer {
                if let Some(pattern) = param.pattern {
                    if self.binding_pattern_is_empty(pattern) {
                        self.write(&param.name);
                        self.write(" = ");
                        self.emit_expression(initializer);
                        self.write(";");
                        self.write_line();
                        continue;
                    }

                    let has_object_rest = self.binding_target_contains_object_rest(pattern);
                    // Special case: rest-only array pattern `[...r] = null`.
                    // When the default is `null` (a keyword literal), tsc inlines:
                    //   `var r = (param === void 0 ? null : param).slice(0)`
                    // Other defaults (`undefined`, `{}`, `[]`) use the temp form.
                    let init_is_null = self
                        .arena
                        .get(initializer)
                        .is_some_and(|n| n.kind == SyntaxKind::NullKeyword as u16);
                    if init_is_null
                        && let Some(rest_name_idx) = self.get_rest_only_array_name(pattern)
                    {
                        let mut started = false;
                        self.emit_param_assignment_prefix(&mut started);
                        self.write_identifier_text(rest_name_idx);
                        self.write(" = (");
                        self.write(&param.name);
                        self.write(" === void 0 ? ");
                        self.emit_expression(initializer);
                        self.write(" : ");
                        self.write(&param.name);
                        self.write(").slice(0)");
                        if started {
                            self.write(";");
                            self.write_line();
                        }
                    } else if !self.ctx.target_es5 && !has_object_rest {
                        // Non-ES5 with native destructuring: keep the binding
                        // pattern intact in the body prologue so the output
                        // matches tsc's `var <pattern> = <param> === void 0 ? <init> : <param>;`
                        // instead of allocating an extra pattern temp and
                        // walking the pattern out into property-access
                        // assignments.
                        self.emit_native_param_binding_prologue(
                            &param.name,
                            pattern,
                            Some(initializer),
                        );
                    } else if has_object_rest {
                        self.emit_param_default_assignment(&param.name, initializer);
                        let mut started = false;
                        self.emit_param_binding_assignments(pattern, &param.name, &mut started);
                        if started {
                            self.write(";");
                            self.write_line();
                        }
                    } else {
                        // ES5: var _b = _a === void 0 ? default : _a, _c = _b[1], ...
                        let hoisted_start = self.hoisted_assignment_temps.len();
                        let hoist_anchor = self.capture_hoist_anchor();
                        let mut started = false;
                        let temp = self.get_temp_var_name();
                        self.emit_param_assignment_prefix(&mut started);
                        self.write(&temp);
                        self.write(" = ");
                        self.write(&param.name);
                        self.write(" === void 0 ? ");
                        self.emit_expression(initializer);
                        self.write(" : ");
                        self.write(&param.name);

                        self.emit_param_binding_assignments(pattern, &temp, &mut started);
                        if started {
                            self.write(";");
                            self.write_line();
                        }
                        self.insert_param_binding_hoisted_temps(hoisted_start, hoist_anchor);
                    }
                } else {
                    // Only default, no pattern: use if statement
                    self.emit_param_default_assignment(&param.name, initializer);
                }
            } else if let Some(pattern) = param.pattern {
                if self.pattern_eligible_for_native_param_prologue(pattern) {
                    self.emit_native_param_binding_prologue(&param.name, pattern, None);
                } else {
                    let hoisted_start = self.hoisted_assignment_temps.len();
                    let hoist_anchor = self.capture_hoist_anchor();
                    let mut started = false;
                    self.emit_param_binding_assignments(pattern, &param.name, &mut started);
                    if started {
                        self.write(";");
                        self.write_line();
                    }
                    self.insert_param_binding_hoisted_temps(hoisted_start, hoist_anchor);
                }
            }
        }
    }

    /// At ES2015+ the body prologue can keep binding patterns native unless
    /// the pattern contains an object rest, which still needs the `__rest`
    /// helper below ES2018 and forces the legacy lowering path.
    fn pattern_eligible_for_native_param_prologue(&self, pattern: NodeIndex) -> bool {
        !self.ctx.target_es5 && !self.binding_target_contains_object_rest(pattern)
    }

    fn emit_native_param_binding_prologue(
        &mut self,
        param_name: &str,
        pattern: NodeIndex,
        initializer: Option<NodeIndex>,
    ) {
        let hoisted_start = self.hoisted_assignment_temps.len();
        let hoist_anchor = self.capture_hoist_anchor();
        self.write("var ");
        self.emit_decl_name(pattern);
        self.write(" = ");
        if let Some(initializer) = initializer {
            self.write(param_name);
            self.write(" === void 0 ? ");
            self.emit_expression(initializer);
            self.write(" : ");
            self.write(param_name);
        } else {
            self.write(param_name);
        }
        self.write(";");
        self.write_line();
        self.insert_param_binding_hoisted_temps(hoisted_start, hoist_anchor);
    }

    fn insert_param_binding_hoisted_temps(
        &mut self,
        hoisted_start: usize,
        anchor: super::super::hoist_anchor::HoistAnchor,
    ) {
        let hoisted: Vec<_> = self
            .hoisted_assignment_temps
            .drain(hoisted_start..)
            .collect();
        if hoisted.is_empty() {
            return;
        }
        let indent = self.writer.indent_string_at(anchor.indent_level);
        let var_decl = format!("{}var {};", indent, hoisted.join(", "));
        self.writer
            .insert_line_at(anchor.byte_offset, anchor.line_no, &var_decl);
    }

    pub(in crate::emitter) fn emit_rest_param_prologue(&mut self, transforms: &ParamTransformPlan) {
        if let Some(rest) = &transforms.rest {
            if !rest.name.is_empty() {
                self.write("var ");
                self.write(&rest.name);
                self.write(" = [];");
                self.write_line();

                let iter_name = "_i".to_string();
                self.write("for (var ");
                self.write(&iter_name);
                self.write(" = ");
                self.write_usize(rest.index);
                self.write("; ");
                self.write(&iter_name);
                self.write(" < arguments.length; ");
                self.write(&iter_name);
                self.write("++) {");
                self.write_line();
                self.increase_indent();
                self.write(&rest.name);
                self.write("[");
                self.write(&iter_name);
                if rest.index > 0 {
                    self.write(" - ");
                    self.write_usize(rest.index);
                }
                self.write("] = arguments[");
                self.write(&iter_name);
                self.write("];");
                self.write_line();
                self.decrease_indent();
                self.write("}");
                self.write_line();
            }

            if let Some(pattern) = rest.pattern {
                let mut started = false;
                self.emit_param_binding_assignments(pattern, &rest.name, &mut started);
                if started {
                    self.write(";");
                    self.write_line();
                }
            }
        }
    }

    pub(in crate::emitter) fn emit_param_default_assignment(
        &mut self,
        name: &str,
        initializer: NodeIndex,
    ) {
        if name.is_empty() {
            return;
        }
        let hoisted_start = self.hoisted_assignment_temps.len();
        let value_start = self.hoisted_assignment_value_temps.len();
        let initializer_text = self.capture_emit(initializer);
        self.emit_param_initializer_temp_declarations(hoisted_start, value_start);

        self.write("if (");
        self.write(name);
        self.write(" === void 0) { ");
        self.write(name);
        self.write(" = ");
        self.write(&initializer_text);
        self.write("; }");
        self.write_line();
    }

    fn emit_param_initializer_temp_declarations(
        &mut self,
        hoisted_start: usize,
        value_start: usize,
    ) {
        let value_temps: Vec<_> = self
            .hoisted_assignment_value_temps
            .drain(value_start..)
            .collect();
        if !value_temps.is_empty() {
            self.write("var ");
            self.write(&value_temps.join(", "));
            self.write(";");
            self.write_line();
        }

        let hoisted_temps: Vec<_> = self
            .hoisted_assignment_temps
            .drain(hoisted_start..)
            .collect();
        if !hoisted_temps.is_empty() {
            self.write("var ");
            self.write(&hoisted_temps.join(", "));
            self.write(";");
            self.write_line();
        }
    }

    pub(in crate::emitter) fn emit_param_binding_assignments(
        &mut self,
        pattern_idx: NodeIndex,
        temp_name: &str,
        started: &mut bool,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };

        match pattern_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                    let mut rest_props = Vec::new();
                    for &elem_idx in &pattern.elements.nodes {
                        if elem_idx.is_none() {
                            continue;
                        }
                        let Some(elem_node) = self.arena.get(elem_idx) else {
                            continue;
                        };
                        let Some(elem) = self.arena.get_binding_element(elem_node) else {
                            continue;
                        };
                        if elem.dot_dot_dot_token {
                            self.emit_param_object_rest_element(
                                elem,
                                &rest_props,
                                temp_name,
                                started,
                            );
                        } else if let Some(rest_prop) =
                            self.emit_param_object_binding_element(elem_idx, temp_name, started)
                        {
                            rest_props.push(rest_prop);
                        }
                    }
                }
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                if let Some(pattern) = self.arena.get_binding_pattern(pattern_node) {
                    let source_name = self
                        .emit_param_array_downlevel_read(pattern_node, temp_name, started)
                        .unwrap_or_else(|| temp_name.to_string());
                    for (i, &elem_idx) in pattern.elements.nodes.iter().enumerate() {
                        self.emit_param_array_binding_element(elem_idx, &source_name, i, started);
                    }
                }
            }
            _ => {}
        }
    }

    fn emit_param_array_downlevel_read(
        &mut self,
        pattern_node: &Node,
        temp_name: &str,
        started: &mut bool,
    ) -> Option<String> {
        if !self.ctx.target_es5
            || !self.ctx.options.downlevel_iteration
            || pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN
        {
            return None;
        }

        let pattern = self.arena.get_binding_pattern(pattern_node)?;
        if pattern.elements.nodes.is_empty() {
            return None;
        }

        let read_name = self.get_temp_var_name();
        self.emit_param_assignment_prefix(started);
        self.write(&read_name);
        self.write(" = ");
        self.write_helper("__read");
        self.write("(");
        self.write(temp_name);
        if let Some(limit) = self.binding_pattern_read_limit(pattern_node) {
            self.write(", ");
            self.write_usize(limit);
        }
        self.write(")");
        Some(read_name)
    }

    pub(in crate::emitter) fn emit_param_object_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        started: &mut bool,
    ) -> Option<ES5RestProp> {
        let elem_node = self.arena.get(elem_idx)?;
        let elem = self.arena.get_binding_element(elem_node)?;

        if elem.dot_dot_dot_token {
            return None;
        }

        let key_idx = self.get_binding_element_property_key(elem)?;

        // Check if key is computed and save to temp if needed
        let computed_key_temp = self.emit_computed_key_temp_for_param(key_idx, started);
        let rest_prop = self.es5_rest_prop_for_key(key_idx, computed_key_temp.as_deref());

        if self.is_binding_pattern(elem.name) {
            if self.binding_pattern_is_empty(elem.name) {
                let value_name = self.get_temp_var_name();
                self.emit_param_assignment_prefix(started);
                self.write(&value_name);
                self.write(" = ");
                self.emit_assignment_target_es5_with_computed(
                    key_idx,
                    temp_name,
                    computed_key_temp.as_deref(),
                );

                let source_name = if elem.initializer.is_some() {
                    let default_name = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&default_name);
                    self.write(" = ");
                    self.write(&value_name);
                    self.write(" === void 0 ? ");
                    self.emit_expression(elem.initializer);
                    self.write(" : ");
                    self.write(&value_name);
                    default_name
                } else {
                    value_name
                };

                // For an empty array pattern with downlevel_iteration, trigger the
                // iterator protocol (__read(source, 0)) so the iterable is consumed.
                // For an empty object pattern or an empty array without downlevel
                // iteration, the source read above is sufficient.
                if self
                    .arena
                    .get(elem.name)
                    .is_some_and(|node| node.kind == syntax_kind_ext::ARRAY_BINDING_PATTERN)
                    && self.ctx.options.downlevel_iteration
                {
                    let empty_name = self.get_temp_var_name();
                    self.write(", ");
                    self.write(&empty_name);
                    self.write(" = ");
                    self.write_helper("__read");
                    self.write("(");
                    self.write(&source_name);
                    self.write(", 0)");
                }
                return Some(rest_prop);
            }

            if elem.initializer.is_none()
                && self.can_inline_param_nested_source(elem.name)
                && let Some(source_name) = self.param_object_binding_source_expr(
                    key_idx,
                    temp_name,
                    computed_key_temp.as_deref(),
                )
            {
                self.emit_param_binding_assignments(elem.name, &source_name, started);
                return Some(rest_prop);
            }

            let value_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );

            if elem.initializer.is_some() {
                self.write(", ");
                self.write(&value_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
            }

            self.emit_param_binding_assignments(elem.name, &value_name, started);
            return Some(rest_prop);
        }

        if !self.has_identifier_text(elem.name) {
            return Some(rest_prop);
        }

        self.emit_param_assignment_prefix(started);
        if elem.initializer.is_some() {
            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        } else {
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.emit_assignment_target_es5_with_computed(
                key_idx,
                temp_name,
                computed_key_temp.as_deref(),
            );
        }

        Some(rest_prop)
    }

    /// Similar to `emit_computed_key_temp_if_needed` but handles started flag for param destructuring
    pub(in crate::emitter) fn emit_computed_key_temp_for_param(
        &mut self,
        key_idx: NodeIndex,
        started: &mut bool,
    ) -> Option<String> {
        let key_node = self.arena.get(key_idx)?;

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME
            && let Some(computed) = self.arena.get_computed_property(key_node)
        {
            let has_inner_class_temp =
                self.reserve_es5_computed_key_inner_class_temps(computed.expression);
            let expression_text = self
                .computed_key_expression_generates_downlevel_temp(computed.expression)
                .then(|| self.capture_emit(computed.expression));
            let temp_name = if has_inner_class_temp {
                self.make_unique_name_fresh()
            } else {
                self.get_temp_var_name()
            };
            self.emit_param_assignment_prefix(started);
            self.write(&temp_name);
            self.write(" = ");
            if let Some(expression_text) = expression_text {
                self.write(&expression_text);
            } else {
                self.emit(computed.expression);
            }
            return Some(temp_name);
        }

        None
    }

    fn computed_key_expression_generates_downlevel_temp(&self, idx: NodeIndex) -> bool {
        emit_utils::parameter_expression_generates_downlevel_temp(
            self.arena,
            self.ctx.needs_es2020_lowering,
            idx,
        )
    }

    pub(in crate::emitter) fn emit_param_array_binding_element(
        &mut self,
        elem_idx: NodeIndex,
        temp_name: &str,
        index: usize,
        started: &mut bool,
    ) {
        if elem_idx.is_none() {
            return;
        }
        let Some(elem_node) = self.arena.get(elem_idx) else {
            return;
        };
        let Some(elem) = self.arena.get_binding_element(elem_node) else {
            return;
        };

        if elem.dot_dot_dot_token {
            self.emit_param_array_rest_element(elem.name, temp_name, index, started);
            return;
        }

        if self.is_binding_pattern(elem.name) {
            if elem.initializer.is_none() && self.can_inline_param_nested_source(elem.name) {
                let source_name = format!("{temp_name}[{index}]");
                self.emit_param_binding_assignments(elem.name, &source_name, started);
                return;
            }

            let value_name = self.get_temp_var_name();
            self.emit_param_assignment_prefix(started);
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");

            let source_name = if elem.initializer.is_some() {
                // Allocate a NEW temp for the defaulted value
                let default_name = self.get_temp_var_name();
                self.write(", ");
                self.write(&default_name);
                self.write(" = ");
                self.write(&value_name);
                self.write(" === void 0 ? ");
                self.emit_expression(elem.initializer);
                self.write(" : ");
                self.write(&value_name);
                default_name
            } else {
                value_name
            };

            self.emit_param_binding_assignments(elem.name, &source_name, started);
            return;
        }

        if !self.has_identifier_text(elem.name) {
            return;
        }

        self.emit_param_assignment_prefix(started);
        if elem.initializer.is_some() {
            let value_name = self.get_temp_var_name();
            self.write(&value_name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
            self.write(", ");
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(&value_name);
            self.write(" === void 0 ? ");
            self.emit_expression(elem.initializer);
            self.write(" : ");
            self.write(&value_name);
        } else {
            self.write_identifier_text(elem.name);
            self.write(" = ");
            self.write(temp_name);
            self.write("[");
            self.write_usize(index);
            self.write("]");
        }
    }

    fn can_inline_param_nested_source(&self, pattern_idx: NodeIndex) -> bool {
        self.param_nested_inline_binding_count(pattern_idx)
            .is_some_and(|binding_count| binding_count == 1)
    }

    fn param_nested_inline_binding_count(&self, pattern_idx: NodeIndex) -> Option<usize> {
        let pattern_node = self.arena.get(pattern_idx)?;
        let pattern = self.arena.get_binding_pattern(pattern_node)?;

        match pattern_node.kind {
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN => {
                let mut binding_count = 0;
                for &elem_idx in &pattern.elements.nodes {
                    let Some(elem_node) = self.arena.get(elem_idx) else {
                        continue;
                    };
                    let elem = self.arena.get_binding_element(elem_node)?;
                    if elem.dot_dot_dot_token || elem.initializer.is_some() {
                        return None;
                    }
                    let key_idx = self.get_binding_element_property_key(elem)?;
                    if !self.is_literal_property_name(key_idx) {
                        return None;
                    }
                    binding_count += self.param_nested_target_inline_binding_count(elem.name)?;
                }
                Some(binding_count)
            }
            k if k == syntax_kind_ext::ARRAY_BINDING_PATTERN => {
                let mut binding_count = 0;
                for &elem_idx in &pattern.elements.nodes {
                    if elem_idx.is_none() {
                        continue;
                    }
                    let elem_node = self.arena.get(elem_idx)?;
                    let elem = self.arena.get_binding_element(elem_node)?;
                    if elem.dot_dot_dot_token || elem.initializer.is_some() {
                        return None;
                    }
                    binding_count += self.param_nested_target_inline_binding_count(elem.name)?;
                }
                Some(binding_count)
            }
            _ => None,
        }
    }

    fn param_nested_target_inline_binding_count(&self, target_idx: NodeIndex) -> Option<usize> {
        let target_node = self.arena.get(target_idx)?;

        match target_node.kind {
            k if k == SyntaxKind::Identifier as u16 => Some(1),
            k if k == syntax_kind_ext::OBJECT_BINDING_PATTERN
                || k == syntax_kind_ext::ARRAY_BINDING_PATTERN =>
            {
                self.param_nested_inline_binding_count(target_idx)
            }
            _ => None,
        }
    }

    fn param_object_binding_source_expr(
        &self,
        key_idx: NodeIndex,
        temp_name: &str,
        computed_key_temp: Option<&str>,
    ) -> Option<String> {
        let key_node = self.arena.get(key_idx)?;

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            let computed_key_temp = computed_key_temp?;
            return Some(format!("{temp_name}[{computed_key_temp}]"));
        }

        if key_node.kind == SyntaxKind::Identifier as u16 {
            let ident = self.arena.get_identifier(key_node)?;
            return Some(format!("{temp_name}.{}", ident.escaped_text));
        }

        if key_node.kind == SyntaxKind::NumericLiteral as u16 {
            let lit = self.arena.get_literal(key_node)?;
            return Some(format!("{temp_name}[{}]", lit.text));
        }

        None
    }

    pub(in crate::emitter) fn emit_param_object_rest_element(
        &mut self,
        elem: &BindingElementData,
        rest_props: &[ES5RestProp],
        temp_name: &str,
        started: &mut bool,
    ) {
        let rest_target = elem.name;
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.emit_param_assignment_prefix(started);
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            self.emit(rest_target);
        }
        self.write(" = ");
        self.write_helper("__rest");
        self.write("(");
        self.write(temp_name);
        self.write(", ");
        self.emit_rest_exclude_list(rest_props);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_param_binding_assignments(rest_target, name, started);
        }
    }

    pub(in crate::emitter) fn emit_param_array_rest_element(
        &mut self,
        rest_target: NodeIndex,
        temp_name: &str,
        index: usize,
        started: &mut bool,
    ) {
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.emit_param_assignment_prefix(started);
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            if !self.has_identifier_text(rest_target) {
                return;
            }
            self.write_identifier_text(rest_target);
        }
        self.write(" = ");
        self.write(temp_name);
        self.write(".slice(");
        self.write_usize(index);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_param_binding_assignments(rest_target, name, started);
        }
    }

    pub(in crate::emitter) fn emit_param_assignment_prefix(&mut self, started: &mut bool) {
        if !*started {
            self.write("var ");
            *started = true;
        } else {
            self.write(", ");
        }
    }

    pub(in crate::emitter) fn emit_es5_object_rest_element(
        &mut self,
        elem: &BindingElementData,
        rest_props: &[ES5RestProp],
        temp_name: &str,
    ) {
        let rest_target = elem.name;
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.write(", ");
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            self.emit(rest_target);
        }
        self.write(" = ");
        self.write_helper("__rest");
        self.write("(");
        self.write(temp_name);
        self.write(", ");
        self.emit_rest_exclude_list(rest_props);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_es5_destructuring_pattern_idx(rest_target, name);
        }
    }

    pub(in crate::emitter) fn emit_es5_array_rest_element(
        &mut self,
        rest_target: NodeIndex,
        temp_name: &str,
        index: usize,
    ) {
        let is_pattern = self.is_binding_pattern(rest_target);
        let rest_temp = is_pattern.then(|| self.get_temp_var_name());

        self.write(", ");
        if let Some(ref name) = rest_temp {
            self.write(name);
        } else {
            if !self.has_identifier_text(rest_target) {
                return;
            }
            self.write_binding_identifier_text(rest_target);
        }
        self.write(" = ");
        self.write(temp_name);
        self.write(".slice(");
        self.write_usize(index);
        self.write(")");

        if let Some(ref name) = rest_temp {
            self.emit_es5_destructuring_pattern_idx(rest_target, name);
        }
    }

    pub(in crate::emitter) fn emit_es5_destructuring_pattern_idx(
        &mut self,
        pattern_idx: NodeIndex,
        temp_name: &str,
    ) {
        let Some(pattern_node) = self.arena.get(pattern_idx) else {
            return;
        };
        self.emit_es5_destructuring_pattern(pattern_node, temp_name);
    }

    /// If the pattern is an array binding with only a rest element and no other
    /// non-omitted bindings, returns the `NodeIndex` of the rest element's name.
    /// Used to inline `var r = (expr).slice(0)` instead of a temp variable.
    fn get_rest_only_array_name(&self, pattern_idx: NodeIndex) -> Option<NodeIndex> {
        let pattern_node = self.arena.get(pattern_idx)?;
        if pattern_node.kind != syntax_kind_ext::ARRAY_BINDING_PATTERN {
            return None;
        }
        let pattern = self.arena.get_binding_pattern(pattern_node)?;
        let mut rest_name = None;
        for &elem_idx in &pattern.elements.nodes {
            if elem_idx.is_none() {
                continue;
            }
            let elem_node = self.arena.get(elem_idx)?;
            let elem = self.arena.get_binding_element(elem_node)?;
            if elem.dot_dot_dot_token {
                // Must be a simple identifier, not a nested pattern
                if !self.has_identifier_text(elem.name) {
                    return None;
                }
                rest_name = Some(elem.name);
            } else {
                // Non-rest element found — not rest-only
                return None;
            }
        }
        rest_name
    }

    pub(super) fn es5_rest_prop_for_key(
        &self,
        key_idx: NodeIndex,
        computed_temp: Option<&str>,
    ) -> ES5RestProp {
        if let Some(temp) = computed_temp {
            ES5RestProp::Dynamic(temp.to_string())
        } else {
            ES5RestProp::Static(key_idx)
        }
    }

    fn emit_rest_exclude_list(&mut self, props: &[ES5RestProp]) {
        self.write("[");
        let mut first = true;
        for prop in props {
            if !first {
                self.write(", ");
            }
            first = false;
            self.emit_rest_excluded_prop(prop);
        }
        self.write("]");
    }

    fn emit_rest_excluded_prop(&mut self, prop: &ES5RestProp) {
        match prop {
            ES5RestProp::Static(key_idx) => self.emit_rest_property_key(*key_idx),
            ES5RestProp::Dynamic(temp) => {
                self.write("typeof ");
                self.write(temp);
                self.write(" === \"symbol\" ? ");
                self.write(temp);
                self.write(" : ");
                self.write(temp);
                self.write(" + \"\"");
            }
        }
    }

    fn emit_rest_property_key(&mut self, key_idx: NodeIndex) {
        let Some(key_node) = self.arena.get(key_idx) else {
            return;
        };

        if key_node.kind == syntax_kind_ext::COMPUTED_PROPERTY_NAME {
            if let Some(computed) = self.arena.get_computed_property(key_node) {
                self.emit_expression(computed.expression);
            }
            return;
        }

        if let Some(ident) = self.arena.get_identifier(key_node) {
            self.write("\"");
            self.write(&ident.escaped_text);
            self.write("\"");
            return;
        }

        if let Some(lit) = self.arena.get_literal(key_node) {
            self.write("\"");
            self.write(&lit.text);
            self.write("\"");
            return;
        }

        self.emit_expression(key_idx);
    }

    pub(in crate::emitter) fn emit_for_of_statement_es5(
        &mut self,
        for_of_idx: NodeIndex,
        for_in_of: &ForInOfData,
    ) {
        if for_in_of.await_modifier {
            self.emit_for_of_statement_es5_async_iterator(for_of_idx, for_in_of);
        } else if self.ctx.options.downlevel_iteration {
            self.emit_for_of_statement_es5_iterator(for_of_idx, for_in_of);
        } else {
            self.emit_for_of_statement_es5_array_indexing(for_in_of);
        }
    }

    /// Emit for-of using full iterator protocol (--downlevelIteration enabled)
    ///
    /// Transforms:
    /// ```typescript
    /// for (const item of iterable) { body }
    /// ```
    /// Into:
    /// ```javascript
    /// var e_1, _a, e_1_1;
    /// try {
    ///     for (e_1 = __values(iterable), _a = e_1.next(); !_a.done; _a = e_1.next()) {
    ///         var item = _a.value;
    ///         body
    ///     }
    /// }
    /// catch (e_1_1) { e_1 = { error: e_1_1 }; }
    /// finally {
    ///     try {
    ///         if (_a && !_a.done && (_a = e_1["return"])) _a.call(e_1);
    ///     }
    ///     finally { if (e_1) throw e_1.error; }
    /// }
    /// ```
    pub(in crate::emitter) fn emit_for_of_statement_es5_iterator(
        &mut self,
        for_of_idx: NodeIndex,
        for_in_of: &ForInOfData,
    ) {
        let counter = self.ctx.destructuring_state.for_of_counter;

        // TypeScript's variable naming pattern:
        // - Simple identifier expression `arr`: iterator=arr_1, result=arr_1_1
        // - Complex expression: iterator=_b, result=_c (generic temps)
        // - Top-level hoisted: e_N (error container), _a (return temp)
        // - Catch: e_N_1 (error value, not pre-declared)
        let error_container_name = format!("e_{}", counter + 1);
        let return_temp_name = self
            .reserved_iterator_return_temps
            .remove(&for_of_idx)
            .unwrap_or_else(|| self.get_temp_var_name()); // _a, _b, ...
        let is_nested_iterator_for_of = self.iterator_for_of_depth > 0;
        self.iterator_for_of_depth += 1;

        // Reserve return temps for nested iterator for-of loops in this body before
        // allocating this loop's iterator/result temps.
        self.preallocate_nested_iterator_return_temps(for_in_of.statement);

        // Derive iterator/result names from the expression when it's a simple identifier,
        // matching tsc's naming: `arr` -> `arr_1` (iterator), `arr_1_1` (result).
        // For complex expressions, fall back to generic temp names (_b, _c).
        let (loop_iterator_name, loop_result_name) = if let Some(expr_node) =
            self.arena.get(for_in_of.expression)
            && expr_node.is_identifier()
            && let Some(ident) = self.arena.get_identifier(expr_node)
        {
            let base = self.arena.resolve_identifier_text(ident).to_string();
            // Find unique iterator name: base_1, base_2, ...
            let mut iter_name = None;
            for suffix in 1..=100 {
                let candidate = format!("{base}_{suffix}");
                if !self.file_identifiers.contains(&candidate)
                    && !self.generated_temp_names.contains(&candidate)
                {
                    iter_name = Some(candidate);
                    break;
                }
            }
            if let Some(iter_name) = iter_name {
                self.generated_temp_names.insert(iter_name.clone());
                // Result name: iterator_name + "_1"
                let result_name = format!("{iter_name}_1");
                self.generated_temp_names.insert(result_name.clone());
                (iter_name, result_name)
            } else {
                let a = self.get_temp_var_name();
                let b = self.get_temp_var_name();
                (a, b)
            }
        } else {
            let a = self.get_temp_var_name();
            let b = self.get_temp_var_name();
            (a, b)
        };
        let catch_error_name = format!("e_{}_1", counter + 1);

        self.ctx.destructuring_state.for_of_counter += 1;

        // Hoist error container + return temp to the top of the source file scope.
        // This matches tsc's combined var preamble shape when multiple transformed for-of
        // loops appear in the same file.
        self.hoisted_for_of_temps.push(error_container_name.clone());
        self.hoisted_for_of_temps.push(return_temp_name.clone());

        // try block
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Leading comments for downlevel for-of are deferred by statement emitters
        // and emitted here so they stay attached to the transformed loop body.
        if let Some(for_of_node) = self.arena.get(for_of_idx) {
            let actual_start = self.skip_trivia_forward(for_of_node.pos, for_of_node.end);
            self.emit_comments_before_pos(actual_start);
        }

        // for loop with iterator protocol, using NEW temp vars
        self.write("for (var ");
        self.write(&loop_iterator_name);
        self.write(" = ");
        if is_nested_iterator_for_of {
            self.write("(");
            self.write(&error_container_name);
            self.write(" = void 0, ");
            self.write_helper("__values");
            self.write("(");
            self.emit_expression(for_in_of.expression);
            self.write(")), ");
        } else {
            self.write_helper("__values");
            self.write("(");
            self.emit_expression(for_in_of.expression);
            self.write("), ");
        }
        self.write(&loop_result_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".next(); !");
        self.write(&loop_result_name);
        self.write(".done; ");
        self.write(&loop_result_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".next()) {");
        self.write_line();
        self.increase_indent();

        // Enter a new scope for the loop body to track variable shadowing
        self.ctx.block_scope_state.enter_scope();

        // Pre-register loop variables before emitting (needed for shadowing)
        // Note: We only pre-register for VARIABLE_DECLARATION_LIST nodes, not assignment targets
        self.pre_register_for_of_loop_variable(for_in_of.initializer);

        // Emit the value binding: var item = _c.value;
        self.emit_for_of_value_binding_iterator_es5(for_in_of.initializer, &loop_result_name);
        self.write_line();

        // Emit the loop body
        self.emit_for_of_body(for_in_of.statement);

        // Exit the loop body scope
        self.ctx.block_scope_state.exit_scope();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        // catch block
        self.write("catch (");
        self.write(&catch_error_name);
        self.write(") { ");
        self.write(&error_container_name);
        self.write(" = { error: ");
        self.write(&catch_error_name);
        self.write(" }; }");
        self.write_line();

        // finally block
        self.write("finally {");
        self.write_line();
        self.increase_indent();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Cleanup: if (_c && !_c.done && (_a = _b.return)) _a.call(_b);
        self.write("if (");
        self.write(&loop_result_name);
        self.write(" && !");
        self.write(&loop_result_name);
        self.write(".done && (");
        self.write(&return_temp_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".return)) ");
        self.write(&return_temp_name);
        self.write(".call(");
        self.write(&loop_iterator_name);
        self.write(");");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.write("finally { if (");
        self.write(&error_container_name);
        self.write(") throw ");
        self.write(&error_container_name);
        self.write(".error; }");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.iterator_for_of_depth = self.iterator_for_of_depth.saturating_sub(1);
    }

    /// Emit for-await-of using async iterator protocol (`__asyncValues`).
    ///
    /// Transforms:
    /// ```typescript
    /// for await (const item of iterable) { body }
    /// ```
    /// Into:
    /// ```javascript
    /// var e_1, _a, e_1_1;
    /// try {
    ///     for (var _c = true, iterable_1 = __asyncValues(iterable), iterable_1_1 = yield iterable_1.next(), _a = iterable_1_1.done, !_a; _c = true) {
    ///         var _d = iterable_1_1.value;
    ///         _c = false;
    ///         var item = _d;
    ///         body
    ///     }
    /// }
    /// catch (e_1_1) { e_1 = { error: e_1_1 }; }
    /// finally {
    ///     try {
    ///         if (!_c && !_a && (_b = iterable_1.return)) yield _b.call(iterable_1);
    ///     }
    ///     finally { if (e_1) throw e_1.error; }
    /// }
    /// ```
    pub(in crate::emitter) fn emit_for_of_statement_es5_async_iterator(
        &mut self,
        for_of_idx: NodeIndex,
        for_in_of: &ForInOfData,
    ) {
        let counter = self.ctx.destructuring_state.for_of_counter;

        // TypeScript's variable naming pattern:
        // - Simple identifier expression `arr`: iterator=arr_1, result=arr_1_1
        // - Complex expression: iterator=_d, result=_e (generic temps)
        // - Top-level hoisted: _a (done), e_N (error), _b (return), _c (value)
        // - For loop: _d (guard)
        // Catch: e_N_1 (error value, not pre-declared)
        let error_container_name = format!("e_{}", counter + 1);
        let loop_done_name = self.get_temp_var_name();
        let return_temp_name = self
            .reserved_iterator_return_temps
            .remove(&for_of_idx)
            .unwrap_or_else(|| self.get_temp_var_name()); // _a, _b, ...
        let is_nested_iterator_for_of = self.iterator_for_of_depth > 0;
        self.iterator_for_of_depth += 1;

        // Reserve return temps for nested iterator for-of loops in this body before
        // allocating this loop's iterator/result vars.
        self.preallocate_nested_iterator_return_temps(for_in_of.statement);

        let value_temp_name = self.get_temp_var_name();
        let loop_guard_name = self.get_temp_var_name();
        let (loop_iterator_name, loop_result_name) = if let Some(expr_node) =
            self.arena.get(for_in_of.expression)
            && expr_node.is_identifier()
            && let Some(ident) = self.arena.get_identifier(expr_node)
        {
            let base = self.arena.resolve_identifier_text(ident).to_string();
            let mut iter_name = None;
            for suffix in 1..=100 {
                let candidate = format!("{base}_{suffix}");
                if !self.file_identifiers.contains(&candidate)
                    && !self.generated_temp_names.contains(&candidate)
                {
                    iter_name = Some(candidate);
                    break;
                }
            }
            if let Some(iter_name) = iter_name {
                self.generated_temp_names.insert(iter_name.clone());
                let result_name = format!("{iter_name}_1");
                self.generated_temp_names.insert(result_name.clone());
                (iter_name, result_name)
            } else {
                let a = self.get_temp_var_name();
                let b = self.get_temp_var_name();
                (a, b)
            }
        } else {
            let a = self.get_temp_var_name();
            let b = self.get_temp_var_name();
            (a, b)
        };
        let catch_error_name = format!("e_{}_1", counter + 1);

        self.ctx.destructuring_state.for_of_counter += 1;

        // Hoist done/error/return/value temps to the top of the source file scope.
        self.hoisted_for_of_temps.push(loop_done_name.clone());
        self.hoisted_for_of_temps.push(error_container_name.clone());
        self.hoisted_for_of_temps.push(return_temp_name.clone());
        self.hoisted_for_of_temps.push(value_temp_name.clone());

        // try block
        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Leading comments for downlevel for-await-of are deferred by statement emitters
        // and emitted here so they stay attached to the transformed loop body.
        if let Some(for_of_node) = self.arena.get(for_of_idx) {
            let actual_start = self.skip_trivia_forward(for_of_node.pos, for_of_node.end);
            self.emit_comments_before_pos(actual_start);
        }

        // for (var _d = true, iterable_1 = __asyncValues(iterable), iterable_1_1; iterable_1_1 = [await/yield] iterable_1.next(), _a = iterable_1_1.done, !_a; _d = true) {
        let await_or_yield = if self.ctx.emit_await_as_yield {
            "yield"
        } else {
            "await"
        };
        self.write("for (var ");
        self.write(&loop_guard_name);
        self.write(" = true, ");
        self.write(&loop_iterator_name);
        self.write(" = ");
        if is_nested_iterator_for_of {
            self.write("(");
            self.write(&error_container_name);
            self.write(" = void 0, ");
            self.write_helper("__asyncValues");
            self.write("(");
            self.emit_expression(for_in_of.expression);
            self.write(")), ");
        } else {
            self.write_helper("__asyncValues");
            self.write("(");
            self.emit_expression(for_in_of.expression);
            self.write("), ");
        }
        self.write(&loop_result_name);
        self.write("; ");
        self.write(&loop_result_name);
        self.write(" = ");
        self.write(await_or_yield);
        self.write(" ");
        self.write(&loop_iterator_name);
        self.write(".next(), ");
        self.write(&loop_done_name);
        self.write(" = ");
        self.write(&loop_result_name);
        self.write(".done, !");
        self.write(&loop_done_name);
        self.write("; ");
        self.write(&loop_guard_name);
        self.write(" = true) {");
        self.write_line();
        self.increase_indent();

        // Enter a new scope for the loop body to track variable shadowing
        self.ctx.block_scope_state.enter_scope();

        // Pre-register loop variables before emitting (needed for shadowing)
        // Note: We only pre-register for VARIABLE_DECLARATION_LIST nodes, not assignment targets
        self.pre_register_for_of_loop_variable(for_in_of.initializer);

        // Check if the initializer is a `using` declaration that needs dispose lowering.
        let using_info = if !self.ctx.options.target.supports_es2025() {
            crate::transforms::emit_utils::for_of_using_info(self.arena, for_in_of.initializer)
        } else {
            None
        };

        if let Some(using_info) = using_info {
            // For `using` in for-await-of with async iterator lowering:
            // Emit: _c = _f.value;
            // Then: _d = false;
            // Then: const d1_1 = _c;
            // Then: const env = ...; try { const d1 = __addDisposable(env, d1_1, ...); body } catch/finally
            let var_name = if using_info.recovered_missing_binding {
                self.get_temp_var_name()
            } else {
                using_info.binding_name
            };
            let using_async = using_info.using_async;
            let value_temp = loop_result_name.clone();

            // Emit value assignment to the temp already reserved with the loop temps.
            let value_assign_temp = value_temp_name.clone();
            self.write(&value_assign_temp);
            self.write(" = ");
            self.write(&value_temp);
            self.write(".value;");
            self.write_line();
            self.write(&loop_guard_name);
            self.write(" = false;");
            self.write_line();

            // Register the outer for-await-of error container name so that
            // next_disposable_env_names doesn't collide with it.
            self.generated_temp_names
                .insert(error_container_name.clone());

            // Generate temp name for the renamed variable: d1 -> d1_1.
            // The surrounding for-await transform already owns e_1, but tsc
            // still uses env_1/result_1 for the resource region and only
            // bumps the catch variable to e_2.
            let (env_name, error_name, result_name, env_id) =
                self.next_disposable_env_names_allowing_error_gap();
            let temp_var_name = format!("{var_name}_{env_id}");
            self.generated_temp_names.insert(temp_var_name.clone());

            // Determine if we use const or var based on target
            let kw = if self.ctx.target_es5 { "var" } else { "const" };

            // Emit: const d1_1 = _c;
            self.write(kw);
            self.write(" ");
            self.write(&temp_var_name);
            self.write(" = ");
            self.write(&value_assign_temp);
            self.write(";");
            self.write_line();

            // Emit dispose wrapper: const env = ...; try { const d1 = __addDisposable(env, d1_1, false); body } catch/finally
            self.write(kw);
            self.write(" ");
            self.write(&env_name);
            self.write(" = { stack: [], error: void 0, hasError: false };");
            self.write_line();
            self.write("try {");
            self.write_line();
            self.increase_indent();

            self.write(kw);
            self.write(" ");
            self.write(&var_name);
            self.write(" = ");
            self.write_helper("__addDisposableResource");
            self.write("(");
            self.write(&env_name);
            self.write(", ");
            self.write(&temp_var_name);
            self.write(", ");
            self.write(if using_async { "true" } else { "false" });
            self.write(");");
            self.write_line();

            // Emit body
            self.emit_for_of_body(for_in_of.statement);

            self.decrease_indent();
            self.write("}");
            self.write_line();
            self.write("catch (");
            self.write(&error_name);
            self.write(") {");
            self.write_line();
            self.increase_indent();
            self.write(&env_name);
            self.write(".error = ");
            self.write(&error_name);
            self.write(";");
            self.write_line();
            self.write(&env_name);
            self.write(".hasError = true;");
            self.write_line();
            self.decrease_indent();
            self.write("}");
            self.write_line();
            self.write("finally {");
            self.write_line();
            self.increase_indent();
            if using_async {
                let await_kw = if self.ctx.emit_await_as_yield {
                    "yield"
                } else {
                    "await"
                };
                self.write(kw);
                self.write(" ");
                self.write(&result_name);
                self.write(" = ");
                self.write_helper("__disposeResources");
                self.write("(");
                self.write(&env_name);
                self.write(");");
                self.write_line();
                self.write("if (");
                self.write(&result_name);
                self.write(")");
                self.write_line();
                self.increase_indent();
                self.write(await_kw);
                self.write(" ");
                self.write(&result_name);
                self.write(";");
                self.write_line();
                self.decrease_indent();
            } else {
                self.write_helper("__disposeResources");
                self.write("(");
                self.write(&env_name);
                self.write(");");
                self.write_line();
            }
            self.decrease_indent();
            self.write("}");
            self.write_line();
        } else {
            // Normal (non-using) path
            self.write(&value_temp_name);
            self.write(" = ");
            self.write(&loop_result_name);
            self.write(".value;");
            self.write_line();
            self.write(&loop_guard_name);
            self.write(" = false;");
            self.write_line();
            self.emit_for_of_value_binding_iterator_es5_async(
                for_in_of.initializer,
                &value_temp_name,
            );
            self.write_line();

            // Emit the loop body
            self.emit_for_of_body(for_in_of.statement);
        }

        // Exit the loop body scope
        self.ctx.block_scope_state.exit_scope();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.decrease_indent();
        self.write("}");
        self.write_line();

        // catch block
        self.write("catch (");
        self.write(&catch_error_name);
        self.write(") { ");
        self.write(&error_container_name);
        self.write(" = { error: ");
        self.write(&catch_error_name);
        self.write(" }; }");
        self.write_line();

        // finally block
        self.write("finally {");
        self.write_line();
        self.increase_indent();

        self.write("try {");
        self.write_line();
        self.increase_indent();

        // Cleanup: if (!_e && !_d && (_a = _b.return)) [await/yield] _a.call(_b);
        self.write("if (!");
        self.write(&loop_guard_name);
        self.write(" && !");
        self.write(&loop_done_name);
        self.write(" && (");
        self.write(&return_temp_name);
        self.write(" = ");
        self.write(&loop_iterator_name);
        self.write(".return)) ");
        self.write(await_or_yield);
        self.write(" ");
        self.write(&return_temp_name);
        self.write(".call(");
        self.write(&loop_iterator_name);
        self.write(");");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.write_line();

        self.write("finally { if (");
        self.write(&error_container_name);
        self.write(") throw ");
        self.write(&error_container_name);
        self.write(".error; }");

        self.write_line();
        self.decrease_indent();
        self.write("}");
        self.iterator_for_of_depth = self.iterator_for_of_depth.saturating_sub(1);
    }
}
