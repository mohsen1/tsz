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
pub struct DeclarationEmitter {
    source_text: Option<String>,
    output_name: Option<String>,
    source_name: Option<String>,
    source_map_enabled: bool,
}

impl DeclarationEmitter {
    pub fn new<T>(_arena: &T) -> Self {
        Self {
            source_text: None,
            output_name: None,
            source_name: None,
            source_map_enabled: false,
        }
    }

    pub fn with_type_info<TArena, TCache, TInterner, TBinder>(
        _arena: &TArena,
        _cache: TCache,
        _interner: &TInterner,
        _binder: &TBinder,
    ) -> Self {
        Self::new(&_arena)
    }

    pub fn set_current_arena<TArena>(&mut self, _arena: TArena, _file_name: String) {}

    pub fn set_arena_to_path<TMap>(&mut self, _arena_to_path: TMap) {}

    pub fn set_binder<TBinder>(&mut self, _binder: Option<&TBinder>) {}

    pub fn set_source_map_text(&mut self, source_text: &str) {
        self.source_text = Some(source_text.to_string());
    }

    pub fn enable_source_map(&mut self, output_name: &str, source_name: &str) {
        self.source_map_enabled = true;
        self.output_name = Some(output_name.to_string());
        self.source_name = Some(source_name.to_string());
    }

    pub fn set_used_symbols<TSymbols>(&mut self, _symbols: TSymbols) {}

    pub fn set_foreign_symbols<TSymbols>(&mut self, _symbols: TSymbols) {}

    pub fn emit<TNode>(&mut self, _root: TNode) -> String {
        String::new()
    }

    pub fn generate_source_map_json(&mut self) -> Option<String> {
        if !self.source_map_enabled {
            return None;
        }

        let output_name = self.output_name.clone()?;
        let source_name = self.source_name.clone()?;
        let source_text = self.source_text.clone().unwrap_or_default();

        Some(
            serde_json::json!({
                "version": 3,
                "file": output_name,
                "sourceRoot": "",
                "sources": [source_name],
                "sourcesContent": [source_text],
                "names": [],
                "mappings": ";",
            })
            .to_string(),
        )
    }
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
