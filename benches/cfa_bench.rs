//! Control Flow Analysis benchmarks.
//!
//! Measures the performance impact of CFA on type checking operations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use wasm::checker::CheckerOptions;
use wasm::solver::{TypeId, TypeInterner};
use wasm::thin_binder::ThinBinderState;
use wasm::thin_checker::ThinCheckerState;
use wasm::thin_parser::ThinParserState;

/// Simple code without complex control flow.
const SIMPLE_CODE: &str = r#"
let x: number = 1;
let y: string = "hello";
let z: boolean = true;
console.log(x, y, z);
"#;

/// Code with if-else branching.
const IF_ELSE_CODE: &str = r#"
let x: string | number;
if (Math.random() > 0.5) {
    x = "a";
} else {
    x = 1;
}
console.log(x);
"#;

/// Code with deeply nested control flow.
const NESTED_CONTROL_FLOW: &str = r#"
let x: string | number | boolean | null;
if (Math.random() > 0.5) {
    if (Math.random() > 0.5) {
        if (Math.random() > 0.5) {
            x = "deep";
        } else {
            x = 42;
        }
    } else {
        x = true;
    }
} else {
    x = null;
}
console.log(x);
"#;

/// Code with loop control flow.
const LOOP_CODE: &str = r#"
let x: number;
for (let i = 0; i < 10; i++) {
    if (i === 5) break;
    if (i % 2 === 0) continue;
    x = i;
}
console.log(x);
"#;

/// Code with try-catch-finally.
const TRY_CATCH_CODE: &str = r#"
let x: number;
try {
    x = 1;
    throw new Error();
} catch (e) {
    x = 2;
} finally {
    x = 3;
}
console.log(x);
"#;

/// Code with switch statement.
const SWITCH_CODE: &str = r#"
let x: "a" | "b" | "c" = "a";
let result: number;
switch (x) {
    case "a":
        result = 1;
        break;
    case "b":
        result = 2;
        break;
    case "c":
        result = 3;
        break;
}
console.log(result);
"#;

/// Code with type narrowing via typeof.
const TYPE_NARROWING_CODE: &str = r#"
function process(x: string | number | boolean) {
    if (typeof x === "string") {
        return x.length;
    } else if (typeof x === "number") {
        return x.toFixed(2);
    } else {
        return x.toString();
    }
}
"#;

/// Code with closures and callbacks.
const CLOSURE_CODE: &str = r#"
let x: string | number;
x = "initial";
const arr = [1, 2, 3];
arr.forEach((item) => {
    const y = x;
    console.log(y, item);
});
const callback = () => {
    return x;
};
"#;

/// Code with class and property initialization.
const CLASS_CODE: &str = r#"
class Foo {
    value: number;
    optional?: string;

    constructor(init: boolean) {
        if (init) {
            this.value = 1;
        } else {
            this.value = 2;
        }
    }

    method(): number {
        return this.value;
    }
}
"#;

/// Complex code combining multiple CFA patterns.
const COMPLEX_CODE: &str = r#"
function complex(input: string | number | null, flag: boolean) {
    let result: string;

    if (input === null) {
        result = "null";
    } else if (typeof input === "string") {
        result = input.toUpperCase();
    } else {
        result = input.toString();
    }

    try {
        if (flag) {
            for (let i = 0; i < 5; i++) {
                if (i === 3) break;
                result += i.toString();
            }
        } else {
            switch (result.length) {
                case 0:
                    result = "empty";
                    break;
                case 1:
                    result = "single";
                    break;
                default:
                    result = "multi";
            }
        }
    } catch (e) {
        result = "error";
    } finally {
        result += "_done";
    }

    return result;
}
"#;

/// Benchmark parsing and binding (includes flow graph construction).
fn bench_parse_and_bind(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfa_parse_bind");

    let test_cases = [
        ("simple", SIMPLE_CODE),
        ("if_else", IF_ELSE_CODE),
        ("nested_control_flow", NESTED_CONTROL_FLOW),
        ("loop", LOOP_CODE),
        ("try_catch", TRY_CATCH_CODE),
        ("switch", SWITCH_CODE),
        ("type_narrowing", TYPE_NARROWING_CODE),
        ("closure", CLOSURE_CODE),
        ("class", CLASS_CODE),
        ("complex", COMPLEX_CODE),
    ];

    for (name, code) in test_cases {
        group.bench_with_input(BenchmarkId::new("parse_bind", name), code, |b, code| {
            b.iter(|| {
                let mut parser = ThinParserState::new("bench.ts".to_string(), code.to_string());
                let root = parser.parse_source_file();
                let mut binder = ThinBinderState::new();
                binder.bind_source_file(parser.get_arena(), root);
                black_box(root)
            })
        });
    }

    group.finish();
}

/// Benchmark type checking with CFA.
fn bench_type_check(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfa_type_check");

    let test_cases = [
        ("simple", SIMPLE_CODE),
        ("if_else", IF_ELSE_CODE),
        ("nested_control_flow", NESTED_CONTROL_FLOW),
        ("loop", LOOP_CODE),
        ("try_catch", TRY_CATCH_CODE),
        ("switch", SWITCH_CODE),
        ("type_narrowing", TYPE_NARROWING_CODE),
        ("closure", CLOSURE_CODE),
        ("class", CLASS_CODE),
        ("complex", COMPLEX_CODE),
    ];

    for (name, code) in test_cases {
        group.bench_with_input(BenchmarkId::new("check", name), code, |b, code| {
            b.iter(|| {
                let mut parser = ThinParserState::new("bench.ts".to_string(), code.to_string());
                let root = parser.parse_source_file();
                let mut binder = ThinBinderState::new();
                binder.bind_source_file(parser.get_arena(), root);
                let types = TypeInterner::new();
                let compiler_options = wasm::cli::config::CheckerOptions::default();
                let mut checker = ThinCheckerState::new(
                    parser.get_arena(),
                    &binder,
                    &types,
                    "bench.ts".to_string(),
                    compiler_options,
                );
                checker.check_source_file(root);
                black_box(checker.ctx.diagnostics.len())
            })
        });
    }

    group.finish();
}

/// Benchmark flow analysis specifically.
fn bench_flow_analysis(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfa_flow_analysis");

    // Prepare pre-parsed code for flow analysis benchmarks
    let test_cases = [
        ("if_else", IF_ELSE_CODE),
        ("nested", NESTED_CONTROL_FLOW),
        ("loop", LOOP_CODE),
        ("switch", SWITCH_CODE),
        ("complex", COMPLEX_CODE),
    ];

    for (name, code) in test_cases {
        group.bench_with_input(BenchmarkId::new("bind_with_flow", name), code, |b, code| {
            b.iter(|| {
                let mut parser = ThinParserState::new("bench.ts".to_string(), code.to_string());
                let root = parser.parse_source_file();
                let mut binder = ThinBinderState::new();
                binder.bind_source_file(parser.get_arena(), root);
                black_box(root)
            })
        });
    }

    group.finish();
}

/// Benchmark scaling with code size.
fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("cfa_scaling");

    // Generate code with increasing numbers of if-else branches
    let branch_counts = [1, 5, 10, 20, 50];

    for &n in &branch_counts {
        let mut code = String::from("let x: number;\n");
        for i in 0..n {
            if i == 0 {
                code.push_str(&format!("if (Math.random() > 0.5) {{ x = {}; }}\n", i));
            } else {
                code.push_str(&format!("else if (Math.random() > 0.5) {{ x = {}; }}\n", i));
            }
        }
        code.push_str("else { x = -1; }\nconsole.log(x);\n");

        group.bench_with_input(BenchmarkId::new("branches", n), &code, |b, code| {
            b.iter(|| {
                let mut parser = ThinParserState::new("bench.ts".to_string(), code.clone());
                let root = parser.parse_source_file();
                let mut binder = ThinBinderState::new();
                binder.bind_source_file(parser.get_arena(), root);
                let types = TypeInterner::new();
                let mut checker = ThinCheckerState::new(
                    parser.get_arena(),
                    &binder,
                    &types,
                    "bench.ts".to_string(),
                    CheckerOptions::default(),
                );
                checker.check_source_file(root);
                black_box(root)
            })
        });
    }

    // Generate code with increasing nesting depth
    let nesting_depths = [1, 3, 5, 10];

    for &depth in &nesting_depths {
        let mut code = String::from("let x: number;\n");
        for _ in 0..depth {
            code.push_str("if (Math.random() > 0.5) {\n");
        }
        code.push_str("x = 42;\n");
        for _ in 0..depth {
            code.push_str("} else { x = 0; }\n");
        }
        code.push_str("console.log(x);\n");

        group.bench_with_input(BenchmarkId::new("nesting", depth), &code, |b, code| {
            b.iter(|| {
                let mut parser = ThinParserState::new("bench.ts".to_string(), code.clone());
                let root = parser.parse_source_file();
                let mut binder = ThinBinderState::new();
                binder.bind_source_file(parser.get_arena(), root);
                let types = TypeInterner::new();
                let mut checker = ThinCheckerState::new(
                    parser.get_arena(),
                    &binder,
                    &types,
                    "bench.ts".to_string(),
                    CheckerOptions::default(),
                );
                checker.check_source_file(root);
                black_box(root)
            })
        });
    }

    group.finish();
}

criterion_group!(
    cfa_benches,
    bench_parse_and_bind,
    bench_type_check,
    bench_flow_analysis,
    bench_scaling
);
criterion_main!(cfa_benches);
