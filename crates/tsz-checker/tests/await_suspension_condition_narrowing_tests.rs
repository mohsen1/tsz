//! Witness matrix for the `await`/`yield` suspension-point defer fix in the
//! CONDITION arm of the backward flow walk
//! (`condition_antecedent_requires_defer` in
//! `crates/tsz-checker/src/flow/control_flow/core/flow_traversal.rs`).
//!
//! Structural rule:
//!
//! > When a reference is narrowed by an assignment, then an `await`/`yield`
//! > suspension point occurs, then control enters a conditional branch (`if`),
//! > `tsc` keeps the reference narrowed inside and after the branch. `tsz` must
//! > do the same: a CONDITION node whose antecedent is a suspension point — or a
//! > non-targeting initializer produced *after* one (`const d = await …;`) —
//! > must defer through it to the narrowing assignment behind it instead of
//! > finalizing with the un-narrowed declared type.
//!
//! Before the fix `condition_antecedent_requires_defer` recognized CALL,
//! ASSIGNMENT, LABEL and ARRAY_MUTATION antecedents as deferrable but not
//! `AWAIT_POINT`/`YIELD_POINT`, so `x = f(); await p; if (c) { x }` re-widened
//! `x` back to its declared type and emitted a false `TS18048` (and, when the
//! reference was the function's returned value, a cascading false `TS2322` on
//! the inferred `Promise<T | undefined>` return). Witnessed on the `ofetch`
//! corpus row (`fetch.ts`: `context.response` after `const data = await
//! context.response.text()`).
//!
//! All positive cases are `tsc`-clean; the negatives are genuinely
//! possibly-undefined and `tsc` reports `TS18048` on them too. Each case uses
//! distinct binder / parameter names so the behavior follows the structural
//! shape, not any identifier spelling (CLAUDE.md anti-hardcoding gate).

use tsz_checker::test_utils::check_source_codes;

const TS18048_POSSIBLY_UNDEFINED: u32 = 18048;
const TS2322_NOT_ASSIGNABLE: u32 = 2322;

fn assert_narrowing_preserved(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        !diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "narrowing must survive the suspension point + condition \
         (unexpected TS18048); got: {diags:?}",
    );
    assert!(
        !diags.contains(&TS2322_NOT_ASSIGNABLE),
        "the narrowed return must not widen back to `T | undefined` \
         (unexpected TS2322); got: {diags:?}",
    );
}

fn assert_possibly_undefined(source: &str) {
    let diags = check_source_codes(source);
    assert!(
        diags.contains(&TS18048_POSSIBLY_UNDEFINED),
        "a genuinely possibly-undefined read must still report TS18048; got: {diags:?}",
    );
}

// =========================================================================
// Narrowing is PRESERVED across a suspension point + following condition.
// =========================================================================

/// The `ofetch` witness: a property assigned, an initializer produced *after* an
/// `await` (`const data = await …`), then an `if` reading the property. The
/// CONDITION must defer through the non-targeting `data` initializer to the
/// `AWAIT_POINT` and on to the targeting assignment. The returned property must
/// stay non-optional so the inferred `Promise` return keeps no `undefined`.
#[test]
fn property_narrowing_survives_await_initializer_before_condition() {
    assert_narrowing_preserved(
        r#"
interface Payload { code: number; body?: unknown; }
interface Envelope { payload?: Payload; }
declare function produce(): Payload;
async function handle(envelope: Envelope): Promise<Payload> {
    envelope.payload = produce();
    const chunk = await Promise.resolve("x");
    if (chunk) {
        envelope.payload.body = 1;
    }
    return envelope.payload;
}
"#,
    );
}

/// A standalone `await` (no initializer) directly precedes the `if`: the
/// CONDITION's antecedent is the `AWAIT_POINT` itself.
#[test]
fn property_narrowing_survives_standalone_await_before_condition() {
    assert_narrowing_preserved(
        r#"
interface Frame { status: number; }
interface Session { frame?: Frame; }
declare function open(): Frame;
async function drive(session: Session): Promise<Frame> {
    session.frame = open();
    await Promise.resolve(0);
    if (Math.random() > 0) {
        session.frame.status;
    }
    return session.frame;
}
"#,
    );
}

/// The same shape on a plain local `let` (not a property): a suspension point
/// followed by a condition must keep the assignment-narrowing.
#[test]
fn local_narrowing_survives_await_before_condition() {
    assert_narrowing_preserved(
        r#"
declare function fetchCount(): Promise<number>;
async function tally(): Promise<number> {
    let amount: number | undefined = await fetchCount();
    await Promise.resolve("y");
    if (globalThis) {
        amount += 1;
    }
    return amount;
}
"#,
    );
}

/// Generator `yield` is the other suspension point: the `YIELD_POINT` arm must
/// defer just like `AWAIT_POINT`.
#[test]
fn property_narrowing_survives_yield_initializer_before_condition() {
    assert_narrowing_preserved(
        r#"
interface Record_ { size: number; note?: unknown; }
interface Container { record?: Record_; }
declare function assemble(): Record_;
function* stream(container: Container): Generator<number, Record_> {
    container.record = assemble();
    const signal = yield 1;
    if (signal) {
        container.record.note = 1;
    }
    return container.record;
}
"#,
    );
}

/// Anti-hardcoding: an unrelated set of binder / property names in the same
/// structural shape behaves identically — the fix keys on flow structure, not
/// spelling.
#[test]
fn narrowing_survival_is_name_independent() {
    assert_narrowing_preserved(
        r#"
interface Widget { id: number; extra?: unknown; }
interface Registry { entry?: Widget; }
declare function mint(): Widget;
async function register(registry: Registry): Promise<Widget> {
    registry.entry = mint();
    const receipt = await Promise.resolve(true);
    if (receipt) {
        registry.entry.extra = 1;
    }
    return registry.entry;
}
"#,
    );
}

// =========================================================================
// Genuinely possibly-undefined reads still report (no over-narrowing).
// =========================================================================

/// The optional property is never assigned, so a read after the suspension +
/// condition is genuinely possibly-undefined and must still report `TS18048`.
#[test]
fn unassigned_optional_still_reports_after_await() {
    assert_possibly_undefined(
        r#"
interface Slot { value?: { n: number }; }
async function inspect(slot: Slot): Promise<void> {
    await Promise.resolve(0);
    if (globalThis) {
        slot.value.n;
    }
}
"#,
    );
}

/// A conditional assignment does not dominate the later read, so the property
/// remains possibly-undefined across the suspension + condition; `TS18048`
/// must still fire (the fix must not over-narrow).
#[test]
fn conditionally_assigned_optional_still_reports_after_await() {
    assert_possibly_undefined(
        r#"
interface Cell { data?: { size: number }; }
declare function derive(): { size: number };
async function evaluate(cell: Cell, flag: boolean): Promise<void> {
    if (flag) {
        cell.data = derive();
    }
    await Promise.resolve(0);
    if (globalThis) {
        cell.data.size;
    }
}
"#,
    );
}
