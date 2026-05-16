//! ES5 downlevel emission support.
//!
//! Contains destructuring bindings, helper utilities (spread, async, tagged templates),
//! and template literal lowering for ES5 target output.

mod bindings;
mod bindings_assignment;
mod bindings_for_of;
mod bindings_patterns;
mod bindings_read;
mod helpers;
mod helpers_async;
mod helpers_async_generator;
mod helpers_async_shadowing;
#[allow(dead_code)]
pub(in crate::emitter) mod loop_capture;
mod templates;
