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

/// Convergence (#11586): a recursive unwrapper applied to a *literal* argument
/// must not merely terminate — it must resolve to the unwrapped literal, exactly
/// like `tsc`. Before the per-query cross-evaluator memo this either hung or
/// bailed to an opaque/deferred form, so assigning the unwrapped literal back
/// spuriously failed. We assert the *functional* outcome by source line rather
/// than the rendered type name (which the structural-depth bail may still leave
/// unexpanded): the inner-literal assignment must type-check and the unrelated
/// one must error. `assert_terminates` also enforces termination within the
/// deadline.
fn assert_convergence(name: &str, source: &str, ok_line: u32, bad_line: u32) {
    let out = assert_terminates(name, source);
    if out.is_empty() {
        return; // binary not found; assert_terminates already logged the skip.
    }
    assert!(
        out.contains(&format!("repro.ts({bad_line},")),
        "expected a diagnostic on the unrelated assignment (line {bad_line}) for `{name}`,\n\
         showing the recursive type resolved to its unwrapped literal.\noutput:\n{out}"
    );
    assert!(
        !out.contains(&format!("repro.ts({ok_line},")),
        "the inner-literal assignment (line {ok_line}) must type-check for `{name}` — the \
         recursive type converged to the wrong value.\noutput:\n{out}"
    );
}

/// Self-referential unwrapper over an interface resolves a literal argument to
/// the unwrapped literal. Renamed binders (`Wrapper`/`Peel`/`Held`) keep the
/// check name-agnostic.
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

/// A string-literal argument converges the same way (the bug reproduced for any
/// fresh literal/object identity, not just numbers).
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
