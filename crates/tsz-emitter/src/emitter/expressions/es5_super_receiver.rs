//! ES5 `super` member-call receiver selection.
//!
//! `super.m(...)` / `super[e](...)` lower to `_super.prototype.m.call(R, ...)`.
//! The receiver `R` is the lowered function's own `this`, except when the call
//! sits inside a `this`-capturing arrow: there it must be the captured lexical
//! `this` (`_this`). The lowering pass marks the callee's `super` keyword with a
//! [`TransformDirective::SubstituteThis`] carrying the active capture name; this
//! helper reads that mark so the decision is planned, not reconstructed at emit.

use super::super::Printer;
use crate::context::transform::TransformDirective;
use tsz_parser::parser::NodeIndex;

impl<'a> Printer<'a> {
    /// Emit the receiver argument for a lowered ES5 `super` member call.
    ///
    /// `super_keyword_idx` is the callee's underlying `super` keyword node. If
    /// the lowering pass marked it for lexical-`this` substitution (the call is
    /// inside a `this`-capturing arrow), emit the capture name (`_this`);
    /// otherwise emit `this`.
    pub(in crate::emitter) fn emit_es5_super_call_receiver(
        &mut self,
        super_keyword_idx: NodeIndex,
    ) {
        if let Some(TransformDirective::SubstituteThis { capture_name }) =
            self.transforms.get(super_keyword_idx)
        {
            let name = std::sync::Arc::clone(capture_name);
            self.write(&name);
        } else {
            self.write("this");
        }
    }
}
