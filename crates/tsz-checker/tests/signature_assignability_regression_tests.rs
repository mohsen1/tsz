use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_common::common::ScriptTarget;
use tsz_parser::parser::ParserState;
use tsz_solver::TypeInterner;

fn get_codes_with_options(source: &str, options: CheckerOptions) -> Vec<u32> {
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
        options,
    );

    checker.check_source_file(root);
    checker
        .ctx
        .diagnostics
        .into_iter()
        .filter(|diag| diag.code != 2318)
        .map(|diag| diag.code)
        .collect()
}

#[test]
fn nested_call_signature_assignability_does_not_stack_overflow() {
    let source = r#"
class Base { foo: string; }
class Derived extends Base { bar: string; }
class Derived2 extends Derived { baz: string; }
class OtherDerived extends Base { bing: string; }

declare var a6: (x: (arg: Base) => Derived) => Base;
declare var a7: (x: (arg: Base) => Derived) => (r: Base) => Derived;
declare var a8: (x: (arg: Base) => Derived, y: (arg2: Base) => Derived) => (r: Base) => Derived;
declare var a9: (x: (arg: Base) => Derived, y: (arg2: Base) => Derived) => (r: Base) => Derived;
declare var a15: {
    (x: number): number[];
    (x: string): string[];
};
declare var a16: {
    <T extends Derived>(x: T): number[];
    <U extends Base>(x: U): number[];
};
declare var a17: {
    (x: (a: number) => number): number[];
    (x: (a: string) => string): string[];
};
declare var a18: {
    (x: {
        (a: number): number;
        (a: string): string;
    }): any[];
    (x: {
        (a: boolean): boolean;
        (a: Date): Date;
    }): any[];
}

declare var b6: <T extends Base, U extends Derived>(x: (arg: T) => U) => T;
a6 = b6;
b6 = a6;
declare var b7: <T extends Base, U extends Derived>(x: (arg: T) => U) => (r: T) => U;
a7 = b7;
b7 = a7;
declare var b8: <T extends Base, U extends Derived>(x: (arg: T) => U, y: (arg2: T) => U) => (r: T) => U;
a8 = b8;
b8 = a8;
declare var b9: <T extends Base, U extends Derived>(x: (arg: T) => U, y: (arg2: { foo: string; bing: number }) => U) => (r: T) => U;
a9 = b9;
b9 = a9;
declare var b15: <T>(x: T) => T[];
a15 = b15;
b15 = a15;
declare var b16: <T extends Base>(x: T) => number[];
a16 = b16;
b16 = a16;
declare var b17: <T>(x: (a: T) => T) => T[];
a17 = b17;
b17 = a17;
declare var b18: <T>(x: (a: T) => T) => T[];
a18 = b18;
b18 = a18;
"#;

    let codes = get_codes_with_options(
        source,
        CheckerOptions {
            strict: false,
            strict_null_checks: false,
            strict_function_types: false,
            strict_property_initialization: false,
            no_implicit_any: false,
            no_implicit_this: false,
            use_unknown_in_catch_variables: false,
            target: ScriptTarget::ES2015,
            ..CheckerOptions::default()
        },
    );

    assert!(codes.is_empty(), "expected no diagnostics, got {codes:?}");
}
