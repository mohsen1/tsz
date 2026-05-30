//! ES2022+ legacy-decorator class self-alias publication.
//!
//! When a legacy-decorated (`@experimentalDecorators`) class declaration names
//! itself in its own body, tsc allocates a hoisted `C_1` alias so the decorator
//! application (`C = C_1 = __decorate([...], C)`) can rebind the outer name
//! without breaking internal self-references.
//!
//! How that alias is *published* into the class depends on whether the emit
//! target has native static blocks:
//!
//! * Target `< ES2022` (no native static blocks): tsc prefixes the class
//!   expression with the alias assignment, i.e.
//!   `let C = C_1 = class C { ... };`. Internal references read `C_1`.
//! * Target `>= ES2022` (native static blocks): tsc emits a plain
//!   `let C = class C { ... };` and publishes the alias from a synthetic
//!   *leading* static block inside the class body: `static { C_1 = this; }`.
//!   Internal references still read `C_1`.
//!
//! This module owns the decision and the synthetic-block emission so the large
//! `emit_es6` orchestrator only calls two thin helpers.

use super::super::super::{Printer, ScriptTarget};
use tsz_parser::parser::node::Node;

impl Printer<'_> {
    /// True when the class-self alias for a legacy-decorated, self-referencing
    /// class declaration should be published via a synthetic leading
    /// `static { <alias> = this; }` block (the native-static-block form tsc uses
    /// at ES2022+), rather than the pre-ES2022 outer-expression alias prefix
    /// `let C = <alias> = class C { ... }`.
    ///
    /// The trigger is purely target-driven: native static blocks exist at
    /// ES2022+. It is gated to legacy-decorated classes that already carry a
    /// self-alias (`assignment_alias`), which is the only context where the
    /// pre-ES2022 outer prefix would otherwise be emitted.
    fn legacy_decorator_alias_uses_static_block(
        &self,
        node: &Node,
        assignment_alias: Option<&str>,
    ) -> bool {
        if assignment_alias.is_none()
            || (self.ctx.options.target as u32) < (ScriptTarget::ES2022 as u32)
            || !self.ctx.options.legacy_decorators
        {
            return false;
        }
        self.arena
            .get_class(node)
            .is_some_and(|class| !self.collect_class_decorators(&class.modifiers).is_empty())
    }

    /// Write the outer-expression alias prefix (`<alias> = `) for a
    /// legacy-decorated self-referencing class, e.g. `let C = <alias> = class C`.
    /// Writes nothing when the alias is instead published via a synthetic static
    /// block (ES2022+ native form).
    pub(in crate::emitter) fn write_outer_alias_prefix(
        &mut self,
        node: &Node,
        assignment_alias: Option<&str>,
    ) {
        if self.legacy_decorator_alias_uses_static_block(node, assignment_alias) {
            return;
        }
        if let Some(alias) = assignment_alias {
            self.write(alias);
            self.write(" = ");
        }
    }

    /// Emit the synthetic leading `static { <alias> = this; }` block that
    /// publishes the class-self alias for the native-static-block form (ES2022+).
    /// Only emits when the static-block form applies and the alias differs from a
    /// real class name. tsc places this before every user-authored class member.
    /// Returns whether a block was emitted so the caller can mark a member as
    /// printed.
    pub(in crate::emitter) fn emit_synthetic_static_self_alias_block(
        &mut self,
        node: &Node,
        assignment_alias: Option<&str>,
    ) -> bool {
        if !self.legacy_decorator_alias_uses_static_block(node, assignment_alias) {
            return false;
        }
        // A self-referencing class always has a real name; resolve it so the
        // alias is only published when it actually differs from that name.
        let class_name = self
            .arena
            .get_class(node)
            .filter(|class| class.name.is_some())
            .map(|class| self.get_identifier_text_idx(class.name))
            .unwrap_or_default();
        let alias = match assignment_alias {
            Some(a) if !class_name.is_empty() && a != class_name => a,
            _ => return false,
        };
        self.write("static { ");
        self.write(alias);
        self.write(" = this; }");
        self.write_line();
        true
    }
}
