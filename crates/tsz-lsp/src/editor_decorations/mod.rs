//! Editor decoration providers (inline annotations).
//!
//! - **Inlay hints**: inline type/parameter annotations in source code
//! - **Code lens**: actionable commands displayed above declarations
//! - **Document colors**: inline color swatches for hex color literals

pub mod code_lens;
pub mod document_color;
pub mod inlay_hints;
