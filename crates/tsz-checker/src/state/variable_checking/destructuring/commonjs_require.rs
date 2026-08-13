//! `const { x } = require('./mod')` destructuring helpers, split out of the
//! parent module to satisfy the source-file line cap.
//!
//! `const { x } = require('./mod')` resolves `x` through the module's
//! *named-export* surface, not generic structural property lookup — a static
//! name absent from a CommonJS module's export surface is TS2305 ("has no
//! exported member"), matching `import { x } from './mod'`, not TS2339.
//! `require_ts2305` returns `true` (having
//! already emitted TS2305) so the caller can return `TypeId::ERROR` instead
//! of `TypeId::ANY` for the miss — matching the sibling
//! `parent_type == TypeId::UNKNOWN` branch's "suppress cascading
//! diagnostics" convention, so the miss doesn't cascade into unrelated
//! diagnostics downstream (e.g. a JS var-redeclaration type-conflict check).

use super::*;

impl<'a> CheckerState<'a> {
    /// The `require(...)` module specifier feeding this binding pattern's
    /// source, when the pattern is the top-level name of a variable
    /// declaration initialized directly by a CommonJS `require` call
    /// (`const { x } = require('./mod')`).
    fn require_module_specifier_for_binding_pattern(
        &self,
        pattern_idx: NodeIndex,
    ) -> Option<String> {
        let parent_idx = self.ctx.arena.get_extended(pattern_idx)?.parent;
        let parent_node = self.ctx.arena.get(parent_idx)?;
        if parent_node.kind != syntax_kind_ext::VARIABLE_DECLARATION {
            return None;
        }
        let decl = self.ctx.arena.get_variable_declaration(parent_node)?;
        if decl.name != pattern_idx || decl.initializer.is_none() {
            return None;
        }
        self.get_require_module_specifier(decl.initializer)
    }

    /// If `pattern_idx` destructures a `require(...)` of a resolvable
    /// CommonJS JS module, emits TS2305 for `prop_name_str` and returns
    /// `true`. The caller has already established (via property resolution
    /// on the `require()` call's own type) that `prop_name_str` is missing;
    /// this only confirms the source is a real CJS module before redirecting
    /// the diagnostic kind.
    pub(super) fn require_ts2305(
        &mut self,
        pattern_idx: NodeIndex,
        prop_name_str: &str,
        error_node: NodeIndex,
    ) -> bool {
        let Some(module_specifier) = self.require_module_specifier_for_binding_pattern(pattern_idx)
        else {
            return false;
        };
        if !self.js_commonjs_require_target_is_js_module(
            &module_specifier,
            Some(self.ctx.current_file_idx),
        ) {
            return false;
        }
        self.emit_no_exported_member_error(&module_specifier, prop_name_str, error_node);
        true
    }
}
