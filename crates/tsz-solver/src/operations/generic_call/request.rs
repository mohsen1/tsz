//! Typed entry request for generic call resolution.
//!
//! The resolver owns traversal, inference rounds, and result construction.
//! This module names the request stage so the call inputs are bundled
//! explicitly and future cache-key construction or policy options have a
//! single owner rather than being threaded through every call site.

use crate::types::{FunctionShape, TypeId};

/// A normalized request to resolve a single generic function call.
///
/// `GenericCallRequest` bundles a function shape and its argument types into a
/// named boundary. This is the typed entry point for generic call resolution;
/// future cache policy and call-site options will live here rather than on
/// individual call parameters.
pub struct GenericCallRequest<'a> {
    func: &'a FunctionShape,
    arg_types: &'a [TypeId],
}

impl<'a> GenericCallRequest<'a> {
    pub const fn new(func: &'a FunctionShape, arg_types: &'a [TypeId]) -> Self {
        Self { func, arg_types }
    }

    pub const fn func(&self) -> &'a FunctionShape {
        self.func
    }

    pub const fn arg_types(&self) -> &'a [TypeId] {
        self.arg_types
    }
}

#[cfg(test)]
mod tests {
    use super::GenericCallRequest;
    use crate::types::{FunctionShape, TypeId};

    fn empty_func() -> FunctionShape {
        FunctionShape::new(vec![], TypeId::VOID)
    }

    #[test]
    fn request_exposes_func_and_arg_types() {
        let func = empty_func();
        let args = [TypeId::STRING, TypeId::NUMBER];
        let req = GenericCallRequest::new(&func, &args);
        assert_eq!(req.arg_types(), &[TypeId::STRING, TypeId::NUMBER]);
        assert_eq!(req.func().params.len(), 0);
    }

    #[test]
    fn request_accepts_empty_arg_types() {
        let func = empty_func();
        let req = GenericCallRequest::new(&func, &[]);
        assert!(req.arg_types().is_empty());
    }

    #[test]
    fn request_preserves_arg_type_order() {
        let func = empty_func();
        let args = [TypeId::BOOLEAN, TypeId::STRING, TypeId::NUMBER];
        let req = GenericCallRequest::new(&func, &args);
        assert_eq!(req.arg_types()[0], TypeId::BOOLEAN);
        assert_eq!(req.arg_types()[1], TypeId::STRING);
        assert_eq!(req.arg_types()[2], TypeId::NUMBER);
    }
}
