//! ES5 downlevel emission support.
//!
//! Contains destructuring bindings, helper utilities (spread, async, tagged templates),
//! and template literal lowering for ES5 target output.

mod bindings;
mod bindings_assignment;
mod bindings_disposable_names;
mod bindings_for_of;
mod bindings_param_patterns;
mod bindings_patterns;
mod bindings_read;
mod for_of_destructure_prealloc;
mod helpers;
mod helpers_async;
mod helpers_async_generator;
mod helpers_async_shadowing;
mod helpers_class_expression_comments;
mod helpers_class_expression_names;
mod helpers_object_literal;
#[allow(dead_code)]
pub(in crate::emitter) mod loop_capture;
pub(in crate::emitter) mod loop_this_capture;
mod templates;
