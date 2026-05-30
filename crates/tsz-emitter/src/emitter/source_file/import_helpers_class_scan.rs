//! Ambient-aware scan that decides whether `--importHelpers` must bind the
//! `tslib` module for class emit (the downlevel `__extends`/`__decorate`
//! helpers).
//!
//! Only classes that actually reach JS emit can use a tslib helper. Ambient
//! declarations — a class carrying the `declare` modifier, or any class nested
//! inside an ambient `declare namespace`/`declare module` — produce no runtime
//! output, so they never require a tslib binding. The scan therefore skips
//! ambient classes and does not descend into ambient module bodies.

use super::super::Printer;
use tsz_parser::parser::node::NodeAccess;
use tsz_parser::parser::syntax_kind_ext;
use tsz_parser::parser::{NodeIndex, NodeList};
use tsz_scanner::SyntaxKind;

impl<'a> Printer<'a> {
    pub(in crate::emitter) fn import_helpers_need_tslib_binding_for_class_emit(
        &self,
        statements: &NodeList,
    ) -> bool {
        self.ctx.options.import_helpers
            && statements
                .nodes
                .iter()
                .copied()
                .any(|idx| self.node_needs_tslib_binding_for_class_emit(idx))
    }

    fn node_needs_tslib_binding_for_class_emit(&self, idx: NodeIndex) -> bool {
        let Some(node) = self.arena.get(idx) else {
            return false;
        };

        // Ambient `declare namespace`/`declare module` bodies are fully erased
        // in JS emit, so no class inside them can reference a tslib helper.
        // Stop descending instead of recursing into their members.
        if node.kind == syntax_kind_ext::MODULE_DECLARATION
            && let Some(module_data) = self.arena.get_module(node)
            && self
                .arena
                .has_modifier(&module_data.modifiers, SyntaxKind::DeclareKeyword)
        {
            return false;
        }

        if matches!(
            node.kind,
            k if k == syntax_kind_ext::CLASS_DECLARATION
                || k == syntax_kind_ext::CLASS_EXPRESSION
        ) && let Some(class_data) = self.arena.get_class(node)
        {
            // A `declare class` is ambient: no runtime emit, no helper.
            if self
                .arena
                .has_modifier(&class_data.modifiers, SyntaxKind::DeclareKeyword)
            {
                return false;
            }

            let needs_extends_helper = self.ctx.target_es5
                && crate::transforms::emit_utils::get_extends_expression_index(
                    self.arena,
                    &class_data.heritage_clauses,
                )
                .is_some();
            let needs_decorator_helper =
                self.ctx.options.legacy_decorators && self.class_has_decorators(class_data);
            if needs_extends_helper || needs_decorator_helper {
                return true;
            }
        }

        self.arena
            .get_children(idx)
            .into_iter()
            .any(|child_idx| self.node_needs_tslib_binding_for_class_emit(child_idx))
    }
}
