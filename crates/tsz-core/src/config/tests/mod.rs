//! Unit tests for the tsconfig / compiler-options parser.
//!
//! Split from `config/mod.rs` into concern-focused shards to keep each file
//! under the 2000-line limit (§19; ratchet tracked by #8280). This module
//! contains no test logic itself; it only wires up the shards.

mod module_resolution;
mod options_parsing;
mod strict_lib_extends;
