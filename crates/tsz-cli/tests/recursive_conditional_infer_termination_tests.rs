//! End-to-end regression for issue #11586 — recursive conditional / `infer`
//! evaluation over generic applications must terminate.
//!
//! `type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T` applied to a literal
//! argument (`Unbox<Box<2>>`), and the equivalent `Awaited<Promise<2>>`, used to
//! hang the compiler: the recursion bounces through fresh `TypeEvaluator` /
//! `SubtypeChecker` instances whose per-instance cycle/depth/iteration guards all
//! reset to zero each level, so none ever fires. The cross-instance per-query
//! operation budget in `TypeEvaluator::evaluate` now bounds it.
//!
//! These tests run the real binary in a subprocess with a small
//! `TSZ_MAX_EVAL_OPS` budget (so the bail fires quickly instead of spinning
//! through the multi-million-op production default) and a wall-clock deadline.
//! The property under test is *termination*: before the fix the process ran
//! forever; now it must exit before the deadline.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("tsz_recursive_infer_term_{name}_{nanos}"));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn write_file(path: &Path, contents: &str) {
    std::fs::write(path, contents).expect("write repro file");
}

fn find_tsz_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_tsz") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    let current_exe = std::env::current_exe().ok()?;
    let debug_dir = current_exe.parent()?.parent()?;
    let candidate = debug_dir.join("tsz");
    candidate.exists().then_some(candidate)
}

/// Run `source` through the real binary with a tiny per-query op budget and a
/// deadline, asserting it terminates. Returns the combined stdout+stderr.
fn assert_terminates(name: &str, source: &str) -> String {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping {name}: tsz binary not found");
        return String::new();
    };
    let temp = TempDir::new(name).expect("temp dir");
    write_file(&temp.path.join("repro.ts"), source);

    let mut child = Command::new(tsz_bin)
        .args(["repro.ts", "--noEmit", "--pretty", "false"])
        .current_dir(&temp.path)
        // Force the cross-instance runaway bail at a small budget so the test is
        // fast even in a debug build; the production default is far higher.
        .env("TSZ_MAX_EVAL_OPS", "20000")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn tsz repro");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if child.try_wait().expect("poll tsz repro").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect killed tsz repro");
            panic!(
                "tsz hung on recursive conditional/infer repro `{name}` instead of terminating.\n\
                 stdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let output = child.wait_with_output().expect("collect tsz repro output");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Self-referential unwrapper over an interface, applied to a literal argument.
#[test]
fn recursive_unwrapper_literal_arg_terminates() {
    assert_terminates(
        "unbox_literal",
        "interface Box<T> { value: T; }\n\
         type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T;\n\
         type X = Unbox<Box<2>>;\n",
    );
}

/// Nested (non-self-referential) conditional whose inner `extends` is itself a
/// generic-with-`infer` pattern, applied to a literal argument.
#[test]
fn nested_conditional_infer_literal_arg_terminates() {
    assert_terminates(
        "nested_literal",
        "interface Box<T> { value: T; }\n\
         type Unwrap2<T> = T extends Box<infer U> ? (U extends Box<infer V> ? V : U) : T;\n\
         type Y = Unwrap2<Box<3>>;\n",
    );
}

/// The same unwrapper reached through an assignment (a separate evaluation path
/// that also previously hung).
#[test]
fn recursive_unwrapper_via_assignment_terminates() {
    assert_terminates(
        "unbox_assign",
        "interface Box<T> { value: T; }\n\
         type Unbox<T> = T extends Box<infer U> ? Unbox<U> : T;\n\
         type X = Unbox<Box<2>>;\n\
         const e: { __: X } = 1 as any;\n",
    );
}

/// The standard-library `Awaited<Promise<literal>>` shape, applied to a literal.
#[test]
fn lib_awaited_promise_literal_terminates() {
    assert_terminates(
        "awaited_literal",
        "type X = Awaited<Promise<Promise<2>>>;\n",
    );
}

/// Run `source` through the real binary at the *production* per-query budget
/// (so the `Awaited` fold runs to completion rather than being forced to bail by
/// a tiny test budget) and return its combined stdout+stderr. Resolution
/// correctness — not just termination — is the property under test here.
fn run_check(name: &str, source: &str) -> String {
    let Some(tsz_bin) = find_tsz_binary() else {
        println!("skipping {name}: tsz binary not found");
        return String::new();
    };
    let temp = TempDir::new(name).expect("temp dir");
    write_file(&temp.path.join("repro.ts"), source);

    let output = Command::new(tsz_bin)
        .args(["repro.ts", "--noEmit", "--pretty", "false"])
        .current_dir(&temp.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run tsz repro");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Regression for issue #11586: a concrete `Awaited<Promise<Promise<T>>>` nest
/// must resolve to the unwrapped value (matching tsc's `getAwaitedType`), not
/// bail to a deferred conditional that then fails assignability.
#[test]
fn lib_awaited_double_nested_promise_resolves_to_literal() {
    let out = run_check(
        "awaited_double_ok",
        "declare const b: Awaited<Promise<Promise<2>>>;\nconst out: 2 = b;\n",
    );
    if out.is_empty() {
        return;
    }
    assert!(
        !out.contains("TS2322"),
        "Awaited<Promise<Promise<2>>> must resolve to 2; got:\n{out}"
    );
}

/// Three Promise layers reach the assignability evaluator deep enough to trip
/// the instantiation depth/fuel guard. All three layers must still unwrap.
#[test]
fn lib_awaited_triple_nested_promise_resolves_to_literal() {
    let out = run_check(
        "awaited_triple_ok",
        "declare const d: Awaited<Promise<Promise<Promise<\"x\">>>>;\nconst out: \"x\" = d;\n",
    );
    if out.is_empty() {
        return;
    }
    assert!(
        !out.contains("TS2322"),
        "Awaited<Promise<Promise<Promise<\"x\">>>> must resolve to \"x\"; got:\n{out}"
    );
}

/// The fold must preserve the unwrapped *value*, not erase or widen the type.
#[test]
fn lib_awaited_nested_promise_preserves_value_for_negative_case() {
    let out = run_check(
        "awaited_double_neg",
        "declare const b: Awaited<Promise<Promise<2>>>;\nconst out: 3 = b;\n",
    );
    if out.is_empty() {
        return;
    }
    assert!(
        out.contains("TS2322"),
        "resolved literal 2 must not be assignable to 3; got:\n{out}"
    );
}

/// Convergence (#11586): a recursive unwrapper applied to a *literal* argument
/// must not merely terminate — it must resolve to the unwrapped literal.
fn assert_convergence(name: &str, source: &str, ok_line: u32, bad_line: u32) {
    let out = assert_terminates(name, source);
    if out.is_empty() {
        return;
    }
    assert!(
        out.contains(&format!("repro.ts({bad_line},")),
        "expected a diagnostic on the unrelated assignment (line {bad_line}) for `{name}`.\noutput:\n{out}"
    );
    assert!(
        !out.contains(&format!("repro.ts({ok_line},")),
        "the inner-literal assignment (line {ok_line}) must type-check for `{name}`.\noutput:\n{out}"
    );
}

#[test]
fn recursive_unwrapper_literal_resolves_to_inner_literal() {
    assert_convergence(
        "peel_resolves",
        "interface Wrapper<Inner> { contents: Inner; }\n\
         type Peel<Wrapped> = Wrapped extends Wrapper<infer Held> ? Peel<Held> : Wrapped;\n\
         declare const a: Peel<Wrapper<Wrapper<7>>>;\n\
         const ok: 7 = a;\n\
         const bad: 8 = a;\n",
        4,
        5,
    );
}

#[test]
fn recursive_unwrapper_string_literal_resolves() {
    assert_convergence(
        "peel_string_resolves",
        "interface Cell<Held> { item: Held; }\n\
         type Open<W> = W extends Cell<infer H> ? Open<H> : W;\n\
         declare const a: Open<Cell<Cell<Cell<\"deep\">>>>;\n\
         const ok: \"deep\" = a;\n\
         const bad: \"other\" = a;\n",
        4,
        5,
    );
}

/// Convergence (#11586): a *nested concrete* `Awaited<Promise<Promise<…>>>` must
/// resolve to the inner literal — exactly like `tsc` — not just terminate. The
/// lazy conditional/`infer` evaluation of the standard-library `Awaited<T>` alias
/// did not converge once the inner `Promise` materialized to its structural
/// `{ then }` Object shape (the outer conditional bailed to its `: T` branch and
/// yielded the still-wrapped argument), so assigning the unwrapped literal back
/// spuriously failed. We assert the *functional* outcome by source line: the
/// matching-literal assignment must type-check and the mismatching one must error
/// (proving the type converged to the precise literal, not an opaque/widened
/// form). `assert_terminates` also enforces termination within the deadline.
fn assert_awaited_convergence(name: &str, source: &str, ok_line: u32, bad_line: u32) {
    let out = assert_terminates(name, source);
    if out.is_empty() {
        return; // binary not found; assert_terminates already logged the skip.
    }
    assert!(
        out.contains(&format!("repro.ts({bad_line},")),
        "expected a diagnostic on the mismatching assignment (line {bad_line}) for `{name}`, \
         showing the nested Awaited resolved to its precise inner literal.\noutput:\n{out}"
    );
    assert!(
        !out.contains(&format!("repro.ts({ok_line},")),
        "the matching-literal assignment (line {ok_line}) must type-check for `{name}` — the \
         nested Awaited converged to the wrong value.\noutput:\n{out}"
    );
}

/// Two levels of plain `Promise` nesting around a numeric literal.
#[test]
fn awaited_nested_promise_converges_to_inner_literal() {
    assert_awaited_convergence(
        "awaited_converge_num",
        "declare const a: Awaited<Promise<Promise<2>>>;\n\
         const ok: 2 = a;\n\
         const bad: 3 = a;\n",
        2,
        3,
    );
}

/// Three levels of nesting around a string literal — convergence must hold at
/// arbitrary depth, not just two.
#[test]
fn awaited_deeply_nested_promise_converges_to_inner_string() {
    assert_awaited_convergence(
        "awaited_converge_str",
        "declare const a: Awaited<Promise<Promise<Promise<\"deep\">>>>;\n\
         const ok: \"deep\" = a;\n\
         const bad: \"other\" = a;\n",
        2,
        3,
    );
}

/// A user-declared structural thenable nested inside a `Promise` is unwrapped
/// too — the fold stays a faithful `getAwaitedType` and does not stop one layer
/// early on a non-lib thenable. Renamed binders keep the check structural.
#[test]
fn awaited_unwraps_user_thenable_nested_in_promise() {
    assert_awaited_convergence(
        "awaited_converge_thenable",
        "interface Holder<Carried> { then(cb: (value: Carried) => void): void; }\n\
         declare const a: Awaited<Promise<Holder<7>>>;\n\
         const ok: 7 = a;\n\
         const bad: 8 = a;\n",
        3,
        4,
    );
}

// ===========================================================================
// Issue #14123 — recursive conditional `infer` over a generic-alias array
// element must not stack-overflow and must resolve like `tsc`.
// ===========================================================================
//
// `type D<T> = T extends Promise<infer U> ? D<U> : T extends { payload: infer P }
//  ? D<P> : T extends (infer E)[] ? D<E> : T` applied through a *generic type
// alias* whose array element is an object type parameter
// (`type Box<T> = Promise<{ payload: T[] }>`; `D<Box<{ id: 0 }>>`) used to abort
// the process with a stack overflow (SIGABRT). The object-pattern `infer`
// matcher unwrapped each deferred `Application` source by recursing into itself
// directly, bypassing the `visited` cycle guard at the `match_infer_pattern`
// entry; with the modern `esnext.iterator` lib active a pair of
// mutually-recursive deferred conditionals evaluated into each other and the
// unwrap loop never converged. `tsc`/`tsgo` resolve the alias to `{ id: 0 }` in
// a few steps. These run the real binary (full pipeline + embedded libs, which
// is required — the in-crate checker harness leaves the recursive alias
// deferred and cannot reproduce it) and assert termination plus the correct
// resolved shape, with binder names varied so the fix is structural.

/// Assert the binary did not abort with a stack overflow on `source`.
fn assert_no_stack_overflow(name: &str, out: &str) {
    assert!(
        !out.contains("overflowed its stack") && !out.contains("stack overflow"),
        "`{name}` stack-overflowed instead of terminating:\n{out}"
    );
}

#[test]
fn recursive_conditional_infer_generic_alias_array_element_resolves() {
    let out = run_check(
        "cond_infer_alias_14123",
        "type D<T> =\n\
         \x20   T extends Promise<infer U> ? D<U> :\n\
         \x20   T extends { payload: infer P } ? D<P> :\n\
         \x20   T extends (infer E)[] ? D<E> :\n\
         \x20   T;\n\
         type Box<T> = Promise<{ payload: T[] }>;\n\
         type R = D<Box<{ id: 0 }>>;\n\
         declare const r: R;\n\
         const good: { id: number } = r;\n\
         const bad: string = r;\n",
    );
    if out.is_empty() {
        return; // binary not found; run_check already logged the skip.
    }
    assert_no_stack_overflow("cond_infer_alias_14123", &out);
    // R resolves to `{ id: 0 }`: the `{ id: number }` target (line 9) type-checks,
    // the `string` target (line 10) is a genuine mismatch and must error.
    assert!(
        out.contains("repro.ts(10,"),
        "resolved `{{ id: 0 }}` must not be assignable to `string` (line 10).\noutput:\n{out}"
    );
    assert!(
        !out.contains("repro.ts(9,"),
        "resolved `{{ id: 0 }}` must be assignable to `{{ id: number }}` (line 9).\noutput:\n{out}"
    );
}

#[test]
fn recursive_conditional_infer_generic_alias_renamed_binders_resolve() {
    // Same structure, every binder renamed: the fix is structural, not by name.
    let out = run_check(
        "cond_infer_alias_renamed_14123",
        "type Unwrap<Source> =\n\
         \x20   Source extends Promise<infer Inner> ? Unwrap<Inner> :\n\
         \x20   Source extends { payload: infer Field } ? Unwrap<Field> :\n\
         \x20   Source extends (infer Element)[] ? Unwrap<Element> :\n\
         \x20   Source;\n\
         type Wrapper<Value> = Promise<{ payload: Value[] }>;\n\
         type Result = Unwrap<Wrapper<{ tag: 7 }>>;\n\
         declare const r: Result;\n\
         const good: { tag: number } = r;\n\
         const bad: string = r;\n",
    );
    if out.is_empty() {
        return;
    }
    assert_no_stack_overflow("cond_infer_alias_renamed_14123", &out);
    assert!(
        out.contains("repro.ts(10,"),
        "renamed alias must resolve to `{{ tag: 7 }}` (line 10 mismatch).\noutput:\n{out}"
    );
    assert!(
        !out.contains("repro.ts(9,"),
        "renamed alias must accept a `{{ tag: number }}` target (line 9).\noutput:\n{out}"
    );
}

#[test]
fn recursive_conditional_infer_inlined_form_resolves() {
    // The inlined equivalent (no generic alias) was always clean; guard the
    // non-aliased path so the fix does not perturb it.
    let out = run_check(
        "cond_infer_inlined_14123",
        "type D<T> =\n\
         \x20   T extends Promise<infer U> ? D<U> :\n\
         \x20   T extends { payload: infer P } ? D<P> :\n\
         \x20   T extends (infer E)[] ? D<E> :\n\
         \x20   T;\n\
         type R = D<Promise<{ payload: { id: 0 }[] }>>;\n\
         declare const r: R;\n\
         const good: { id: number } = r;\n\
         const bad: string = r;\n",
    );
    if out.is_empty() {
        return;
    }
    assert_no_stack_overflow("cond_infer_inlined_14123", &out);
    assert!(
        out.contains("repro.ts(9,"),
        "inlined form must resolve to `{{ id: 0 }}` (line 9 mismatch).\noutput:\n{out}"
    );
    assert!(
        !out.contains("repro.ts(8,"),
        "inlined form must accept a `{{ id: number }}` target (line 8).\noutput:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// Issue #14123 — process-safety guards (crash must never recur).
// ---------------------------------------------------------------------------
//
// The headline of #14123 is a **stack-overflow abort (SIGABRT, exit 134)**:
// evaluating the recursive `infer` unwrapper over a generic-alias array element
// used to overflow the OS stack and wedge the whole compilation. The crash is
// fixed (the object-pattern `infer` matcher now unwraps deferred `Application`
// sources through the shared cycle guard, #14135). These guards assert the
// **process-safety contract** directly — the conditional/`infer` evaluation path
// must terminate and must never abort the process — independently of whether the
// alias fully *converges* to `tsc`'s reduced shape.
//
// NOTE on the residual: the convergence assertions above
// (`*_resolves` / `*_resolve`) require the alias to reduce to its `tsc` value
// (`{ id: 0 }`). They do not yet hold: a recursive conditional whose *matching*
// branch follows a non-matching `infer`-application branch (`Promise<infer U>` /
// `Fn<infer U>`) leaves the alias deferred instead of reduced, because the
// conditional's `extends`-operand semantic ref (`TypeData::Lazy(DefId)` for
// `Promise<infer U>`) resolves to an *unregistered* body at the use site
// (`resolve_lazy_type: body unregistered`), so the branch relation is reported
// `Undetermined` and the conditional defers (the deferral introduced for #14238).
// That is a distinct `Lazy`-operand registration defect tracked under #14123; the
// process-safety guards below stand on their own and must always pass.

/// The canonical #14123 minimal repro must terminate without aborting the
/// process, regardless of the resolved shape.
#[test]
fn recursive_conditional_infer_canonical_repro_is_process_safe() {
    let out = assert_terminates(
        "cond_infer_process_safe_14123",
        "type D<T> =\n\
         \x20   T extends Promise<infer U> ? D<U> :\n\
         \x20   T extends { payload: infer P } ? D<P> :\n\
         \x20   T extends (infer E)[] ? D<E> :\n\
         \x20   T;\n\
         type Box<T> = Promise<{ payload: T[] }>;\n\
         type R = D<Box<{ id: 0 }>>;\n\
         declare const r: R;\n",
    );
    if out.is_empty() {
        return;
    }
    assert_no_stack_overflow("cond_infer_process_safe_14123", &out);
}

/// Renamed binders — the process-safety contract is structural, not name-keyed.
#[test]
fn recursive_conditional_infer_renamed_repro_is_process_safe() {
    let out = assert_terminates(
        "cond_infer_process_safe_renamed_14123",
        "type Unwrap<Source> =\n\
         \x20   Source extends Promise<infer Inner> ? Unwrap<Inner> :\n\
         \x20   Source extends { payload: infer Field } ? Unwrap<Field> :\n\
         \x20   Source extends (infer Element)[] ? Unwrap<Element> :\n\
         \x20   Source;\n\
         type Wrapper<Value> = Promise<{ payload: Value[] }>;\n\
         type Result = Unwrap<Wrapper<{ tag: 7 }>>;\n\
         declare const r: Result;\n",
    );
    if out.is_empty() {
        return;
    }
    assert_no_stack_overflow("cond_infer_process_safe_renamed_14123", &out);
}

/// A genuinely non-converging recursive `infer` conditional must degrade to a
/// bounded `TS2589`, never a stack-overflow abort — the conditional-eval path
/// owns its own recursion budget.
#[test]
fn recursive_conditional_infer_divergent_reports_ts2589_not_crash() {
    // Production op budget (via `run_check`): the non-converging recursion runs
    // far enough to trip the conditional tail-recursion depth limit and surface
    // `TS2589`, rather than being cut short by the tiny `assert_terminates`
    // budget. It self-terminates, so no deadline wrapper is needed.
    let out = run_check(
        "cond_infer_divergent_14123",
        "type Deep<T> = T extends (infer E)[] ? Deep<E[]> : T;\n\
         type Y = Deep<number[]>;\n\
         declare const y: Y;\n",
    );
    if out.is_empty() {
        return;
    }
    assert_no_stack_overflow("cond_infer_divergent_14123", &out);
    assert!(
        out.contains("TS2589"),
        "a non-converging recursive `infer` conditional must surface TS2589, \
         not abort:\n{out}"
    );
}
