//! Per-file syntax pipeline. Parsed trees are immutable after construction.

mod ast;
mod parser;
mod scanner;
mod token;

pub use ast::*;
pub use parser::{ParseOutput, parse_source};
pub use scanner::{ScanOutput, scan_source};
pub use token::{Token, TokenKind};
