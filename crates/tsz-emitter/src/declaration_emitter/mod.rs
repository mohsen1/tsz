//! Declaration File (.d.ts) Emitter
//!
//! Generates TypeScript declaration files from source code.
//!
//! ```typescript
//! // input.ts
//! export function add(a: number, b: number): number {
//!     return a + b;
//! }
//! export class Calculator {
//!     private value: number;
//!     add(n: number): this { ... }
//! }
//! ```
//!
//! Generates:
//!
//! ```typescript
//! // input.d.ts
//! export declare function add(a: number, b: number): number;
//! export declare class Calculator {
//!     private value;
//!     add(n: number): this;
//! }
//! ```

mod core;
mod exports;
mod helpers;
mod interfaces;
mod type_emission;
pub mod usage_analyzer;

#[cfg(test)]
mod tests;

pub use self::core::DeclarationEmitter;
pub(crate) use self::core::{
    ImportPlan, JsNestedModuleExportNamespaces, PlannedImportModule, PlannedImportSymbol,
};

// Re-export for test access
#[cfg(test)]
pub(super) use crate::type_cache_view::TypeCacheView;
