//! Parallel Processing Module
//!
//! Provides parallel file parsing and processing using Rayon.
//! This enables significant speedups on multi-core machines.
//!
//! # Architecture
//!
//! The compilation pipeline has these parallelization opportunities:
//!
//! 1. **Parsing** - Each file can be parsed independently (embarrassingly parallel)
//! 2. **Binding** - After parsing, binding can be parallelized per-file
//! 3. **Type Checking** - Function bodies can be checked in parallel
//!    (once global symbols are merged)
//!
//! # Usage
//!
//! ```text
//! use tsz::parallel::parse_files_parallel;
//!
//! let files = vec![
//!     ("src/a.ts".to_string(), "let a = 1;".to_string()),
//!     ("src/b.ts".to_string(), "let b = 2;".to_string()),
//! ];
//!
//! let results = parse_files_parallel(files);
//! // results is Vec<ParseResult> with parsed ASTs
//! ```

use crate::binder::BinderOptions;
use crate::binder::BinderState;
use crate::binder::state::{
    BinderStateScopeInputs, CrossFileNodeSymbols, DeclarationArenaMap, SymToDeclIndicesMap,
    WildcardReexportsMap, WildcardReexportsTypeOnlyMap,
};
use crate::binder::{
    FlowNodeArena, FlowNodeId, Scope, ScopeId, SymbolArena, SymbolId, SymbolTable,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::config::resolve_default_lib_files;
use crate::emitter::ScriptTarget;
use crate::lib_loader;
use crate::parser::NodeIndex;
use crate::parser::NodeList;
use crate::parser::node::{NodeArena, SourceFileData};
use crate::parser::{ParseDiagnostic, ParserState};
use anyhow::{Context, Result, bail};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Once;
use tsz_common::interner::{Atom, Interner};
use tsz_scanner::SyntaxKind;

include!("core/parse_and_libs.rs");
include!("core/merge_support.rs");
include!("core/bind_result_reducer.rs");
include!("core/checking.rs");
