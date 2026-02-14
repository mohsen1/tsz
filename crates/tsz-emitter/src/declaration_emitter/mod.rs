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

#![allow(clippy::print_stderr)]

pub mod usage_analyzer;

/// Temporary compatibility shim while declaration emission is being refactored.
/// This keeps downstream crates building; declaration output is currently empty.
pub struct DeclarationEmitter;

impl DeclarationEmitter {
    pub fn new<T>(_arena: &T) -> Self {
        Self
    }

    pub fn with_type_info<TArena, TCache, TInterner, TBinder>(
        _arena: &TArena,
        _cache: TCache,
        _interner: &TInterner,
        _binder: &TBinder,
    ) -> Self {
        Self
    }

    pub fn set_current_arena<TArena>(&mut self, _arena: TArena, _file_name: String) {}

    pub fn set_arena_to_path<TMap>(&mut self, _arena_to_path: TMap) {}

    pub fn set_binder<TBinder>(&mut self, _binder: Option<&TBinder>) {}

    pub fn set_source_map_text(&mut self, _source_text: &str) {}

    pub fn enable_source_map(&mut self, _output_name: &str, _source_name: &str) {}

    pub fn set_used_symbols<TSymbols>(&mut self, _symbols: TSymbols) {}

    pub fn set_foreign_symbols<TSymbols>(&mut self, _symbols: TSymbols) {}

    pub fn emit<TNode>(&mut self, _root: TNode) -> String {
        String::new()
    }

    pub fn generate_source_map_json(&mut self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests;
