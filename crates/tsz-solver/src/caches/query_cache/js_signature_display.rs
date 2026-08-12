//! [`JsSignatureDisplaySource`] forwarding for [`QueryCache`]: the masks live
//! on the wrapped [`TypeInterner`]; the cache adds nothing on top.

use super::QueryCache;
use crate::caches::db::JsSignatureDisplaySource;
use crate::types::{FunctionShape, FunctionShapeId, TypeId};
use std::sync::Arc;

impl JsSignatureDisplaySource for QueryCache<'_> {
    fn function_with_arity_optional_mask(&self, shape: FunctionShape, mask: &[bool]) -> TypeId {
        self.interner.function_with_arity_optional_mask(shape, mask)
    }

    fn function_shape_arity_optional_mask(&self, id: FunctionShapeId) -> Option<Arc<[bool]>> {
        self.interner.function_shape_arity_optional_mask(id)
    }
}
