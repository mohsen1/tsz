use super::type_node::TypeNodeChecker;
use crate::context::CheckerContext;

impl<'a, 'ctx> TypeNodeChecker<'a, 'ctx> {
    /// Get the context reference (for read-only access).
    pub const fn context(&self) -> &CheckerContext<'ctx> {
        self.ctx
    }
}
