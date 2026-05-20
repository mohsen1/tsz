use tsz_checker::context::CheckerOptions;
use tsz_common::common::ModuleKind;

fn compile_entry_file(files: &[(&str, &str)], entry_file: &str) -> Vec<(u32, String)> {
    tsz_checker::test_utils::check_multi_file(
        files,
        entry_file,
        CheckerOptions {
            module: ModuleKind::CommonJS,
            strict: true,
            strict_null_checks: true,
            ..CheckerOptions::default()
        },
    )
    .into_iter()
    .filter(|diag| diag.code != 2318)
    .map(|diag| (diag.code, diag.message_text))
    .collect()
}

#[test]
fn imported_generator_return_type_is_iterable_in_for_of() {
    let globals = r#"
interface SymbolConstructor {
    readonly iterator: unique symbol;
}
declare var Symbol: SymbolConstructor;

interface IteratorResult<T, TReturn = any> {
    done?: boolean;
    value: T | TReturn;
}

interface Iterator<T = unknown, TReturn = any, TNext = unknown> {
    next(...args: [] | [TNext]): IteratorResult<T, TReturn>;
}

interface Iterable<T> {
    [Symbol.iterator](): Iterator<T>;
}

interface Generator<T = unknown, TReturn = any, TNext = unknown>
    extends Iterator<T, TReturn, TNext>, Iterable<T> {}
"#;

    let generator = r#"
export function* generatorExport(): Generator<number> {
    yield 1;
    yield 2;
}
"#;

    let consumer = r#"
import { generatorExport } from "./generator";

for (const n of generatorExport()) {
    const _n: number = n;
}
"#;

    let diagnostics = compile_entry_file(
        &[
            ("globals.d.ts", globals),
            ("generator.ts", generator),
            ("consumer.ts", consumer),
        ],
        "consumer.ts",
    );

    let ts2488 = diagnostics
        .iter()
        .filter(|(code, _)| *code == 2488)
        .collect::<Vec<_>>();
    assert!(
        ts2488.is_empty(),
        "imported Generator<T> return type should be iterable in for-of; got TS2488 {ts2488:?}. All diagnostics: {diagnostics:#?}",
    );
}
