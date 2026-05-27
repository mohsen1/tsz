use super::super::{Printer, get_operator_text};
use tsz_parser::parser::{
    NodeIndex,
    node::{AccessExprData, NodeAccess},
    syntax_kind_ext,
};
use tsz_scanner::SyntaxKind;

enum StaticSuperMember {
    Property(NodeIndex),
    Element(NodeIndex),
}

impl<'a> Printer<'a> {
    /// True when `super` is captured through the async-lowering accessor objects
    /// (`_super = Object.create(null, { x: { get, set } })` / the `_superIndex`
    /// value-accessor closure) rather than the static-member `Reflect.get`/`set`
    /// alias. In that mode reads and writes go directly through the captured
    /// accessor (`_super.x`, `_superIndex("x").value`); the `Reflect`-based
    /// rewrite is only correct for the static-member super alias.
    pub(in crate::emitter) fn in_async_captured_super(&self) -> bool {
        self.scoped_static_super_direct_access || self.scoped_static_super_index_alias.is_some()
    }

    pub(in crate::emitter) fn emit_scoped_static_super_assignment(
        &mut self,
        left: NodeIndex,
        operator: u16,
        right: NodeIndex,
    ) -> bool {
        if self.in_async_captured_super() {
            return false;
        }
        let Some(member) = self.scoped_static_super_member(left) else {
            return false;
        };

        if !self.ctx.flags.in_statement_expression {
            return self.emit_scoped_static_super_value_assignment(&member, operator, right);
        }

        if operator == SyntaxKind::EqualsToken as u16 {
            self.emit_scoped_static_super_set_start(&member);
            self.emit(right);
            self.emit_scoped_static_super_set_end();
            return true;
        }

        if self.is_static_super_compound_assignment(operator) {
            let key_temp = self.scoped_static_super_element_key_temp(&member);
            self.emit_scoped_static_super_set_start_with_key(&member, key_temp.as_deref(), true);
            self.emit_scoped_static_super_get_with_key(&member, key_temp.as_deref(), false);
            self.write(" ");
            self.write(self.static_super_compound_base_operator(operator));
            self.write(" ");
            self.emit(right);
            self.emit_scoped_static_super_set_end();
            return true;
        }

        false
    }

    pub(in crate::emitter) fn emit_scoped_static_super_update(
        &mut self,
        operand: NodeIndex,
        operator: u16,
        is_prefix: bool,
    ) -> bool {
        if self.in_async_captured_super() {
            return false;
        }
        let Some(member) = self.scoped_static_super_member(operand) else {
            return false;
        };
        if operator != SyntaxKind::PlusPlusToken as u16
            && operator != SyntaxKind::MinusMinusToken as u16
        {
            return false;
        }

        if !self.ctx.flags.in_statement_expression {
            return self.emit_scoped_static_super_value_update(&member, operator, is_prefix);
        }

        let key_temp = self.scoped_static_super_element_key_temp(&member);
        let value_temp = self.make_unique_name_hoisted();
        let op_text = get_operator_text(operator);
        self.emit_scoped_static_super_set_start_with_key(&member, key_temp.as_deref(), true);
        self.write("(");
        self.write(&value_temp);
        self.write(" = ");
        self.emit_scoped_static_super_get_with_key(&member, key_temp.as_deref(), false);
        self.write(", ");
        if is_prefix {
            self.write(op_text);
            self.write(&value_temp);
        } else {
            self.write(&value_temp);
            self.write(op_text);
            self.write(", ");
            self.write(&value_temp);
        }
        self.write(")");
        self.emit_scoped_static_super_set_end();
        true
    }

    fn emit_scoped_static_super_value_assignment(
        &mut self,
        member: &StaticSuperMember,
        operator: u16,
        right: NodeIndex,
    ) -> bool {
        if operator != SyntaxKind::EqualsToken as u16
            && !self.is_static_super_compound_assignment(operator)
        {
            return false;
        }

        self.emit_scoped_static_super_value_iife(|this| {
            let key_temp = if operator == SyntaxKind::EqualsToken as u16 {
                None
            } else {
                this.scoped_static_super_element_key_temp(member)
            };
            let result_temp = this.make_unique_name();

            let mut temps = Vec::new();
            if let Some(key_temp) = key_temp.as_ref() {
                temps.push(key_temp.clone());
            }
            temps.push(result_temp.clone());

            this.emit_scoped_static_super_iife_var_decl(&temps);
            this.write("return ");
            this.emit_scoped_static_super_set_start_with_key(member, key_temp.as_deref(), true);
            this.write(&result_temp);
            this.write(" = ");
            if operator == SyntaxKind::EqualsToken as u16 {
                this.emit(right);
            } else {
                this.emit_scoped_static_super_get_with_key(member, key_temp.as_deref(), false);
                this.write(" ");
                this.write(this.static_super_compound_base_operator(operator));
                this.write(" ");
                this.emit(right);
            }
            this.emit_scoped_static_super_set_end();
            this.write(", ");
            this.write(&result_temp);
            this.write_semicolon();
        });
        true
    }

    fn emit_scoped_static_super_value_update(
        &mut self,
        member: &StaticSuperMember,
        operator: u16,
        is_prefix: bool,
    ) -> bool {
        if operator != SyntaxKind::PlusPlusToken as u16
            && operator != SyntaxKind::MinusMinusToken as u16
        {
            return false;
        }

        self.emit_scoped_static_super_value_iife(|this| {
            let key_temp = this.scoped_static_super_element_key_temp(member);
            let result_temp = this.make_unique_name();
            let value_temp = this.make_unique_name();
            let op_text = get_operator_text(operator);

            let mut temps = Vec::new();
            if let Some(key_temp) = key_temp.as_ref() {
                temps.push(key_temp.clone());
            }
            temps.push(result_temp.clone());
            temps.push(value_temp.clone());

            this.emit_scoped_static_super_iife_var_decl(&temps);
            this.write("return ");
            this.emit_scoped_static_super_set_start_with_key(member, key_temp.as_deref(), true);
            this.write("(");
            this.write(&value_temp);
            this.write(" = ");
            this.emit_scoped_static_super_get_with_key(member, key_temp.as_deref(), false);
            this.write(", ");
            this.write(&result_temp);
            this.write(" = ");
            if is_prefix {
                this.write(op_text);
                this.write(&value_temp);
            } else {
                this.write(&value_temp);
                this.write(op_text);
                this.write(", ");
                this.write(&value_temp);
            }
            this.write(")");
            this.emit_scoped_static_super_set_end();
            this.write(", ");
            this.write(&result_temp);
            this.write_semicolon();
        });
        true
    }

    fn emit_scoped_static_super_value_iife(&mut self, emit_body: impl FnOnce(&mut Self)) {
        self.write("(() => {");
        self.write_line();
        self.increase_indent();
        self.push_temp_scope();
        emit_body(self);
        self.pop_temp_scope();
        self.decrease_indent();
        self.write_line();
        self.write("})()");
    }

    fn emit_scoped_static_super_iife_var_decl(&mut self, temps: &[String]) {
        if temps.is_empty() {
            return;
        }
        self.write("var ");
        self.write(&temps.join(", "));
        self.write_semicolon();
        self.write_line();
    }

    pub(in crate::emitter) fn pattern_has_scoped_static_super_assignment_target(
        &self,
        idx: NodeIndex,
    ) -> bool {
        if self.scoped_static_super_member(idx).is_some() {
            return true;
        }
        self.arena
            .get_children(idx)
            .into_iter()
            .any(|child| self.pattern_has_scoped_static_super_assignment_target(child))
    }

    pub(in crate::emitter) fn emit_with_scoped_static_super_assignment_targets(
        &mut self,
        idx: NodeIndex,
    ) {
        let prev = self.scoped_static_super_assignment_target;
        self.scoped_static_super_assignment_target = true;
        self.emit(idx);
        self.scoped_static_super_assignment_target = prev;
    }

    pub(in crate::emitter) fn emit_scoped_static_super_assignment_target(
        &mut self,
        access: &AccessExprData,
        is_element: bool,
    ) -> bool {
        if !self.scoped_static_super_assignment_target {
            return false;
        }
        if self.in_async_captured_super() {
            return false;
        }
        let Some(base_node) = self.arena.get(access.expression) else {
            return false;
        };
        if base_node.kind != SyntaxKind::SuperKeyword as u16
            || self.scoped_static_super_base_alias.is_none()
        {
            return false;
        }

        self.write("({ set value(_a) { ");
        self.emit_scoped_static_super_set_start(&self.member_from_access(access, is_element));
        self.write("_a");
        self.emit_scoped_static_super_set_end();
        self.write("; } }).value");
        true
    }

    fn scoped_static_super_member(&self, idx: NodeIndex) -> Option<StaticSuperMember> {
        let node = self.arena.get(idx)?;
        if node.kind != syntax_kind_ext::PROPERTY_ACCESS_EXPRESSION
            && node.kind != syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION
        {
            return None;
        }
        let access = self.arena.get_access_expr(node)?;
        let base_node = self.arena.get(access.expression)?;
        if base_node.kind != SyntaxKind::SuperKeyword as u16
            || self.scoped_static_super_base_alias.is_none()
        {
            return None;
        }
        let is_element = node.kind == syntax_kind_ext::ELEMENT_ACCESS_EXPRESSION;
        Some(self.member_from_access(&access, is_element))
    }

    const fn member_from_access(
        &self,
        access: &AccessExprData,
        is_element: bool,
    ) -> StaticSuperMember {
        if is_element {
            StaticSuperMember::Element(access.name_or_argument)
        } else {
            StaticSuperMember::Property(access.name_or_argument)
        }
    }

    fn emit_scoped_static_super_get_with_key(
        &mut self,
        member: &StaticSuperMember,
        key_temp: Option<&str>,
        assign_key: bool,
    ) {
        let Some(base_alias) = self.scoped_static_super_base_alias.as_ref().cloned() else {
            return;
        };
        self.write("Reflect.get(");
        self.write(&base_alias);
        self.write(", ");
        self.emit_static_super_member_name(member, key_temp, assign_key);
        self.write(", ");
        self.emit_scoped_static_super_receiver();
        self.write(")");
    }

    fn emit_scoped_static_super_set_start(&mut self, member: &StaticSuperMember) {
        self.emit_scoped_static_super_set_start_with_key(member, None, false);
    }

    fn emit_scoped_static_super_set_start_with_key(
        &mut self,
        member: &StaticSuperMember,
        key_temp: Option<&str>,
        assign_key: bool,
    ) {
        let Some(base_alias) = self.scoped_static_super_base_alias.as_ref().cloned() else {
            return;
        };
        self.write("Reflect.set(");
        self.write(&base_alias);
        self.write(", ");
        self.emit_static_super_member_name(member, key_temp, assign_key);
        self.write(", ");
    }

    fn emit_scoped_static_super_set_end(&mut self) {
        self.write(", ");
        self.emit_scoped_static_super_receiver();
        self.write(")");
    }

    fn emit_static_super_member_name(
        &mut self,
        member: &StaticSuperMember,
        key_temp: Option<&str>,
        assign_key: bool,
    ) {
        match member {
            StaticSuperMember::Property(name) => self.emit_scoped_static_super_property_name(*name),
            StaticSuperMember::Element(argument) => {
                if let Some(key_temp) = key_temp {
                    self.write(key_temp);
                    if assign_key {
                        self.write(" = ");
                        self.emit(*argument);
                    }
                } else {
                    self.emit(*argument);
                }
            }
        }
    }

    fn scoped_static_super_element_key_temp(
        &mut self,
        member: &StaticSuperMember,
    ) -> Option<String> {
        self.scoped_static_super_member_needs_key_temp(member)
            .then(|| self.make_unique_name_hoisted())
    }

    fn scoped_static_super_member_needs_key_temp(&self, member: &StaticSuperMember) -> bool {
        let StaticSuperMember::Element(argument) = member else {
            return false;
        };
        self.arena.get(*argument).is_none_or(|node| {
            node.kind != SyntaxKind::StringLiteral as u16
                && node.kind != SyntaxKind::NumericLiteral as u16
                && node.kind != SyntaxKind::NoSubstitutionTemplateLiteral as u16
        })
    }

    const fn is_static_super_compound_assignment(&self, operator: u16) -> bool {
        operator == SyntaxKind::PlusEqualsToken as u16
            || operator == SyntaxKind::MinusEqualsToken as u16
            || operator == SyntaxKind::AsteriskEqualsToken as u16
            || operator == SyntaxKind::SlashEqualsToken as u16
            || operator == SyntaxKind::PercentEqualsToken as u16
            || operator == SyntaxKind::AsteriskAsteriskEqualsToken as u16
            || operator == SyntaxKind::LessThanLessThanEqualsToken as u16
            || operator == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16
            || operator == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16
            || operator == SyntaxKind::AmpersandEqualsToken as u16
            || operator == SyntaxKind::CaretEqualsToken as u16
            || operator == SyntaxKind::BarEqualsToken as u16
    }

    const fn static_super_compound_base_operator(&self, operator: u16) -> &'static str {
        match operator {
            t if t == SyntaxKind::PlusEqualsToken as u16 => "+",
            t if t == SyntaxKind::MinusEqualsToken as u16 => "-",
            t if t == SyntaxKind::AsteriskEqualsToken as u16 => "*",
            t if t == SyntaxKind::SlashEqualsToken as u16 => "/",
            t if t == SyntaxKind::PercentEqualsToken as u16 => "%",
            t if t == SyntaxKind::AsteriskAsteriskEqualsToken as u16 => "**",
            t if t == SyntaxKind::LessThanLessThanEqualsToken as u16 => "<<",
            t if t == SyntaxKind::GreaterThanGreaterThanEqualsToken as u16 => ">>",
            t if t == SyntaxKind::GreaterThanGreaterThanGreaterThanEqualsToken as u16 => ">>>",
            t if t == SyntaxKind::AmpersandEqualsToken as u16 => "&",
            t if t == SyntaxKind::CaretEqualsToken as u16 => "^",
            t if t == SyntaxKind::BarEqualsToken as u16 => "|",
            _ => "",
        }
    }
}
