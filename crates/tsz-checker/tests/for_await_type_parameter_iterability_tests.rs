//! `for await ... of` over a **type parameter** must resolve the parameter to
//! its apparent type (constraint) when deciding async-iterability, exactly like
//! the sync `for ... of` path. Regression coverage for the false `TS2504`
//! emitted when the operand's type is a type parameter whose constraint is a
//! generic `AsyncIterableIterator<...>` / `AsyncIterable<...>` application.
//!
//! These run against the full default lib bundle (which includes
//! `es2018.asynciterable` / `es2018.asyncgenerator`) so `AsyncIterableIterator`
//! and friends are real global types, not stubs.

use std::sync::Arc;
use tsz_binder::lib_loader::LibFile;
use tsz_checker::context::CheckerOptions;
use tsz_checker::test_utils::{
    check_multi_file_with_libs, check_source_with_libs_code_messages, diagnostic_code_messages,
    load_default_lib_files, load_lib_files,
};
use tsz_common::common::ScriptTarget;

const TS2504: u32 = 2504;

fn check(source: &str, libs: &[Arc<LibFile>]) -> Vec<(u32, String)> {
    check_source_with_libs_code_messages(
        source,
        "test.ts",
        CheckerOptions {
            target: ScriptTarget::ES2018,
            ..CheckerOptions::default()
        },
        libs,
    )
}

fn assert_no_ts2504(source: &str, libs: &[Arc<LibFile>], context: &str) {
    let diags = check(source, libs);
    assert!(
        !diags.iter().any(|(code, _)| *code == TS2504),
        "{context}: expected no TS2504, got: {diags:#?}"
    );
}

fn assert_has_ts2504(source: &str, libs: &[Arc<LibFile>], context: &str) {
    let diags = check(source, libs);
    assert!(
        diags.iter().any(|(code, _)| *code == TS2504),
        "{context}: expected TS2504, got: {diags:#?}"
    );
}

fn load_es2022_dom_lib_files() -> Vec<Arc<LibFile>> {
    load_lib_files(&[
        "es5.d.ts",
        "es2015.d.ts",
        "es2015.core.d.ts",
        "es2015.collection.d.ts",
        "es2015.iterable.d.ts",
        "es2015.generator.d.ts",
        "es2015.promise.d.ts",
        "es2015.proxy.d.ts",
        "es2015.reflect.d.ts",
        "es2015.symbol.d.ts",
        "es2015.symbol.wellknown.d.ts",
        "es2016.array.include.d.ts",
        "es2017.arraybuffer.d.ts",
        "es2017.date.d.ts",
        "es2017.object.d.ts",
        "es2017.sharedmemory.d.ts",
        "es2017.string.d.ts",
        "es2017.typedarrays.d.ts",
        "es2018.asynciterable.d.ts",
        "es2018.asyncgenerator.d.ts",
        "es2018.promise.d.ts",
        "es2018.regexp.d.ts",
        "es2019.array.d.ts",
        "es2019.object.d.ts",
        "es2019.string.d.ts",
        "es2019.symbol.d.ts",
        "es2020.bigint.d.ts",
        "es2020.date.d.ts",
        "es2020.promise.d.ts",
        "es2020.sharedmemory.d.ts",
        "es2020.string.d.ts",
        "es2020.symbol.wellknown.d.ts",
        "es2021.promise.d.ts",
        "es2021.string.d.ts",
        "es2021.weakref.d.ts",
        "es2022.array.d.ts",
        "es2022.error.d.ts",
        "es2022.object.d.ts",
        "es2022.regexp.d.ts",
        "es2022.string.d.ts",
        "dom.d.ts",
    ])
}

#[test]
fn type_param_constrained_to_async_iterable_iterator_is_async_iterable() {
    let libs = load_default_lib_files();
    assert!(!libs.is_empty(), "default lib files must be available");
    assert_no_ts2504(
        r#"
async function f<T extends AsyncIterableIterator<number>>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends AsyncIterableIterator<number>",
    );
}

#[test]
fn type_param_constrained_to_async_iterable_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<S extends AsyncIterable<string>>(s: S) {
    for await (const x of s) { void x; }
}
"#,
        &libs,
        "S extends AsyncIterable<string>",
    );
}

#[test]
fn type_param_constrained_to_async_generator_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<G extends AsyncGenerator<boolean>>(g: G) {
    for await (const x of g) { void x; }
}
"#,
        &libs,
        "G extends AsyncGenerator<boolean>",
    );
}

#[test]
fn nested_type_param_constrained_to_async_iterable_is_async_iterable() {
    // The constraint is itself a type parameter, so resolution must be
    // transitive. Renamed bound variables (`Outer`/`Inner`) prove the fix is
    // structural rather than keyed on a `T`/`U` spelling.
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<Outer extends AsyncIterableIterator<number>, Inner extends Outer>(t: Inner) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "Inner extends Outer extends AsyncIterableIterator<number>",
    );
}

#[test]
fn type_param_constrained_to_intersection_with_async_iterable_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f<T extends AsyncIterableIterator<number> & { extra: string }>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends AsyncIterableIterator<number> & { extra }",
    );
}

#[test]
fn direct_async_iterable_iterator_remains_async_iterable() {
    // Regression guard: the non-type-parameter case already worked and must
    // keep working.
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
async function f(t: AsyncIterableIterator<number>) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "direct AsyncIterableIterator<number>",
    );
}

#[test]
fn async_iterable_iterator_from_function_call_result_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
interface QueryResult<R> {
    rows: R[];
}

interface DatabaseConnection {
    streamQuery<R>(query: string, chunkSize?: number): AsyncIterableIterator<QueryResult<R>>;
}

function patch(connection: DatabaseConnection): void {
    const streamQuery = connection.streamQuery;
    connection.streamQuery = async function* (
        query,
        chunkSize,
    ): AsyncIterableIterator<QueryResult<any>> {
        for await (const result of streamQuery.call(connection, query, chunkSize)) {
            yield result;
        }
    };
}
"#,
        &libs,
        "AsyncIterableIterator<QueryResult<unknown>> returned through Function.prototype.call",
    );
}

#[test]
fn imported_async_iterable_iterator_call_result_is_async_iterable() {
    let libs = load_es2022_dom_lib_files();
    let diagnostics = diagnostic_code_messages(check_multi_file_with_libs(
        &[
            (
                "./kysely.ts",
                r#"
declare global {
    interface AsyncDisposable {}
    interface SymbolConstructor {
        readonly asyncDispose: unique symbol;
    }
}

export {};
"#,
            ),
            (
                "./query-compiler/compiled-query.ts",
                r#"
export interface CompiledQuery {
    sql: string;
}
"#,
            ),
            (
                "./driver/database-connection.ts",
                r#"
import type { CompiledQuery } from "../query-compiler/compiled-query.js";

export interface DatabaseConnection {
    streamQuery<R>(
        compiledQuery: CompiledQuery,
        chunkSize?: number,
    ): AsyncIterableIterator<QueryResult<R>>;
}

export interface QueryResult<O> {
    readonly rows: O[];
}
"#,
            ),
            (
                "./driver/runtime-driver.ts",
                r#"
import type { DatabaseConnection, QueryResult } from "./database-connection.js";

class RuntimeDriver {
    #addLogging(connection: DatabaseConnection): void {
        const streamQuery = connection.streamQuery;

        connection.streamQuery = async function* (
            compiledQuery,
            chunkSize,
        ): AsyncIterableIterator<QueryResult<any>> {
            for await (const result of streamQuery.call(
                connection,
                compiledQuery,
                chunkSize,
            )) {
                yield result;
            }
        };
    }
}
"#,
            ),
        ],
        "./driver/runtime-driver.ts",
        CheckerOptions {
            target: ScriptTarget::ES2017,
            strict: true,
            strict_null_checks: true,
            no_implicit_any: true,
            ..CheckerOptions::default()
        },
        &libs,
    ));

    assert!(
        !diagnostics.iter().any(|(code, _)| *code == TS2504),
        "imported AsyncIterableIterator<QueryResult<unknown>> returned through Function.prototype.call must not emit TS2504, got: {diagnostics:#?}"
    );
}

#[test]
fn interface_extending_async_iterable_iterator_is_async_iterable() {
    let libs = load_default_lib_files();
    assert_no_ts2504(
        r#"
interface QueryResult<R> {
    readonly rows: R[];
}

interface QueryStream<R> extends AsyncIterableIterator<QueryResult<R>> {}

async function consume(stream: QueryStream<unknown>) {
    for await (const result of stream) {
        void result;
    }
}
"#,
        &libs,
        "interface inheriting AsyncIterableIterator<QueryResult<unknown>>",
    );
}

#[test]
fn type_param_constrained_to_non_iterable_object_still_reports_ts2504() {
    // Negative case: an object-shaped constraint without [Symbol.asyncIterator]
    // is not async iterable, so tsc (and tsz) still report TS2504.
    let libs = load_default_lib_files();
    assert_has_ts2504(
        r#"
async function f<T extends { foo: number }>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends { foo: number }",
    );
}

#[test]
fn type_param_constrained_to_number_still_reports_ts2504() {
    // Negative case: a primitive constraint is neither async- nor sync-iterable.
    let libs = load_default_lib_files();
    assert_has_ts2504(
        r#"
async function f<T extends number>(t: T) {
    for await (const x of t) { void x; }
}
"#,
        &libs,
        "T extends number",
    );
}
