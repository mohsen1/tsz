//! Keyword lowering for the implicit awaits emitted by the downlevel
//! `for await...of` loop transform.
//!
//! A `for await...of` statement lowers to a synchronous `for` loop that
//! awaits the async iterator protocol calls (`iter.next()`, `iter.return()`)
//! and any async `using` disposal. The keyword used for those implicit awaits
//! depends on the lowering mode of the *enclosing* function body, mirroring how
//! an explicit `await` expression is lowered in the same context:
//!
//! * Native async (`async function`, target supports `await`): emit `await x`.
//! * Async-to-generator via `__awaiter` (ES2015/ES2016 async lowering): emit
//!   `yield x`.
//! * Async-generator-to-generator via `__asyncGenerator` (an
//!   `async function*` downleveled below ES2018): emit `yield __await(x)`.
//!
//! The third mode is the load-bearing one: the lowered loop body is a plain
//! `function*`, so a literal `await` there is a `SyntaxError`. tsc wraps the
//! implicit awaits as `yield __await(...)` exactly as it does for explicit
//! `await` expressions inside an async generator.

use super::super::Printer;

impl Printer<'_> {
    /// Emit the opening of an implicit `for await...of` await, leaving the
    /// awaited expression to be emitted next. Pair every call with
    /// [`Self::emit_for_await_implicit_await_suffix`].
    ///
    /// Produces one of: `await `, `yield `, or `yield __await(` depending on the
    /// enclosing function body's async lowering mode.
    pub(in crate::emitter) fn emit_for_await_implicit_await_prefix(&mut self) {
        if self.ctx.emit_await_as_yield_await {
            self.write("yield ");
            self.write_helper("__await");
            self.write("(");
        } else if self.ctx.emit_await_as_yield {
            self.write("yield ");
        } else {
            self.write("await ");
        }
    }

    /// Close an implicit `for await...of` await opened with
    /// [`Self::emit_for_await_implicit_await_prefix`]. Only the
    /// `yield __await(...)` async-generator form needs a closing paren.
    pub(in crate::emitter) fn emit_for_await_implicit_await_suffix(&mut self) {
        if self.ctx.emit_await_as_yield_await {
            self.write(")");
        }
    }
}
