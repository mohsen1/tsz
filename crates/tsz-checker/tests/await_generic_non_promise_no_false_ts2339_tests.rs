//! `await Promise<Interface<T>>` must unwrap to `Interface<T>`, not further
//! drill into `T`.
//!
//! Regression: `promise_like_return_type_argument`'s "fallback for generic
//! applications" path unconditionally returned `args.first()` for any
//! Application whose base wasn't recognized as Promise-like. That caused the
//! await loop to re-enter with the first type argument, producing false
//! TS2339 diagnostics like:
//!
//!   interface Box<T> { data: T; }
//!   async function `f()` {
//!       const p: Promise<Box<number>> = null as any;
//!       const r = await p;
//!       r.data;   // tsz (before fix): TS2339 "does not exist on type 'number'"
//!   }

use tsz_binder::BinderState;
use tsz_checker::CheckerState;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_diagnostic_codes(source: &str) -> Vec<u32> {
    let mut parser = ParserState::new("test.ts".to_string(), source.to_string());
    let root = parser.parse_source_file();

    let mut binder = BinderState::new();
    binder.bind_source_file(parser.get_arena(), root);

    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        parser.get_arena(),
        &binder,
        &types,
        "test.ts".to_string(),
        Default::default(),
    );

    checker.check_source_file(root);

    checker.ctx.diagnostics.iter().map(|d| d.code).collect()
}

#[test]
fn await_promise_of_generic_interface_preserves_interface_type() {
    // Minimal repro from `destructureOfVariableSameAsShorthand.ts`.
    let source = r#"
interface Box<T> { data: T; }
async function f() {
    const p: Promise<Box<number>> = null as any;
    const r = await p;
    const body = r.data;
}
"#;
    let codes = get_diagnostic_codes(source);
    assert!(
        !codes.contains(&2339),
        "unexpected TS2339 after `await p` where p: Promise<Box<number>> — the await loop must stop at Box<number>, not unwrap further into `number`. got: {codes:?}"
    );
}

#[test]
fn await_promise_of_generic_interface_with_default_param_preserves_type() {
    // Mirrors the conformance fixture: the interface has a default type arg
    // and the function uses multi-level type-parameter defaults. This locks
    // in that the fix works even in the presence of type-parameter default
    // chains, not just when explicit args are supplied.
    let source = r#"
interface AxiosResponse<T = never> { data: T; }
declare function get<T = never, R = AxiosResponse<T>>(): Promise<R>;
async function main() {
    const response = await get();
    const body = response.data;
}
"#;
    let codes = get_diagnostic_codes(source);
    assert!(
        !codes.contains(&2339),
        "unexpected TS2339 after `await get()` — response must type as AxiosResponse<never>, not `never`. got: {codes:?}"
    );
}
