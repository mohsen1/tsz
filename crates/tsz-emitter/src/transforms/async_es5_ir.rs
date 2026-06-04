use std::cell::{Cell, RefCell};

use crate::transforms::class_es5_ir::ES5ClassTransformer;

use crate::transforms::helpers::HelpersNeeded;

use crate::transforms::ir::{IRCatchClause, IRGeneratorCase, IRNode, IRParam};

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_common::common::ModuleKind;

use tsz_parser::parser::NodeIndex;

use tsz_parser::parser::node::NodeArena;

use tsz_parser::parser::node_flags;

use tsz_parser::parser::syntax_kind_ext;

#[path = "async_es5_ir_bindings.rs"]
mod bindings;

#[path = "async_es5_ir_calls.rs"]
mod calls;

#[path = "async_es5_ir_cases.rs"]
mod cases;

#[path = "async_es5_ir_condition_await.rs"]
mod condition_await;

#[path = "async_es5_ir_discovery.rs"]
mod discovery;

#[path = "async_es5_ir_element_access.rs"]
mod element_access;

#[path = "async_es5_ir_for_await.rs"]
mod for_await;

#[path = "async_es5_ir_for_of.rs"]
mod for_of;

#[path = "async_es5_ir_generator_fn.rs"]
mod generator_fn;

#[path = "async_es5_ir_hoists.rs"]
mod hoists;

#[path = "async_es5_ir_loop_control.rs"]
mod loop_control;

#[path = "async_es5_ir_names.rs"]
mod names;

#[path = "async_es5_ir_state.rs"]
mod state;

#[path = "async_es5_ir_statement_helpers.rs"]
mod statement_helpers;

#[path = "async_es5_ir_suspension.rs"]
mod suspension;

#[path = "async_es5_ir_switch.rs"]
mod switch;

#[path = "async_es5_ir_try_region.rs"]
mod try_region;

pub use state::AsyncTransformState;

use state::{ForInAssignmentTarget, ForInSuspendedElementIndex, ForInSuspendedObject};

use try_region::{TryRegionPlaceholders, TryRegionResolution, patch_try_region_placeholders};

#[path = "async_es5_ir_opcodes.rs"]
pub mod opcodes;

/// Pieces of an ES5 class factory broken out from a transformed
/// `ES5ClassIIFE` so that callers can splice the body into a generator
/// case while still emitting weakmap declarations / instantiations and
/// deferred static blocks alongside the class assignment.
struct ES5ClassFactoryParts {
    factory: IRNode,
    /// Names of `WeakMap` declarations for private fields. Must be
    /// declared as part of the surrounding scope (otherwise references
    /// to them in the class body fail at runtime with `ReferenceError`).
    weakmap_decls: Vec<String>,
    /// Pre-rendered `WeakMap` instantiation expression strings (e.g.
    /// `_value = new WeakMap()`). Emitted after the class assignment.
    weakmap_inits: Vec<String>,
    /// Static block IIFEs deferred to after the class assignment.
    deferred_static_blocks: Vec<IRNode>,
}

/// Async ES5 transformer that produces IR nodes instead of strings.
///
/// This transformer mirrors the `GeneratorES5Transformer` pattern from generators.rs.
/// It converts async functions to ES5 code using __awaiter and __generator helpers.
pub struct AsyncES5Transformer<'a> {
    pub(crate) arena: &'a NodeArena,
    pub(super) source_text: Option<&'a str>,
    pub(crate) state: AsyncTransformState,
    helpers_needed: HelpersNeeded,
    /// When true, looks for yield instead of await.
    pub(crate) generator_mode: bool,
    /// When true, generator-mode yields feed `__await(...)` values to
    /// `__asyncGenerator`.
    pub(crate) async_generator_mode: bool,
    /// Whether ES5 `for..of` lowering must use iterator protocol helpers.
    pub(crate) downlevel_iteration: bool,
    temp_var_counter: Cell<u32>,
    blocked_temp_names: RefCell<FxHashSet<String>>,
    disposable_env_counter: Cell<u32>,
    blocked_disposable_env_names: FxHashSet<String>,
    generated_disposable_env_names: Vec<String>,
    lexical_this_capture: Cell<bool>,
    capture_this_references: Cell<bool>,
    loop_exit_placeholder_counter: Cell<u32>,
    /// Pending hoisted-temp names accumulated by IR-conversion lowerings
    /// (nullish coalescing, optional chaining, etc.) so callers can declare
    /// them in the surrounding state-machine scope. Drained by every
    /// `transform_*` entry point after the generator body is built.
    pub(super) pending_lowering_hoists: RefCell<Vec<String>>,
    /// Whether this async body is emitted inside a derived ES5 class method.
    pub(super) class_has_super: bool,
    /// Generated super parameter name for the surrounding ES5 class IIFE.
    pub(super) class_super_name: String,
    /// Whether the surrounding class member is static.
    pub(super) class_super_is_static: bool,
    /// Module kind for dynamic `import()` lowering inside generator bodies.
    pub(super) module_kind: ModuleKind,
    /// Whether the emit target is ES5. Controls arrow-vs-`function` form in
    /// dynamic-import lowering inside async generator bodies.
    pub(super) target_es5: bool,
    /// Counter for AMD/UMD dynamic import promise callback identifiers.
    pub(in crate::transforms) dynamic_import_promise_counter: Cell<u32>,
    /// Active async-lowered loop labels and the generator label that implements
    /// `continue <label>` for that loop.
    pub(super) labeled_continue_targets: Vec<(String, u32)>,
    /// Active async-lowered loop labels and the generator label that implements
    /// `break <label>` for that loop.
    pub(super) labeled_break_targets: Vec<(String, u32)>,
    /// Active catch binding substitutions used while lowering async try regions.
    pub(super) catch_binding_renames: Vec<(String, String)>,
    /// File-wide ordinal counter for each source catch-binding name.
    /// tsc increments the suffix across async functions in the same file so
    /// the first `catch (e)` in the file becomes `e_1`, the second becomes
    /// `e_2`, and so on, regardless of function boundaries.
    pub(super) catch_binding_ordinals: RefCell<rustc_hash::FxHashMap<String, u32>>,
    /// Catch-binding temps reserved for the async function body currently being
    /// lowered. Reserving them before body conversion lets nested async function
    /// expressions continue after the outer body's catch names.
    pub(super) planned_catch_binding_temps: RefCell<FxHashMap<u32, String>>,
}

include!("async_es5_ir_parts/part1.rs");
include!("async_es5_ir_parts/part2.rs");
include!("async_es5_ir_parts/part3.rs");
include!("async_es5_ir_parts/part4.rs");

#[cfg(test)]
#[path = "../../tests/async_es5_ir.rs"]
mod tests;
