//! `WebAssembly` bindings for the `tsz` TypeScript compiler.
//!
//! This crate provides the WASM entry point (cdylib) and TypeScript API
//! compatibility layer for the tsz compiler. It wraps the core `tsz` library
//! with wasm-bindgen bindings.

// wasm_bindgen functions cannot be `const fn` — the proc macro doesn't support it.
#![allow(clippy::missing_const_for_fn)]

// Initialize panic hook for WASM to prevent worker crashes
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn wasm_init() {
    console_error_panic_hook::set_once();

    let version = env!("CARGO_PKG_VERSION");
    let hash = env!("TSZ_GIT_HASH");
    web_sys::console::log_1(&format!("tsz-wasm v{version} ({hash})").into());
}

// Re-export everything from the core library so wasm-bindgen picks up
// all #[wasm_bindgen] annotated types from the root crate
pub use tsz::*;

// WASM integration module - parallel type checking exports
pub mod wasm;
pub use wasm::{WasmParallelChecker, WasmParallelParser, WasmTypeInterner};

// TypeScript API compatibility layer - exposes TS-compatible APIs via WASM
pub mod wasm_api;
pub use wasm_api::{
    TsDiagnostic, TsProgram, TsSignature, TsSourceFile, TsSymbol, TsType, TsTypeChecker,
};
