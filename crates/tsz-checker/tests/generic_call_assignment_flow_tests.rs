//! Flow fallback coverage for generic call results assigned inside deferred closures.
//!
//! Structural rule: when a missed call-result cache entry has one generic signature
//! whose ordinary arguments determine a fully concrete return type, assignment flow
//! may use that instantiated return. Ambiguous, spread, underconstrained, and
//! nullable results remain conservative.

use std::sync::Arc;
use tsz_binder::BinderState;
use tsz_checker::context::CheckerOptions;
use tsz_checker::state::CheckerState;
use tsz_checker::test_utils::{
    check_multi_file_with_libs_stamped, check_source_with_libs, diagnostic_codes, load_lib_files,
    strict_checker_options,
};
use tsz_parser::parser::{NodeIndex, ParserState};
use tsz_solver::construction::TypeInterner;

const TS18048: u32 = 18048;
const TS2322: u32 = 2322;
const TS2339: u32 = 2339;
const TS2344: u32 = 2344;
const TS2345: u32 = 2345;
const TS2349: u32 = 2349;
const TS7006: u32 = 7006;

fn codes(source: &str) -> Vec<u32> {
    codes_with_options(source, strict_checker_options())
}

fn codes_with_options(source: &str, options: CheckerOptions) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts", "es2015.core.d.ts"]);
    let mut codes = diagnostic_codes(&check_source_with_libs(source, "test.ts", options, &libs));
    // These tests pin the complete diagnostic multiset from the TypeScript 7.0.2
    // oracle. Checker discovery order is not a source-order API guarantee.
    codes.sort_unstable();
    codes
}

fn multi_codes(files: &[(&str, &str)], entry: &str) -> Vec<u32> {
    let libs = load_lib_files(&["es5.d.ts", "es2015.core.d.ts"]);
    let mut codes = diagnostic_codes(&check_multi_file_with_libs_stamped(
        files,
        entry,
        strict_checker_options(),
        &libs,
    ));
    codes.sort_unstable();
    codes
}

fn resolve_generated_reexport_chain(
    barrel_count: usize,
    wildcard: bool,
) -> (
    Option<(tsz_binder::SymbolId, usize)>,
    tsz_binder::SymbolId,
    usize,
) {
    assert!(barrel_count > 0);
    let mut files = Vec::with_capacity(barrel_count + 2);
    files.push((
        "consumer.ts".to_string(),
        "import { seal } from './barrel0.js';\nseal({ value: 1 });".to_string(),
    ));
    for index in 0..barrel_count {
        let target = if index + 1 == barrel_count {
            "provider".to_string()
        } else {
            format!("barrel{}", index + 1)
        };
        let source = if wildcard {
            format!("export * from './{target}.js';")
        } else {
            format!("export {{ seal }} from './{target}.js';")
        };
        files.push((format!("barrel{index}.ts"), source));
    }
    files.push((
        "provider.ts".to_string(),
        "export function seal<Value>(value: Value): Value { return value; }".to_string(),
    ));

    let mut arenas = Vec::with_capacity(files.len());
    let mut binders = Vec::with_capacity(files.len());
    for (file_idx, (name, source)) in files.iter().enumerate() {
        let mut parser = ParserState::new(name.clone(), source.clone());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.set_file_idx(file_idx as u32);
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
    }

    let local_alias = binders[0]
        .file_locals
        .get("seal")
        .expect("consumer import alias");
    let provider_idx = files.len() - 1;
    let terminal = binders[provider_idx]
        .file_locals
        .get("seal")
        .expect("provider export");
    let file_names = files
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let (resolved_module_paths, resolved_modules) =
        tsz_checker::module_resolution::build_module_resolution_maps(&file_names);
    let arenas = Arc::new(arenas);
    let binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arenas[0].as_ref(),
        binders[0].as_ref(),
        &types,
        file_names[0].clone(),
        strict_checker_options(),
    );
    checker.ctx.set_all_arenas(Arc::clone(&arenas));
    checker.ctx.set_all_binders(Arc::clone(&binders));
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    (
        checker
            .ctx
            .resolve_import_alias_chain_with_owner_and_register(local_alias),
        terminal,
        provider_idx,
    )
}

#[test]
fn inferred_generic_result_narrows_inside_reduce_arrow() {
    let diagnostics = codes(
        r#"
interface Bucket {
    readonly name: string;
    readonly values: string[];
}
declare function freeze<T>(value: T): Readonly<T>;

function collect(names: string[]): Bucket[] {
    return names.reduce<Bucket[]>((buckets, name) => {
        let bucket = buckets.find((item) => item.name === name);
        if (!bucket) {
            bucket = freeze({ name, values: [] });
            buckets.push(bucket);
        }
        bucket.values.push(name);
        return buckets;
    }, []);
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "a fully inferred generic call result must kill undefined in the reducer closure: {diagnostics:?}"
    );
}

#[test]
fn imported_generic_result_narrows_inside_reduce_arrow() {
    let diagnostics = multi_codes(
        &[
            (
                "object-utils.ts",
                r#"
export function freeze<T>(value: T): Readonly<T> {
    return value;
}
"#,
            ),
            (
                "mysql-introspector.ts",
                r#"
import { freeze } from './object-utils.js';

interface TableMetadata {
    readonly name: string;
    readonly isView: boolean;
    readonly columns: ColumnMetadata[];
    readonly schema?: string;
}
interface ColumnMetadata { readonly name: string; }

interface RawColumnMetadata {
    readonly tableName: string;
    readonly tableType: string;
    readonly columnName: string;
}

export function collect(columns: RawColumnMetadata[]): TableMetadata[] {
    return columns.reduce<TableMetadata[]>((tables, item) => {
        let table = tables.find((candidate) => candidate.name === item.tableName);
        if (!table) {
            table = freeze({
                name: item.tableName,
                isView: item.tableType === "VIEW",
                schema: undefined,
                columns: [],
            });
            tables.push(table);
        }
        table.columns.push({ name: item.columnName });
        return tables;
    }, []);
}
"#,
            ),
        ],
        "mysql-introspector.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "an imported generic callee must publish the same concrete deferred result: {diagnostics:?}"
    );
}

#[test]
fn imported_generic_result_narrows_with_renamed_binders() {
    let diagnostics = multi_codes(
        &[
            (
                "sealer.ts",
                r#"
export function sealDeep<Payload>(input: Payload): Readonly<Payload> {
    return input;
}
"#,
            ),
            (
                "grouper.ts",
                r#"
import { sealDeep } from './sealer.js';

interface Bucket {
    readonly label: string;
    readonly entries: Entry[];
}
interface Entry { readonly id: string; }

export function group(ids: string[]): Bucket[] {
    return ids.reduce<Bucket[]>((buckets, id) => {
        let bucket = buckets.find((candidate) => candidate.label === id);
        if (!bucket) {
            bucket = sealDeep({ label: id, entries: [] });
            buckets.push(bucket);
        }
        bucket.entries.push({ id });
        return buckets;
    }, []);
}
"#,
            ),
        ],
        "grouper.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "renamed binders must follow the same imported-callee recovery rule: {diagnostics:?}"
    );
}

#[test]
fn imported_generic_result_narrows_inside_reduce_arrow_in_import_cycle() {
    // Import cycle: no file check order can type the provider first, so the
    // on-demand forcing retry is the only way flow can see the callee.
    let diagnostics = multi_codes(
        &[
            (
                "object-utils.ts",
                r#"
import type { TableMetadata } from './mysql-introspector.js';

export function freeze<T>(value: T): Readonly<T> {
    return value;
}

export function firstTable(tables: TableMetadata[]): TableMetadata | undefined {
    return tables[0];
}
"#,
            ),
            (
                "mysql-introspector.ts",
                r#"
import { freeze } from './object-utils.js';

export interface TableMetadata {
    readonly name: string;
    readonly isView: boolean;
    readonly columns: ColumnMetadata[];
}
interface ColumnMetadata { readonly name: string; }

export function collect(names: string[]): TableMetadata[] {
    return names.reduce<TableMetadata[]>((tables, name) => {
        let table = tables.find((candidate) => candidate.name === name);
        if (!table) {
            table = freeze({ name, isView: false, columns: [] });
            tables.push(table);
        }
        table.columns.push({ name });
        return tables;
    }, []);
}
"#,
            ),
        ],
        "mysql-introspector.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "an import cycle must not degrade the imported-callee recovery: {diagnostics:?}"
    );
}

#[test]
fn imported_generic_nullable_result_keeps_possibly_undefined() {
    // Negative control: recovering the imported signature must not
    // over-narrow. A `Readonly<T> | undefined` result keeps `undefined`
    // alive, so tsc reports TS2345 at `tables.push(table)` and TS18048 at
    // the later property access (oracle: typescript@7.0.2).
    let diagnostics = multi_codes(
        &[
            (
                "object-utils.ts",
                r#"
export function maybeFreeze<T>(value: T): Readonly<T> | undefined {
    return value;
}
"#,
            ),
            (
                "mysql-introspector.ts",
                r#"
import { maybeFreeze } from './object-utils.js';

interface TableMetadata {
    readonly name: string;
    readonly isView: boolean;
    readonly columns: ColumnMetadata[];
}
interface ColumnMetadata { readonly name: string; }

export function collect(names: string[]): TableMetadata[] {
    return names.reduce<TableMetadata[]>((tables, name) => {
        let table = tables.find((candidate) => candidate.name === name);
        if (!table) {
            table = maybeFreeze({ name, isView: false, columns: [] });
            tables.push(table);
        }
        table.columns.push({ name });
        return tables;
    }, []);
}
"#,
            ),
        ],
        "mysql-introspector.ts",
    );

    assert_eq!(
        diagnostics,
        vec![TS2345, TS18048],
        "a nullable imported generic result must keep both oracle diagnostics"
    );
}

#[test]
fn reexported_generic_result_narrows_inside_deferred_function() {
    let diagnostics = multi_codes(
        &[
            (
                "object-utils.ts",
                r#"
export function seal<Value>(value: Value): Readonly<Value> {
    return value;
}
"#,
            ),
            (
                "index.ts",
                r#"
export { seal as preserve } from "./object-utils.js";
"#,
            ),
            (
                "consumer.ts",
                r#"
import { preserve } from "./index.js";

interface Entry {
    readonly key: string;
    readonly values: string[];
}

export function callback(entry: Entry | undefined, key: string) {
    return function (): void {
        if (!entry) {
            entry = preserve({ key, values: [] });
        }
        entry.values.push(key);
    };
}
"#,
            ),
        ],
        "consumer.ts",
    );

    assert!(
        diagnostics.is_empty(),
        "a re-export alias must reach the terminal generic signature without retaining undefined: {diagnostics:?}"
    );
}

#[test]
fn named_alias_to_wildcard_reexport_tracks_owner_when_raw_symbol_ids_repeat() {
    let files = [
        (
            "consumer.ts",
            "type Local0 = 0;\ntype Local1 = 1;\nimport { poison } from './decoy.js';\nimport { preserve } from './bridge.js';\npreserve({ value: poison });",
        ),
        (
            "bridge.ts",
            "type Pad0 = 0;\ntype Pad1 = 1;\nimport { seal as preserve } from './barrel.js';\nexport { preserve };",
        ),
        ("barrel.ts", "export * from './object-utils.js';"),
        (
            "object-utils.ts",
            "type Pad0 = 0;\ntype Pad1 = 1;\nexport function seal<Value>(value: Value): Value { return value; }",
        ),
        ("decoy.ts", "export const poison = 1;"),
    ];
    let mut arenas = Vec::new();
    let mut binders = Vec::new();
    for (file_idx, (name, source)) in files.iter().enumerate() {
        let mut parser = ParserState::new((*name).to_string(), (*source).to_string());
        let root = parser.parse_source_file();
        let mut binder = BinderState::new();
        binder.set_file_idx(file_idx as u32);
        binder.bind_source_file(parser.get_arena(), root);
        arenas.push(Arc::new(parser.get_arena().clone()));
        binders.push(Arc::new(binder));
    }

    let local_alias = binders[0]
        .file_locals
        .get("preserve")
        .expect("consumer import alias");
    let terminal = binders[3]
        .file_locals
        .get("seal")
        .expect("terminal generic function");
    let unrelated_local_alias = binders[0]
        .file_locals
        .get("poison")
        .expect("unrelated local import alias");
    assert_ne!(
        local_alias, terminal,
        "the actual import binding must differ from the foreign terminal id"
    );
    assert_eq!(
        unrelated_local_alias, terminal,
        "foreign terminal must collide with an unrelated current-file alias"
    );
    assert!(
        binders[0]
            .get_symbol(terminal)
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::ALIAS)),
        "the colliding current-file symbol must be the decoy alias"
    );
    assert!(
        binders[3]
            .get_symbol(terminal)
            .is_some_and(|symbol| symbol.has_any_flags(tsz_binder::symbol_flags::FUNCTION)),
        "the foreign owner must classify the same raw id as the terminal function"
    );
    let preserve_use = arenas[0]
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            arenas[0]
                .get_identifier(node)
                .filter(|identifier| identifier.escaped_text == "preserve")
                .map(|_| NodeIndex(index as u32))
        })
        .next_back()
        .expect("preserve call identifier");
    assert_eq!(
        binders[0].resolve_identifier_with_filter(arenas[0].as_ref(), preserve_use, &[], |_| true,),
        Some(local_alias),
        "unfollowed scope lookup must return the actual local import binding"
    );

    let file_names = files
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect::<Vec<_>>();
    let (resolved_module_paths, resolved_modules) =
        tsz_checker::module_resolution::build_module_resolution_maps(&file_names);
    let arenas = Arc::new(arenas);
    let binders = Arc::new(binders);
    let types = TypeInterner::new();
    let mut checker = CheckerState::new(
        arenas[0].as_ref(),
        binders[0].as_ref(),
        &types,
        file_names[0].clone(),
        strict_checker_options(),
    );
    checker.ctx.set_all_arenas(Arc::clone(&arenas));
    checker.ctx.set_all_binders(Arc::clone(&binders));
    checker.ctx.set_current_file_idx(0);
    checker.ctx.set_lib_contexts(Vec::new());
    checker
        .ctx
        .set_resolved_module_paths(Arc::new(resolved_module_paths));
    checker.ctx.set_resolved_modules(resolved_modules);

    let resolved = checker
        .ctx
        .resolve_import_alias_chain_with_owner_and_register(local_alias);
    assert_eq!(resolved, Some((terminal, 3)));
    assert_eq!(
        checker.ctx.resolve_dynamic_symbol_file_index(terminal),
        Some(3)
    );
}

#[test]
fn owner_carried_reexport_walk_has_one_shared_step_budget() {
    for wildcard in [false, true] {
        let (at_cap, terminal, provider_idx) = resolve_generated_reexport_chain(63, wildcard);
        assert_eq!(
            at_cap,
            Some((terminal, provider_idx)),
            "a supported named/wildcard chain must resolve at the 64-step cap"
        );

        let (over_cap, _, _) = resolve_generated_reexport_chain(64, wildcard);
        assert_eq!(
            over_cap, None,
            "a 65-step named/wildcard chain must fail closed"
        );
    }
}

#[test]
fn colliding_generic_terminals_keep_flow_diagnostics_clean_across_file_orders() {
    const LEFT: &str = r#"
export function retain<Value>(value: Value): Readonly<Value> {
    return value;
}
"#;
    const RIGHT: &str = r#"
export function enclose<Item>(value: Item): { payload: Item } {
    return { payload: value };
}
"#;
    const CONSUMER: &str = r#"
import { retain } from "./left.js";
import { enclose } from "./right.js";

interface LeftValue { readonly name: string; readonly values: string[]; }
interface RightValue { readonly payload: { readonly count: number }; }

export function deferred(leftValues: LeftValue[], rightValues: RightValue[]) {
    return (): void => {
        let left = leftValues.find((candidate) => candidate.name === "missing");
        let right = rightValues.find((candidate) => candidate.payload.count === -1);
        if (!left) left = retain({ name: "left", values: [] });
        if (!right) right = enclose({ count: 1 });
        left.values.push(left.name);
        right.payload.count.toFixed();
    };
}
"#;

    let terminal_ids = [("left.ts", LEFT, "retain"), ("right.ts", RIGHT, "enclose")].map(
        |(name, source, export)| {
            let mut parser = ParserState::new(name.to_string(), source.to_string());
            let root = parser.parse_source_file();
            let mut binder = BinderState::new();
            binder.bind_source_file(parser.get_arena(), root);
            binder.file_locals.get(export).expect("terminal export")
        },
    );
    assert_eq!(
        terminal_ids[0], terminal_ids[1],
        "the diagnostic witness requires colliding binder-relative terminal ids"
    );

    for files in [
        [
            ("left.ts", LEFT),
            ("right.ts", RIGHT),
            ("consumer.ts", CONSUMER),
        ],
        [
            ("right.ts", RIGHT),
            ("left.ts", LEFT),
            ("consumer.ts", CONSUMER),
        ],
    ] {
        let libs = load_lib_files(&["es5.d.ts", "es2015.core.d.ts"]);
        let diagnostics = check_multi_file_with_libs_stamped(
            &files,
            "consumer.ts",
            strict_checker_options(),
            &libs,
        );
        assert!(
            diagnostics.is_empty(),
            "terminal owner identity must not depend on provider order: {diagnostics:#?}"
        );
    }
}

#[test]
fn inferred_generic_result_narrows_inside_function_expression() {
    let diagnostics = codes(
        r#"
interface Group {
    readonly id: string;
    readonly members: string[];
}
declare function retain<T>(value: T): Readonly<T>;

function callback(groups: Group[]) {
    return function (id: string): void {
        let group = groups.find((candidate) => candidate.id === id);
        if (!group) {
            group = retain({ id, members: [] });
        }
        group.members.push(id);
    };
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "renamed binders in a deferred function expression must use the same structural rule: {diagnostics:?}"
    );
}

#[test]
fn concrete_factory_assignment_remains_narrowed() {
    let diagnostics = codes(
        r#"
interface Item {
    key: string;
    values: string[];
}
declare function makeItem(key: string): Item;

function callback(items: Item[]) {
    return (key: string): void => {
        let item = items.find((candidate) => candidate.key === key);
        if (!item) {
            item = makeItem(key);
        }
        item.values.push(key);
    };
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "the existing concrete-call flow path must remain narrowed: {diagnostics:?}"
    );
}

#[test]
fn ordinary_function_generic_assignment_remains_narrowed() {
    let diagnostics = codes(
        r#"
interface Row {
    key: string;
    values: string[];
}
declare function preserve<T>(value: T): Readonly<T>;

function append(rows: Row[], key: string): void {
    let row = rows.find((candidate) => candidate.key === key);
    if (!row) {
        row = preserve({ key, values: [] });
    }
    row.values.push(key);
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "ordinary-function generic assignment flow must remain clean: {diagnostics:?}"
    );
}

#[test]
fn defaulted_generic_result_narrows_after_implicit_inference() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}
declare function preserve<T, U = T>(value: T): Readonly<U>;

function callback(box: BoxValue | undefined) {
    return function (label: string): void {
        if (!box) {
            box = preserve({ label, values: [] });
        }
        box.values.push(label);
    };
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "an uninferred parameter with a concrete default must instantiate before flow: {diagnostics:?}"
    );
}

#[test]
fn constrained_generic_result_narrows_when_argument_satisfies_constraint() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}
declare function preserve<T extends BoxValue>(value: T): Readonly<T>;

function callback(box: BoxValue | undefined) {
    return function (label: string): void {
        if (!box) {
            box = preserve({ label, values: [] });
        }
        box.values.push(label);
    };
}
"#,
    );

    assert!(
        diagnostics.is_empty(),
        "a satisfied generic constraint must retain its concrete non-null result: {diagnostics:?}"
    );
}

#[test]
fn nullable_generic_result_stays_possibly_undefined() {
    let diagnostics = codes(
        r#"
interface Slot {
    name: string;
    values: string[];
}
declare function maybePreserve<T>(value: T): Readonly<T> | undefined;

function callback(slots: Slot[]) {
    return (name: string): void => {
        let slot = slots.find((candidate) => candidate.name === name);
        if (!slot) {
            slot = maybePreserve({ name, values: [] });
        }
        slot.values.push(name);
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS18048],
        "a nullable generic return must remain nullable after assignment"
    );
}

#[test]
fn call_without_assignment_does_not_kill_undefined() {
    let diagnostics = codes(
        r#"
interface Queue {
    name: string;
    values: string[];
}
declare function preserve<T>(value: T): Readonly<T>;

function callback(queues: Queue[]) {
    return (name: string): void => {
        let queue = queues.find((candidate) => candidate.name === name);
        if (!queue) {
            preserve({ name, values: [] });
        }
        queue.values.push(name);
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS18048],
        "a non-assignment call must not kill undefined"
    );
}

#[test]
fn underconstrained_generic_result_keeps_conservative_flow() {
    let diagnostics = codes(
        r#"
interface Cell {
    label: string;
    values: string[];
}
declare function project<T, U>(value: T): U;

function callback(cells: Cell[]) {
    return (label: string): void => {
        let cell = cells.find((candidate) => candidate.label === label);
        if (!cell) {
            cell = project({ label, values: [] });
        }
        cell.values.push(label);
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS18048],
        "an uninferred return parameter must not become a killing definition"
    );
}

#[test]
fn implicit_any_argument_still_narrows_non_nullable_result() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}

declare function preserve<T>(value: T): Readonly<T>;

function callback(box: BoxValue | undefined) {
    return function (label) {
        if (!box) {
            box = preserve({ label, values: [] });
        }
        box.values.push("x");
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS7006],
        "semantic any still infers a definitely non-null generic call result"
    );
}

#[test]
fn free_argument_parameter_does_not_publish_killing_definition() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}

declare function preserve<T>(value: T): Readonly<T>;

function callback<K>(box: BoxValue | undefined) {
    return function (label: K): void {
        if (!box) {
            box = preserve({ label, values: [] });
        }
        box.values.push("x");
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2322, TS18048],
        "a free argument parameter must not become a fully concrete provisional result"
    );
}

#[test]
fn incompatible_generic_result_keeps_possibly_undefined_flow() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}
declare function preserve<T>(value: T): Readonly<T>;

function callback(box: BoxValue | undefined) {
    return function (): void {
        if (!box) {
            box = preserve({ label: 1, values: [] });
        }
        box.values.push("x");
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2322, TS18048],
        "a concrete but incompatible return must not become a killing definition"
    );
}

#[test]
fn violated_constraint_uses_the_constrained_non_null_result() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}
declare function preserve<T extends BoxValue>(value: T): Readonly<T>;

function callback(box: BoxValue | undefined) {
    return function (): void {
        if (!box) {
            box = preserve({ label: 1, values: [] });
        }
        box.values.push("x");
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2322],
        "failed argument inference must fall back to the signature constraint"
    );
}

#[test]
fn dependent_constraint_fallback_narrows_the_return() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}
declare function preserve<T extends U, U extends BoxValue>(value: T): Readonly<T>;

function callback(box: BoxValue | undefined) {
    return function (): void {
        if (!box) {
            box = preserve({ label: 1, values: [] });
        }
        box.values.push("x");
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2322],
        "dependent constraints must be instantiated after all candidates are known"
    );
}

#[test]
fn dependent_default_constraint_fallback_narrows_the_return() {
    let diagnostics = codes(
        r#"
interface BoxValue {
    label: string;
    values: string[];
}
declare function preserve<T extends U, U extends BoxValue = BoxValue>(value: T): Readonly<T>;

function callback(box: BoxValue | undefined) {
    return function (): void {
        if (!box) {
            box = preserve({ label: 1, values: [] });
        }
        box.values.push("x");
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2322],
        "a later default must be available while validating a dependent constraint"
    );
}

#[test]
fn invalid_default_uses_the_constraint_for_call_flow() {
    let diagnostics = codes(
        r#"
declare function choose<T extends string = number>(): T;

function callback(value: string | undefined) {
    return function (): void {
        if (!value) {
            value = choose();
        }
        value.toUpperCase();
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2344],
        "an invalid default diagnoses its declaration but the call uses the constraint"
    );
}

#[test]
fn overloaded_and_spread_nullable_calls_keep_conservative_flow() {
    let overloaded = codes(
        r#"
interface NodeValue { name: string; values: string[]; }
declare function maybeCopy<T>(value: T): Readonly<T> | undefined;
declare function maybeCopy<T>(value: T, marker: boolean): Readonly<T> | undefined;

function callback(nodes: NodeValue[]) {
    return (name: string): void => {
        let node = nodes.find((candidate) => candidate.name === name);
        if (!node) node = maybeCopy({ name, values: [] });
        node.values.push(name);
    };
}
"#,
    );
    assert_eq!(
        overloaded,
        vec![TS18048],
        "flow fallback must not select among overloaded generic signatures"
    );

    let spread = codes(
        r#"
interface NodeValue { name: string; values: string[]; }
declare function maybeCopy<T>(...values: [T]): Readonly<T> | undefined;

function callback(nodes: NodeValue[]) {
    return (name: string): void => {
        let node = nodes.find((candidate) => candidate.name === name);
        const args: [{ name: string; values: string[] }] = [{ name, values: [] }];
        if (!node) node = maybeCopy(...args);
        node.values.push(name);
    };
}
"#,
    );
    assert_eq!(
        spread,
        vec![TS18048],
        "flow fallback must not infer through a spread argument"
    );
}

#[test]
fn partially_overlapping_generic_result_does_not_narrow_invalid_assignment() {
    let diagnostics = codes(
        r#"
declare function preserve<T>(value: T): T | boolean;

function callback(value: string | undefined) {
    return function (): void {
        if (value === undefined) {
            value = preserve("x");
        }
        value.toUpperCase();
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2322, TS18048],
        "one compatible union member must not hide an invalid whole-RHS assignment"
    );
}

#[test]
fn annotated_initializer_whole_rhs_respects_strict_null_checks() {
    let source = r#"
declare const source: { value: string } | null;
const target: { value: string } | { other: number } = source;
target.value.toUpperCase();
"#;
    let strict = strict_checker_options();
    assert_eq!(
        codes_with_options(source, strict.clone()),
        vec![TS2322, TS2339]
    );

    let mut loose = strict;
    loose.strict_null_checks = false;
    assert!(codes_with_options(source, loose).is_empty());
}

#[test]
fn annotated_initializer_whole_rhs_respects_strict_function_types() {
    let source = r#"
interface Animal { animal: true }
interface Dog extends Animal { dog: true }
declare const source: (value: Dog) => void;
const target: ((value: Animal) => void) | { other: number } = source;
target({ animal: true });
"#;
    let strict = strict_checker_options();
    assert_eq!(
        codes_with_options(source, strict.clone()),
        vec![TS2322, TS2349]
    );

    let mut loose = strict;
    loose.strict_function_types = false;
    assert!(codes_with_options(source, loose).is_empty());
}

#[test]
fn annotated_initializer_whole_rhs_respects_exact_optional_properties() {
    let source = r#"
declare const source: { value: string; optional: undefined };
const target: { value: string; optional?: string } | { other: number } = source;
target.value.toUpperCase();
"#;
    let mut exact = strict_checker_options();
    exact.exact_optional_property_types = true;
    assert_eq!(codes_with_options(source, exact), vec![TS2322, TS2339]);

    let loose = strict_checker_options();
    assert!(codes_with_options(source, loose).is_empty());
}

#[test]
fn provisional_generic_fallback_does_not_poison_canonical_rhs_flow() {
    let diagnostics = codes(
        r#"
interface ExactBox {
    kind: "fixed";
    values: string[];
}
declare function preserve<T>(value: T): Readonly<T>;

function callback(box: ExactBox | undefined) {
    return function (): number {
        if (box === undefined) {
            box = preserve({ kind: "fixed", values: [] }) as unknown;
        }
        return box.values.length;
    };
}
"#,
    );

    assert_eq!(
        diagnostics,
        vec![TS2322, TS18048],
        "an early syntactic fallback must not outlive the canonical unknown RHS type"
    );
}
