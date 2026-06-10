//! Unified recursion guard for cycle detection, depth limiting,
//! and iteration bounding in recursive type computations.
//!
//! # Design
//!
//! `RecursionGuard` replaces the scattered `in_progress` / `visiting` / `depth` /
//! `total_checks` fields that were manually reimplemented across `SubtypeChecker`,
//! `TypeEvaluator`, `PropertyAccessEvaluator`, and others.
//!
//! It combines three safety mechanisms:
//! 1. **Cycle detection** via a visiting set (`FxHashSet<K>`)
//! 2. **Depth limiting** to prevent stack overflow
//! 3. **Iteration bounding** to prevent infinite loops
//!
//! # Profiles
//!
//! [`RecursionProfile`] provides named presets that eliminate magic numbers and
//! make the intent of each guard clear at the call site:
//!
//! ```text
//! // Before (what does 50, 100_000 mean?)
//! let guard = RecursionGuard::new(50, 100_000);
//!
//! // After (intent is clear, limits are centralized)
//! let guard = RecursionGuard::with_profile(RecursionProfile::TypeEvaluation);
//! ```
//!
//! # Safety
//!
//! - **Debug leak detection**: In debug builds, dropping a guard with active entries
//!   triggers a panic, catching forgotten `leave()` calls.
//! - **Debug double-leave detection**: In debug builds, leaving a key that isn't in
//!   the visiting set triggers a panic.
//! - **Overflow protection**: Iteration counting uses saturating arithmetic.
//! - **Encapsulated exceeded state**: The `exceeded` flag is private; use
//!   [`is_exceeded()`](RecursionGuard::is_exceeded) and
//!   [`mark_exceeded()`](RecursionGuard::mark_exceeded).

use rustc_hash::FxHashSet;
use std::hash::Hash;

// ---------------------------------------------------------------------------
// RecursionProfile
// ---------------------------------------------------------------------------

/// Named recursion limit presets.
///
/// Each profile encodes a `(max_depth, max_iterations)` pair that is
/// appropriate for a particular kind of recursive computation. Using profiles
/// instead of raw numbers:
/// - Documents *why* a guard exists at every call site
/// - Centralises limit values so they can be tuned in one place
/// - Prevents copy-paste of magic numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursionProfile {
    /// Subtype checking: deep structural comparison of recursive types.
    ///
    /// Used by `SubtypeChecker`.
    /// Needs the deepest depth limit because structural comparison of
    /// recursive types can legitimately nest deeply before a cycle is found.
    ///
    /// depth = 100, iterations = 100,000
    SubtypeCheck,

    /// Type evaluation: conditional types, mapped types, indexed access.
    ///
    /// Used by `TypeEvaluator` (both `TypeId` guard and `DefId` guard).
    ///
    /// depth = 50, iterations = 100,000
    TypeEvaluation,

    /// Generic type application / instantiation.
    ///
    /// Used by `TypeApplicationEvaluator`.
    /// Matches TypeScript's instantiation depth limit for TS2589.
    ///
    /// depth = 50, iterations = 100,000
    TypeApplication,

    /// Property access resolution on complex types.
    ///
    /// Used by `PropertyAccessEvaluator`.
    ///
    /// depth = 50, iterations = 100,000
    PropertyAccess,

    /// Variance computation.
    ///
    /// Used by `VarianceVisitor`.
    ///
    /// depth = 50, iterations = 100,000
    Variance,

    /// Shape extraction for compatibility checking.
    ///
    /// Used by `ShapeExtractor`.
    ///
    /// depth = 50, iterations = 100,000
    ShapeExtraction,

    /// Shallow type traversal: contains-type checks, type collection.
    ///
    /// Used by `RecursiveTypeCollector`, `ContainsTypeChecker`.
    /// Intentionally shallow — these just walk the top-level structure.
    ///
    /// depth = 20, iterations = 100,000
    ShallowTraversal,

    /// Const assertion processing.
    ///
    /// Used by `ConstAssertionVisitor`.
    ///
    /// depth = 50, iterations = 100,000
    ConstAssertion,

    // ----- Checker profiles -----
    /// Expression type checking depth.
    ///
    /// Used by `ExpressionChecker`.
    /// Generous limit for deeply nested expressions.
    ///
    /// depth = 500
    ExpressionCheck,

    /// Type node resolution depth.
    ///
    /// Used by `TypeNodeChecker`.
    /// Generous limit for deeply nested type annotations.
    ///
    /// depth = 500
    TypeNodeCheck,

    /// Function call resolution depth.
    ///
    /// Used by `get_type_of_call_expression`.
    /// Relatively low to catch infinite recursion in overload resolution.
    ///
    /// depth = 20
    CallResolution,

    /// General checker recursion depth.
    ///
    /// Used by `enter_recursion`/`leave_recursion` on checker functions.
    ///
    /// depth = 50
    CheckerRecursion,

    /// Custom limits for one-off or test scenarios.
    Custom { max_depth: u32, max_iterations: u32 },
}

impl RecursionProfile {
    /// Maximum recursion depth for this profile.
    pub const fn max_depth(self) -> u32 {
        match self {
            Self::SubtypeCheck | Self::TypeEvaluation | Self::TypeApplication => 100,
            Self::PropertyAccess
            | Self::Variance
            | Self::ShapeExtraction
            | Self::ConstAssertion
            | Self::CheckerRecursion => 50,
            Self::ShallowTraversal | Self::CallResolution => 20,
            Self::ExpressionCheck | Self::TypeNodeCheck => 500,
            Self::Custom { max_depth, .. } => max_depth,
        }
    }

    /// Maximum iteration count for this profile.
    pub const fn max_iterations(self) -> u32 {
        match self {
            Self::SubtypeCheck
            | Self::TypeEvaluation
            | Self::TypeApplication
            | Self::PropertyAccess
            | Self::Variance
            | Self::ShapeExtraction
            | Self::ConstAssertion
            | Self::ExpressionCheck
            | Self::TypeNodeCheck
            | Self::CallResolution
            | Self::ShallowTraversal
            | Self::CheckerRecursion => 100_000,
            Self::Custom { max_iterations, .. } => max_iterations,
        }
    }
}

// ---------------------------------------------------------------------------
// RecursionResult
// ---------------------------------------------------------------------------

/// Result of attempting to enter a recursive computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecursionResult {
    /// Proceed with the computation.
    Entered,
    /// This key is already being visited — cycle detected.
    Cycle,
    /// Maximum recursion depth exceeded.
    DepthExceeded,
    /// Maximum iteration count exceeded.
    IterationExceeded,
}

impl RecursionResult {
    /// Returns `true` if entry was successful.
    #[inline]
    pub const fn is_entered(self) -> bool {
        matches!(self, Self::Entered)
    }

    /// Returns `true` if a cycle was detected.
    #[inline]
    pub const fn is_cycle(self) -> bool {
        matches!(self, Self::Cycle)
    }

    /// Returns `true` if any limit was exceeded (depth or iterations).
    #[inline]
    pub const fn is_exceeded(self) -> bool {
        matches!(self, Self::DepthExceeded | Self::IterationExceeded)
    }

    /// Returns `true` if entry was denied for any reason (cycle or exceeded).
    #[inline]
    pub const fn is_denied(self) -> bool {
        !self.is_entered()
    }
}

// ---------------------------------------------------------------------------
// RecursionGuard
// ---------------------------------------------------------------------------

/// Tracks recursion state for cycle detection, depth limiting,
/// and iteration bounding.
///
/// # Usage
///
/// ```text
/// use crate::recursion::{RecursionGuard, RecursionProfile, RecursionResult};
///
/// let mut guard = RecursionGuard::with_profile(RecursionProfile::TypeEvaluation);
///
/// match guard.enter(key) {
///     RecursionResult::Entered => {
///         let result = do_work();
///         guard.leave(key);
///         result
///     }
///     RecursionResult::Cycle => handle_cycle(),
///     RecursionResult::DepthExceeded
///     | RecursionResult::IterationExceeded => handle_exceeded(),
/// }
/// ```
///
/// # Debug-mode safety
///
/// In debug builds (`#[cfg(debug_assertions)]`):
/// - Dropping a guard with entries still in the visiting set panics.
/// - Calling `leave(key)` with a key not in the visiting set panics.
pub struct RecursionGuard<K: Hash + Eq + Copy> {
    visiting: FxHashSet<K>,
    depth: u32,
    iterations: u32,
    max_depth: u32,
    max_iterations: u32,
    max_visiting: u32,
    exceeded: bool,
    /// Sticky flag set when the iteration (relation-count) budget is exhausted.
    /// Distinct from `exceeded` which also covers depth overflow.
    iteration_exceeded: bool,
}

impl<K: Hash + Eq + Copy> RecursionGuard<K> {
    /// Create a guard with explicit limits.
    ///
    /// Prefer [`with_profile`](Self::with_profile) for standard use cases.
    pub fn new(max_depth: u32, max_iterations: u32) -> Self {
        Self {
            visiting: FxHashSet::default(),
            depth: 0,
            iterations: 0,
            max_depth,
            max_iterations,
            max_visiting: 10_000,
            exceeded: false,
            iteration_exceeded: false,
        }
    }

    /// Create a guard from a named [`RecursionProfile`].
    pub fn with_profile(profile: RecursionProfile) -> Self {
        Self::new(profile.max_depth(), profile.max_iterations())
    }

    /// Builder: set a custom max visiting-set size.
    pub const fn with_max_visiting(mut self, max_visiting: u32) -> Self {
        self.max_visiting = max_visiting;
        self
    }

    // -----------------------------------------------------------------------
    // Core enter / leave API
    // -----------------------------------------------------------------------

    /// Try to enter a recursive computation for `key`.
    ///
    /// Returns [`RecursionResult::Entered`] if the computation may proceed.
    /// On success the caller **must** call [`leave`](Self::leave) with the
    /// same key when done.
    ///
    /// The other variants indicate why entry was denied:
    /// - [`Cycle`](RecursionResult::Cycle): `key` is already being visited.
    /// - [`DepthExceeded`](RecursionResult::DepthExceeded): nesting is too deep.
    /// - [`IterationExceeded`](RecursionResult::IterationExceeded): total work budget exhausted.
    pub fn enter(&mut self, key: K) -> RecursionResult {
        // Saturating add prevents overflow with very high max_iterations.
        self.iterations = self.iterations.saturating_add(1);

        if self.iterations > self.max_iterations {
            self.exceeded = true;
            self.iteration_exceeded = true;
            return RecursionResult::IterationExceeded;
        }
        if self.depth >= self.max_depth {
            self.exceeded = true;
            return RecursionResult::DepthExceeded;
        }
        if self.visiting.contains(&key) {
            return RecursionResult::Cycle;
        }
        if self.visiting.len() as u32 >= self.max_visiting {
            self.exceeded = true;
            return RecursionResult::DepthExceeded;
        }

        self.visiting.insert(key);
        self.depth += 1;
        RecursionResult::Entered
    }

    /// Leave a recursive computation for `key`.
    ///
    /// **Must** be called exactly once after every successful [`enter`](Self::enter).
    ///
    /// # Debug panics
    ///
    /// In debug builds, panics if `key` is not in the visiting set (double-leave
    /// or leave without matching enter).
    pub fn leave(&mut self, key: K) {
        let was_present = self.visiting.remove(&key);

        debug_assert!(
            was_present,
            "RecursionGuard::leave() called with a key that is not in the visiting set. \
             This indicates a double-leave or a leave without a matching enter()."
        );

        self.depth = self.depth.saturating_sub(1);
    }

    // -----------------------------------------------------------------------
    // Closure-based RAII helper
    // -----------------------------------------------------------------------

    /// Execute `f` inside a guarded scope.
    ///
    /// Calls `enter(key)`, runs `f` if entered, then calls `leave(key)`.
    /// Returns `Ok(value)` on success or `Err(reason)` if entry was denied.
    ///
    /// This is the safest API when the guard is standalone (not a field of a
    /// struct that `f` also needs to mutate).
    ///
    /// # Panic safety
    ///
    /// If `f` panics, `leave()` is **not** called — the entry leaks. This is
    /// safe because the guard's `Drop` impl (debug builds) checks
    /// `std::thread::panicking()` and suppresses the leak-detection panic
    /// during unwinding.
    pub fn scope<T>(&mut self, key: K, f: impl FnOnce() -> T) -> Result<T, RecursionResult> {
        match self.enter(key) {
            RecursionResult::Entered => {
                let result = f();
                self.leave(key);
                Ok(result)
            }
            denied => Err(denied),
        }
    }

    // -----------------------------------------------------------------------
    // Query API
    // -----------------------------------------------------------------------

    /// Check if `key` is currently being visited (without entering).
    #[inline]
    pub fn is_visiting(&self, key: &K) -> bool {
        self.visiting.contains(key)
    }

    /// Check if any currently-visiting key satisfies the predicate.
    ///
    /// Used for symbol-level cycle detection: the same interface may appear
    /// with different `DefIds` in different checker contexts, so we need to
    /// check all visiting entries for symbol-level matches.
    pub fn is_visiting_any(&self, predicate: impl Fn(&K) -> bool) -> bool {
        self.visiting.iter().any(predicate)
    }

    /// Iterate over currently-visiting keys.  Order is unspecified.
    pub fn visiting_iter(&self) -> impl Iterator<Item = &K> {
        self.visiting.iter()
    }

    /// Current recursion depth (number of active entries on the stack).
    #[inline]
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    /// Total enter attempts so far (successful or not).
    #[inline]
    pub const fn iterations(&self) -> u32 {
        self.iterations
    }

    /// Number of keys currently in the visiting set.
    #[inline]
    pub fn visiting_count(&self) -> usize {
        self.visiting.len()
    }

    /// Returns `true` if the guard has any active entries.
    #[inline]
    pub const fn is_active(&self) -> bool {
        self.depth > 0
    }

    /// The configured maximum depth.
    #[inline]
    pub const fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// The configured maximum iterations.
    #[inline]
    pub const fn max_iterations(&self) -> u32 {
        self.max_iterations
    }

    // -----------------------------------------------------------------------
    // Exceeded-state management
    // -----------------------------------------------------------------------

    /// Returns `true` if any limit was previously exceeded.
    ///
    /// Once set, this flag stays `true` until [`reset()`](Self::reset) is called.
    /// This is sticky: even if depth later decreases below the limit, the flag
    /// remains set. This is intentional — callers use it to bail out early on
    /// subsequent calls (e.g. TS2589 "excessively deep" diagnostics).
    #[inline]
    pub const fn is_exceeded(&self) -> bool {
        self.exceeded
    }

    /// Whether the iteration (relation-count) budget was exhausted.
    ///
    /// Maps to TS2859 "Excessive complexity comparing types". Sticky until
    /// [`reset()`](Self::reset) is called.
    #[inline]
    pub const fn iteration_exceeded(&self) -> bool {
        self.iteration_exceeded
    }

    /// Manually mark the guard as exceeded.
    ///
    /// Useful when an external condition (e.g. distribution size limit) means
    /// further recursion should be blocked.
    #[inline]
    pub const fn mark_exceeded(&mut self) {
        self.exceeded = true;
    }

    /// Clear the depth-`exceeded` flag while preserving `visiting` / `depth` /
    /// `iterations` and the `iteration_exceeded` flag.
    ///
    /// Punches a deliberate hole in the sticky-`exceeded` contract: callers
    /// that treat a particular depth bailout as non-fatal use this so sibling
    /// evaluations at shallower depth can proceed. Reserve for that narrow
    /// purpose — a misuse will silently mask a real exceedance.
    ///
    /// Does **not** clear `iteration_exceeded` — iteration budget exhaustion is
    /// always fatal and must not be silently suppressed.
    #[inline]
    pub(crate) const fn clear_exceeded(&mut self) {
        self.exceeded = false;
    }

    // -----------------------------------------------------------------------
    // Reset
    // -----------------------------------------------------------------------

    /// Reset all state while preserving configured limits.
    ///
    /// After reset the guard behaves as if freshly constructed.
    pub fn reset(&mut self) {
        self.visiting.clear();
        self.depth = 0;
        self.iterations = 0;
        self.exceeded = false;
        self.iteration_exceeded = false;
    }
}

// ---------------------------------------------------------------------------
// Debug-mode leak detection
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
impl<K: Hash + Eq + Copy> Drop for RecursionGuard<K> {
    fn drop(&mut self) {
        if !std::thread::panicking() && !self.visiting.is_empty() {
            panic!(
                "RecursionGuard dropped with {} active entries still in the visiting set. \
                 This indicates leaked enter() calls without matching leave() calls.",
                self.visiting.len(),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// DepthCounter — depth-only guard (no cycle detection)
// ---------------------------------------------------------------------------

/// A lightweight depth counter for stack overflow protection.
///
/// Unlike [`RecursionGuard`], `DepthCounter` does not track which keys are
/// being visited — it only limits nesting depth. Use this when:
/// - The same node/key may be legitimately revisited (e.g., expression
///   re-checking with different contextual types)
/// - You only need stack overflow protection, not cycle detection
///
/// # Safety
///
/// Shares the same debug-mode safety features as `RecursionGuard`:
/// - **Debug leak detection**: Dropping with depth > 0 panics.
/// - **Debug underflow detection**: Calling `leave()` at depth 0 panics.
///
/// # Usage
///
/// ```text
/// let mut counter = DepthCounter::with_profile(RecursionProfile::ExpressionCheck);
///
/// if !counter.enter() {
///     return TypeId::ERROR; // depth exceeded
/// }
/// let result = do_work();
/// counter.leave();
/// result
/// ```
#[derive(Debug)]
pub struct DepthCounter {
    depth: u32,
    max_depth: u32,
    exceeded: bool,
    /// The depth at construction time. Used to distinguish inherited depth
    /// from depth added by this counter's own `enter()` calls.
    /// Debug leak detection only fires if `depth > base_depth`.
    base_depth: u32,
}

impl DepthCounter {
    /// Create a counter with an explicit max depth.
    ///
    /// Prefer [`with_profile`](Self::with_profile) for standard use cases.
    pub const fn new(max_depth: u32) -> Self {
        Self {
            depth: 0,
            max_depth,
            exceeded: false,
            base_depth: 0,
        }
    }

    /// Create a counter from a named [`RecursionProfile`].
    ///
    /// Only the profile's `max_depth` is used (iterations are not relevant
    /// for a depth-only counter).
    pub const fn with_profile(profile: RecursionProfile) -> Self {
        Self::new(profile.max_depth())
    }

    /// Create a counter with an initial depth already set.
    ///
    /// Used when inheriting depth from a parent context to maintain
    /// the overall depth limit across context boundaries. The inherited
    /// depth is treated as the "base" — debug leak detection only fires
    /// if depth exceeds this base at drop time.
    pub const fn with_initial_depth(max_depth: u32, initial_depth: u32) -> Self {
        Self {
            depth: initial_depth,
            max_depth,
            exceeded: false,
            base_depth: initial_depth,
        }
    }

    /// Try to enter a deeper level.
    ///
    /// Returns `true` if the depth limit has not been reached and entry
    /// is allowed. The caller **must** call [`leave`](Self::leave) when done.
    ///
    /// Returns `false` if the depth limit has been reached. The `exceeded`
    /// flag is set and the depth is **not** incremented — do **not** call
    /// `leave()` in this case.
    #[inline]
    pub const fn enter(&mut self) -> bool {
        if self.depth >= self.max_depth {
            self.exceeded = true;
            return false;
        }
        self.depth += 1;
        true
    }

    /// Leave the current depth level.
    ///
    /// **Must** be called exactly once after every successful [`enter`](Self::enter).
    ///
    /// # Debug panics
    ///
    /// In debug builds, panics if depth is already 0 (leave without enter).
    #[inline]
    pub fn leave(&mut self) {
        debug_assert!(
            self.depth > 0,
            "DepthCounter::leave() called at depth 0. \
             This indicates a leave without a matching enter()."
        );
        self.depth = self.depth.saturating_sub(1);
    }

    /// Current depth.
    #[inline]
    pub const fn depth(&self) -> u32 {
        self.depth
    }

    /// The configured maximum depth.
    #[inline]
    pub const fn max_depth(&self) -> u32 {
        self.max_depth
    }

    /// Returns `true` if the depth limit was previously exceeded.
    ///
    /// Sticky — stays `true` until [`reset`](Self::reset).
    #[inline]
    pub const fn is_exceeded(&self) -> bool {
        self.exceeded
    }

    /// Manually mark as exceeded.
    #[inline]
    pub const fn mark_exceeded(&mut self) {
        self.exceeded = true;
    }

    /// Reset to initial state, preserving the max depth and base depth.
    pub const fn reset(&mut self) {
        self.depth = self.base_depth;
        self.exceeded = false;
    }
}

#[cfg(debug_assertions)]
impl Drop for DepthCounter {
    fn drop(&mut self) {
        if !std::thread::panicking() && self.depth > self.base_depth {
            panic!(
                "DepthCounter dropped with depth {} > base_depth {}. \
                 This indicates leaked enter() calls without matching leave() calls.",
                self.depth, self.base_depth,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Cross-operation stack-frame breaker
// ---------------------------------------------------------------------------

/// Maximum number of *simultaneously active* solver recursion frames on a
/// single thread, summed across every recursive solver operation.
///
/// # Why a shared counter is needed
///
/// Every recursive solver operation already carries its own per-instance logical
/// guard: evaluation uses [`RecursionProfile::TypeEvaluation`] (depth 100) plus a
/// thread-local `GLOBAL_EVAL_DEPTH` cap of 200, subtyping uses
/// `MAX_SUBTYPE_DEPTH` (100), and instantiation uses `MAX_INSTANTIATION_DEPTH`
/// (100). Each of those guards lives on the *evaluator / checker / instantiator
/// instance*, so it is reset to zero whenever a fresh instance is constructed.
///
/// The hot solver path does exactly that: `SubtypeChecker::evaluate_type`
/// builds a fresh `TypeEvaluator`, evaluation of an `Application` builds a fresh
/// `TypeInstantiator`, instantiated members are then subtyped through a fresh
/// `SubtypeChecker`, and so on. The cross-operation cycle
/// `evaluate -> subtype -> instantiate -> evaluate -> ...` therefore accumulates
/// real call-stack frames that *no single per-instance guard ever counts*. On a
/// pathological type shape this recursion is bounded only by the OS thread stack
/// and aborts the process with a stack overflow (issue #7574: ~10k-file repo
/// overflows even a 4 GB worker stack — recursion depth in the millions).
///
/// This counter closes that gap: every major recursive solver entry point enters
/// a [`SolverStackFrame`] on the way in (RAII-decremented on the way out), so the
/// *combined* depth of the whole cross-operation cycle is bounded regardless of
/// how many fresh instances are spun up along the way.
///
/// # Calibration
///
/// The cap sits an order of magnitude above the per-instance logical caps
/// (100-200), so it never trips on the finite-but-deep recursion those guards
/// already bound — a successfully-checked 1.47 M LOC corpus peaks far below it.
/// It sits well *below* the frame count that overflows the solver's 128 MB
/// worker stack ([`tsz_common::limits::THREAD_STACK_SIZE_BYTES`]), leaving a
/// comfortable margin even when individual frames carry large stack locals.
pub const MAX_SOLVER_STACK_FRAMES: u32 = 2_000;

thread_local! {
    static SOLVER_STACK_FRAMES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// RAII guard representing one active solver recursion frame.
///
/// Obtained from [`try_enter_solver_frame`]. While held, it keeps the
/// thread-local frame count incremented; dropping it (including during a panic
/// unwind) decrements the count, so the budget can never leak across a normal
/// or panicking return.
#[derive(Debug)]
#[must_use = "the frame budget is only held while this guard is alive"]
pub struct SolverStackFrame {
    // Private field keeps construction restricted to `try_enter_solver_frame`.
    _private: (),
}

impl Drop for SolverStackFrame {
    #[inline]
    fn drop(&mut self) {
        SOLVER_STACK_FRAMES.with(|c| c.set(c.get().saturating_sub(1)));
    }
}

/// Try to enter one solver recursion frame.
///
/// Returns `Some(guard)` when the thread-local frame budget still has headroom;
/// the caller should proceed with its recursive work and let the returned guard
/// drop at the end of the frame. Returns `None` when
/// [`MAX_SOLVER_STACK_FRAMES`] active frames are already on the stack; the
/// caller must then bail out with a safe, relation-preserving default (e.g.
/// `SubtypeResult::DepthExceeded`, an opaque/un-instantiated type) instead of
/// recursing deeper and overflowing the OS stack.
///
/// Most call sites should prefer [`with_solver_frame`], which couples this
/// budget check with the shared `stacker::maybe_grow` segment sizing.
#[inline]
pub fn try_enter_solver_frame() -> Option<SolverStackFrame> {
    SOLVER_STACK_FRAMES.with(|c| {
        let depth = c.get();
        if depth >= MAX_SOLVER_STACK_FRAMES {
            None
        } else {
            c.set(depth + 1);
            Some(SolverStackFrame { _private: () })
        }
    })
}

/// `stacker::maybe_grow` red-zone size shared by every guarded solver recursion
/// site: grow the stack once free headroom drops below this many bytes.
const STACK_RED_ZONE_BYTES: usize = 256 * 1024;
/// `stacker::maybe_grow` new-segment size: how many bytes each fresh stack
/// segment adds when the red zone is reached.
const STACK_GROW_BYTES: usize = 2 * 1024 * 1024;

/// Run `body` on a (possibly freshly grown) stack while accounting for one
/// cross-operation solver recursion frame.
///
/// This is the ergonomic wrapper every recursive solver entry point should use.
/// It pairs [`try_enter_solver_frame`] with a `stacker::maybe_grow` so the two
/// stack-safety mechanisms — the logical frame budget and dynamic stack growth —
/// stay in lockstep with one shared set of segment sizes.
///
/// Returns `Some(body())` when the frame budget had headroom, or `None` when
/// [`MAX_SOLVER_STACK_FRAMES`] frames are already active. On `None`, `body` is
/// **not** run; the caller substitutes a safe, relation-preserving default with
/// `.unwrap_or(...)` / `.unwrap_or_else(...)`. The held frame guard is dropped
/// before the value is returned, so a bail closure can freely re-borrow the same
/// `&mut self` that `body` captured.
#[inline]
pub fn with_solver_frame<T>(body: impl FnOnce() -> T) -> Option<T> {
    let _frame = try_enter_solver_frame()?;
    Some(stacker::maybe_grow(
        STACK_RED_ZONE_BYTES,
        STACK_GROW_BYTES,
        body,
    ))
}

/// Current number of active solver recursion frames on this thread.
///
/// Exposed for tests and diagnostics; normal callers use
/// [`try_enter_solver_frame`].
#[inline]
pub fn solver_stack_frame_depth() -> u32 {
    SOLVER_STACK_FRAMES.with(std::cell::Cell::get)
}

/// Reset the thread-local solver frame counter to zero.
///
/// The counter is RAII-balanced, so it returns to zero on its own after any
/// well-behaved recursion. This reset is a defensive backstop against drift
/// from a panic that was caught and swallowed mid-recursion; call it at the
/// same per-file / per-compilation boundaries that reset the checker and binder
/// stack-overflow breakers.
#[inline]
pub fn reset_solver_stack_frames() {
    SOLVER_STACK_FRAMES.with(|c| c.set(0));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../tests/recursion_tests.rs"]
mod tests;
