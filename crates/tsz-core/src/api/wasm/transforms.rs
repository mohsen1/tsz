use wasm_bindgen::prelude::wasm_bindgen;

use crate::common::ModuleKind;
use crate::context::transform::TransformContext;

/// Opaque wrapper for transform directives across the wasm boundary.
#[wasm_bindgen]
pub struct WasmTransformContext {
    pub(crate) inner: TransformContext,
    pub(crate) target_es5: bool,
    pub(crate) module_kind: ModuleKind,
}

#[wasm_bindgen]
impl WasmTransformContext {
    /// Get the number of transform directives generated.
    #[wasm_bindgen(js_name = getCount)]
    pub fn get_count(&self) -> usize {
        self.inner.len()
    }
}
