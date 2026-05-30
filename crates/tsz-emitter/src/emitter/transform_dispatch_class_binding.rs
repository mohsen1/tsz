//! ES5 block-scoped class binding-name resolution, split out of
//! `transform_dispatch.rs`.
//!
//! Computes the emitted `var` binding name for a class lowered to an ES5 IIFE.
//! A nested-block class only renames to `Name_N` when it self-references (which
//! forces the hoisted self-alias pattern) or when its name collides with a
//! binding already visible in scope; otherwise it reuses its source name, just
//! like tsc.

use super::declarations::class::class_has_self_references;
use super::*;

impl<'a> Printer<'a> {
    pub(super) fn register_es5_class_binding_name(
        &mut self,
        class_node: NodeIndex,
    ) -> Option<String> {
        let (name_idx, members) = {
            let class_data = self
                .arena
                .get(class_node)
                .and_then(|node| self.arena.get_class(node))?;
            (class_data.name, class_data.members.nodes.clone())
        };
        let original_name = self.get_identifier_text_opt(name_idx)?;
        // A self-referencing block-scoped class forces the hoisted self-alias
        // pattern, so its outer `var` binding must rename (tsc emits
        // `var Foo_1` for `class Foo { static f(): Foo { return new Foo() } }`
        // inside a block). Plain block-scoped classes keep their name.
        let self_referencing = class_has_self_references(
            self.arena,
            self.source_text_for_map(),
            &original_name,
            &members,
        );
        let emitted_name = self
            .ctx
            .block_scope_state
            .register_block_scoped_class(&original_name, self_referencing);
        (emitted_name != original_name).then_some(emitted_name)
    }
}
