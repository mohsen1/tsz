```rust
// src/transforms/block_scoping_es5_tests.rs

use super::block_scoping_es5;
use swc_ecma_parser::{Parser, StringInput, Syntax};
use swc_ecma_transforms_testing::{test, test_fixture};
use swc_common::SourceMap;
use std::sync::Arc;

// Helper to run a single test case
fn
