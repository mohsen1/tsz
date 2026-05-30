//! Hoisted-static-field `super` rewrite for pre-ES2022 decorated classes.
//!
//! When a TC39-decorated class is lowered for a target below ES2022, plain
//! static fields cannot stay as class-body declarations: they are hoisted to
//! `_classThis.<name> = <init>` statements emitted *after* the class. A bare
//! `super` token is only legal inside a class/object method body, so an
//! initializer such as `static a = super.x` becomes `_classThis.a = super.x`,
//! which Node rejects with `'super' keyword unexpected here`.
//!
//! `tsc` rewrites the value to the scoped-static-super form
//! (`Reflect.get(_classSuper, "x", _classThis)` for reads, an IIFE wrapping
//! `Reflect.set(...)` for writes) before hoisting. The hoist assembly lives in
//! the `TC39DecoratorEmitter` transform, which only has raw source text. This
//! helper renders the rewritten *value* (no name, no modifiers, no trailing
//! `;`) through the main emitter's scoped-static-super machinery and hands it
//! to the transform via `set_hoisted_static_field_value_text`.

use super::Printer;
use tsz_parser::parser::NodeIndex;

impl<'a> Printer<'a> {
    /// Seed both the in-body static-member text and (for pre-ES2022 lowering)
    /// the hoisted-assignment value text for a static field whose initializer
    /// references `super` or `this`. Combining the two seedings keeps the
    /// dispatch call site to a single statement.
    pub(in crate::emitter) fn seed_tc39_decorator_static_member_and_hoisted(
        &mut self,
        emitter: &mut crate::transforms::es_decorators::TC39DecoratorEmitter<'a>,
        member_idx: NodeIndex,
        prop: &tsz_parser::parser::node::PropertyDeclData,
        needs_super_alias: bool,
        this_alias: Option<&str>,
        super_alias: Option<&str>,
    ) {
        self.seed_tc39_decorator_static_member(emitter, member_idx, prop, this_alias, super_alias);
        self.seed_tc39_decorator_hoisted_static_field_value(
            emitter,
            member_idx,
            prop,
            needs_super_alias,
            this_alias,
            super_alias,
        );
    }

    /// Seed the value-only, scoped-static-super-rewritten initializer text used
    /// by the hoisted `_classThis.<name> = <value>` assignment for a plain
    /// static field whose initializer references `super`.
    pub(in crate::emitter) fn seed_tc39_decorator_hoisted_static_field_value(
        &mut self,
        emitter: &mut crate::transforms::es_decorators::TC39DecoratorEmitter<'a>,
        member_idx: NodeIndex,
        prop: &tsz_parser::parser::node::PropertyDeclData,
        needs_super_alias: bool,
        this_alias: Option<&str>,
        super_alias: Option<&str>,
    ) {
        // Only a `super`-referencing initializer needs the rewrite, and only the
        // pre-ES2022 lowering hoists static fields out of the class body; the
        // ES2022 in-body path keeps `super` legal and is rewritten elsewhere.
        if !needs_super_alias
            || !self.ctx.needs_es2022_lowering
            || prop.initializer == NodeIndex::NONE
        {
            return;
        }
        let start = self.writer.len();
        let prev_statement_expression = self.ctx.flags.in_statement_expression;
        self.ctx.flags.in_statement_expression = false;
        self.emit_expression_with_scoped_static_initializer_mode(
            prop.initializer,
            this_alias,
            super_alias,
            false,
        );
        self.ctx.flags.in_statement_expression = prev_statement_expression;
        let value = self.writer.get_output()[start..].trim().to_string();
        self.writer.truncate(start);
        emitter.set_hoisted_static_field_value_text(member_idx, value);
    }
}
