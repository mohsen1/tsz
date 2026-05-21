//! Regression tests for infinite-recursion / stack-overflow when `match_infer_pattern`
//! recurses through callable return types that form a self-referential cycle.
//!
//! Structural rule: when an interface declares a method whose return type is a generic
//! application that includes `this` (e.g. `optional(): ZodOptional<this>`), infer-pattern
//! matching must propagate the outer `visited` set into callable/function sub-matchers.
//! Without propagation each sub-matcher starts a fresh set and the (source, pattern) pair
//! is revisited indefinitely, overflowing the stack.
//!
//! Adjacent cases tested:
//! 1. Simple builder — no indexed-access in extends clause, verifies correct inference
//! 2. Zod-like optional chaining — verifies no crash
//! 3. Builder/wrap with indexed access — verifies no crash
//! 4. Codec/nullable pattern — verifies no crash
//! 5. Negative case — unrelated type produces no spurious TS2322

use crate::test_utils::check_source_diagnostics;

fn assert_no_ts2322(source: &str, context: &str) {
    let diags = check_source_diagnostics(source);
    let ts2322: Vec<_> = diags.iter().filter(|d| d.code == 2322).collect();
    assert!(
        ts2322.is_empty(),
        "Expected no TS2322 for {context}. Got: {:?}",
        ts2322
            .iter()
            .map(|d| (d.code, &d.message_text))
            .collect::<Vec<_>>()
    );
}

fn assert_no_crash(source: &str) {
    let _diags = check_source_diagnostics(source);
}

const SIMPLE_BOX_DEFS: &str = r#"
interface Box<T> {
    lift(): Lifted<this>;
    val: T;
}
interface Lifted<T extends Box<unknown>> extends Box<T> {}
interface StringBox extends Box<string> {
    trim(): this;
}
type BoxVal<T extends Box<unknown>> = T extends Box<infer V> ? V : never;
"#;

/// `BoxVal<StringBox>` = string (direct property, no indexed access).
#[test]
fn simple_box_val_string_no_ts2322() {
    let source = format!(
        r#"
{SIMPLE_BOX_DEFS}
type R = BoxVal<StringBox>;
declare let r: R;
const _ok: string = r;
"#
    );
    assert_no_ts2322(&source, "BoxVal<StringBox> = string");
}

/// Lifting a StringBox must not crash.
#[test]
fn simple_box_lift_no_crash() {
    let source = format!(
        r#"
{SIMPLE_BOX_DEFS}
declare const sb: StringBox;
const lifted = sb.lift();
type R = BoxVal<typeof lifted>;
declare let r: R;
"#
    );
    assert_no_crash(&source);
}

const ZOD_DEFS: &str = r#"
interface ZodType<Output, Def = unknown> {
    optional(): ZodOptional<this>;
    _output: Output;
}
interface ZodOptional<T extends ZodType<unknown>> extends ZodType<T["_output"] | undefined> {
    unwrap(): T;
}
interface ZodString extends ZodType<string> {
    min(n: number): this;
}
type ZOutput<T extends ZodType<unknown>> = T extends ZodType<infer O, unknown> ? O : never;
"#;

/// `ZOutput<ZodOptional<ZodString>>` must not crash (the reported regression shape from #8772).
#[test]
fn zod_optional_output_no_crash() {
    let source = format!(
        r#"
{ZOD_DEFS}
declare const zStr: ZodString;
const zOpt = zStr.optional();
type R = ZOutput<typeof zOpt>;
declare let r: R;
"#
    );
    assert_no_crash(&source);
}

const CHAIN_DEFS: &str = r#"
interface Chain<T> {
    wrap(): Wrapped<this>;
    value: T;
}
interface Wrapped<T extends Chain<unknown>> extends Chain<T["value"]> {
    inner(): T;
}
interface NumberChain extends Chain<number> {
    inc(): this;
}
type ChainValue<T extends Chain<unknown>> = T extends Chain<infer V> ? V : never;
"#;

/// `ChainValue<NumberChain>` = number.
#[test]
fn chain_value_number_no_ts2322() {
    let source = format!(
        r#"
{CHAIN_DEFS}
type R = ChainValue<NumberChain>;
declare let r: R;
const _ok: number = r;
"#
    );
    assert_no_ts2322(&source, "ChainValue<NumberChain> = number");
}

/// `ChainValue<Wrapped<NumberChain>>` must not crash.
#[test]
fn chain_wrapped_value_no_crash() {
    let source = format!(
        r#"
{CHAIN_DEFS}
declare const nc: NumberChain;
const w = nc.wrap();
type R = ChainValue<typeof w>;
declare let r: R;
"#
    );
    assert_no_crash(&source);
}

const CODEC_DEFS: &str = r#"
interface Codec<A> {
    nullable(): NullableCodec<this>;
    decode(input: unknown): A;
}
interface NullableCodec<C extends Codec<unknown>> extends Codec<ReturnType<C["decode"]> | null> {
    base(): C;
}
interface StringCodec extends Codec<string> {
    trim(): this;
}
type Decoded<C extends Codec<unknown>> = C extends Codec<infer A> ? A : never;
"#;

/// `Decoded<NullableCodec<StringCodec>>` must not crash.
#[test]
fn codec_nullable_decoded_no_crash() {
    let source = format!(
        r#"
{CODEC_DEFS}
declare const sc: StringCodec;
const nullable = sc.nullable();
type R = Decoded<typeof nullable>;
declare let r: R;
"#
    );
    assert_no_crash(&source);
}

/// A type that doesn't satisfy the constraint produces `never` — no crash and no TS2322
/// for the `never` assignment (cycle-break returning `true` must not spuriously bind infer
/// vars for structurally unrelated types under `never`).
#[test]
fn unrelated_never_no_crash() {
    let source = format!(
        r#"
{SIMPLE_BOX_DEFS}
type R = BoxVal<never>;
declare let r: R;
const _ok: never = r;
"#
    );
    assert_no_crash(&source);
    assert_no_ts2322(&source, "BoxVal<never> = never");
}
