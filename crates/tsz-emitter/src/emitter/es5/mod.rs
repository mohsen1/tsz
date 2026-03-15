//! ES5 downlevel emission support.
//!
//! Contains destructuring bindings, helper utilities (spread, async, tagged templates),
//! and template literal lowering for ES5 target output.

mod bindings;
mod bindings_assignment;
mod bindings_patterns;
mod helpers;
mod helpers_async;
#[allow(dead_code)] // WIP: loop capture transform is under active development
pub(in crate::emitter) mod loop_capture;
mod templates;
